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
                    "a list comma cannot follow a statement newline [syntax.grammar]",
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
                        "invalid trailing comma for the list layout [src.trailing-comma]",
                    ));
                }
                self.cursor += 1;
                return Ok(items);
            }
            if !comma {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "list elements require commas [src.list-comma]",
                ));
            }
        }
    }

    fn parameter(&mut self) -> Result<Parameter, Diagnostic> {
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "expected a parameter name [src.ident]",
            ));
        };
        super::names::check(super::names::Shape::Value, name, location)?;
        let name = name.to_owned();
        self.cursor += 1;
        if self.token().kind != Kind::Symbol(":") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "parameter type is required [fn.parameter]",
            ));
        }
        self.cursor += 1;
        let ty = self.parse_type()?;
        if ty == Type::Void {
            return Err(location.error(
                ErrorKind::InvalidSource,
                "parameters cannot have type void [type.void]",
            ));
        }
        Ok(Parameter { name, ty, location })
    }

    pub(super) fn parse_type(&mut self) -> Result<Type, Diagnostic> {
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
                        &format!("unknown type '{name}' [type.name]"),
                    )
                })?,
            _ => {
                return Err(
                    self.error(ErrorKind::InvalidSource, "expected a type [syntax.grammar]")
                );
            }
        };
        self.cursor += 1;
        Ok(ty)
    }

    pub(super) fn function(&mut self, has_main: bool) -> Result<Function, Diagnostic> {
        self.cursor += 1;
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "expected a function name [src.ident]",
            ));
        };
        super::names::check(super::names::Shape::Value, name, location)?;
        let name = name.to_owned();
        self.cursor += 1;
        let (parameters, result) = if name == "main" {
            if has_main {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "main must be declared exactly once [prog.main]",
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
                        "main must have the signature fn main() -> u8 [prog.main]",
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
                    "function return type is required [fn.return-type]",
                ));
            }
            self.cursor += 1;
            (parameters, self.parse_type()?)
        };
        let (statements, end) = self.block()?;
        Ok(Function {
            name,
            location,
            parameters,
            result,
            statements,
            end,
        })
    }
}
