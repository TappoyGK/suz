//! if・match・値ブロックの構文を解析し、分岐と末尾の値をASTへまとめる。
//!
//! 利用元: [`crate::parser::expressions`]、[`crate::parser::statements`]。
//! 主な対象規則: `[block.value]`、`[expr.else-line]`、`[expr.if-value]`、`[expr.jump-prohibited]`、`[if.elif]`、`[match.separator]`。

use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Location, MatchArm, Statement},
    lexer::Kind,
};

impl Parser<'_, '_> {
    pub(super) fn value_block(&mut self) -> Result<(Expression, Location), Diagnostic> {
        let location = self.location();
        let saved = self.in_value;
        self.in_value = true;
        let (mut statements, end) = self.block()?;
        self.in_value = saved;
        let Some(Statement::Expression(value)) = statements.pop() else {
            return Err(end.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("block.value", "a value block requires a final expression"),
            ));
        };
        Ok((
            Expression {
                height: super::nesting::block_height(&statements).max(1 + value.height),
                kind: ExpressionKind::Block {
                    statements,
                    value: Box::new(value),
                },
                location,
            },
            end,
        ))
    }

    pub(super) fn if_expression(&mut self) -> Result<ExpressionKind, Diagnostic> {
        self.cursor += 1;
        let condition = self.condition()?;
        let (value, mut end) = self.value_block()?;
        let mut branches = vec![(condition, value)];
        loop {
            self.check_branch_line(end)?;
            match self.token().kind {
                Kind::Keyword("elif") => {
                    self.cursor += 1;
                    let condition = self.condition()?;
                    let (value, location) = self.value_block()?;
                    end = location;
                    branches.push((condition, value));
                }
                Kind::Keyword("else") => {
                    self.cursor += 1;
                    let (otherwise, _) = self.value_block()?;
                    return Ok(ExpressionKind::If {
                        branches,
                        otherwise: Box::new(otherwise),
                    });
                }
                _ => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("expr.if-value", "ifexpr requires an else branch"),
                    ));
                }
            }
        }
    }

    fn check_branch_line(&self, end: Location) -> Result<(), Diagnostic> {
        let next = self.tokens[self.cursor..]
            .iter()
            .find(|t| !t.kind.is_newline())
            .unwrap();
        if matches!(next.kind, Kind::Keyword("elif" | "else")) && next.line != end.line {
            return Err(Location {
                line: next.line,
                column: next.column,
            }
            .error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "expr.else-line",
                    "elif and else must follow '}}' on the same line"
                ),
            ));
        }
        Ok(())
    }

    pub(super) fn if_statement(&mut self) -> Result<Statement, Diagnostic> {
        self.cursor += 1;
        let condition = self.condition()?;
        let (body, mut end) = self.block()?;
        let mut branches = vec![(condition, body)];
        loop {
            self.check_branch_line(end)?;
            match self.token().kind {
                Kind::Keyword("elif") => {
                    self.cursor += 1;
                    let condition = self.condition()?;
                    let (body, location) = self.block()?;
                    end = location;
                    branches.push((condition, body));
                }
                Kind::Keyword("else") => {
                    self.cursor += 1;
                    if self.token().kind == Kind::Keyword("if") {
                        return Err(self.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("if.elif", "use elif for another condition"),
                        ));
                    }
                    let (otherwise, _) = self.block()?;
                    return Ok(Statement::If {
                        branches,
                        otherwise: Some(otherwise),
                    });
                }
                _ => {
                    return Ok(Statement::If {
                        branches,
                        otherwise: None,
                    });
                }
            }
        }
    }

    pub(super) fn match_arms<T>(
        &mut self,
        parse_body: fn(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<Vec<MatchArm<T>>, Diagnostic> {
        self.expect(FixedToken::Symbol("{"))?;
        self.newlines();
        let mut arms = Vec::new();
        while self.token().kind != Kind::Symbol("}") {
            let location = self.location();
            let pattern = if self.token().kind == Kind::Wildcard {
                self.cursor += 1;
                None
            } else {
                self.constant_expression = true;
                let pattern = self.expression()?;
                self.constant_expression = false;
                Some(pattern)
            };
            self.expect(FixedToken::Symbol("=>"))?;
            let body = parse_body(self)?;
            arms.push(MatchArm {
                pattern,
                location,
                body,
            });
            match self.token().kind {
                Kind::Newline | Kind::BlankLine => self.newlines(),
                Kind::Symbol("}") => {}
                _ => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "match.separator",
                            "match arms must be separated by a newline"
                        ),
                    ));
                }
            }
        }
        self.cursor += 1;
        Ok(arms)
    }

    pub(super) fn match_statement_body(&mut self) -> Result<Vec<Statement>, Diagnostic> {
        if self.token().kind == Kind::Symbol("{") {
            Ok(self.block()?.0)
        } else {
            Ok(vec![Statement::Expression(
                self.nested(self.location(), Self::expression)?,
            )])
        }
    }

    pub(super) fn match_value_body(&mut self) -> Result<Expression, Diagnostic> {
        if self.token().kind == Kind::Symbol("{") {
            Ok(self.value_block()?.0)
        } else {
            if matches!(
                self.token().kind,
                Kind::Keyword("return" | "break" | "continue")
            ) {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.jump-prohibited",
                        "control transfers are forbidden inside value expressions"
                    ),
                ));
            }
            let mut value = self.nested(self.location(), Self::expression)?;
            value.height += 1;
            Ok(value)
        }
    }
}
