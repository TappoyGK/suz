//! 字句解析済みトークンからASTを作る。宣言を収集し、文・式・型注釈の構文を解析する。
//!
//! 利用元: [`crate::analyze`]。
//! 主な対象規則: `[const.decl]`、`[decl.location]`、`[decl.top-level]`、`[fn.return]`、`[for.range]`、`[prog.main]`。

mod allows;
mod branches;
mod closures;
mod enums;
mod errors;
mod expressions;
mod function_types;
mod functions;
mod logs;
mod names;
pub(crate) mod nesting;
mod statements;
mod structs;

use crate::{
    Diagnostic, ErrorKind,
    ast::{Location, Program, TopLevelBinding, Type},
    lexer::{Kind, Token},
};

enum FixedToken {
    Keyword(&'static str),
    Symbol(&'static str),
}

struct Parser<'a, 's> {
    allows: Vec<crate::warnings::AllowScope>,
    tokens: &'a [Token<'s>],
    enums: &'a [crate::ast::EnumDeclaration],
    structs: &'a [crate::ast::StructDeclaration],
    fnshapes: &'a [crate::ast::FnShapeDeclaration],
    restricted_condition: bool,
    cursor: usize,
    constant_expression: bool,
    in_value: bool,
    depth: usize,
    limit_location: Option<Location>,
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

    fn error(&self, kind: ErrorKind, message: crate::DiagnosticMessage) -> Diagnostic {
        // 有効な範囲演算子はforのヘッダーで消費する。それ以外で解析に失敗した場合は、
        // トークンの欠落ではなく、使用場所の制約を報告する。
        let message = if kind == ErrorKind::InvalidSource
            && matches!(self.token().kind, Kind::Symbol("..<" | "..="))
        {
            crate::diagnostic!("for.range", "range notation is only allowed in for or rev")
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
                crate::diagnostic!("syntax.grammar", "expected '{text}'"),
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

    fn top_level_binding(&mut self) -> Result<TopLevelBinding, Diagnostic> {
        let start = self.location();
        let mutable = self.token().kind == Kind::Keyword("var");
        self.cursor += 1;
        let location = self.location();
        let name = match self.token().kind {
            Kind::Ident(name) => name.to_owned(),
            _ => {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "src.ident",
                        "expected a non-reserved top-level binding name"
                    ),
                ));
            }
        };
        names::check(names::Shape::TopLevelBinding, &name, location)?;
        self.cursor += 1;
        let annotation = if self.token().kind == Kind::Symbol(":") {
            self.cursor += 1;
            let ty = self.stored_type()?;
            if ty == Type::Void {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("type.void", "bindings cannot have type void"),
                ));
            }
            Some(ty)
        } else {
            None
        };
        if self.token().kind != Kind::Symbol("=") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    if mutable { "top.var" } else { "const.decl" },
                    "a top-level binding requires an initializer"
                ),
            ));
        }
        self.cursor += 1;
        self.constant_expression = true;
        let initializer = self.expression()?;
        self.constant_expression = false;
        self.allow_clause(start, allows::expression_contains_block(&initializer))?;
        Ok(TopLevelBinding {
            mutable,
            name,
            location,
            annotation,
            initializer,
        })
    }
}

pub(crate) fn parse(tokens: &[Token<'_>]) -> Result<Program, Diagnostic> {
    let enums = enums::collect(tokens)?;
    let mut fnshapes = function_types::headers(tokens)?;
    let mut structs = structs::collect(tokens, &enums, &fnshapes)?;
    let function_types = function_types::resolve(tokens, &enums, &mut structs, &mut fnshapes)?;
    let mut p = Parser {
        allows: Vec::new(),
        enums: &enums,
        structs: &structs,
        fnshapes: &fnshapes,
        restricted_condition: false,
        tokens,
        cursor: 0,
        constant_expression: false,
        in_value: false,
        depth: 0,
        limit_location: None,
    };
    let mut bindings = Vec::new();
    let mut functions = Vec::new();
    let mut main = false;
    p.newlines();
    while p.token().kind != Kind::End {
        match p.token().kind {
            Kind::Keyword("fix" | "var") => bindings.push(p.top_level_binding()?),
            Kind::Keyword("fnshape") => {
                p.cursor = fnshapes.iter().find(|d| d.start == p.cursor).unwrap().end;
            }
            Kind::Keyword("struct") => {
                p.cursor = structs
                    .iter()
                    .find(|decl| decl.start == p.cursor)
                    .unwrap()
                    .end;
            }
            Kind::Keyword("enum" | "error") => {
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
                    crate::diagnostic!("fn.return", "return requires an enclosing function"),
                ));
            }
            _ => {
                return Err(p.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "decl.top-level",
                        "expected a fix, var, fn, fnshape, struct or enum declaration"
                    ),
                ));
            }
        }
        if !matches!(p.token().kind, Kind::Newline | Kind::End) {
            return Err(p.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "src.statement-end",
                    "declarations must be separated by a newline"
                ),
            ));
        }
        p.newlines();
    }
    if !main {
        return Err(p.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!("prog.main", "main must be declared exactly once"),
        ));
    }
    Ok(Program {
        allows: p.allows,
        fnshapes,
        function_types,
        structs,
        enums,
        bindings,
        functions,
    })
}
