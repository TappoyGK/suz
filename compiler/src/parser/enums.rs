//! 列挙型の宣言を先に収集し、基底型・列挙子・名前の条件を検査する。
//!
//! 利用元: [`crate::parser`]。
//! 主な対象規則: `[decl.duplicate]`、`[enum.base]`、`[enum.nonempty]`、`[enum.variant]`、`[src.ident]`。

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
                crate::diagnostic!("enum.variant", "expected an enum variant name"),
            ));
        };
        super::names::check(super::names::Shape::Variant, name, location)?;
        self.cursor += 1;
        if self.token().kind == Kind::Symbol("=") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("enum.variant", "enum variants are numbered automatically"),
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
                crate::diagnostic!("src.ident", "expected an enum name"),
            ));
        };
        super::names::check(super::names::Shape::Type, name, location)?;
        let name = name.to_owned();
        self.cursor += 1;
        if self.token().kind != Kind::Symbol(":") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("enum.base", "enum requires an integer base type"),
            ));
        }
        self.cursor += 1;
        let base = match self.token().kind {
            Kind::TypeName(name) => {
                IntegerType::from_name(name).filter(|ty| *ty != IntegerType::Len)
            }
            _ => None,
        }
        .ok_or_else(|| {
            self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("enum.base", "enum base must be an integer type"),
            )
        })?;
        self.cursor += 1;
        let variants = self.delimited_list("{", "}", Self::variant)?;
        if variants.is_empty() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("enum.nonempty", "enum requires at least one variant"),
            ));
        }
        let mut seen = BTreeSet::new();
        for (index, (name, location)) in variants.iter().enumerate() {
            if !seen.insert(name) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("enum.variant", "duplicate enum variant"),
                ));
            }
            if index as i128 > base.bounds().1 {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "enum.variant",
                        "enum variant exceeds the base type's range"
                    ),
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

// 後方の宣言も参照できるよう、型注釈を解析する前に列挙型を登録する。
// 関数と式のブロックは区切りの深さを使って読み飛ばす。
pub(super) fn collect(tokens: &[Token<'_>]) -> Result<Vec<EnumDeclaration>, Diagnostic> {
    let mut p = Parser {
        allows: Vec::new(),
        tokens,
        enums: &[],
        structs: &[],
        fntypes: &[],
        restricted_condition: false,
        cursor: 0,
        constant_expression: false,
        in_value: false,
        depth: 0,
        limit_location: None,
    };
    let mut enums = Vec::<EnumDeclaration>::new();
    let mut depth = 0usize;
    while p.token().kind != Kind::End {
        if depth == 0 && p.token().kind == Kind::Keyword("enum") {
            let declaration = p.enum_declaration(enums.len())?;
            if enums.iter().any(|e| e.name == declaration.name) {
                return Err(declaration.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("decl.duplicate", "duplicate enum name"),
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
