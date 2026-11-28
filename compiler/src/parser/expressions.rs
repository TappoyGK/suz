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
    // 再帰するunaryのフレームに配列の構築用一時値を置かない。
    fn constant_array(&mut self, soa: bool) -> Result<Expression, Diagnostic> {
        let value = self.array_initializer(true, soa)?;
        self.postfix(value)
    }

    fn text_buffer(&mut self) -> Result<Expression, Diagnostic> {
        let location = self.location();
        self.cursor += 1;
        let capacity = if self.token().kind == Kind::Symbol("[") {
            self.cursor += 1;
            let value = self.expression()?;
            self.expect(FixedToken::Symbol("]"))?;
            Some(Box::new(value))
        } else {
            None
        };
        let kind = ExpressionKind::TextBuffer {
            capacity,
            value: Box::new(self.unary()?),
        };
        let result = self.primary_node(kind, location)?;
        self.postfix(result)
    }

    pub(super) fn expression(&mut self) -> Result<Expression, Diagnostic> {
        let result = self.expression_before_assignment()?;
        self.check_value_context()?;
        Ok(result)
    }

    // 値式を読み終えた位置で、書き換え対象や代入の続きを拒否する。
    // 文の解析では、#fieldを代入先として扱う経路とこの検査を分ける。
    pub(super) fn check_value_context(&self) -> Result<(), Diagnostic> {
        if self.token().kind == Kind::Symbol("#") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "assign.target",
                    "target#field is an assignment target, not a value; use target.field to read the field"
                ),
            ));
        }
        if self.token().kind == Kind::Symbol("<-") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "text.append",
                    "text append is a statement, not an expression"
                ),
            ));
        }
        if matches!(
            self.token().kind,
            Kind::Symbol(
                "=" | "+="
                    | "-="
                    | "*="
                    | "/="
                    | "%="
                    | "%%="
                    | "+%="
                    | "-%="
                    | "*%="
                    | "+|="
                    | "-|="
                    | "*|="
                    | "&="
                    | "|="
                    | "^="
                    | "<<="
                    | ">>="
                    | ">>>="
                    | "<<%="
                    | ">>%="
                    | ">>>%="
                    | "<<|="
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
        Ok(())
    }

    pub(super) fn expression_before_assignment(&mut self) -> Result<Expression, Diagnostic> {
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
        if self.token().kind == Kind::Keyword("error") {
            return self.error_handler();
        }
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
            return Ok(FailureHandler::Ignore(self.ignore_reason()?));
        }
        if self.token().kind == Kind::Keyword("fail") {
            let location = self.location();
            self.cursor += 1;
            let value = if matches!(
                self.token().kind,
                Kind::Newline | Kind::BlankLine | Kind::Symbol("}" | ")" | ",") | Kind::End
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
                Kind::Newline | Kind::BlankLine | Kind::Symbol("}" | ")" | ",") | Kind::End
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
        while matches!(
            self.token().kind,
            Kind::Symbol("as%" | "as|") | Kind::Keyword("as")
        ) {
            let kind = match self.token().kind {
                Kind::Symbol("as%") => crate::ast::CastKind::Wrap,
                Kind::Symbol("as|") => crate::ast::CastKind::Saturating,
                _ => crate::ast::CastKind::Checked,
            };
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
                kind,
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

    pub(super) fn unary(&mut self) -> Result<Expression, Diagnostic> {
        self.nested(self.location(), Self::unary_inner)
    }

    fn function_value_node(
        &mut self,
        location: crate::ast::Location,
    ) -> Result<Expression, Diagnostic> {
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
        if matches!(self.token().kind, Kind::Symbol("." | "#")) {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "method.no-fn-value",
                    "methods cannot be extracted as function values"
                ),
            ));
        }
        let type_arguments = if self.token().kind == Kind::Symbol("[") {
            let arguments = self.delimited_list("[", "]", Self::type_argument)?;
            if arguments.is_empty() {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.type-arguments",
                        "at least one type argument is required"
                    ),
                ));
            }
            arguments
        } else {
            Vec::new()
        };
        if matches!(self.token().kind, Kind::Symbol("(" | "." | "[")) {
            return Err(self.error(ErrorKind::InvalidSource, crate::diagnostic!("fn.value-mark", "fn applies to a function name only; parenthesize the function value before calling it")));
        }
        self.primary_node(
            ExpressionKind::FunctionValue {
                name: name.into(),
                type_arguments,
            },
            location,
        )
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
            Kind::Keyword("fn") => return self.function_value_node(location),
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
            Kind::Keyword("nil") => {
                self.cursor += 1;
                ExpressionKind::Nil
            }
            Kind::Keyword("true" | "false") => {
                let value = self.token().kind == Kind::Keyword("true");
                self.cursor += 1;
                ExpressionKind::Bool(value)
            }
            Kind::Symbol("*") => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "arena.ptr-operations",
                        "unary *p is not supported; use .p to read the pointee or #p to assign to it"
                    ),
                ));
            }
            Kind::Symbol("#")
                if self
                    .tokens
                    .get(self.cursor + 1)
                    .is_some_and(|t| t.kind == Kind::Symbol("[")) =>
            {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.new",
                        "mutable array initializer requires new #[...]"
                    ),
                ));
            }
            Kind::Symbol("#") => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "arena.ptr-operations",
                        "#p is an assignment target, not a value; use .p to read the pointee"
                    ),
                ));
            }
            Kind::Symbol(".")
                if self
                    .tokens
                    .get(self.cursor + 1)
                    .is_some_and(|t| t.kind == Kind::Symbol("[")) =>
            {
                self.cursor += 1;
                return self.constant_array(false);
            }
            Kind::Keyword("soa") => return self.soa_expression(location),
            Kind::Symbol(".") => {
                self.cursor += 1;
                let operand = self.unary()?;
                // .p.field は参照解除とフィールド選択の順が読み取りにくい。
                // Groupで区切った (.p).field / .(p.field) は区別して受け付ける。
                let mut target = &operand;
                while let ExpressionKind::Invoke { target: inner, .. }
                | ExpressionKind::Slice { target: inner, .. }
                | ExpressionKind::With { target: inner, .. } = &target.kind
                {
                    target = inner;
                }
                if matches!(
                    target.kind,
                    ExpressionKind::Field { .. } | ExpressionKind::MethodCall { .. }
                ) {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "arena.ptr-operations",
                            "parenthesize dereference and field access: (.p).field or .(p.field)"
                        ),
                    ));
                }
                ExpressionKind::Deref(Box::new(operand))
            }
            Kind::Keyword("textbuf") => return self.text_buffer(),
            Kind::Keyword("new") => return self.new_expression(location),
            Kind::Symbol("~" | "-%" | "-|")
            | Kind::Keyword("len" | "not" | "c_bytes" | "c_byte_len") => {
                let constructor: fn(Box<Expression>) -> ExpressionKind = match self.token().kind {
                    Kind::Symbol("~") => ExpressionKind::BitNot,
                    Kind::Symbol("-%") => ExpressionKind::WrapNegate,
                    Kind::Symbol("-|") => ExpressionKind::SaturatingNegate,
                    Kind::Keyword("len") => ExpressionKind::Len,
                    Kind::Keyword("not") => ExpressionKind::Not,
                    Kind::Keyword("c_bytes") => ExpressionKind::CBytes,
                    Kind::Keyword("c_byte_len") => ExpressionKind::CByteLen,
                    _ => unreachable!(),
                };
                self.cursor += 1;
                constructor(Box::new(self.unary()?))
            }
            Kind::Keyword("return") if self.in_ensures => {
                self.cursor += 1;
                ExpressionKind::Name("return".into())
            }
            Kind::Ident(name) | Kind::Keyword(name @ "self") => {
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
                if matches!(self.token().kind, Kind::Symbol("{" | "["))
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
                    let ty = self.struct_type(id)?;
                    ExpressionKind::Struct {
                        ty,
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
                } else if self.explicit_type_arguments(name) {
                    let type_arguments = self.delimited_list("[", "]", Self::type_argument)?;
                    if type_arguments.is_empty() {
                        return Err(location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "generic.type-arguments",
                                "type arguments cannot be empty"
                            ),
                        ));
                    }
                    if self.constant_expression {
                        return Err(location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "const.expression",
                                "function calls are not constant expressions"
                            ),
                        ));
                    }
                    ExpressionKind::Call {
                        type_arguments,
                        name: name.into(),
                        arguments: self.list(Self::parenthesized_expression)?,
                    }
                } else if self.token().kind == Kind::Symbol("(") {
                    ExpressionKind::Call {
                        type_arguments: Vec::new(),
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
        let result = self.primary_node(kind, location)?;
        self.postfix(result)
    }

    // 後置式の一時値を、原子式の再帰解析中のスタックに積まない。
    fn postfix(&mut self, mut result: Expression) -> Result<Expression, Diagnostic> {
        while matches!(
            self.token().kind,
            Kind::Symbol("." | "(") | Kind::Keyword("with")
        ) || (self.token().kind == Kind::Symbol("#")
            && self
                .tokens
                .get(self.cursor + 1)
                .is_some_and(|t| matches!(t.kind, Kind::Ident(_)))
            && self
                .tokens
                .get(self.cursor + 2)
                .is_some_and(|t| t.kind == Kind::Symbol("(")))
        {
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
            if self.token().kind == Kind::Symbol(".")
                && self
                    .tokens
                    .get(self.cursor + 1)
                    .is_some_and(|t| t.kind == Kind::Symbol("["))
            {
                let location = self.location();
                self.check_height(result.height, location)?;
                let previous_limit = self.limit_location.replace(location);
                self.cursor += 2;
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
                let (fallback, fallback_location) = if self.token().kind == Kind::Keyword("or") {
                    let or_location = self.location();
                    let height = result
                        .height
                        .max(start.as_ref().map_or(0, |v| v.height))
                        .max(end.as_ref().map_or(0, |v| v.height));
                    self.check_height(height + 1, or_location)?;
                    self.limit_location = Some(or_location);
                    self.cursor += 1;
                    (Some(self.failure_handler()?), Some(or_location))
                } else {
                    (None, None)
                };
                self.limit_location = previous_limit;
                result = self.primary_node(
                    ExpressionKind::Slice {
                        target: Box::new(result),
                        start,
                        end,
                        index,
                        fallback,
                        fallback_location,
                    },
                    location,
                )?;
                continue;
            }
            self.check_height(result.height, self.location())?;
            let mutable = self.token().kind == Kind::Symbol("#");
            self.cursor += 1;
            let location = self.location();
            let Kind::Ident(name) = self.token().kind else {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("struct.access", "expected a field name"),
                ));
            };
            self.cursor += 1;
            if self.token().kind == Kind::Symbol("(") {
                if self.constant_expression {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "const.expression",
                            "method calls are not constant expressions"
                        ),
                    ));
                }
                let arguments = self.list(Self::parenthesized_expression)?;
                result = self.primary_node(
                    ExpressionKind::MethodCall {
                        target: Box::new(result),
                        name: name.into(),
                        mutable,
                        arguments,
                    },
                    location,
                )?;
                continue;
            }
            result = self.primary_node(
                ExpressionKind::Field {
                    target: Box::new(result),
                    name: name.into(),
                },
                location,
            )?;
        }
        // #fieldは代入先として文の解析へ残す。ただし後ろにメソッド呼び出しが
        // 続く形は、ポインタ代入ではなく受け手のたどり方の誤りとして診断する。
        if self.token().kind == Kind::Symbol("#") {
            let mut cursor = self.cursor;
            while self
                .tokens
                .get(cursor)
                .is_some_and(|t| matches!(t.kind, Kind::Symbol("." | "#")))
                && self
                    .tokens
                    .get(cursor + 1)
                    .is_some_and(|t| matches!(t.kind, Kind::Ident(_)))
            {
                cursor += 2;
            }
            if self
                .tokens
                .get(cursor)
                .is_some_and(|t| t.kind == Kind::Symbol("("))
            {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "method.call",
                        "use . to access receiver fields before calling a method"
                    ),
                ));
            }
        }
        Ok(result)
    }

    #[inline(never)]
    fn soa_expression(&mut self, location: crate::ast::Location) -> Result<Expression, Diagnostic> {
        if self
            .tokens
            .get(self.cursor + 1)
            .is_some_and(|t| t.kind == Kind::Symbol("."))
            && self
                .tokens
                .get(self.cursor + 2)
                .is_some_and(|t| t.kind == Kind::Symbol("["))
        {
            self.cursor += 2;
            return self.constant_array(true);
        }
        if self
            .tokens
            .get(self.cursor + 1)
            .is_some_and(|t| t.kind == Kind::Symbol("#"))
            && self
                .tokens
                .get(self.cursor + 2)
                .is_some_and(|t| t.kind == Kind::Symbol("["))
        {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "array.new",
                    "mutable soa array initializer requires new soa #[...]"
                ),
            ));
        }
        Err(location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "array.soa-type",
                "expected soa .[...], new soa #[...], or new soa .[...]"
            ),
        ))
    }

    #[inline(never)]
    fn new_expression(&mut self, location: crate::ast::Location) -> Result<Expression, Diagnostic> {
        let kind = if self
            .tokens
            .get(self.cursor + 1)
            .is_some_and(|t| t.kind == Kind::Keyword("soa"))
            && self
                .tokens
                .get(self.cursor + 2)
                .is_some_and(|t| matches!(t.kind, Kind::Symbol("#" | ".")))
            && self
                .tokens
                .get(self.cursor + 3)
                .is_some_and(|t| t.kind == Kind::Symbol("["))
        {
            let readonly = self.tokens[self.cursor + 2].kind == Kind::Symbol(".");
            self.cursor += 3;
            ExpressionKind::New(Box::new(self.array_initializer(readonly, true)?))
        } else if self
            .tokens
            .get(self.cursor + 1)
            .is_some_and(|t| matches!(t.kind, Kind::Symbol("#" | ".")))
            && self
                .tokens
                .get(self.cursor + 2)
                .is_some_and(|t| t.kind == Kind::Symbol("["))
        {
            let readonly = self.tokens[self.cursor + 1].kind == Kind::Symbol(".");
            self.cursor += 2;
            ExpressionKind::New(Box::new(self.array_initializer(readonly, false)?))
        } else {
            self.cursor += 1;
            ExpressionKind::New(Box::new(self.unary()?))
        };
        self.primary_node(kind, location)
    }
}
