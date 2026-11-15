use super::{expression, operand_type};
use crate::check::{
    ir::{CheckedExpression, CheckedHandler, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FailureHandler, Type},
};

pub(super) fn check(expr: &Expression, bindings: &Scope) -> Result<CheckedExpression, Diagnostic> {
    let ExpressionKind::Slice {
        target,
        start,
        end,
        index,
        fallback,
    } = &expr.kind
    else {
        unreachable!()
    };
    let target = expression(target, Some(Type::Text), bindings)?;
    let bound_type = start
        .as_deref()
        .and_then(|value| operand_type(value, bindings))
        .or_else(|| {
            end.as_deref()
                .and_then(|value| operand_type(value, bindings))
        });
    let bound = |value: &Expression| -> Result<Box<CheckedExpression>, Diagnostic> {
        let checked = expression(value, bound_type, bindings)?;
        if !matches!(checked.ty, Type::Integer(_)) {
            return Err(value.location.error(
                ErrorKind::InvalidSource,
                "text positions require integer operands [text.slice-bounds]",
            ));
        }
        Ok(Box::new(checked))
    };
    let start = start.as_deref().map(bound).transpose()?;
    let end = end.as_deref().map(bound).transpose()?;
    let fallible = *index || start.is_some() || end.is_some();
    if fallible != fallback.is_some() {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            if fallible {
                "text extraction requires or [text.slice-handler]"
            } else {
                "full text view does not accept or [text.slice-handler]"
            },
        ));
    }
    let ty = if *index { Type::Char } else { Type::Text };
    let handler =
        match fallback {
            Some(FailureHandler::Value(value)) => Some(CheckedHandler::Value(Box::new(
                expression(value, Some(ty), bindings)?,
            ))),
            Some(FailureHandler::Return { value, .. }) => Some(CheckedHandler::Return(Box::new(
                crate::check::functions::return_value(value, bindings)?,
            ))),
            None => None,
        };
    Ok(CheckedExpression {
        ty,
        value: Value::Slice {
            target: Box::new(target),
            start,
            end,
            index: *index,
            handler,
        },
    })
}
