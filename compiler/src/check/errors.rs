//! エラー集合とfailを検査し、呼び出しの成功値と失敗処理を結び付ける。
use super::{
    expression, functions,
    ir::{CallHandler, CheckedExpression, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{ErrorVariant, Expression, FailureHandler, Location, Type},
};

pub(super) fn fail(value: &Expression, scope: &Scope) -> Result<CheckedExpression, Diagnostic> {
    if scope.errors.is_empty() {
        return Err(value.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "err.fail",
                "fail requires an enclosing function with declared errors"
            ),
        ));
    }
    let checked = expression::coerced(value, None, scope)?;
    let invalid = || {
        value.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "err.fail",
                "failed value is not included in the function's declared errors"
            ),
        )
    };
    let ty = match checked.ty {
        Type::Enum(ty) if ty.is_error => ty,
        _ => {
            return Err(value.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "err.fail",
                    "fail requires an error value, found {}",
                    scope.type_name(checked.ty)
                ),
            ));
        }
    };
    let constant = super::constants::constant_integer(&checked)?;
    let variants = match constant {
        Some(v) => v as usize..v as usize + 1,
        None => 0..ty.count,
    };
    if !variants.into_iter().all(|variant| {
        scope.errors.contains(&ErrorVariant {
            type_id: ty.id,
            variant,
        })
    }) {
        return Err(invalid());
    }
    Ok(checked)
}

pub(super) fn finish(
    call: CheckedExpression,
    errors: &[ErrorVariant],
    handler: Option<&FailureHandler>,
    expr: &Expression,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let location = expr.location;
    if errors.is_empty() {
        if handler.is_some() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("err.must-handle", "this call cannot fail"),
            ));
        }
        return Ok(call);
    }
    let Some(handler) = handler else {
        return Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("err.must-handle", "fallible calls require an or handler"),
        ));
    };
    if let FailureHandler::Errors(handler) = handler {
        let crate::ast::ErrorHandler { arms, location } = handler.as_ref();
        return branches(call, errors, arms, *location, expr, scope);
    }
    let handler = match handler {
        FailureHandler::Errors(_) => unreachable!(),
        FailureHandler::Value(value) => {
            if call.ty == Type::Void && !matches!(value.kind, crate::ast::ExpressionKind::Panic(_))
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "err.or-fallback",
                        "void calls cannot have a fallback value"
                    ),
                ));
            }
            CallHandler::Value(expression::coerced(value, Some(call.ty), scope)?)
        }
        FailureHandler::Return { value, .. } => {
            CallHandler::Return(functions::return_value(value, scope)?)
        }
        FailureHandler::Fail {
            value: Some(value), ..
        } => CallHandler::Fail(Some(fail(value, scope)?)),
        FailureHandler::Fail { value: None, .. } => {
            if !errors.iter().all(|e| scope.errors.contains(e)) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "err.or-fail",
                        "callee errors are not included in the enclosing function's errors"
                    ),
                ));
            }
            CallHandler::Fail(None)
        }
        FailureHandler::Ignore(_) => {
            if call.ty != Type::Void {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("err.or-ignore", "ignore is only allowed for void calls"),
                ));
            }
            CallHandler::Ignore
        }
    };
    Ok(CheckedExpression {
        ty: call.ty,
        value: Value::HandledCall {
            call: Box::new(call),
            handler: Box::new(handler),
        },
    })
}

pub(super) fn invalid_handler(location: Location) -> Diagnostic {
    location.error(
        ErrorKind::InvalidSource,
        crate::diagnostic!(
            "err.must-handle",
            "fail, ignore, and error handlers require a fallible function call"
        ),
    )
}

pub(super) fn panic(
    parts: &[crate::ast::TemplatePart<Expression>],
    location: Location,
    ty: Type,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    Ok(CheckedExpression {
        ty,
        value: Value::Panic {
            parts: super::text::parts(parts, scope)?,
            location,
        },
    })
}

