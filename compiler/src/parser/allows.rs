use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        Expression, ExpressionKind, Location, Statement,
        visit::{self, Visitor},
    },
    lexer::Kind,
    warnings::{AllowItem, AllowScope},
};

struct ContainsBlock(bool);
impl Visitor for ContainsBlock {
    fn scope(&mut self, _: &[Statement], _: Option<&Expression>) -> Result<(), Diagnostic> {
        self.0 = true;
        Ok(())
    }
    fn expression(&mut self, expression: &Expression) -> Result<(), Diagnostic> {
        if matches!(
            expression.kind,
            ExpressionKind::If { .. } | ExpressionKind::Match { .. } | ExpressionKind::Block { .. }
        ) {
            self.0 = true;
        }
        visit::walk_expression(self, expression)
    }
}
pub(super) fn statement_contains_block(statement: &Statement) -> bool {
    let mut visitor = ContainsBlock(false);
    visitor.statement(statement).unwrap();
    visitor.0
}
pub(super) fn expression_contains_block(expression: &Expression) -> bool {
    let mut visitor = ContainsBlock(false);
    visitor.expression(expression).unwrap();
    visitor.0
}

impl Parser<'_, '_> {
    fn allow_items(&mut self) -> Result<Vec<AllowItem>, Diagnostic> {
        self.expect(FixedToken::Keyword("allow"))?;
        let mut items: Vec<AllowItem> = Vec::new();
        loop {
            let location = self.location();
            let Kind::AllowItem(raw) = self.token().kind else {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "expected [rule: reason] [allow.item]",
                ));
            };
            let Some((rule, reason)) = raw.split_once(':') else {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    "allow requires ':' before its reason [allow.item]",
                ));
            };
            if rule.split('.').any(|word| {
                word.is_empty()
                    || !word
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            }) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    "invalid allow rule name [allow.item]",
                ));
            }
            if !matches!(
                rule,
                "diagnostic.arithmetic"
                    | "diagnostic.constant-comparison"
                    | "diagnostic.integer-division-to-float"
            ) || items.iter().any(|item| item.rule == rule)
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    "unknown, non-warning or repeated allow rule [allow.rule-check]",
                ));
            }
            let reason = reason.trim_matches(' ');
            if reason.is_empty() {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    "allow requires a nonempty reason [allow.reason]",
                ));
            }
            items.push(AllowItem {
                rule: rule.into(),
                reason: reason.into(),
                location,
            });
            self.cursor += 1;
            if self.token().kind != Kind::Symbol(",") {
                break;
            }
            self.cursor += 1;
        }
        Ok(items)
    }

    pub(super) fn allow_clause(
        &mut self,
        start: Location,
        contains_block: bool,
    ) -> Result<(), Diagnostic> {
        if self.token().kind != Kind::Keyword("allow") {
            return Ok(());
        }
        if contains_block {
            return Err(self.error(ErrorKind::InvalidSource, "a statement containing a block requires an enclosing allow block [allow.contains-block]"));
        }
        let end = self.location();
        let items = self.allow_items()?;
        self.allows.push(AllowScope { start, end, items });
        Ok(())
    }

    pub(super) fn allow_block(&mut self) -> Result<Statement, Diagnostic> {
        let items = self.allow_items()?;
        let start = self.location();
        let (body, end) = self.block()?;
        self.allows.push(AllowScope { start, end, items });
        Ok(Statement::Block(body))
    }
}
