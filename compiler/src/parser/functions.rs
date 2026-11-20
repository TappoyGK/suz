//! 関数の引数・戻り値型・本体を解析し、mainの形と型名を検査する。
//!
//! 利用元: [`crate::parser`]。
//! 主な対象規則: `[fn.parameter]`、`[fn.return-type]`、`[prog.main]`、`[src.ident]`、`[src.list-comma]`、`[src.trailing-comma]`。

use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Function, Method, Parameter, Type},
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

    pub(super) fn parameter_name(&mut self) -> Result<(String, crate::ast::Location), Diagnostic> {
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
        Ok((name, location))
    }

    pub(super) fn parameter(&mut self) -> Result<Parameter, Diagnostic> {
        // keep: 型は通常の名前。keep 名前: 型だけをextern専用の修飾語として診断する。
        if self.token().kind == Kind::Ident("keep")
            && self
                .tokens
                .get(self.cursor + 1)
                .is_some_and(|t| matches!(t.kind, Kind::Ident(_)))
        {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "ffi.keep",
                    "keep is only allowed on extern typed C pointer parameters"
                ),
            ));
        }
        let (name, location) = self.parameter_name()?;
        let ty = self.function_parameter_type()?;
        if ty == Type::Void {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("type.void", "parameters cannot have type void"),
            ));
        }
        Ok(Parameter { name, ty, location })
    }

    pub(super) fn parse_type(&mut self) -> Result<Type, Diagnostic> {
        let location = self.location();
        let ty = self.type_atom()?;
        if matches!(ty, Type::Function(_)) {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "fn.type-call",
                    "fn types are only allowed on function parameters and fnshape parameters; use static fn here"
                ),
            ));
        }
        Ok(ty)
    }

    /// 束縛とフィールドの型にはエラー集合を保存できない。
    pub(super) fn stored_type(&mut self) -> Result<Type, Diagnostic> {
        let ty = self.parse_type()?;
        self.reject_error_spec()?;
        Ok(ty)
    }

    pub(super) fn function_parameter_type(&mut self) -> Result<Type, Diagnostic> {
        let ty = self.type_atom()?;
        self.reject_error_spec()?;
        Ok(ty)
    }

    fn reject_error_spec(&self) -> Result<(), Diagnostic> {
        if self.token().kind == Kind::Keyword("or") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "err.no-first-class",
                    "error specifications are only allowed on function and fnshape return types"
                ),
            ));
        }
        Ok(())
    }

    // 戻り値に続くエラー指定と、変換に続くorハンドラーは呼び出し側で解析する。
    pub(super) fn type_atom(&mut self) -> Result<Type, Diagnostic> {
        let mut layers = Vec::new();
        while self.token().kind == Kind::Keyword("ptr") {
            if layers.len() == 256 {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("arena.ptr-type", "pointer type nesting exceeds 256"),
                ));
            }
            self.cursor += 1;
            let nullable = self.token().kind == Kind::Symbol("?");
            if nullable {
                self.cursor += 1;
            }
            layers.push(nullable);
        }
        let mut ty = self.type_leaf()?;
        if !self.type_parameters.is_empty() && ty.generic() {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.numeric-existing",
                    "named type parameters cannot be combined with int or float"
                ),
            ));
        }
        if !layers.is_empty() && (ty == Type::Void || matches!(ty, Type::Function(_))) {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("arena.ptr-type", "pointer requires a stored value type"),
            ));
        }
        for nullable in layers.into_iter().rev() {
            ty = ty.pointer().with_nullable(nullable);
        }
        Ok(ty)
    }

    fn type_leaf(&mut self) -> Result<Type, Diagnostic> {
        if self.token().kind == Kind::Symbol("*") {
            self.cursor += 1;
            if self.token().kind != Kind::Ident("const") {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("ffi.pointer-type", "typed C pointers are only allowed on extern parameters; use ptr T for Suz pointers"),
                ));
            }
            self.cursor += 1;
            if self.token().kind != Kind::TypeName("void") {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("ffi.pointer-type", "typed C pointers are only allowed on extern parameters; use ptr T for Suz pointers"),
                ));
            }
            self.cursor += 1;
            return Ok(Type::CPointer);
        }

        let persistent = self.token().kind == Kind::Keyword("static");
        if persistent {
            self.cursor += 1;
            if self.token().kind != Kind::Keyword("fn") {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "fn.type-mark",
                        "static must be followed by fn and a declared fnshape name"
                    ),
                ));
            }
        }
        let marked = self.token().kind == Kind::Keyword("fn");
        if marked {
            self.cursor += 1;
        }
        if let Kind::Ident(name) = self.token().kind
            && let Some(decl) = self.fnshapes.iter().find(|d| d.name == name)
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
            return Ok(if persistent {
                Type::StaticFunction(decl.type_id)
            } else {
                Type::Function(decl.type_id)
            });
        }
        if marked {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("fn.type-mark", "fn requires a declared fnshape name"),
            ));
        }
        if let Kind::Ident(name) = self.token().kind
            && let Some(index) = self.type_parameters.iter().position(|p| p.name == name)
        {
            self.cursor += 1;
            return Ok(Type::Parameter(index));
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
        let (Kind::Ident(name) | Kind::TypeName(name)) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("src.ident", "expected a function name"),
            ));
        };
        let owner_name = name;
        let ordinary_name = matches!(self.token().kind, Kind::Ident(_));
        self.cursor += 1;
        self.type_parameters = if self.token().kind == Kind::Symbol("[") {
            self.generic_parameters()?
        } else {
            Vec::new()
        };
        let (mut name, method) = if matches!(self.token().kind, Kind::Symbol("." | "#")) {
            let mutable = self.token().kind == Kind::Symbol("#");
            let owner = self
                .structs
                .iter()
                .position(|s| s.name == owner_name)
                .map(Type::Struct)
                .or_else(|| {
                    self.enums
                        .iter()
                        .find(|e| e.name == owner_name && !e.ty.is_error)
                        .map(|e| Type::Enum(e.ty))
                })
                .ok_or_else(|| {
                    location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "method.target-type",
                            "methods require a struct or enum declared in this source"
                        ),
                    )
                })?;
            let owner_token = &self.tokens[self.cursor - 1];
            let separator = self.token();
            self.cursor += 1;
            let (method_name, operator) = match self.token().kind {
                Kind::Ident(name) => (name, None),
                Kind::Symbol(
                    op @ ("+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>" | "~" | "<"
                    | "<=" | ">" | ">="),
                )
                | Kind::Keyword(op @ ("not" | "len")) => {
                    let symbol = match op {
                        "+" => "+",
                        "-" => "-",
                        "*" => "*",
                        "/" => "/",
                        "%" => "%",
                        "&" => "&",
                        "|" => "|",
                        "^" => "^",
                        "<<" => "<<",
                        ">>" => ">>",
                        "~" => "~",
                        "not" => "not",
                        "len" => "len",
                        "<" => "<",
                        "<=" => "<=",
                        ">" => ">",
                        ">=" => ">=",
                        _ => unreachable!(),
                    };
                    (op, Some(symbol))
                }
                Kind::Symbol("==" | "!=") => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "operator.no-equality",
                            "equality operators cannot be defined"
                        ),
                    ));
                }
                _ => {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "operator.method",
                            "expected a method name or a supported operator"
                        ),
                    ));
                }
            };
            if owner_token.span.end != separator.span.start
                || separator.span.end != self.token().span.start
            {
                return Err(self.error(ErrorKind::InvalidSource, crate::diagnostic!("method.decl", "method declarations cannot separate the type, receiver symbol and method name")));
            }
            if operator.is_none() {
                super::names::check(super::names::Shape::Value, method_name, self.location())?;
            }
            let name = format!("{owner_name}.{method_name}");
            self.cursor += 1;
            (
                name,
                Some(Method {
                    owner,
                    mutable,
                    operator,
                }),
            )
        } else {
            if !ordinary_name {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("src.ident", "expected a function name"),
                ));
            }
            super::names::check(super::names::Shape::Value, owner_name, location)?;
            (owner_name.to_owned(), None)
        };
        if method.is_some() && !self.type_parameters.is_empty() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.receiver-parameters",
                    "receiver parameters must match the struct declaration"
                ),
            ));
        }
        let needs = self.declaration_needs()?;
        if name == "main" && !self.type_parameters.is_empty() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("generic.parameters", "main cannot have type parameters"),
            ));
        }
        let (mut parameters, result) = if name == "main" {
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
        if let Some(method) = method {
            if let Some(operator) = method.operator {
                if method.mutable
                    || !match operator {
                        "~" | "not" | "len" => parameters.is_empty(),
                        "-" => parameters.len() <= 1,
                        _ => parameters.len() == 1,
                    }
                    || result == Type::Void
                    || (matches!(operator, "<" | "<=" | ">" | ">=" | "not") && result != Type::Bool)
                    || (operator == "len" && result != Type::Integer(crate::ast::IntegerType::Len))
                {
                    return Err(location.error(ErrorKind::InvalidSource, crate::diagnostic!(
                        "operator.method", "operator methods require a read receiver and a value result; binary operators take one parameter, unary operators take none, comparisons and not return bool, len returns len_t")));
                }
                // 引数のない単項と二項を区別する。二項の数値は種類ごと、その他は型ごとに一意にする。
                let key = if parameters.is_empty() {
                    "unary".into()
                } else if parameters[0].ty.is_integer() {
                    "integer".into()
                } else if parameters[0].ty.is_float() {
                    "float".into()
                } else {
                    format!("{:?}", parameters[0].ty)
                };
                name.push_str(&format!("@{key}"));
            }
            parameters.insert(
                0,
                Parameter {
                    name: "self".into(),
                    ty: method.owner,
                    location,
                },
            );
        }
        let errors = self.error_spec()?;
        if method.is_some_and(|m| m.operator.is_some()) && !errors.is_empty() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("operator.method", "operator methods cannot return errors"),
            ));
        }
        if name == "main" && !errors.is_empty() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("prog.main", "main cannot return errors"),
            ));
        }
        let (statements, end) = self.block()?;
        Ok(Function {
            type_parameters: std::mem::take(&mut self.type_parameters),
            needs,
            method,
            arena: false,
            keyword_location,
            name,
            location,
            parameters,
            result,
            errors,
            statements,
            end,
        })
    }
}

