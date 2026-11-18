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
    location: Location,
    scope: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
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
    let handler = match handler {
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
            "fail and ignore handlers require a fallible function call"
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
