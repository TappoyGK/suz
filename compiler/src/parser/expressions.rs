//! 演算子の優先順位に従って式を解析し、呼び出し・構造体・比較の連鎖などのASTを作る。
//!
//! 利用元: [`crate::parser::statements`]、[`crate::parser::functions`]。
//! 主な対象規則: `[assign.statement]`、`[const.expression]`、`[enum.access]`、`[expr.block]`、`[expr.comparison-chain]`、`[expr.if-value]`。

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
        self.check_height(operand.height + 1, location)?;
        let fallback = Some(self.operator_child(location, Self::failure_handler)?);
        self.node(
            ExpressionKind::ArithmeticHandler {
                operand: Box::new(operand),
                fallback,
            },
            location,
        )
    }

    fn failure_handler(&mut self) -> Result<FailureHandler, Diagnostic> {
        if self.token().kind == Kind::Keyword("panic") {
            let crate::ast::Statement::Log {
                parts, location, ..
            } = self.panic()?
            else {
                unreachable!()
            };
            return Ok(FailureHandler::Value(Box::new(
                self.node(ExpressionKind::Panic(parts), location)?,
            )));
        }
        if self.token().kind == Kind::Keyword("ignore") {
            let location = self.location();
            self.cursor += 1;
            if !matches!(self.token().kind, Kind::IgnoreReason(_)) {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("err.or-ignore", "ignore requires ': reason'"),
                ));
            }
            self.cursor += 1;
            return Ok(FailureHandler::Ignore(location));
        }
        if self.token().kind == Kind::Keyword("fail") {
            let location = self.location();
            self.cursor += 1;
            let value = if matches!(
                self.token().kind,
                Kind::Newline | Kind::Symbol("}" | ")" | ",") | Kind::End
            ) {
                None
            } else {
                Some(Box::new(self.expression()?))
            };
            return Ok(FailureHandler::Fail { value, location });
        }
        if self.token().kind == Kind::Keyword("return") {
            if self.constant_expression {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "const.expression",
                        "control transfers are not constant expressions"
                    ),
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
                        height: 0,
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
            self.check_height(left.height + 1, location)?;
            let right = self.operator_child(location, operand)?;
            left = self.node(
                ExpressionKind::Logical {
                    operator,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                location,
            )?;
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
                    crate::diagnostic!(
                        "expr.comparison-chain",
                        "!= cannot occur in a comparison chain"
                    ),
                ));
            }
            if let Some(next) = operator.direction() {
                if direction.is_some_and(|previous| previous != next) {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "expr.comparison-chain",
                            "comparison chain changes ordering direction"
                        ),
                    ));
                }
                direction = Some(next);
            }
            locations.push(self.location());
            not_equal = matches!(operator, ComparisonOperator::NotEqual);
            self.cursor += 1;
            self.check_height(operands[0].height + 1, location)?;
            operands.push(self.operator_child(location, |p| p.binary(0))?);
            operators.push(operator);
        }
        if operators.is_empty() {
            return Ok(operands.pop().unwrap());
        }
        self.node(
            ExpressionKind::Comparison {
                locations,
                operators,
                operands,
            },
            location,
        )
    }

    fn binary(&mut self, level: u8) -> Result<Expression, Diagnostic> {
        // 演算子のない式で優先順位の段数だけ再帰しない。
        let mut left = self.cast()?;
        while let Kind::Symbol(symbol) = self.token().kind {
            let Some(operator) = BinaryOperator::from_symbol(symbol) else {
                break;
            };
            if operator.precedence() < level {
                break;
            }
            let location = self.location();
            self.cursor += 1;
            self.check_height(left.height + 1, location)?;
            let right = self.operator_child(location, |p| p.binary(operator.precedence() + 1))?;
            left = self.node(
                ExpressionKind::Binary {
                    operator,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                location,
            )?;
        }
        Ok(left)
    }

    fn cast(&mut self) -> Result<Expression, Diagnostic> {
        let mut operand = self.unary()?;
        while matches!(self.token().kind, Kind::Symbol("as%") | Kind::Keyword("as")) {
            let truncate = self.token().kind == Kind::Symbol("as%");
            let location = self.location();
            self.cursor += 1;
            self.check_height(operand.height + 1, location)?;
            let target = self.parse_type()?;
            let fallback = if self.token().kind == Kind::Keyword("or") {
                let handler_location = self.location();
                self.check_height(operand.height + 2, handler_location)?;
                self.cursor += 1;
                Some(self.operator_child(handler_location, Self::failure_handler)?)
            } else {
                None
            };
            let kind = ExpressionKind::Cast {
                wrap: truncate,
                operand: Box::new(operand),
                target,
                fallback,
            };
            operand = self.node(kind, location)?;
        }
        Ok(operand)
    }

    fn slice_bound(&mut self) -> Result<Expression, Diagnostic> {
        let reject_range = |parser: &Self| {
            if matches!(parser.token().kind, Kind::Symbol("..<" | "..=")) {
                Err(parser.location().error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "text.slice-omit",
                        "text extraction uses ':' instead of a range operator"
                    ),
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
        self.nested(self.location(), Self::unary_inner)
    }

    fn unary_inner(&mut self) -> Result<Expression, Diagnostic> {
        if self.constant_expression
            && matches!(
                self.token().kind,
                Kind::Keyword("expr" | "ifexpr" | "matchexpr")
            )
        {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "const.expression",
                    "value blocks are not constant expressions"
                ),
            ));
        }
        let location = self.location();
        let kind = match self.token().kind {
            Kind::Keyword("fn") => {
                self.cursor += 1;
                if self.token().kind == Kind::Symbol("[") {
                    return self.closure_node(location);
                }
                let Kind::Ident(name) = self.token().kind else {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("fn.value-mark", "fn requires a function name"),
                    ));
                };
                self.cursor += 1;
                if matches!(self.token().kind, Kind::Symbol("(" | "." | "[")) {
                    return Err(self.error(ErrorKind::InvalidSource, crate::diagnostic!("fn.value-mark", "fn applies to a function name only; parenthesize the function value before calling it")));
                }
                return self.primary_node(ExpressionKind::FunctionValue(name.into()), location);
            }
            Kind::Keyword("if") => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.if-value",
                        "if is a statement; use ifexpr for a value"
                    ),
                ));
            }
            Kind::Symbol("{") => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.block",
                        "a standalone block is a statement; use expr for a value"
                    ),
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
            Kind::Keyword("len") => {
                self.cursor += 1;
                ExpressionKind::Len(Box::new(self.unary()?))
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
                        crate::diagnostic!(
                            "const.expression",
                            "function calls are not constant expressions"
                        ),
                    ));
                }
                if self.token().kind == Kind::Symbol("{")
                    && self.structs.iter().any(|decl| decl.name == name)
                {
                    if self.restricted_condition {
                        return Err(location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "expr.struct-in-cond",
                                "struct literals in conditions require parentheses"
                            ),
                        ));
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
                            crate::diagnostic!("enum.access", "expected an enum variant"),
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
                // 負のリテラルへ畳み込む前に、単項演算子の下のリテラルの段を検査する。
                self.check_height(1, self.limit_location.unwrap_or(self.location()))?;
                let kind = match self.token().kind {
                    Kind::Integer(text) => ExpressionKind::Integer {
                        digits: text.replace('_', ""),
                        negative: true,
                    },
                    Kind::Float(text) | Kind::Keyword(text @ ("nan" | "inf")) => {
                        ExpressionKind::Float(format!("-{text}"))
                    }
                    _ => {
                        let child = self.unary()?;
                        return self
                            .primary_node(ExpressionKind::Negate(Box::new(child)), location);
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
                    crate::diagnostic!("syntax.grammar", "expected an expression"),
                ));
            }
        };
        let mut result = self.primary_node(kind, location)?;
        while matches!(
            self.token().kind,
            Kind::Symbol("." | "[" | "(") | Kind::Keyword("with")
        ) {
            if self.token().kind == Kind::Symbol("(") {
                let location = self.location();
                if self.constant_expression {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "const.expression",
                            "function calls are not constant expressions"
                        ),
                    ));
                }
                self.check_height(result.height, location)?;
                let arguments = self.list(Self::parenthesized_expression)?;
                result = self.primary_node(
                    ExpressionKind::Invoke {
                        target: Box::new(result),
                        arguments,
                    },
                    location,
                )?;
                continue;
            }
            if self.token().kind == Kind::Keyword("with") {
                result = self.with_expression(result)?;
                continue;
            }
            if self.token().kind == Kind::Symbol("[") {
                let location = self.location();
                self.check_height(result.height, location)?;
                let previous_limit = self.limit_location.replace(location);
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
                    let height = result
                        .height
                        .max(start.as_ref().map_or(0, |v| v.height))
                        .max(end.as_ref().map_or(0, |v| v.height));
                    self.check_height(height + 1, self.location())?;
                    self.limit_location = Some(self.location());
                    self.cursor += 1;
                    Some(self.failure_handler()?)
                } else {
                    None
                };
                self.limit_location = previous_limit;
                result = self.primary_node(
                    ExpressionKind::Slice {
                        target: Box::new(result),
                        start,
                        end,
                        index,
                        fallback,
                    },
                    location,
                )?;
                continue;
            }
            self.check_height(result.height, self.location())?;
            self.cursor += 1;
            let location = self.location();
            let Kind::Ident(name) = self.token().kind else {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("struct.access", "expected a field name"),
                ));
            };
            self.cursor += 1;
            result = self.primary_node(
                ExpressionKind::Field {
                    target: Box::new(result),
                    name: name.into(),
                },
                location,
            )?;
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
                crate::diagnostic!(
                    "assign.statement",
                    "assignment is a statement, not an expression"
                ),
            ));
        }
        Ok(result)
    }
}
