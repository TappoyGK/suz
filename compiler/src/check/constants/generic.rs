//! Substitute only an already checked constant value, without revisiting source
//! expressions or declarations. Constant rules can then use the ordinary evaluator.
use super::*;
use crate::check::{
    exact::{Exact, ExactKind},
    generic::domain,
    scope::Scope,
};

pub(in crate::check) fn at(
    value: &CheckedExpression,
    index: usize,
    scope: &Scope,
) -> Result<Option<CheckedExpression>, Diagnostic> {
    if !is_constant(value) {
        return Ok(None);
    }
    let mut value = value.clone();
    if substitute(&mut value, index, scope).is_none() {
        return Ok(None);
    }
    evaluate(&value)
}

fn exact(value: &mut Exact, index: usize, scope: &Scope) -> Option<()> {
    value.range = value.range_at(index).ok()?;
    value.domains = None;
    match &mut value.kind {
        ExactKind::Literal(_) => {}
        ExactKind::Leaf(value) => substitute(value, index, scope)?,
        ExactKind::Negate(value) => exact(value, index, scope)?,
        ExactKind::Binary(_, a, b) => {
            exact(a, index, scope)?;
            exact(b, index, scope)?;
        }
    }
    Some(())
}
fn handler(value: &mut Option<CheckedHandler>, index: usize, scope: &Scope) -> Option<()> {
    if let Some(CheckedHandler::Value(value)) = value {
        substitute(value, index, scope)?;
    }
    Some(())
}
fn substitute(value: &mut CheckedExpression, index: usize, scope: &Scope) -> Option<()> {
    value.ty = domain::at(value.ty, index)?;
    match &mut value.value {
        Value::GenericFloatLiteral(text) => {
            let Type::Float(ty) = value.ty else {
                return None;
            };
            value.value = Value::Float(crate::check::expression::float_bits(text, ty)?);
        }
        Value::Exact(e) => exact(e, index, scope)?,
        Value::Cast(e)
        | Value::Negate(e)
        | Value::Truncate(e)
        | Value::BitNot(e)
        | Value::Not(e)
        | Value::TextView(e)
        | Value::Field { target: e, .. }
        | Value::Intern { operand: e, .. } => substitute(e, index, scope)?,
        Value::CheckedCast {
            operand,
            fallback,
            constant_struct,
            ..
        } => {
            substitute(operand, index, scope)?;
            substitute(fallback, index, scope)?;
            if let Some(value) = constant_struct {
                substitute(value, index, scope)?;
            }
        }
        Value::Binary { left, right, .. } | Value::Logical { left, right, .. } => {
            substitute(left, index, scope)?;
            substitute(right, index, scope)?;
        }
        Value::Comparison { operands, .. } => {
            for e in operands {
                substitute(e, index, scope)?;
            }
        }
        Value::Struct { fields } => {
            for (_, e) in fields {
                substitute(e, index, scope)?;
            }
        }
        Value::Template {
            parts, constant, ..
        } => {
            for part in parts.iter_mut() {
                if let crate::ast::TemplatePart::Value(e) = part {
                    substitute(e, index, scope)?;
                }
            }
            *constant = crate::check::text::constant_parts(parts, scope);
        }
        Value::Slice {
            target,
            start,
            end,
            handler: h,
            ..
        } => {
            substitute(target, index, scope)?;
            for e in [start, end].into_iter().flatten() {
                substitute(e, index, scope)?;
            }
            handler(h, index, scope)?;
        }
        Value::IntegerArithmetic {
            operation,
            handler: h,
            ..
        } => {
            match operation {
                IntegerOperation::Exact(e) => exact(e, index, scope)?,
                IntegerOperation::Binary { left, right, .. } => {
                    substitute(left, index, scope)?;
                    substitute(right, index, scope)?;
                }
                IntegerOperation::Negate(e) => substitute(e, index, scope)?,
                IntegerOperation::NegatedLiteral(_) => {}
            }
            handler(h, index, scope)?;
        }
        Value::Integer { .. } | Value::Float(_) | Value::Bool(_) | Value::Text { .. } => {}
        Value::Void
        | Value::Local(_)
        | Value::Call { .. }
        | Value::Block { .. }
        | Value::Match { .. }
        | Value::If { .. }
        | Value::CastReturn { .. } => return None,
    }
    Some(())
}
