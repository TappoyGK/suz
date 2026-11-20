//! 名前付き型引数を、要求による定義時検査と具体型での通常検査へ接続する。
//! 数値のint・floatとは代入表を分け、演算のASTや格納境界を変更しない。
mod inference;
mod requirements;
mod shapes;
pub(super) mod structs;
use super::{
    functions,
    ir::{CheckedExpression, CheckedFunction, Value},
    scope::{Scope, Signature},
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, FailureHandler, Location, Program, Type},
};
pub(super) use requirements::{hint, operation, struct_equality};
use std::rc::Rc;

/// 具体化待ちの関数。依存経路は型引数が増え続ける再帰を検出するために保持する。
#[derive(Clone)]
pub(super) struct Instance {
    function: usize,
    arguments: Vec<Type>,
    id: usize,
    path: Rc<Vec<(usize, Vec<Type>)>>,
    location: Location,
}

pub(super) fn call(
    expr: &Expression,
    signature: &Signature,
    explicit: &[Type],
    arguments: &[Expression],
    scope: &Scope,
    handler: Option<&FailureHandler>,
) -> Result<CheckedExpression, Diagnostic> {
    let receiver_arguments;
    let explicit = if signature.method.is_some() {
        let receiver = arguments
            .first()
            .and_then(|a| super::expression::operand_type(a, scope))
            .ok_or_else(|| {
                expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("method.call", "cannot determine receiver type"),
                )
            })?;
        receiver_arguments = scope.method_arguments(receiver);
        receiver_arguments.as_slice()
    } else {
        explicit
    };
    if (signature.method.is_some() || !explicit.is_empty())
        && explicit.len() != signature.type_parameters
    {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "generic.type-arguments",
                "expected {} explicit type arguments, found {}",
                signature.type_parameters,
                explicit.len()
            ),
        ));
    }
    let types: Vec<_> = if signature.method.is_some() {
        explicit.to_vec()
    } else if explicit.is_empty() {
        inference::resolve(signature, arguments, scope, expr.location, true)?
    } else {
        explicit
            .iter()
            .map(|t| scope.resolve_type_at(*t, expr.location))
            .collect::<Result<_, _>>()?
    };
    check_requirements(signature, &types, scope, expr.location)?;
    let result = scope.substitute(signature.result, &types, expr.location)?;
    if matches!(result, Type::Function(_)) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.type-call",
                "call-only function values cannot be returned"
            ),
        ));
    }
    for &ty in signature.parameters.iter().chain([&signature.result]) {
        let ty = scope.substitute(ty, &types, expr.location)?;
        if ty.pointee().is_some() && matches!(ty.leaf(), Type::Function(_)) {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("arena.ptr-type", "pointer requires a stored value type"),
            ));
        }
    }
    let checked_arguments = arguments
        .iter()
        .zip(&signature.parameters)
        .map(|(arg, ty)| {
            super::function_values::argument(
                arg,
                scope.substitute(*ty, &types, expr.location)?,
                scope,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let symbolic = types.iter().any(|t| scope.structs.unresolved(*t));
    let id = if symbolic {
        signature.id
    } else {
        request(signature.id, types.clone(), scope, expr.location)?
    };
    super::errors::finish(
        CheckedExpression {
            ty: result,
            value: Value::Call {
                id,
                arguments: checked_arguments,
                arena: signature.arena,
                location: expr.location,
            },
        },
        &signature.errors,
        handler,
        expr,
        scope,
    )
}

// 具体型の要求は呼び出し元に依存しないので、成功を宣言と型引数ごとに再利用する。
// 型引数が残る転送は、呼び出し元の有効な要求が異なるため毎回検査する。
fn check_requirements(
    signature: &Signature,
    arguments: &[Type],
    scope: &Scope,
    location: Location,
) -> Result<(), Diagnostic> {
    if signature.needs.is_empty() {
        return Ok(());
    }
    let concrete = !arguments.iter().any(|t| scope.structs.unresolved(*t));
    let key = (signature.id, arguments.to_vec());
    if concrete && scope.instances.borrow().named_requirements.contains(&key) {
        return Ok(());
    }
    requirements::validate_substitution(&signature.needs, arguments, scope, location)?;
    for need in &signature.needs {
        requirements::satisfy(need, arguments, scope, location)?;
    }
    if concrete {
        scope.instances.borrow_mut().named_requirements.insert(key);
    }
    Ok(())
}

fn request(
    function: usize,
    arguments: Vec<Type>,
    scope: &Scope,
    location: Location,
) -> Result<usize, Diagnostic> {
    // 同じ具体化への再帰は再利用する。異なる具体化への再帰は、全型引数が同じか縮み、
    // 少なくとも一つが厳密に縮む場合だけ有限として扱う。
    for (previous_function, previous) in scope.named.instantiation_path.iter().rev() {
        if *previous_function != function || *previous == arguments {
            continue;
        }
        if !previous
            .iter()
            .zip(&arguments)
            .all(|(&a, &b)| scope.structs.contains_argument(a, b))
        {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.recursion",
                    "recursive instantiation [{}] -> [{}] must preserve or shrink each type argument",
                    previous.iter().map(|t| scope.type_name(*t)).collect::<Vec<_>>().join(", "),
                    arguments.iter().map(|t| scope.type_name(*t)).collect::<Vec<_>>().join(", ")
                ),
            ));
        }
    }
    let mut instances = scope.instances.borrow_mut();
    let key = (function, arguments.clone());
    if let Some(&id) = instances.named_ids.get(&key) {
        return Ok(id);
    }
    let id = instances.next_id;
    instances.next_id += 1;
    instances.named_ids.insert(key, id);
    let mut path = scope.named.instantiation_path.as_ref().clone();
    path.push((function, arguments.clone()));
    instances.named.push(Instance {
        function,
        arguments,
        id,
        path: Rc::new(path),
        location,
    });
    Ok(id)
}