impl Parser<'_, '_> {
    /// 型全体の指定を展開し、重複を拒否してから順序に依存しない集合にする。
    pub(super) fn error_spec(&mut self) -> Result<Vec<crate::ast::ErrorVariant>, Diagnostic> {
        let mut errors = std::collections::BTreeSet::new();
        if self.token().kind != Kind::Keyword("or") {
            return Ok(Vec::new());
        }
        self.cursor += 1;
        loop {
            let location = self.location();
            let Kind::Ident(name) = self.token().kind else {
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("err.fn-return", "expected an error type"),
                ));
            };
            let declaration = self
                .enums
                .iter()
                .find(|d| d.name == name && d.ty.is_error)
                .ok_or_else(|| {
                    self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("err.fn-return", "expected an error type"),
                    )
                })?;
            self.cursor += 1;
            let variants = if self.token().kind == Kind::Symbol(".") {
                self.cursor += 1;
                let Kind::Ident(name) = self.token().kind else {
                    return Err(self.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("err.fn-return", "expected an error variant"),
                    ));
                };
                let index = declaration
                    .variants
                    .iter()
                    .position(|v| v == name)
                    .ok_or_else(|| {
                        self.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("err.fn-return", "unknown error variant"),
                        )
                    })?;
                self.cursor += 1;
                index..index + 1
            } else {
                0..declaration.ty.count
            };
            for variant in variants {
                if !errors.insert(crate::ast::ErrorVariant {
                    type_id: declaration.ty.id,
                    variant,
                }) {
                    return Err(location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("err.fn-return", "duplicate error variant"),
                    ));
                }
            }
            if self.token().kind != Kind::Symbol(",") {
                break;
            }
            self.cursor += 1;
        }
        Ok(errors.into_iter().collect())
    }
}
