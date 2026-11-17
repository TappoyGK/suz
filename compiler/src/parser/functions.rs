//! 関数の引数・戻り値型・本体を解析し、mainの形と型名を検査する。
//!
//! 利用元: [`crate::parser`]。
//! 主な対象規則: `[fn.parameter]`、`[fn.return-type]`、`[prog.main]`、`[src.ident]`、`[src.list-comma]`、`[src.trailing-comma]`。

use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Function, Parameter, Type},
    lexer::Kind,
};

impl Parser<'_, '_> {
    pub(super) fn list<T>(
        &mut self,
        item: fn(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<Vec<T>, Diagnostic> {
        self.delimited_list("(", ")", item)
    }

    pub(super) fn delimited_list<T>(
        &mut self,
        opening: &'static str,
        closing: &'static str,
        item: fn(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<Vec<T>, Diagnostic> {
        let open = self.location();
        self.expect(FixedToken::Symbol(opening))?;
        self.newlines();
        let mut items = Vec::new();
        if self.token().kind == Kind::Symbol(closing) {
            self.cursor += 1;
            return Ok(items);
        }
        loop {
            items.push(item(self)?);
            let end_line = self.tokens[self.cursor - 1].line;
            let separated = self.token().kind == Kind::Newline;
            self.newlines();
            let comma = self.token().kind == Kind::Symbol(",");
            if separated && comma {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "syntax.grammar",
                        "a list comma cannot follow a statement newline"
                    ),
                ));
            }
            if comma {
                self.cursor += 1;
            }
            self.newlines();
            if self.token().kind == Kind::Symbol(closing) {
                let close = self.location();
                if (open.line == close.line && comma)
                    || (open.line != close.line
                        && (closing != ")" || end_line != close.line)
                        && !comma)
                {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "src.trailing-comma",
                            "invalid trailing comma for the list layout"
                        ),
                    ));
                }
                self.cursor += 1;
                return Ok(items);
            }
            if !comma {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("src.list-comma", "list elements require commas"),
                ));
            }
        }
    }

    fn parameter(&mut self) -> Result<Parameter, Diagnostic> {
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("src.ident", "expected a parameter name"),
            ));
        };
        super::names::check(super::names::Shape::Value, name, location)?;
        let name = name.to_owned();
        self.cursor += 1;
        if self.token().kind != Kind::Symbol(":") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("fn.parameter", "parameter type is required"),
            ));
        }
        self.cursor += 1;
        let ty = self.parse_type()?;
        if ty == Type::Void {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("type.void", "parameters cannot have type void"),
            ));
        }
        Ok(Parameter { name, ty, location })
    }

    pub(super) fn parse_type(&mut self) -> Result<Type, Diagnostic> {
        let marked = self.token().kind == Kind::Keyword("fn");
        if marked {
            self.cursor += 1;
        }
        if let Kind::Ident(name) = self.token().kind
            && let Some(decl) = self.fntypes.iter().find(|d| d.name == name)
        {
            if !marked {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "fn.type-mark",
                        "function value types require fn before the type name"
                    ),
                ));
            }
            self.cursor += 1;
            return Ok(Type::Function(decl.type_id));
        }
        if marked {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("fn.type-mark", "fn requires a declared fntype name"),
            ));
        }
        let ty = match self.token().kind {
            Kind::TypeName(name) => Type::from_name(name).unwrap(),
            Kind::Ident(name) => self
                .enums
                .iter()
                .find(|decl| decl.name == name)
                .map(|decl| Type::Enum(decl.ty))
                .or_else(|| {
                    self.structs
                        .iter()
                        .position(|decl| decl.name == name)
                        .map(Type::Struct)
                })
                .ok_or_else(|| {
                    self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("type.name", "unknown type '{name}'"),
                    )
                })?,
            _ => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("syntax.grammar", "expected a type"),
                ));
            }
        };
        self.cursor += 1;
        Ok(ty)
    }

    pub(super) fn function(&mut self, has_main: bool) -> Result<Function, Diagnostic> {
        let keyword_location = self.location();
        self.cursor += 1;
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("src.ident", "expected a function name"),
            ));
        };
        super::names::check(super::names::Shape::Value, name, location)?;
        let name = name.to_owned();
        self.cursor += 1;
        let (parameters, result) = if name == "main" {
            if has_main {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("prog.main", "main must be declared exactly once"),
                ));
            }
            for token in [
                Kind::Symbol("("),
                Kind::Symbol(")"),
                Kind::Symbol("->"),
                Kind::TypeName("u8"),
            ] {
                if self.token().kind != token {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "prog.main",
                            "main must have the signature fn main() -> u8"
                        ),
                    ));
                }
                self.cursor += 1;
            }
            (Vec::new(), Type::from_name("u8").unwrap())
        } else {
            let parameters = self.list(Self::parameter)?;
            if self.token().kind != Kind::Symbol("->") {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("fn.return-type", "function return type is required"),
                ));
            }
            self.cursor += 1;
            (parameters, self.parse_type()?)
        };
        let (statements, end) = self.block()?;
        Ok(Function {
            keyword_location,
            name,
            location,
            parameters,
            result,
            statements,
            end,
        })
    }
}
