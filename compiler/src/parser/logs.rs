use super::Parser;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Statement, TemplatePart},
    lexer::{
        self, Kind,
        templates::{self, Segment},
    },
};

impl Parser<'_, '_> {
    pub(super) fn log(&mut self) -> Result<Statement, Diagnostic> {
        let location = self.location();
        self.cursor += 1;
        if self.token().kind != Kind::Ident("info") {
            return Err(self.error(ErrorKind::InvalidSource, "expected log info [log.syntax]"));
        }
        self.cursor += 1;
        let Kind::Template(raw) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "expected a log template [log.syntax]",
            ));
        };
        let token = self.token();
        let mut parts = Vec::new();
        for segment in templates::scan(raw, token.line, token.column)?.1 {
            match segment {
                Segment::Text(text) => parts.push(TemplatePart::Text(templates::decode(text))),
                Segment::Expression { source, column } => {
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
                        tokens: &tokens,
                        enums: self.enums,
                        structs: self.structs,
                        restricted_condition: self.restricted_condition,
                        cursor: 0,
                        constant_expression: false,
                        in_value: false,
                    };
                    if parser.token().kind == Kind::End {
                        return Err(parser.error(
                            ErrorKind::InvalidSource,
                            "empty interpolation [log.interpolation]",
                        ));
                    }
                    let value = parser.expression()?;
                    if parser.token().kind != Kind::End {
                        return Err(parser.error(
                            ErrorKind::InvalidSource,
                            "expected one interpolation expression [log.interpolation]",
                        ));
                    }
                    parts.push(TemplatePart::Value(value));
                }
            }
        }
        self.cursor += 1;
        Ok(Statement::Log { parts, location })
    }
}
