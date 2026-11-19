//! arenaの補助構文。前置演算そのものは通常の式解析で扱う。
use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{BinaryOperator, Statement},
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
        let target = self.expression_before_assignment()?;
        let location = self.location();
        let operator = match self.token().kind {
            Kind::Symbol("=") => None,
            Kind::Symbol(symbol) if symbol.ends_with('=') => {
                let Some(operator) = symbol
                    .strip_suffix('=')
                    .and_then(BinaryOperator::from_symbol)
                else {
                    return Ok(Statement::Expression(target));
                };
                Some(operator)
            }
            _ => return Ok(Statement::Expression(target)),
        };
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
