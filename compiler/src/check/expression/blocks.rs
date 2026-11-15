use super::{
    super::ir::{CheckedExpression, Value},
    super::statements,
};
use crate::check::scope::Scope;
use crate::{
    Diagnostic,
    ast::{
        Expression, ExpressionKind, Statement, Type,
        visit::{self, Visitor},
    },
};
use std::collections::BTreeSet;

// Block-local IDs need only be distinct from captured IDs: each value block
// becomes its own C function, with a separate local namespace.
fn prefix(
    input: &[Statement],
    bindings: &mut Scope,
) -> Result<(Vec<super::super::ir::CheckedStatement>, Vec<String>), Diagnostic> {
    let mut next_id = bindings
        .values()
        .filter(|b| b.constant.is_none())
        .map(|b| b.id + 1)
        .max()
        .unwrap_or(0);
    let mut declared = Vec::new();
    let checked =
        statements::check_statements(input, bindings, &mut next_id, &mut false, 0, &mut declared)?;
    Ok((checked, declared))
}

pub(super) fn operand_type(
    input: &[Statement],
    value: &Expression,
    bindings: &Scope,
) -> Option<Type> {
    // Type discovery must not count speculative assignments or diagnostics.
    let mut local = bindings.speculative();
    prefix(input, &mut local).ok()?;
    super::operand_type(value, &local)
}

pub(super) fn check(
    input: &[Statement],
    value: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let mut references = References(BTreeSet::new());
    references.scope(input, Some(value))?;
    let captures = bindings
        .iter()
        .filter(|(name, b)| b.constant.is_none() && references.0.contains(name.as_str()))
        .map(|(_, b)| (b.id, b.ty, b.mutable))
        .collect();
    let mut local = bindings.clone();
    let (statements, declared) = prefix(input, &mut local)?;
    let checked_value = super::coerced(value, expected, &local)?;
    if checked_value.ty == Type::Void {
        return Err(value.location.error(
            crate::ErrorKind::InvalidSource,
            "value block must end in a non-void expression [block.value]",
        ));
    }
    let value = checked_value;
    statements::finish_scope(&mut local, declared)?;
    for (name, binding) in bindings.iter() {
        if local[name].assigned.get() {
            binding.assigned.set(true);
        }
    }
    Ok(CheckedExpression {
        ty: value.ty,
        value: Value::Block {
            statements,
            value: Box::new(value),
            captures,
        },
    })
}

// Shadowing is forbidden, so references to an outer name always identify the
// same binding, including references inside nested value blocks.
struct References(BTreeSet<String>);
impl Visitor for References {
    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        if let ExpressionKind::Name(name) = &expr.kind {
            self.0.insert(name.clone());
        }
        visit::walk_expression(self, expr)
    }
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        if let Statement::Assign { name, .. } = statement {
            self.0.insert(name.clone());
        }
        visit::walk_statement(self, statement)
    }
}
