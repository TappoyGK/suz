//! ログレベルとテンプレートの構文を解析し、ログ文のASTを作る。
//!
//! 利用元: [`crate::parser::statements`]。
//! 主な対象規則: `[log.level]`、`[log.syntax]`、`[symbol.interpolation]`。

use super::Parser;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, Statement, TemplatePart},
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
            Kind::Ident("info") => crate::ast::LogLevel::Info,
            Kind::Ident("warn") => crate::ast::LogLevel::Warn,
            _ => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("log.level", "expected info or warn"),
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
        let parts = self.nested(self.location(), |p| p.template(raw))?;
        Ok(Statement::Log {
            parts,
            location,
            level,
        })
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
                        allows: Vec::new(),
                        tokens: &tokens,
                        enums: self.enums,
                        structs: self.structs,
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
