use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        BinaryOperator, ComparisonOperator, Expression, ExpressionKind, FailureHandler,
        LogicalOperator,
    },
    lexer::Kind,
};

impl Parser<'_, '_> {
    pub(super) fn expression(&mut self) -> Result<Expression, Diagnostic> {
        let operand = self.logical(LogicalOperator::Or)?;
        if self.token().kind != Kind::Keyword("or") {
            return Ok(operand);
        }
        let location = self.location();
        self.cursor += 1;
        let fallback = Some(self.failure_handler()?);
        Ok(Expression {
            kind: ExpressionKind::ArithmeticHandler {
                operand: Box::new(operand),
                fallback,
            },
            location,
        })
    }

    fn failure_handler(&mut self) -> Result<FailureHandler, Diagnostic> {
        if self.token().kind == Kind::Keyword("return") {
            if self.constant_expression {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "control transfers are not constant expressions [const.expression]",
                ));
            }
            let location = self.location();
            self.cursor += 1;
            if matches!(
                self.token().kind,
                Kind::Newline | Kind::Symbol("}" | ")" | ",") | Kind::End
            ) {
                return Ok(FailureHandler::Return {
                    value: Box::new(Expression {
                        kind: ExpressionKind::Void,
                        location,
                    }),
                    location,
                });
            }
            Ok(FailureHandler::Return {
                value: Box::new(self.expression()?),
                location,
            })
        } else {
            Ok(FailureHandler::Value(Box::new(self.expression()?)))
        }
    }

    fn logical(&mut self, operator: LogicalOperator) -> Result<Expression, Diagnostic> {
        let operand = |parser: &mut Self| match operator {
            LogicalOperator::Or => parser.logical(LogicalOperator::And),
            LogicalOperator::And => parser.comparison(),
        };
        let mut left = operand(self)?;
        while self.token().kind == Kind::Symbol(operator.symbol()) {
            let location = self.location();
            self.cursor += 1;
            let right = operand(self)?;
            left = Expression {
                kind: ExpressionKind::Logical {
                    operator,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                location,
            };
        }
        Ok(left)
    }

    fn comparison(&mut self) -> Result<Expression, Diagnostic> {
        let first = self.binary(0)?;
        let location = self.location();
        let mut operands = vec![first];
        let mut operators = Vec::new();
        let mut locations = Vec::new();
        let mut direction = None;
        let mut not_equal = false;
        while let Kind::Symbol(symbol) = self.token().kind {
            let Some(operator) = ComparisonOperator::from_symbol(symbol) else {
                break;
            };
            if !operators.is_empty()
                && (not_equal || matches!(operator, ComparisonOperator::NotEqual))
            {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "!= cannot occur in a comparison chain [expr.comparison-chain]",
                ));
            }
            if let Some(next) = operator.direction() {
                if direction.is_some_and(|previous| previous != next) {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        "comparison chain changes ordering direction [expr.comparison-chain]",
                    ));
                }
                direction = Some(next);
            }
            locations.push(self.location());
            not_equal = matches!(operator, ComparisonOperator::NotEqual);
            self.cursor += 1;
            operands.push(self.binary(0)?);
            operators.push(operator);
        }
        if operators.is_empty() {
            return Ok(operands.pop().unwrap());
        }
        Ok(Expression {
            kind: ExpressionKind::Comparison {
                locations,
                operators,
                operands,
            },
            location,
        })
    }

    fn binary(&mut self, level: u8) -> Result<Expression, Diagnostic> {
        if level == 6 {
            return self.cast();
        }
        let mut left = self.binary(level + 1)?;
        while let Kind::Symbol(symbol) = self.token().kind {
            let Some(operator) = BinaryOperator::from_symbol(symbol) else {
                break;
            };
            if operator.precedence() != level {
                break;
            }
            let location = self.location();
            self.cursor += 1;
            let right = self.binary(level + 1)?;
            left = Expression {
                kind: ExpressionKind::Binary {
                    operator,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                location,
            };
        }
        Ok(left)
    }

    fn cast(&mut self) -> Result<Expression, Diagnostic> {
        let mut operand = self.unary()?;
        while matches!(self.token().kind, Kind::Symbol("as%") | Kind::Keyword("as")) {
            let truncate = self.token().kind == Kind::Symbol("as%");
            let location = self.location();
            self.cursor += 1;
            let target = self.parse_type()?;
            let fallback = if self.token().kind == Kind::Keyword("or") {
                self.cursor += 1;
                Some(self.failure_handler()?)
            } else {
                None
            };
            let kind = ExpressionKind::Cast {
                wrap: truncate,
                operand: Box::new(operand),
                target,
                fallback,
            };
            operand = Expression { kind, location };
        }
        Ok(operand)
    }

    fn slice_bound(&mut self) -> Result<Expression, Diagnostic> {
        let reject_range = |parser: &Self| {
            if matches!(parser.token().kind, Kind::Symbol("..<" | "..=")) {
                Err(parser.location().error(
                    ErrorKind::InvalidSource,
                    "text extraction uses ':' instead of a range operator [text.slice-omit]",
                ))
            } else {
                Ok(())
            }
        };
        reject_range(self)?;
        let value = self.expression()?;
        reject_range(self)?;
        Ok(value)
    }

    fn unary(&mut self) -> Result<Expression, Diagnostic> {
        if self.constant_expression
            && matches!(
                self.token().kind,
                Kind::Keyword("expr" | "ifexpr" | "matchexpr")
            )
        {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "value blocks are not constant expressions [const.expression]",
            ));
        }
        let location = self.location();
        let kind = match self.token().kind {
            Kind::Keyword("if") => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "if is a statement; use ifexpr for a value [expr.if-value]",
                ));
            }
            Kind::Symbol("{") => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "a standalone block is a statement; use expr for a value [expr.block]",
                ));
            }
            Kind::Template(raw) => ExpressionKind::Template(self.template(raw)?),
            Kind::Keyword("ifexpr") => self.if_expression()?,
            Kind::Keyword("matchexpr") => {
                self.cursor += 1;
                let target = Box::new(self.condition()?);
                let arms = self.match_arms(Self::match_value_body)?;
                ExpressionKind::Match { target, arms }
            }
            Kind::Float(text) | Kind::Keyword(text @ ("nan" | "inf")) => {
                self.cursor += 1;
                ExpressionKind::Float(text.to_owned())
            }
            Kind::Char(value) => {
                self.cursor += 1;
                ExpressionKind::Char(value)
            }
            Kind::Integer(text) => {
                self.cursor += 1;
                ExpressionKind::Integer {
                    digits: text.replace('_', ""),
                    negative: false,
                }
            }
            Kind::Keyword("expr") => {
                self.cursor += 1;
                self.value_block()?.0.kind
            }
            Kind::Keyword("true" | "false") => {
                let value = self.token().kind == Kind::Keyword("true");
                self.cursor += 1;
                ExpressionKind::Bool(value)
            }
            Kind::Symbol("~") => {
                self.cursor += 1;
                ExpressionKind::BitNot(Box::new(self.unary()?))
            }
            Kind::Keyword("not") => {
                self.cursor += 1;
                ExpressionKind::Not(Box::new(self.unary()?))
            }
            Kind::Ident(name) => {
                self.cursor += 1;
                if self.constant_expression && self.token().kind == Kind::Symbol("(") {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        "function calls are not constant expressions [const.expression]",
                    ));
                }
                if self.token().kind == Kind::Symbol("{")
                    && self.structs.iter().any(|decl| decl.name == name)
                {
                    if self.restricted_condition {
                        return Err(location.error(ErrorKind::InvalidSource, "struct literals in conditions require parentheses [expr.struct-in-cond]"));
                    }
                    let id = self
                        .structs
                        .iter()
                        .position(|decl| decl.name == name)
                        .unwrap();
                    ExpressionKind::Struct {
                        id,
                        fields: self.delimited_list("{", "}", Self::field_initializer)?,
                    }
                } else if self.token().kind == Kind::Symbol(".")
                    && self.enums.iter().any(|decl| decl.name == name)
                {
                    self.cursor += 1;
                    let Kind::Ident(variant) = self.token().kind else {
                        return Err(self.error(
                            ErrorKind::InvalidSource,
                            "expected an enum variant [enum.access]",
                        ));
                    };
                    self.cursor += 1;
                    ExpressionKind::Variant {
                        enum_name: name.into(),
                        variant: variant.into(),
                    }
                } else if self.token().kind == Kind::Symbol("(") {
                    ExpressionKind::Call {
                        name: name.into(),
                        arguments: self.list(Self::parenthesized_expression)?,
                    }
                } else {
                    ExpressionKind::Name(name.into())
                }
            }
            Kind::Symbol("-") => {
                self.cursor += 1;
                let kind = match self.token().kind {
                    Kind::Integer(text) => ExpressionKind::Integer {
                        digits: text.replace('_', ""),
                        negative: true,
                    },
                    Kind::Float(text) | Kind::Keyword(text @ ("nan" | "inf")) => {
                        ExpressionKind::Float(format!("-{text}"))
                    }
                    _ => {
                        return Ok(Expression {
                            kind: ExpressionKind::Negate(Box::new(self.unary()?)),
                            location,
                        });
                    }
                };
                self.cursor += 1;
                kind
            }
            Kind::Symbol("(") => {
                self.cursor += 1;
                let inner = self.parenthesized_expression()?;
                self.expect(FixedToken::Symbol(")"))?;
                ExpressionKind::Group(Box::new(inner))
            }
            _ => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "expected an expression [syntax.grammar]",
                ));
            }
        };
        let mut result = Expression { kind, location };
        while matches!(self.token().kind, Kind::Symbol("." | "[")) {
            if self.token().kind == Kind::Symbol("[") {
                let location = self.location();
                self.cursor += 1;
                let start = if self.token().kind == Kind::Symbol(":") {
                    None
                } else {
                    Some(Box::new(self.slice_bound()?))
                };
                let index = self.token().kind != Kind::Symbol(":");
                let end = if !index {
                    self.cursor += 1;
                    if self.token().kind == Kind::Symbol("]") {
                        None
                    } else {
                        Some(Box::new(self.slice_bound()?))
                    }
                } else {
                    None
                };
                self.expect(FixedToken::Symbol("]"))?;
                let fallback = if self.token().kind == Kind::Keyword("or") {
                    self.cursor += 1;
                    Some(self.failure_handler()?)
                } else {
                    None
                };
                result = Expression {
                    location,
                    kind: ExpressionKind::Slice {
                        target: Box::new(result),
                        start,
                        end,
                        index,
                        fallback,
                    },
                };
                continue;
            }
            self.cursor += 1;
            let location = self.location();
            let Kind::Ident(name) = self.token().kind else {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "expected a field name [struct.access]",
                ));
            };
            self.cursor += 1;
            result = Expression {
                kind: ExpressionKind::Field {
                    target: Box::new(result),
                    name: name.into(),
                },
                location,
            };
        }
        if matches!(
            self.token().kind,
            Kind::Symbol(
                "=" | "+="
                    | "-="
                    | "*="
                    | "/="
                    | "%="
                    | "+%="
                    | "-%="
                    | "*%="
                    | "&="
                    | "|="
                    | "^="
                    | "<<="
                    | ">>="
            )
        ) {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "assignment is a statement, not an expression [assign.statement]",
            ));
        }
        Ok(result)
    }
}
