//! 名前付き型引数を、要求による定義時検査と具体型での通常検査へ接続する。
//! 数値のint・floatとは代入表を分け、演算のASTや格納境界を変更しない。
mod requirements;
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
    if explicit.len() != signature.type_parameters {
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
    let symbolic = types.iter().any(|t| scope.structs.symbolic(*t));
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
    let concrete = !arguments.iter().any(|t| scope.structs.symbolic(*t));
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
