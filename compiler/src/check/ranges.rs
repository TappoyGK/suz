use super::{
    expression::{expression, operand_type},
    ir::CheckedExpression,
};
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
};

struct References(std::collections::BTreeSet<String>);

impl crate::ast::visit::Visitor for References {
    fn expression(&mut self, expr: &Expression) -> Result<(), Diagnostic> {
        if let ExpressionKind::Name(name) = &expr.kind
            && self.0.contains(name)
        {
            return Err(expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "for.bound-reference",
                    "loop inputs cannot reference var bindings"
                ),
            ));
        }
        crate::ast::visit::walk_expression(self, expr)
    }
    fn statement(&mut self, statement: &crate::ast::Statement) -> Result<(), Diagnostic> {
        crate::ast::visit::walk_statement(self, statement)?;
        if let crate::ast::Statement::Binding { name, mutable, .. } = statement {
            if *mutable {
                self.0.insert(name.clone());
            } else {
                self.0.remove(name);
            }
        }
        Ok(())
    }
    fn scope(
        &mut self,
        statements: &[crate::ast::Statement],
        value: Option<&Expression>,
    ) -> Result<(), Diagnostic> {
        let saved = self.0.clone();
        for statement in statements {
            self.statement(statement)?;
        }
        if let Some(value) = value {
            self.expression(value)?;
        }
        self.0 = saved;
        Ok(())
    }
}

pub(super) fn check_references(expr: &Expression, bindings: &Scope) -> Result<(), Diagnostic> {
    use crate::ast::visit::Visitor;
    References(
        bindings
            .locals
            .iter()
            .filter(|(_, b)| b.mutable)
            .map(|(name, _)| name.clone())
            .collect(),
    )
    .expression(expr)
}

pub(super) fn bounds(
    start: &Expression,
    end: &Expression,
    bindings: &Scope,
) -> Result<(CheckedExpression, CheckedExpression), Diagnostic> {
    check_references(start, bindings)?;
    check_references(end, bindings)?;
    let expected = operand_type(start, bindings).or_else(|| operand_type(end, bindings));
    let first = expression(start, expected, bindings)?;
    let last = expression(end, Some(first.ty), bindings)?;
    if !(first.ty.is_integer() || first.ty == Type::Char)
        || !super::generic::domain::same(first.ty, last.ty)
    {
        return Err(start.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "for.bounds",
                "range bounds must have the same integer type or both be char"
            ),
        ));
    }
    Ok((first, last))
}