/// 呼び出しが返しうるバリアントだけを網羅し、腕ごとに独立したローカルスコープを検査する。
fn branches(
    call: CheckedExpression,
    errors: &[ErrorVariant],
    arms: &[crate::ast::MatchArm<crate::ast::ErrorArmBody>],
    location: Location,
    source: &Expression,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    use super::ir::{CallHandler, CheckedStatement};
    use crate::ast::{ErrorArmBody, ExpressionKind};
    let invalid = |at: Location, message: &str| {
        at.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("err.or-error", "{message}"),
        )
    };
    let mut remaining: std::collections::BTreeSet<_> = errors.iter().copied().collect();
    let result_id = scope.locals.values().map(|b| b.id + 1).max().unwrap_or(0);
    let mut next_id = result_id + 1;
    let mut checked_arms = Vec::new();
    for (index, arm) in arms.iter().enumerate() {
        let pattern = if let Some(pattern) = &arm.pattern {
            let ExpressionKind::Variant { enum_name, variant } = &pattern.kind else {
                return Err(invalid(pattern.location, "expected an error variant"));
            };
            let ty = scope
                .enums
                .iter()
                .find(|e| e.name == *enum_name && e.ty.is_error)
                .ok_or_else(|| invalid(pattern.location, "expected an error type"))?;
            let variant = ty
                .variants
                .iter()
                .position(|name| name == variant)
                .ok_or_else(|| invalid(pattern.location, "unknown error variant"))?;
            let value = ErrorVariant {
                type_id: ty.ty.id,
                variant,
            };
            if !remaining.remove(&value) {
                return Err(invalid(
                    pattern.location,
                    "error pattern is duplicated or not returned by this call",
                ));
            }
            Some(value)
        } else {
            if index + 1 != arms.len() || remaining.is_empty() {
                return Err(invalid(
                    arm.location,
                    "_ must be the final arm and cover remaining errors",
                ));
            }
            remaining.clear();
            None
        };
        let mut local = scope.clone();
        let body = match &arm.body {
            ErrorArmBody::Value(value) => {
                if call.ty == Type::Void {
                    return Err(invalid(
                        value.location,
                        "void error arms require ignore or statements",
                    ));
                }
                vec![CheckedStatement::Recover(expression::coerced(
                    value,
                    Some(call.ty),
                    &local,
                )?)]
            }
            ErrorArmBody::Statements { body, block } => {
                if *block && body.is_empty() {
                    return Err(invalid(arm.location, "error block arms must not be empty"));
                }
                if call.ty != Type::Void && !super::control::stops(body) {
                    return Err(invalid(
                        arm.location,
                        "non-void error arms must exit or recover on every path",
                    ));
                }
                local.recovery = Some(call.ty);
                super::statements::block(body, &mut local, &mut next_id, scope.loop_depth)?
            }
        };
        for (name, binding) in &scope.locals {
            if local[name].assigned.get() {
                binding.assigned.set(true);
            }
        }
        checked_arms.push((pattern, body));
    }
    if !remaining.is_empty() {
        return Err(invalid(
            location,
            "error arms do not cover every declared error",
        ));
    }
    let ty = call.ty;
    let handler = Box::new(CallHandler::Branches(checked_arms));
    if super::control::error_arms_escape(arms) {
        return Ok(CheckedExpression {
            ty,
            value: Value::HandledCall {
                call: Box::new(call),
                handler,
            },
        });
    }
    // 脱出しない腕は補助関数内で完結する。引数・条件・代替値などの式内でも順序を維持できる。
    let captures = expression::blocks::handler_captures(source, arms, scope);
    Ok(CheckedExpression {
        ty,
        value: Value::Block {
            statements: vec![CheckedStatement::HandleCall {
                id: result_id,
                call: Box::new(call),
                handler,
            }],
            value: Box::new(CheckedExpression {
                ty,
                value: if ty == Type::Void {
                    Value::Void
                } else {
                    Value::Local(result_id)
                },
            }),
            captures,
        },
    })
}
