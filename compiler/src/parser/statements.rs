//! 束縛・代入・反復・returnなどの文を解析し、改行と制御構文の位置を検査する。
//!
//! 利用元: [`crate::parser::functions`]、[`crate::parser::branches`]。
//! 主な対象規則: `[decl.location]`、`[expr.jump-prohibited]`、`[for.range]`、`[if.elif]`、`[loop.break]`、`[loop.continue]`。

use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{BinaryOperator, Expression, ExpressionKind, Location, Statement, Type},
    lexer::Kind,
};

impl Parser<'_, '_> {
    // 文頭のfnは、宣言なら従来の位置のエラー、値なら式の規則で診断する。
    // fn pick(1)を宣言と取り違えないよう、仮引数の型・戻り値・本体を確認する。
    fn starts_function_declaration(&self) -> bool {
        let tail = &self.tokens[self.cursor + 1..];
        if tail.first().map(|t| t.kind) == Some(Kind::Symbol("[")) {
            return false;
        }
        if !matches!(tail.first().map(|t| t.kind), Some(Kind::Ident(_))) {
            return true;
        }
        if tail.get(1).map(|t| t.kind) != Some(Kind::Symbol("(")) {
            // 仮引数の括弧を欠く宣言も、関数値の式とは区別する。
            return matches!(tail.get(1).map(|t| t.kind), Some(Kind::Symbol("->" | "{")));
        }
        if matches!(tail.get(2).map(|t| t.kind), Some(Kind::Ident(_)))
            && tail.get(3).map(|t| t.kind) == Some(Kind::Symbol(":"))
        {
            return true;
        }
        let mut depth = 0usize;
        for (i, token) in tail.iter().enumerate().skip(1) {
            match token.kind {
                Kind::Symbol("(") => depth += 1,
                Kind::Symbol(")") => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(
                            tail.get(i + 1).map(|t| t.kind),
                            Some(Kind::Symbol("->" | "{"))
                        );
                    }
                }
                Kind::End => break,
                _ => {}
            }
        }
        false
    }

    fn assignment_operator(&self) -> Option<Option<BinaryOperator>> {
        let mut cursor = self.cursor + 1;
        while self
            .tokens
            .get(cursor)
            .is_some_and(|t| t.kind == Kind::Symbol("."))
        {
            if !self
                .tokens
                .get(cursor + 1)
                .is_some_and(|t| matches!(t.kind, Kind::Ident(_)))
            {
                return None;
            }
            cursor += 2;
        }
        match self.tokens.get(cursor)?.kind {
            Kind::Symbol("=") => Some(None),
            Kind::Symbol(symbol) => symbol
                .strip_suffix('=')
                .and_then(BinaryOperator::from_symbol)
                .map(Some),
            _ => None,
        }
    }

    fn statement(&mut self) -> Result<Statement, Diagnostic> {
        let start = self.location();
        let statement = if self.token().kind == Kind::Keyword("fail") {
            self.fail_statement()?
        } else {
            self.statement_inner()?
        };
        self.allow_clause(start, super::allows::statement_contains_block(&statement))?;
        Ok(statement)
    }

    fn fail_statement(&mut self) -> Result<Statement, Diagnostic> {
        if self.in_value {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "expr.jump-prohibited",
                    "fail is forbidden inside value expressions"
                ),
            ));
        }
        self.cursor += 1;
        Ok(Statement::Fail(self.expression()?))
    }

    fn statement_inner(&mut self) -> Result<Statement, Diagnostic> {
        if self.in_value
            && matches!(
                self.token().kind,
                Kind::Keyword("return" | "break" | "continue")
            )
        {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "expr.jump-prohibited",
                    "control transfers are forbidden inside value expressions"
                ),
            ));
        }
        match self.token().kind {
            Kind::Keyword("allow") => self.allow_block(),
            Kind::Keyword("or") => Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "src.or-line",
                    "or cannot start a statement or continue the preceding line"
                ),
            )),
            Kind::Keyword("log") => self.log(),
            Kind::Keyword("panic") => self.panic(),
            Kind::Keyword("match") => {
                let location = self.location();
                self.cursor += 1;
                let target = self.condition()?;
                let arms = self.match_arms(Self::match_statement_body)?;
                Ok(Statement::Match {
                    target,
                    arms,
                    location,
                })
            }
            Kind::Keyword("for" | "rev") => {
                let reverse = self.token().kind == Kind::Keyword("rev");
                self.cursor += 1;
                let location = self.location();
                let Kind::Ident(name) = self.token().kind else {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("src.ident", "expected a loop variable name"),
                    ));
                };
                super::names::check(super::names::Shape::Value, name, location)?;
                let name = name.to_owned();
                self.cursor += 1;
                self.expect(FixedToken::Keyword("in"))?;
                let start = self.condition()?;
                let range_location = self.location();
                let (inclusive, end) = match self.token().kind {
                    Kind::Symbol(operator @ ("..<" | "..=")) => {
                        self.cursor += 1;
                        if self.token().kind == Kind::End {
                            return Err(range_location.error(
                                ErrorKind::InvalidSource,
                                crate::diagnostic!(
                                    "for.range",
                                    "expected an end bound after the range operator"
                                ),
                            ));
                        }
                        (operator == "..=", Some(self.condition()?))
                    }
                    Kind::Symbol("{") => (false, None),
                    _ => {
                        return Err(self.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "for.range",
                                "expected a range operator or loop body"
                            ),
                        ));
                    }
                };
                let (body, _) = self.block()?;
                Ok(Statement::For {
                    name,
                    location,
                    start,
                    end: end.map(Box::new),
                    inclusive,
                    reverse,
                    body,
                })
            }
            Kind::Keyword("fn") if self.starts_function_declaration() => Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "decl.location",
                    "functions and types must be declared at top level"
                ),
            )),
            Kind::Keyword("enum" | "error" | "struct" | "fnshape") => Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "decl.location",
                    "functions and types must be declared at top level"
                ),
            )),
            Kind::Keyword("while" | "loop") => {
                let conditional = self.token().kind == Kind::Keyword("while");
                self.cursor += 1;
                let condition = if conditional {
                    Some(self.condition()?)
                } else {
                    None
                };
                let (body, _) = self.block()?;
                Ok(Statement::Loop { condition, body })
            }
            Kind::Keyword("break" | "continue") => {
                let is_break = self.token().kind == Kind::Keyword("break");
                let location = self.location();
                self.cursor += 1;
                if !matches!(
                    self.token().kind,
                    Kind::Newline | Kind::Symbol("}") | Kind::End | Kind::Keyword("allow")
                ) {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        if is_break {
                            crate::diagnostic!("loop.break", "break does not accept a value")
                        } else {
                            crate::diagnostic!("loop.continue", "continue does not accept a value")
                        },
                    ));
                }
                Ok(if is_break {
                    Statement::Break(location)
                } else {
                    Statement::Continue(location)
                })
            }
            Kind::Keyword("if") => self.if_statement(),
            Kind::Keyword("elif" | "else") => Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("if.elif", "unexpected branch clause"),
            )),
            Kind::Symbol("{") => {
                let (statements, _) = self.block()?;
                Ok(Statement::Block(statements))
            }
            Kind::Keyword("fix" | "var") => {
                let mutable = self.token().kind == Kind::Keyword("var");
                self.cursor += 1;
                let location = self.location();
                let name = match self.token().kind {
                    Kind::Ident(name) => name.to_owned(),
                    Kind::Keyword(_) | Kind::TypeName(_) | Kind::Wildcard => {
                        return Err(self.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("src.ident", "expected a non-reserved identifier"),
                        ));
                    }
                    _ => {
                        return Err(self.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("var.decl", "expected a binding name"),
                        ));
                    }
                };
                self.cursor += 1;
                super::names::check(super::names::Shape::Value, &name, location)?;
                let annotation = if self.token().kind == Kind::Symbol(":") {
                    self.cursor += 1;
                    let ty = self.parse_type()?;
                    if ty == Type::Void {
                        return Err(self.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("type.void", "bindings cannot have type void"),
                        ));
                    }
                    Some(ty)
                } else {
                    None
                };
                if self.token().kind != Kind::Symbol("=") {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("var.initializer", "a binding requires an initializer"),
                    ));
                }
                self.cursor += 1;
                let initializer = self.expression()?;
                Ok(Statement::Binding {
                    mutable,
                    name,
                    location,
                    annotation,
                    initializer,
                })
            }
            Kind::Ident(name) if self.assignment_operator().is_some() => {
                let operator = self.assignment_operator().unwrap();
                let name = name.to_owned();
                let location = self.location();
                self.cursor += 1;
                let mut fields = Vec::new();
                let mut target = Expression {
                    kind: ExpressionKind::Name(name.clone()),
                    location,
                    height: 1,
                };
                while self.token().kind == Kind::Symbol(".") {
                    self.cursor += 1;
                    let Kind::Ident(field) = self.token().kind else {
                        unreachable!()
                    };
                    fields.push(field.to_owned());
                    let height = target.height + 1;
                    self.check_height(height, self.location())?;
                    target = Expression {
                        height,
                        kind: ExpressionKind::Field {
                            target: Box::new(target),
                            name: field.to_owned(),
                        },
                        location: self.location(),
                    };
                    self.cursor += 1;
                }
                let operator_location = self.location();
                self.cursor += 1;
                let right = self.expression()?;
                // 二項演算への変換では、右辺を評価する前に現在の代入先の値を読む。
                let value = if let Some(operator) = operator {
                    Expression {
                        height: 1 + target.height.max(right.height),
                        kind: ExpressionKind::Binary {
                            operator,
                            left: Box::new(target),
                            right: Box::new(right),
                        },
                        location: operator_location,
                    }
                } else {
                    right
                };
                Ok(Statement::Assign {
                    compound: operator.is_some(),
                    name,
                    fields,
                    location,
                    value,
                })
            }
            Kind::Keyword("return") => {
                self.cursor += 1;
                if matches!(
                    self.token().kind,
                    Kind::Newline | Kind::Symbol("}") | Kind::End | Kind::Keyword("allow")
                ) {
                    return Ok(Statement::Return(Expression {
                        kind: ExpressionKind::Void,
                        height: 0,
                        location: self.location(),
                    }));
                }
                Ok(Statement::Return(self.expression()?))
            }
            _ => Ok(Statement::Expression(self.expression()?)),
        }
    }

    pub(super) fn block(&mut self) -> Result<(Vec<Statement>, Location), Diagnostic> {
        self.nested(self.location(), Self::block_inner)
    }

    fn block_inner(&mut self) -> Result<(Vec<Statement>, Location), Diagnostic> {
        self.expect(FixedToken::Symbol("{"))?;
        self.newlines();
        let mut statements = Vec::new();
        while self.token().kind != Kind::Symbol("}") {
            if self.token().kind == Kind::End {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("syntax.grammar", "expected '}}' to close the block"),
                ));
            }
            statements.push(self.statement()?);
            match self.token().kind {
                Kind::Newline => self.newlines(),
                Kind::Symbol("}") => {}
                Kind::Keyword(
                    "fix" | "var" | "return" | "if" | "while" | "loop" | "for" | "rev" | "break"
                    | "continue" | "log" | "panic" | "match" | "matchexpr" | "ifexpr" | "true"
                    | "false" | "not" | "nan" | "inf" | "expr",
                )
                | Kind::Ident(_)
                | Kind::Template(_)
                | Kind::Char(_)
                | Kind::Integer(_)
                | Kind::Float(_)
                | Kind::Symbol("{" | "~") => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "src.statement-end",
                            "statements must be separated by a newline"
                        ),
                    ));
                }
                Kind::Keyword("elif" | "else") => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("if.elif", "unexpected branch clause"),
                    ));
                }
                _ => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "src.statement-end",
                            "expected a newline or '}}' after the statement"
                        ),
                    ));
                }
            }
        }
        let end = self.location();
        self.expect(FixedToken::Symbol("}"))?;
        Ok((statements, end))
    }
}
