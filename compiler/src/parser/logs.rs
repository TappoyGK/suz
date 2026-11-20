//! log文とpanic文のテンプレートを解析し、出力文のASTを作る。
//!
//! 利用元: [`crate::parser::statements`]。
//! 主な対象規則: `[log.level]`、`[log.syntax]`、`[symbol.interpolation]`。

use super::Parser;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Statement, TemplatePart},
    lexer::{
        self, Kind,
        templates::{self, Segment},
    },
};

impl Parser<'_, '_> {
    pub(super) fn log(&mut self) -> Result<Statement, Diagnostic> {
        let location = self.location();
        self.cursor += 1;
        let level = match self.token().kind {
            Kind::Ident("debug") => crate::ast::LogLevel::Debug,
            Kind::Keyword("error") => crate::ast::LogLevel::Error,
            Kind::Ident("info") => crate::ast::LogLevel::Info,
            Kind::Ident("warn") => crate::ast::LogLevel::Warn,
            _ => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("log.level", "expected debug, info, warn, or error"),
                ));
            }
        };
        self.cursor += 1;
        let Kind::Template(raw) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("log.syntax", "expected a log template"),
            ));
        };
        let parts = self.output_template(raw)?;
        Ok(Statement::Log {
            parts,
            location,
            level,
        })
    }

    pub(super) fn panic(&mut self) -> Result<Statement, Diagnostic> {
        let location = self.location();
        self.cursor += 1;
        let Kind::Template(raw) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "panic.stmt",
                    "expected a panic template without parentheses"
                ),
            ));
        };
        if raw == "``" {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("panic.stmt", "panic requires a non-empty template"),
            ));
        }
        let parts = self.output_template(raw)?;
        // テンプレートの検査・走査はlogと共通。Panicレベルが終了する文であることを表す。
        Ok(Statement::Log {
            parts,
            location,
            level: crate::ast::LogLevel::Panic,
        })
    }

    // 出力用テンプレートだけを制限し、値を作るsymbolテンプレートは通常の式を許可する。
    fn output_template(&mut self, raw: &str) -> Result<Vec<TemplatePart<Expression>>, Diagnostic> {
        fn readable(value: &Expression) -> bool {
            match &value.kind {
                ExpressionKind::Name(_) | ExpressionKind::Variant { .. } => true,
                ExpressionKind::Field { target, .. } => readable(target),
                _ => false,
            }
        }
        let parts = self.nested(self.location(), |p| p.template(raw))?;
        for part in &parts {
            if let TemplatePart::Value(value) = part
                && !readable(value)
            {
                return Err(value.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "output.embed-value",
                        "output interpolation requires a name or its field access"
                    ),
                ));
            }
        }
        Ok(parts)
    }

    pub(super) fn template(
        &mut self,
        raw: &str,
    ) -> Result<Vec<TemplatePart<Expression>>, Diagnostic> {
        let token = self.token();
        let mut parts = Vec::new();
        for segment in templates::scan(raw, token.line, token.column)?.1 {
            match segment {
                Segment::Text(text) => parts.push(TemplatePart::Text(templates::decode(text))),
                Segment::Expression { source, column } => {
                    if self.depth == super::nesting::LIMIT {
                        return Err(super::nesting::error(self.limit_location.unwrap_or(
                            crate::ast::Location {
                                line: token.line,
                                column: column - 2,
                            },
                        )));
                    }
                    let mut tokens = lexer::lex(source).map_err(|mut error| {
                        error.line = token.line;
                        error.column += column - 1;
                        error
                    })?;
                    for item in &mut tokens {
                        item.line = token.line;
                        item.column += column - 1;
                    }
                    let mut parser = Parser {
                        applications: self.applications.clone(),
                        type_parameters: self.type_parameters.clone(),
                        pending_needs: Vec::new(),
                        need_groups: self.need_groups.clone(),
                        generic_functions: self.generic_functions.clone(),
                        allows: Vec::new(),
                        tokens: &tokens,
                        enums: self.enums,
                        structs: self.structs,
                        fnshapes: self.fnshapes,
                        fnshape_names: self.fnshape_names.clone(),
                        restricted_condition: self.restricted_condition,
                        cursor: 0,
                        constant_expression: self.constant_expression,
                        in_value: self.in_value,
                        depth: self.depth,
                        limit_location: self.limit_location,
                    };
                    if parser.token().kind == Kind::End {
                        return Err(parser.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("symbol.interpolation", "empty interpolation"),
                        ));
                    }
                    let value = parser.expression()?;
                    if parser.token().kind != Kind::End {
                        return Err(parser.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "symbol.interpolation",
                                "expected one interpolation expression"
                            ),
                        ));
                    }
                    self.allows.extend(parser.allows);
                    parts.push(TemplatePart::Value(value));
                }
            }
        }
        self.cursor += 1;
        Ok(parts)
    }
}