pub(super) fn check_instances(
    program: &Program,
    globals: &Scope,
    output: &mut Vec<CheckedFunction>,
    warnings: &mut Vec<Location>,
) -> Result<(), Diagnostic> {
    loop {
        let instance = {
            let mut instances = globals.instances.borrow_mut();
            let Some(instance) = instances.named.get(instances.named_cursor).cloned() else {
                break;
            };
            instances.named_cursor += 1;
            instance
        };
        let function = &program.functions[instance.function];
        let mut scope = globals.clone();
        let types = instance
            .arguments
            .iter()
            .map(|t| scope.type_name(*t))
            .collect::<Vec<_>>()
            .join(", ");
        scope.named = Rc::new(super::scope::NamedTypes {
            type_arguments: Some(Rc::new(instance.arguments)),
            instantiation_path: instance.path,
            ..Default::default()
        });
        let (checked, found) =
            functions::check_function(function, scope, instance.id).map_err(|error| {
                let mut diagnostic = instance.location.error(
                    error.kind,
                    crate::diagnostic!(
                        "generic.check",
                        "generic function '{}[{}]' is invalid for these type arguments",
                        function.name,
                        types
                    ),
                );
                diagnostic.related.push(error);
                diagnostic
            })?;
        output.push(checked);
        warnings.extend(found);
    }
    Ok(())
}

/// メソッドの署名を受け手の型引数で置き換える。本体の具体化は実際の呼び出し時に行う。
pub(super) fn method_signature(
    signature: &Signature,
    receiver: Type,
    scope: &Scope,
    location: Location,
) -> Result<Signature, Diagnostic> {
    let arguments = scope.method_arguments(receiver);
    let mut resolved = signature.clone();
    if signature.type_parameters == 0 {
        return Ok(resolved);
    }
    check_requirements(signature, &arguments, scope, location)?;
    resolved.parameters = signature
        .parameters
        .iter()
        .map(|t| scope.substitute(*t, &arguments, location))
        .collect::<Result<_, _>>()?;
    resolved.result = scope.substitute(signature.result, &arguments, location)?;
    if let Some(method) = &mut resolved.method {
        method.owner = scope.substitute(method.owner, &arguments, location)?;
    }
    resolved.type_parameters = 0;
    resolved.type_parameter_names = None;
    Ok(resolved)
}

/// 型を使う位置で、構造体本体とフィールドに含まれる型の要求を確認する。
/// メソッド固有の要求はここでは検査しない。
pub(super) fn struct_requirements(
    ty: Type,
    scope: &Scope,
    location: Location,
) -> Result<(), Diagnostic> {
    let mut pending = vec![ty];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(ty) = pending.pop() {
        let Type::Struct(id) = ty.leaf() else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let declaration = scope.structs.get(id);
        if let Some(app) = &declaration.application {
            pending.extend(app.arguments.iter().copied());
            super::operator_methods::validate_instantiation(ty.leaf(), scope, location)?;
            requirements::validate_substitution(
                &declaration.needs,
                &app.arguments,
                scope,
                location,
            )?;
            for need in &declaration.needs {
                requirements::satisfy(need, &app.arguments, scope, location)?;
            }
        }
        pending.extend(declaration.fields.iter().map(|f| f.ty));
    }
    Ok(())
}

/// 呼び出し結果の型ヒントも、実際の呼び出しと同じ引数推論を使う。
pub(super) fn call_hint(
    signature: &Signature,
    explicit: &[Type],
    arguments: &[Expression],
    scope: &Scope,
    location: Location,
) -> Option<Type> {
    let types = if explicit.is_empty() {
        inference::resolve(signature, arguments, scope, location, false).ok()?
    } else {
        if explicit.len() != signature.type_parameters {
            return None;
        }
        explicit
            .iter()
            .map(|ty| scope.resolve_type_at(*ty, location).ok())
            .collect::<Option<Vec<_>>>()?
    };
    scope.substitute(signature.result, &types, location).ok()
}

