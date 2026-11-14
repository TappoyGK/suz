use super::Parser;
use crate::{
    Diagnostic, ErrorKind,
    ast::{EnumDeclaration, EnumType, IntegerType, Location},
    lexer::{Kind, Token},
};
use std::collections::BTreeSet;

impl Parser<'_, '_> {
    fn variant(&mut self) -> Result<(String, Location), Diagnostic> {
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "expected an enum variant name [enum.variant]",
            ));
        };
        self.cursor += 1;
        if self.token().kind == Kind::Symbol("=") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "enum variants are numbered automatically [enum.variant]",
            ));
        }
        Ok((name.into(), location))
    }
    fn enum_declaration(&mut self, id: usize) -> Result<EnumDeclaration, Diagnostic> {
        let start = self.cursor;
        self.cursor += 1;
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "expected an enum name [src.ident]",
            ));
        };
        let name = name.to_owned();
        self.cursor += 1;
        if self.token().kind != Kind::Symbol(":") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                "enum requires an integer base type [enum.base]",
            ));
        }
        self.cursor += 1;
        let base = match self.token().kind {
            Kind::TypeName(name) => IntegerType::from_name(name),
            _ => None,
        }
        .ok_or_else(|| {
            self.error(
                ErrorKind::InvalidSource,
                "enum base must be an integer type [enum.base]",
            )
        })?;
        self.cursor += 1;
        let variants = self.delimited_list("{", "}", Self::variant)?;
        if variants.is_empty() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                "enum requires at least one variant [enum.nonempty]",
            ));
        }
        let mut seen = BTreeSet::new();
        for (index, (name, location)) in variants.iter().enumerate() {
            if !seen.insert(name) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    "duplicate enum variant [enum.variant]",
                ));
            }
            if index as i128 > base.bounds().1 {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    "enum variant exceeds the base type's range [enum.variant]",
                ));
            }
        }
        Ok(EnumDeclaration {
            name,
            location,
            ty: EnumType {
                id,
                base,
                count: variants.len(),
            },
            variants: variants.into_iter().map(|(name, _)| name).collect(),
            start,
            end: self.cursor,
        })
    }
}

// Register enum types before parsing annotations, allowing references to later
// declarations. Function and expression blocks are skipped by delimiter depth.
pub(super) fn collect(tokens: &[Token<'_>]) -> Result<Vec<EnumDeclaration>, Diagnostic> {
    let mut p = Parser {
        tokens,
        enums: &[],
        structs: &[],
        restricted_condition: false,
        cursor: 0,
        constant_expression: false,
        in_value: false,
    };
    let mut enums = Vec::<EnumDeclaration>::new();
    let mut depth = 0usize;
    while p.token().kind != Kind::End {
        if depth == 0 && p.token().kind == Kind::Keyword("enum") {
            let declaration = p.enum_declaration(enums.len())?;
            if enums.iter().any(|e| e.name == declaration.name) {
                return Err(declaration.location.error(
                    ErrorKind::InvalidSource,
                    "duplicate enum name [decl.duplicate]",
                ));
            }
            enums.push(declaration);
            continue;
        }
        match p.token().kind {
            Kind::Symbol("{" | "(" | "[") => depth += 1,
            Kind::Symbol("}" | ")" | "]") => depth = depth.saturating_sub(1),
            _ => {}
        }
        p.cursor += 1;
    }
    Ok(enums)
}
