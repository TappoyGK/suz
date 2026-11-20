//! 名前付き型引数を、要求による定義時検査と具体型での通常検査へ接続する。
//! 数値のint・floatとは代入表を分け、演算のASTや格納境界を変更しない。
mod requirements;
use super::{
    functions,
    ir::{CheckedExpression, CheckedFunction, Value},
    scope::{Scope, Signature},
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, FailureHandler, Location, Program, Type},
};
pub(super) use requirements::{hint, operation};
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

pub(super) fn substitute(ty: Type, arguments: &[Type]) -> Type {
    if let Some(inner) = ty.pointee() {
        substitute(inner, arguments)
            .pointer()
            .with_nullable(ty.is_nullable())
    } else if let Type::Parameter(id) = ty {
        arguments[id]
    } else {
        ty
    }
}

pub(super) fn call(
    expr: &Expression,
    signature: &Signature,
    explicit: &[Type],
    arguments: &[Expression],
    scope: &Scope,
    handler: Option<&FailureHandler>,
) -> Result<CheckedExpression, Diagnostic> {
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
    let types: Vec<_> = explicit.iter().map(|t| scope.resolve_type(*t)).collect();
    requirements::validate_substitution(&signature.needs, &types, expr.location)?;
    for need in &signature.needs {
        requirements::satisfy(need, &types, scope, expr.location)?;
    }
    let result = substitute(signature.result, &types);
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
        let ty = substitute(ty, &types);
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
        .map(|(arg, ty)| super::function_values::argument(arg, substitute(*ty, &types), scope))
        .collect::<Result<Vec<_>, _>>()?;
    let symbolic = types.iter().any(|t| matches!(t.leaf(), Type::Parameter(_)));
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

fn request(
    function: usize,
    arguments: Vec<Type>,
    scope: &Scope,
    location: Location,
) -> Result<usize, Diagnostic> {
    // 同じ具体化への再帰は再利用する。異なる具体化への再帰は、全型引数が同じか縮み、
    // 少なくとも一つが厳密に縮む場合だけ有限として扱う。
    for (previous_function, previous) in scope.instantiation_path.iter().rev() {
        if *previous_function != function || *previous == arguments {
            continue;
        }
        let contained = |mut old: Type, new: Type| {
            loop {
                if old == new {
                    return true;
                }
                let Some(inner) = old.pointee() else {
                    return false;
                };
                old = inner;
            }
        };
        if !previous
            .iter()
            .zip(&arguments)
            .all(|(&a, &b)| contained(a, b))
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
    if let Some(instance) = instances
        .named
        .iter()
        .find(|i| i.function == function && i.arguments == arguments)
    {
        return Ok(instance.id);
    }
    let id = instances.next_id;
    instances.next_id += 1;
    let mut path = scope.instantiation_path.as_ref().clone();
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
        scope.type_arguments = Some(Rc::new(instance.arguments));
        scope.instantiation_path = instance.path;
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
