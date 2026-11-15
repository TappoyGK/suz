mod branches;
mod enums;
mod expressions;
mod functions;
mod logs;
mod names;
mod statements;
mod structs;

use crate::{
    Diagnostic, ErrorKind,
    ast::{ConstDeclaration, Location, Program, Type},
    lexer::{Kind, Token},
};

enum FixedToken {
    Keyword(&'static str),
    Symbol(&'static str),
}

struct Parser<'a, 's> {
    tokens: &'a [Token<'s>],
    enums: &'a [crate::ast::EnumDeclaration],
    structs: &'a [crate::ast::StructDeclaration],
    restricted_condition: bool,
    cursor: usize,
    constant_expression: bool,
    in_value: bool,
}

impl<'a, 's> Parser<'a, 's> {
    fn token(&self) -> &'a Token<'s> {
        &self.tokens[self.cursor]
    }

    fn location(&self) -> Location {
        Location {
            line: self.token().line,
            column: self.token().column,
        }
    }

    fn error(&self, kind: ErrorKind, message: &str) -> Diagnostic {
        // Valid range operators are consumed by the for header. If parsing
        // fails at one, report its restricted use instead of a missing token.
        let message = if kind == ErrorKind::InvalidSource
            && matches!(self.token().kind, Kind::Symbol("..<" | "..="))
        {
            "range notation is only allowed in for or rev [for.range]"
        } else {
            message
        };
        self.location().error(kind, message)
    }

    fn expect(&mut self, expected: FixedToken) -> Result<(), Diagnostic> {
        let (kind, text) = match expected {
            FixedToken::Keyword(text) => (Kind::Keyword(text), text),
            FixedToken::Symbol(text) => (Kind::Symbol(text), text),
        };
        if self.token().kind != kind {
            return Err(self.error(
                ErrorKind::InvalidSource,
                &format!("expected '{text}' [syntax.grammar]"),
            ));
        }
        self.cursor += 1;
        Ok(())
    }

    fn newlines(&mut self) {
        while self.token().kind == Kind::Newline {
            self.cursor += 1;
        }
    }

    fn constant(&mut self) -> Result<ConstDeclaration, Diagnostic> {
        self.expect(FixedToken::Keyword("const"))?;
        let location = self.location();
        let name = match self.token().kind {
            Kind::Ident(name) => name.to_owned(),
            _ => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    "expected a non-reserved constant name [src.ident]",
                ));
            }
        };
        names::check(names::Shape::Constant, &name, location)?;
        self.cursor += 1;
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
                "a constant requires an initializer [const.decl]",
            ));
        }
        self.cursor += 1;
        self.constant_expression = true;
        let initializer = self.expression()?;
        self.constant_expression = false;
        Ok(ConstDeclaration {
            name,
            location,
            annotation,
            initializer,
        })
    }
}

pub(crate) fn parse(tokens: &[Token<'_>]) -> Result<Program, Diagnostic> {
    let enums = enums::collect(tokens)?;
    let structs = structs::collect(tokens, &enums)?;
    let mut p = Parser {
        enums: &enums,
        structs: &structs,
        restricted_condition: false,
        tokens,
        cursor: 0,
        constant_expression: false,
        in_value: false,
    };
    let mut constants = Vec::new();
    let mut functions = Vec::new();
    let mut main = false;
    p.newlines();
    while p.token().kind != Kind::End {
        match p.token().kind {
            Kind::Keyword("const") => constants.push(p.constant()?),
            Kind::Keyword("struct") => {
                p.cursor = structs
                    .iter()
                    .find(|decl| decl.start == p.cursor)
                    .unwrap()
                    .end;
            }
            Kind::Keyword("enum") => {
                p.cursor = enums
                    .iter()
                    .find(|decl| decl.start == p.cursor)
                    .unwrap()
                    .end;
            }
            Kind::Keyword("fn") => {
                let function = p.function(main)?;
                main |= function.name == "main";
                functions.push(function);
            }
            Kind::Keyword("return") => {
                return Err(p.error(
                    ErrorKind::InvalidSource,
                    "return requires an enclosing function [fn.return]",
                ));
            }
            Kind::Keyword("fix" | "var") => {
                return Err(p.error(
                    ErrorKind::InvalidSource,
                    "variables must be declared inside a block [decl.location]",
                ));
            }
            _ => {
                return Err(p.error(
                    ErrorKind::InvalidSource,
                    "expected a const, fn, struct or enum declaration [decl.top-level]",
                ));
            }
        }
        if !matches!(p.token().kind, Kind::Newline | Kind::End) {
            return Err(p.error(
                ErrorKind::InvalidSource,
                "declarations must be separated by a newline [src.statement-end]",
            ));
        }
        p.newlines();
    }
    if !main {
        return Err(p.error(
            ErrorKind::InvalidSource,
            "main must be declared exactly once [prog.main]",
        ));
    }
    Ok(Program {
        structs,
        enums,
        constants,
        functions,
    })
}
