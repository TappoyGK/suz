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

// Exact ranges and template renderings are derived metadata. Update these
// around the common child walk; the list of Value children lives only in IR.
fn exact_ranges(value: &mut Exact, index: usize) -> Option<()> {
    value.range = value.range_at(index).ok()?;
    value.domains = None;
    match &mut value.kind {
        ExactKind::Negate(value) => exact_ranges(value, index)?,
        ExactKind::Binary(_, a, b) => {
            exact_ranges(a, index)?;
            exact_ranges(b, index)?;
        }
        ExactKind::Literal(_) | ExactKind::Leaf(_) => {}
    }
    Some(())
}

fn substitute(value: &mut CheckedExpression, index: usize, scope: &Scope) -> Option<()> {
    value.ty = domain::at(value.ty, index)?;
    if let Value::Exact(exact)
    | Value::IntegerArithmetic {
        operation: IntegerOperation::Exact(exact),
        ..
    } = &mut value.value
    {
        exact_ranges(exact, index)?;
    }
    let mut result = Some(());
    let admitted = value.value.visit_constant_children_mut(&mut |child| {
        if result.is_some() {
            result = substitute(child, index, scope);
        }
    });
    if !admitted {
        return None;
    }
    result?;
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
