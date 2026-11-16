//! Substitute only an already checked constant value, without revisiting source
//! expressions or declarations. Constant rules can then use the ordinary evaluator.
use super::*;
use crate::check::{generic::domain, scope::Scope};

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

// Exact ranges and template renderings are derived metadata. Update these
// around the common child walk; the list of Value children lives only in IR.
fn substitute(value: &mut CheckedExpression, index: usize, scope: &Scope) -> Option<()> {
    value.ty = domain::at(value.ty, index)?;
    let complete = std::cell::Cell::new(true);
    let admitted = value.value.visit_constant_parts_mut(
        &mut |child| {
            if complete.get() {
                complete.set(substitute(child, index, scope).is_some());
            }
        },
        &mut |exact| {
            if complete.get() {
                if let Ok(range) = exact.range_at(index) {
                    exact.range = range;
                    exact.domains = None;
                } else {
                    complete.set(false);
                }
            }
        },
    );
    if !admitted || !complete.get() {
        return None;
    }
    match &mut value.value {
        Value::GenericFloatLiteral(text) => {
            let Type::Float(ty) = value.ty else {
                return None;
            };
            value.value = Value::Float(crate::check::expression::float_bits(text, ty)?);
        }
        Value::Template {
            parts, constant, ..
        } => {
            *constant = crate::check::text::constant_parts(parts, scope);
        }
        _ => {}
    }
    Some(())
}
