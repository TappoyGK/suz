//! arenaの補助構文。前置演算そのものは通常の式解析で扱う。
use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{BinaryOperator, Expression, ExpressionKind, Statement},
    lexer::Kind,
};

impl Parser<'_, '_> {
    pub(super) fn let_statement(&mut self) -> Result<Statement, Diagnostic> {
        self.cursor += 1;
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("src.ident", "expected a let binding"),
            ));
        };
        let name = name.to_owned();
        super::names::check(super::names::Shape::Value, &name, location)?;
        self.cursor += 1;
        self.expect(FixedToken::Symbol("="))?;
        let value = self.condition()?;
        let (body, _) = self.block()?;
        let saved = self.cursor;
        self.newlines();
        let otherwise = if self.token().kind == Kind::Keyword("else") {
            self.cursor += 1;
            self.block()?.0
        } else {
            self.cursor = saved;
            Vec::new()
        };
        Ok(Statement::Let {
            name,
            location,
            value: Box::new(value),
            body,
            otherwise,
        })
    }

    pub(super) fn indirect_statement(&mut self) -> Result<Statement, Diagnostic> {
        // 前置の#は式の演算子にせず、代入文の先頭でだけ受け付ける。
        // 型検査以降は従来の参照解除の代入先として扱う。
        let write_deref = self.token().kind == Kind::Symbol("#");
        let target = if write_deref {
            let location = self.location();
            self.cursor += 1;
            let pointer = self.unary()?;
            self.node(ExpressionKind::Deref(Box::new(pointer)), location)?
        } else {
            self.expression_before_assignment()?
        };
        self.finish_indirect_statement(target, write_deref)
    }

    pub(super) fn finish_indirect_statement(
        &mut self,
        mut target: Expression,
        mut write_target: bool,
    ) -> Result<Statement, Diagnostic> {
        // 関数の返したポインタや括弧付きの対象にも、最後の#fieldだけを認める。
        if !write_target && self.token().kind == Kind::Symbol("#") {
            let location = self.location();
            self.cursor += 1;
            if self.token().kind == Kind::Symbol("[") {
                let location = self.location();
                self.cursor += 1;
                let index = self.expression()?;
                self.expect(FixedToken::Symbol("]"))?;
                target = self.node(
                    ExpressionKind::Slice {
                        target: Box::new(target),
                        start: Some(Box::new(index)),
                        end: None,
                        index: true,
                        fallback: None,
                    },
                    location,
                )?;
                write_target = true;
            } else {
                let Kind::Ident(name) = self.token().kind else {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("assign.target", "expected a field name after #"),
                    ));
                };
                self.cursor += 1;
                target = self.node(
                    ExpressionKind::Field {
                        target: Box::new(target),
                        name: name.into(),
                    },
                    location,
                )?;
                write_target = true;
            }
        }
        let location = self.location();
        let operator = match self.token().kind {
            Kind::Symbol("=") => Some(None),
            Kind::Symbol(symbol) => symbol
                .strip_suffix('=')
                .and_then(BinaryOperator::from_symbol)
                .map(Some),
            _ => None,
        };
        let Some(operator) = operator else {
            return if write_target {
                Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "arena.ptr-operations",
                        "# requires an assignment or compound assignment"
                    ),
                ))
            } else {
                Ok(Statement::Expression(target))
            };
        };
        let mut ungrouped = &target;
        while let ExpressionKind::Group(inner) = &ungrouped.kind {
            ungrouped = inner;
        }
        if !write_target
            && matches!(
                ungrouped.kind,
                ExpressionKind::Deref(_) | ExpressionKind::Field { .. }
            )
        {
            return Err(target.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "arena.ptr-operations",
                    "use #p or target#field to specify an assignment target"
                ),
            ));
        }
        self.cursor += 1;
        let value = self.expression()?;
        Ok(Statement::Store {
            target: Box::new(target),
            value: Box::new(value),
            operator,
            location,
        })
    }
}
