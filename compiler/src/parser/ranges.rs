//! 値域制限型の宣言を先に収集し、名前・基底型・上下限の構文を検査する。
//!
//! 利用元: [`crate::parser`]。
//! 主な対象規則: `[decl.duplicate]`、`[range.bounds]`、`[range.decl]`、`[src.ident]`。

use super::Parser;
use crate::{
    Diagnostic, ErrorKind,
    ast::{EnumDeclaration, IntegerType, RangeDeclaration, RangeType},
    lexer::{Kind, Token},
};

impl Parser<'_, '_> {
    fn range_declaration(&mut self, id: usize) -> Result<RangeDeclaration, Diagnostic> {
        let start = self.cursor;
        self.cursor += 1;
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("src.ident", "expected a range type name"),
            ));
        };
        super::names::check(super::names::Shape::Type, name, location)?;
        let name = name.to_owned();
        self.cursor += 1;
        if self.token().kind != Kind::Symbol(":") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "range.decl",
                    "range type requires a fixed-width integer base type"
                ),
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
                crate::diagnostic!(
                    "range.decl",
                    "range base must be a fixed-width integer type"
                ),
            )
        })?;
        self.cursor += 1;
        if self.token().kind != Kind::Symbol("=") {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("range.decl", "range declaration requires '=' and bounds"),
            ));
        }
        self.cursor += 1;
        self.constant_expression = true;
        let start_expr = self.expression()?;
        let range_location = self.location();
        let inclusive = match self.token().kind {
            Kind::Symbol("..=") => true,
            Kind::Symbol("..<") => false,
            _ => {
                self.constant_expression = false;
                return Err(self.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "range.bounds",
                        "expected '..=' or '..<' in range declaration"
                    ),
                ));
            }
        };
        self.cursor += 1;
        if matches!(
            self.token().kind,
            Kind::End | Kind::Newline | Kind::BlankLine
        ) {
            self.constant_expression = false;
            return Err(range_location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "range.bounds",
                    "expected an upper bound after range operator"
                ),
            ));
        }
        let end_expr = self.expression()?;
        self.constant_expression = false;
        Ok(RangeDeclaration {
            name,
            location,
            // 上下限は意味検査の定数評価で確定する。
            ty: RangeType::unresolved(id, base),
            start_expr,
            end_expr,
            inclusive,
            start,
            end: self.cursor,
        })
    }
}

// 後方の宣言も参照できるよう、構造体・関数・型注釈を解析する前に値域制限型を登録する。
pub(super) fn collect(
    tokens: &[Token<'_>],
    enums: &[EnumDeclaration],
) -> Result<Vec<RangeDeclaration>, Diagnostic> {
    let mut p = Parser {
        applications: Default::default(),
        type_parameters: Vec::new(),
        pending_needs: Vec::new(),
        need_groups: Default::default(),
        generic_functions: Default::default(),
        allows: Vec::new(),
        tokens,
        enums,
        ranges: &[],
        structs: &[],
        fnshapes: &[],
        fnshape_names: Default::default(),
        restricted_condition: false,
        cursor: 0,
        constant_expression: false,
        in_value: false,
        depth: 0,
        limit_location: None,
    };
    let mut ranges = Vec::<RangeDeclaration>::new();
    let mut depth = 0usize;
    while p.token().kind != Kind::End {
        if depth == 0 && p.token().kind == Kind::Keyword("range") {
            let declaration = p.range_declaration(ranges.len())?;
            if enums.iter().any(|e| e.name == declaration.name)
                || ranges.iter().any(|r| r.name == declaration.name)
            {
                return Err(declaration.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("decl.duplicate", "duplicate type name"),
                ));
            }
            ranges.push(declaration);
            continue;
        }
        match p.token().kind {
            Kind::Symbol("{" | "(" | "[") => depth += 1,
            Kind::Symbol("}" | ")" | "]") => depth = depth.saturating_sub(1),
            _ => {}
        }
        p.cursor += 1;
    }
    Ok(ranges)
}
