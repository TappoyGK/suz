use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{BinaryOperator, Expression, ExpressionKind, Location, Statement, Type},
    lexer::Kind,
};

impl Parser<'_, '_> {
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
        if self.in_value
            && matches!(
                self.token().kind,
                Kind::Keyword("return" | "break" | "continue")
            )
        {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "control transfers are forbidden inside value expressions [expr.jump-prohibited]",
            ));
        }
        match self.token().kind {
            Kind::Keyword("or") => Err(self.error(
                ErrorKind::InvalidSource,
                "or cannot start a statement or continue the preceding line [src.or-line]",
            )),
            Kind::Keyword("log") => self.log(),
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
            Kind::Keyword("for") => {
                self.cursor += 1;
                let location = self.location();
                let Kind::Ident(name) = self.token().kind else {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        "expected a loop variable name [src.ident]",
                    ));
                };
                super::names::check(super::names::Shape::Value, name, location)?;
                let name = name.to_owned();
                self.cursor += 1;
                self.expect(FixedToken::Keyword("in"))?;
                let start = self.condition()?;
                let inclusive = match self.token().kind {
                    Kind::Symbol("..<") => false,
                    Kind::Symbol("..=") => true,
                    _ => {
                        return Err(self.error(
                            ErrorKind::InvalidSource,
                            "expected a range operator [for.range]",
                        ));
                    }
                };
                self.cursor += 1;
                let end = self.condition()?;
                let (body, _) = self.block()?;
                Ok(Statement::For {
                    name,
                    location,
                    start,
                    end,
                    inclusive,
                    body,
                })
            }
            Kind::Keyword("const" | "fn" | "enum" | "struct") => Err(self.error(
                ErrorKind::InvalidSource,
                "constants and functions must be declared at top level [decl.location]",
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
                    Kind::Newline | Kind::Symbol("}") | Kind::End
                ) {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        if is_break {
                            "break does not accept a value [loop.break]"
                        } else {
                            "continue does not accept a value [loop.continue]"
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
                "unexpected branch clause [if.elif]",
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
                            "expected a non-reserved identifier [src.ident]",
                        ));
                    }
                    _ => {
                        return Err(self.error(
                            ErrorKind::InvalidSource,
                            "expected a binding name [var.decl]",
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
                            "bindings cannot have type void [type.void]",
                        ));
                    }
                    Some(ty)
                } else {
                    None
                };
                if self.token().kind != Kind::Symbol("=") {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        "a binding requires an initializer [var.initializer]",
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
                };
                while self.token().kind == Kind::Symbol(".") {
                    self.cursor += 1;
                    let Kind::Ident(field) = self.token().kind else {
                        unreachable!()
                    };
                    fields.push(field.to_owned());
                    target = Expression {
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
                // Binary lowering reads the current target before evaluating the RHS.
                let value = if let Some(operator) = operator {
                    Expression {
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
                    Kind::Newline | Kind::Symbol("}") | Kind::End
                ) {
                    return Ok(Statement::Return(Expression {
                        kind: ExpressionKind::Void,
                        location: self.location(),
                    }));
                }
                Ok(Statement::Return(self.expression()?))
            }
            _ => Ok(Statement::Expression(self.expression()?)),
        }
    }

    pub(super) fn block(&mut self) -> Result<(Vec<Statement>, Location), Diagnostic> {
        self.expect(FixedToken::Symbol("{"))?;
        self.newlines();
        let mut statements = Vec::new();
        while self.token().kind != Kind::Symbol("}") {
            if self.token().kind == Kind::End {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "expected '}' to close the block [syntax.grammar]",
                ));
            }
            statements.push(self.statement()?);
            match self.token().kind {
                Kind::Newline => self.newlines(),
                Kind::Symbol("}") => {}
                Kind::Symbol("..<" | "..=") => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        "range notation is only allowed in for [for.range]",
                    ));
                }
                Kind::Keyword(
                    "fix" | "var" | "return" | "if" | "while" | "loop" | "for" | "break"
                    | "continue" | "log" | "match" | "matchexpr" | "ifexpr" | "true" | "false"
                    | "not" | "nan" | "inf" | "expr",
                )
                | Kind::Ident(_)
                | Kind::Template(_)
                | Kind::Integer(_)
                | Kind::Float(_)
                | Kind::Symbol("{" | "~") => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        "statements must be separated by a newline [src.statement-end]",
                    ));
                }
                Kind::Keyword("elif" | "else") => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        "unexpected branch clause [if.elif]",
                    ));
                }
                _ => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        "expected a newline or '}' after the statement [src.statement-end]",
                    ));
                }
            }
        }
        let end = self.location();
        self.expect(FixedToken::Symbol("}"))?;
        Ok((statements, end))
    }
}