fn value_arguments(
    signature: &Signature,
    explicit: &[Type],
    scope: &Scope,
    location: Location,
) -> Result<Vec<Type>, Diagnostic> {
    if explicit.len() != signature.type_parameters || signature.type_parameters == 0 {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "generic.type-arguments",
                "function values require all {} type arguments explicitly; found {}",
                signature.type_parameters,
                explicit.len()
            ),
        ));
    }
    explicit
        .iter()
        .map(|t| scope.resolve_type_at(*t, location))
        .collect()
}

pub(super) fn function_value_type(
    signature: &Signature,
    explicit: &[Type],
    scope: &Scope,
    location: Location,
) -> Result<Type, Diagnostic> {
    let arguments = value_arguments(signature, explicit, scope, location)?;
    specialized_function_type(signature, &arguments, scope, location)
}

fn specialized_function_type(
    signature: &Signature,
    arguments: &[Type],
    scope: &Scope,
    location: Location,
) -> Result<Type, Diagnostic> {
    let parameters = signature
        .parameters
        .iter()
        .map(|t| scope.substitute(*t, arguments, location))
        .collect::<Result<Vec<_>, _>>()?;
    let result = scope.substitute(signature.result, arguments, location)?;
    if matches!(result, Type::Function(_)) {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.type-call",
                "call-only function values cannot be returned"
            ),
        ));
    }
    for ty in parameters.iter().chain([&result]) {
        if ty.pointee().is_some() && matches!(ty.leaf(), Type::Function(_)) {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("arena.ptr-type", "pointer requires a stored value type"),
            ));
        }
    }
    Ok(scope.intern_function(crate::ast::FunctionType {
        parameters,
        result,
        errors: signature.errors.clone(),
    }))
}

pub(super) fn function_value(
    expr: &Expression,
    signature: &Signature,
    explicit: &[Type],
    expected: Option<Type>,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let arguments = value_arguments(signature, explicit, scope, expr.location)?;
    check_requirements(signature, &arguments, scope, expr.location)?;
    let ty = specialized_function_type(signature, &arguments, scope, expr.location)?;
    if let Some(required) = expected.filter(|required| *required != ty) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.value-from-name",
                "function type {} does not match required {}",
                scope.type_name(ty),
                scope.type_name(required)
            ),
        ));
    }
    let id = if arguments.iter().any(|ty| scope.structs.unresolved(*ty)) {
        signature.id
    } else {
        request(signature.id, arguments, scope, expr.location)?
    };
    Ok(CheckedExpression {
        ty,
        value: Value::Function(id),
    })
}

/// 名前付き関数型の要求は、構造的に同じ型へまとめる前に使用した宣言で検査する。
/// 同じ署名を持つ別のfnshapeへ、要求が混入しないようにする。
pub(super) fn shape_requirements(
    ty: Type,
    scope: &Scope,
    location: Location,
) -> Result<(), Diagnostic> {
    let mut pending = vec![ty];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(ty) = pending.pop() {
        let (id, shape) = match ty.leaf() {
            Type::StaticShape(id) | Type::Shape(id) => (id, true),
            Type::Application(id) => (id, false),
            _ => continue,
        };
        if !seen.insert((id, shape)) {
            continue;
        }
        let app = scope.structs.application(id);
        pending.extend(app.arguments.iter().copied());
        if !shape {
            continue;
        }
        let declaration = &scope.structs.shapes.declarations[app.declaration];
        let arguments = app
            .arguments
            .iter()
            .map(|t| {
                scope.substitute(
                    *t,
                    scope
                        .named
                        .type_arguments
                        .as_deref()
                        .map_or(&[], Vec::as_slice),
                    location,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        requirements::validate_substitution(&declaration.needs, &arguments, scope, location)?;
        for need in &declaration.needs {
            requirements::satisfy(need, &arguments, scope, location)?;
        }
        for argument in arguments {
            struct_requirements(argument, scope, location)?;
        }
    }
    Ok(())
}

/// 要求の型を解決してから、別名を含む重複・矛盾と内側の型の要求を検査する。
pub(super) fn definition_needs(scope: &Scope, location: Location) -> Result<(), Diagnostic> {
    if scope.named.needs.is_empty() {
        return Ok(());
    }
    let arguments: Vec<_> = (0..scope.named.type_parameters.len())
        .map(Type::Parameter)
        .collect();
    requirements::validate_substitution(&scope.named.needs, &arguments, scope, location)?;
    for need in scope.named.needs.iter() {
        for &ty in need.parameters.iter().chain([&need.result]) {
            scope.resolve_type_at(ty, need.location)?;
        }
    }
    Ok(())
}
