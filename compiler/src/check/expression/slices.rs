//! 文字列の添字・切り出し範囲と、失敗時のハンドラーを検査する。
//!
//! 利用元: [`crate::check::expression`]。
//! 主な対象規則: `[text.slice-bounds]`、`[text.slice-handler]`。

use super::expression;
use crate::check::{
    ir::{CheckedExpression, CheckedHandler, Value},
    scope::Scope,
};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FailureHandler, Type},
};

pub(crate) fn check(expr: &Expression, bindings: &Scope) -> Result<CheckedExpression, Diagnostic> {
    let ExpressionKind::Slice {
        target,
        start,
        end,
        index,
        fallback,
        ..
    } = &expr.kind
    else {
        unreachable!()
    };
    let non_indexable = |ty: Type| {
        let found = bindings.type_name(ty);
        let message = if ty
            .pointee()
            .is_some_and(|inner| inner.array_id().is_some() || inner.is_text())
        {
            crate::diagnostic!(
                "type.expected",
                "indexing requires an array or text, found {found}; to index through a pointer, write (.p).[i]"
            )
        } else {
            crate::diagnostic!(
                "type.expected",
                "indexing requires an array or text, found {found}"
            )
        };
        target.location.error(ErrorKind::InvalidSource, message)
    };
    if let Some(ty) = super::operand_type(target, bindings)
        && !ty.is_text()
    {
        return Err(non_indexable(ty));
    }
    let target = expression(target, Some(Type::Text), bindings)?;
    let bound = |value: &Expression| -> Result<Box<CheckedExpression>, Diagnostic> {
        let checked = expression(value, None, bindings)?;
        if !checked.ty.is_integer() {
            return Err(value.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "text.slice-bounds",
                    "text positions require integer operands"
                ),
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
                crate::diagnostic!("text.slice-handler", "text extraction requires or")
            } else {
                crate::diagnostic!("text.slice-handler", "full text view does not accept or")
            },
        ));
    }
    let ty = if *index { Type::Char } else { Type::Text };
    let handler =
        match fallback {
            Some(FailureHandler::Ignore(ignore)) => {
                return Err(crate::check::errors::invalid_handler(ignore.location));
            }
            Some(FailureHandler::Errors(handler)) => {
                return Err(crate::check::errors::invalid_handler(handler.location));
            }
            Some(FailureHandler::Fail { location, .. }) => {
                return Err(crate::check::errors::invalid_handler(*location));
            }
            Some(FailureHandler::Value(value)) => Some(CheckedHandler::Value(Box::new(
                expression(value, Some(ty), bindings)?,
            ))),
            Some(FailureHandler::Return { value, location }) => Some(CheckedHandler::Return {
                value: Box::new(crate::check::functions::return_value(value, bindings)?),
                location: *location,
            }),
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
