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
    let mut types = GenericTypes::default();
    if signature.is_generic() {
        let Some(Type::Function(id)) = expected else {
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
) -> Result<CheckedExpression, Diagnostic> {
    let target = expression::coerced(target, None, scope)?;
    let Type::Function(id) = target.ty else {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("fn.value-call", "call target must be a function value"),
        ));
    };
    let signature = scope.function_types.borrow()[id].clone();
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
        .map(|(a, ty)| expression::coerced(a, Some(ty), scope))
        .collect::<Result<_, _>>()?;
    Ok(CheckedExpression {
        ty: signature.result,
        value: Value::Invoke {
            target: Box::new(target),
            arguments,
        },
    })
}

/// 外側の型要求がない分岐から、兄弟の型を使って汎用関数を具体化しない。
/// 型注釈のある文は独自の要求を持つため、ここでは分岐の結果だけをたどる。
pub(super) fn require_context(expr: &Expression, scope: &Scope) -> Result<(), Diagnostic> {
    match &expr.kind {
        ExpressionKind::FunctionValue(name)
            if scope.functions.get(name).is_some_and(|s| s.is_generic()) =>
        {
            value(expr, name, None, scope)?;
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
