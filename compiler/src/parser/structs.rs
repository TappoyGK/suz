use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{EnumDeclaration, Expression, FieldInitializer, StructDeclaration, StructField, Type},
    lexer::{Kind, Token},
};
use std::collections::BTreeSet;

impl Parser<'_, '_> {
    pub(super) fn condition(&mut self) -> Result<Expression, Diagnostic> {
        self.restricted_expression(true)
    }

    pub(super) fn parenthesized_expression(&mut self) -> Result<Expression, Diagnostic> {
        self.restricted_expression(false)
    }

    fn restricted_expression(&mut self, restricted: bool) -> Result<Expression, Diagnostic> {
        let previous = std::mem::replace(&mut self.restricted_condition, restricted);
        let result = self.expression();
        self.restricted_condition = previous;
        result
    }

    fn field_declaration(&mut self) -> Result<StructField, Diagnostic> {
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("struct.field", "expected a field name"),
            ));
        };
        super::names::check(super::names::Shape::Value, name, location)?;
        let name = name.to_owned();
        self.cursor += 1;
        if self.token().kind != Kind::Symbol(":") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("struct.field", "field type is required"),
            ));
        }
        self.cursor += 1;
        let ty = self.parse_type()?;
        if ty == Type::Void {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("type.void", "field type cannot be void"),
            ));
        }
        Ok(StructField { name, ty, location })
    }
    pub(super) fn field_initializer(&mut self) -> Result<FieldInitializer, Diagnostic> {
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("struct.literal", "expected a named field initializer"),
            ));
        };
        let name = name.to_owned();
        self.cursor += 1;
        if self.token().kind != Kind::Symbol(":") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("struct.literal", "field initializer requires ':'"),
            ));
        }
        self.cursor += 1;
        Ok(FieldInitializer {
            name,
            location,
            value: self.expression()?,
        })
    }
}

pub(super) fn collect(
    tokens: &[Token<'_>],
    enums: &[EnumDeclaration],
) -> Result<Vec<StructDeclaration>, Diagnostic> {
    let mut p = Parser {
        allows: Vec::new(),
        tokens,
        enums,
        structs: &[],
        restricted_condition: false,
        cursor: 0,
        constant_expression: false,
        in_value: false,
    };
    let mut headers = Vec::<StructDeclaration>::new();
    let mut depth = 0usize;
    while p.token().kind != Kind::End {
        if depth == 0 && p.token().kind == Kind::Keyword("struct") {
            let start = p.cursor;
            p.cursor += 1;
            let location = p.location();
            let Kind::Ident(name) = p.token().kind else {
                return Err(p.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("src.ident", "expected a struct name"),
                ));
            };
            super::names::check(super::names::Shape::Type, name, location)?;
            let name = name.to_owned();
            if enums.iter().any(|decl| decl.name == name)
                || headers.iter().any(|decl| decl.name == name)
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("decl.duplicate", "duplicate type name"),
                ));
            }
            p.cursor += 1;
            p.expect(FixedToken::Symbol("{"))?;
            let mut braces = 1;
            while braces != 0 {
                match p.token().kind {
                    Kind::Symbol("{") => braces += 1,
                    Kind::Symbol("}") => braces -= 1,
                    Kind::End => {
                        return Err(p.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("struct.field", "unclosed struct declaration"),
                        ));
                    }
                    _ => {}
                }
                p.cursor += 1;
            }
            headers.push(StructDeclaration {
                name,
                location,
                fields: Vec::new(),
                start,
                end: p.cursor,
            });
            continue;
        }
        match p.token().kind {
            Kind::Symbol("{" | "(" | "[") => depth += 1,
            Kind::Symbol("}" | ")" | "]") => depth = depth.saturating_sub(1),
            _ => {}
        }
        p.cursor += 1;
    }
    p.structs = &headers;
    let mut declarations = headers.clone();
    for decl in &mut declarations {
        p.cursor = decl.start + 2;
        decl.fields = p.delimited_list("{", "}", Parser::field_declaration)?;
        if decl.fields.is_empty() {
            return Err(decl.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("struct.nonempty", "struct requires at least one field"),
            ));
        }
        let mut names = BTreeSet::new();
        for field in &decl.fields {
            if !names.insert(&field.name) {
                return Err(field.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("struct.field", "duplicate field name"),
                ));
            }
        }
    }
    fn visit(
        id: usize,
        declarations: &[StructDeclaration],
        states: &mut [u8],
    ) -> Result<(), Diagnostic> {
        if states[id] == 2 {
            return Ok(());
        }
        states[id] = 1;
        for field in &declarations[id].fields {
            if let Type::Struct(child) = field.ty {
                if states[child] == 1 {
                    return Err(field.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("struct.recursion", "struct contains itself by value"),
                    ));
                }
                visit(child, declarations, states)?;
            }
        }
        states[id] = 2;
        Ok(())
    }
    let mut states = vec![0; declarations.len()];
    for id in 0..declarations.len() {
        visit(id, &declarations, &mut states)?;
    }
    Ok(declarations)
}
