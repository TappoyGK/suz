//! 文字列の添字・切り出し範囲と、失敗時のハンドラーを検査する。
//!
//! 利用元: [`crate::check::expression`]。
//! 主な対象規則: `[text.slice-bounds]`、`[text.slice-handler]`。

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
    // 型の決まった両端は独立に扱う。未確定のリテラルは従来どおり他方から
    // 型を受け取り、u64の大きな位置など既存の有効な式を狭めない。
    let literal_hint = start
        .as_deref()
        .and_then(|v| operand_type(v, bindings))
        .or_else(|| end.as_deref().and_then(|v| operand_type(v, bindings)))
        .filter(|ty| ty.is_integer());
    let bound = |value: &Expression| -> Result<Box<CheckedExpression>, Diagnostic> {
        let checked = expression(
            value,
            operand_type(value, bindings).or(literal_hint),
            bindings,
        )?;
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
