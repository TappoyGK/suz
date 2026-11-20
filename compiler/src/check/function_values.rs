//! 関数名を具体型の関数値へ解決し、間接呼び出しの引数を検査する。
//! 汎用関数の具体化は直接呼び出しと同じ登録先を使い、本体の保留した判定も行う。

use super::{
    expression,
    generic::GenericTypes,
    ir::{CheckedExpression, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FunctionType, Type},
};

pub(super) fn value(
    expr: &Expression,
    name: &str,
    explicit: &[Type],
    expected: Option<Type>,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    if scope.contains_key(name) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.value-mark",
                "do not prefix a binding with fn; use {name} directly"
            ),
        ));
    }
    let error = || {
        expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.value-from-name",
                "fn requires a top-level function other than main"
            ),
        )
    };
    let signature = scope
        .functions
        .get(name)
        .filter(|_| name != "main")
        .ok_or_else(error)?;
    if signature.arena {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "arena.fn-value",
                "arena fn cannot be used as a function value"
            ),
        ));
    }
    if signature.external {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "ffi.function-value",
                "C functions cannot be used as Suz function values"
            ),
        ));
    }
    if signature.type_parameters != 0 || !explicit.is_empty() {
        return super::type_parameters::function_value(expr, signature, explicit, expected, scope);
    }
    let mut types = GenericTypes::default();
    if signature.is_generic() {
        let Some(Type::StaticFunction(id)) = expected else {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "fn.value-generic",
                    "specify a concrete function value type to specialize this generic function"
                ),
            ));
        };
        let required = scope.function_types.borrow()[id].clone();
        let mismatch = || {
            expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "fn.value-generic",
                    "generic function signature does not match the required function value type"
                ),
            )
        };
        if signature.parameters.len() != required.parameters.len() {
            return Err(mismatch());
        }
        for (&from, &to) in signature
            .parameters
            .iter()
            .chain([&signature.result])
            .zip(required.parameters.iter().chain([&required.result]))
        {
            let (mut from, mut to) = (from, to);
            while let (Some(a), Some(b)) = (from.pointee(), to.pointee()) {
                from = a;
                to = b;
            }
            match (from, to) {
                (Type::GenericInt, Type::Integer(ty)) => {
                    if types.int.is_some_and(|old| old != ty) {
                        return Err(mismatch());
                    }
                    types.int = Some(ty);
                }
                (Type::GenericFloat, Type::Float(ty)) => {
                    if types.float.is_some_and(|old| old != ty) {
                        return Err(mismatch());
                    }
                    types.float = Some(ty);
                }
                _ if from == to => {}
                _ => return Err(mismatch()),
            }
        }
    }
    let ty = scope.intern_function(FunctionType {
        parameters: signature
            .parameters
            .iter()
            .map(|&ty| types.resolve(ty))
            .collect(),
        result: types.resolve(signature.result),
        errors: signature.errors.clone(),
    });
    if expected.is_some_and(|expected| expected != ty) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.value-from-name",
                "function signature does not match the required function value type"
            ),
        ));
    }
    let id = if signature.is_generic() {
        if scope.generic_check() {
            scope.defer_call(
                signature.id,
                types.int.map(Type::Integer),
                types.float.map(Type::Float),
                expr.location,
                true,
            );
            signature.id
        } else {
            scope.instances.borrow_mut().request(
                signature.id,
                types,
                &scope.generic_calls,
                expr.location,
                true,
            )
        }
    } else {
        signature.id
    };
    Ok(CheckedExpression {
        ty,
        value: Value::Function(id),
    })
}

pub(super) fn invoke(
    expr: &Expression,
    target: &Expression,
    arguments: &[Expression],
    scope: &Scope,
    handler: Option<&crate::ast::FailureHandler>,
) -> Result<CheckedExpression, Diagnostic> {
    let target = call_target(target, scope)?;
    let (Type::StaticFunction(id) | Type::Function(id)) = target.ty else {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("fn.value-call", "call target must be a function value"),
        ));
    };
    let signature = scope.function_types.borrow()[id].clone();
    if super::arena::contains(signature.result, scope) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "arena.parameter",
                "ordinary function values cannot return arena pointers"
            ),
        ));
    }
    if arguments.len() != signature.parameters.len() {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fn.call",
                "argument count does not match the function value type"
            ),
        ));
    }
    let arguments = arguments
        .iter()
        .zip(signature.parameters)
        .map(|(a, ty)| argument(a, ty, scope))
        .collect::<Result<_, _>>()?;
    super::errors::finish(
        CheckedExpression {
            ty: signature.result,
            value: Value::Invoke {
                location: expr.location,
                target: Box::new(target),
                arguments,
            },
        },
        &signature.errors,
        handler,
        expr,
        scope,
    )
}

/// 外側の型要求がない分岐から、兄弟の型を使って汎用関数を具体化しない。
/// 型注釈のある文は独自の要求を持つため、ここでは分岐の結果だけをたどる。
pub(super) fn require_context(expr: &Expression, scope: &Scope) -> Result<(), Diagnostic> {
    match &expr.kind {
        ExpressionKind::FunctionValue {
            name,
            type_arguments,
        } if scope.functions.get(name).is_some_and(|s| s.is_generic()) => {
            value(expr, name, type_arguments, None, scope)?;
        }
        ExpressionKind::Group(v) | ExpressionKind::Block { value: v, .. } => {
            require_context(v, scope)?
        }
        ExpressionKind::If {
            branches,
            otherwise,
        } => {
            for (_, v) in branches {
                require_context(v, scope)?;
            }
            require_context(otherwise, scope)?;
        }
        ExpressionKind::Match { arms, .. } => {
            for arm in arms {
                require_context(&arm.body, scope)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// 呼び出し専用の仮引数は、名前をそのまま渡したときだけ転送を認める。
/// 保存可能な関数値は評価時の値を環境へ写し、後続の引数の副作用から切り離す。
pub(super) fn argument(
    expr: &Expression,
    required: Type,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let Type::Function(id) = required else {
        return expression::coerced(expr, Some(required), scope);
    };
    if let ExpressionKind::Closure(closure) = &expr.kind {
        return super::closures::check(closure, id, scope);
    }
    if let ExpressionKind::Name(name) = &expr.kind
        && let Some(binding) = scope.get(name)
        && matches!(binding.ty, Type::Function(_))
    {
        if binding.ty != required {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "type.expected",
                    "expected {}, found {}",
                    scope.type_name(required),
                    scope.type_name(binding.ty)
                ),
            ));
        }
        return Ok(CheckedExpression {
            ty: required,
            value: Value::Local(binding.id),
        });
    }
    let value = expression::coerced(expr, Some(Type::StaticFunction(id)), scope)?;
    Ok(CheckedExpression {
        ty: required,
        value: Value::FunctionBorrow(Box::new(value)),
    })
}

/// 呼び出す位置だけで呼び出し専用の名前を読み、一般の式としては取り出さない。
fn call_target(expr: &Expression, scope: &Scope) -> Result<CheckedExpression, Diagnostic> {
    match &expr.kind {
        ExpressionKind::Name(name)
            if scope
                .get(name)
                .is_some_and(|b| matches!(b.ty, Type::Function(_))) =>
        {
            let binding = scope.get(name).unwrap();
            Ok(CheckedExpression {
                ty: binding.ty,
                value: Value::Local(binding.id),
            })
        }
        ExpressionKind::Group(inner) => call_target(inner, scope),
        _ => expression::coerced(expr, None, scope),
    }
}
