//! 構造体宣言を収集し、フィールドの名前・型・再帰とリテラルの構文を検査する。
//!
//! 利用元: [`crate::parser`]。
//! 主な対象規則: `[decl.duplicate]`、`[src.ident]`、`[struct.field]`、`[struct.literal]`、`[struct.nonempty]`、`[struct.recursion]`。

use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        EnumDeclaration, Expression, ExpressionKind, FieldInitializer, StructDeclaration,
        StructField, Type,
    },
    lexer::{Kind, Token},
};
use std::collections::BTreeSet;

impl Parser<'_, '_> {
    // with固有の解析を分離し、再帰する式解析のスタック使用量を増やさない。
    pub(super) fn with_expression(&mut self, target: Expression) -> Result<Expression, Diagnostic> {
        let location = self.location();
        // 丸括弧内では字句解析が改行トークンを省くため、物理行も確認する。
        if self.tokens[self.cursor - 1].line != location.line {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "struct.with-line",
                    "with must follow its target on the same line"
                ),
            ));
        }
        if self.restricted_condition {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "expr.struct-in-cond",
                    "with expressions in conditions require parentheses"
                ),
            ));
        }
        self.cursor += 1;
        self.check_height(target.height, location)?;
        let previous_limit = self.limit_location.replace(location);
        let fields = self.delimited_list("{", "}", Self::with_field_initializer);
        self.limit_location = previous_limit;
        let fields = fields?;
        if fields.is_empty() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("struct.with-fields", "with requires at least one field"),
            ));
        }
        self.primary_node(
            ExpressionKind::With {
                target: Box::new(target),
                fields,
            },
            location,
        )
    }

    pub(super) fn condition(&mut self) -> Result<Expression, Diagnostic> {
        self.restricted_expression(true)
    }

    fn with_field_initializer(&mut self) -> Result<FieldInitializer, Diagnostic> {
        // pos.x: 式は受け付けず、内側のwith式で更新するよう案内する。
        if self
            .tokens
            .get(self.cursor + 1)
            .is_some_and(|token| token.kind == Kind::Symbol("."))
        {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "struct.with-nested",
                    "with accepts direct fields only; use a nested with expression"
                ),
            ));
        }
        self.field_initializer()
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
        let ty = self.stored_type()?;
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
        let value = if self.token().kind == Kind::Symbol(":") {
            self.cursor += 1;
            self.expression()?
        } else {
            // 省略形も通常の名前参照にして、名前解決・型変換・評価順序を共通にする。
            self.node(ExpressionKind::Name(name.clone()), location)?
        };
        Ok(FieldInitializer {
            name,
            location,
            value,
        })
    }
}

pub(super) fn collect(
    tokens: &[Token<'_>],
    enums: &[EnumDeclaration],
    fnshapes: &[crate::ast::FnShapeDeclaration],
    applications: crate::ast::generics::Applications,
    need_groups: std::rc::Rc<super::need_groups::Groups>,
) -> Result<Vec<StructDeclaration>, Diagnostic> {
    let mut p = Parser {
        need_groups,
        applications,
        type_parameters: Vec::new(),
        pending_needs: Vec::new(),
        generic_functions: Default::default(),
        allows: Vec::new(),
        tokens,
        enums,
        structs: &[],
        fnshapes,
        fnshape_names: super::function_types::name_index(fnshapes),
        restricted_condition: false,
        cursor: 0,
        constant_expression: false,
        in_value: false,
        depth: 0,
        limit_location: None,
    };
    let mut headers = Vec::<StructDeclaration>::new();
    let mut requirement_ranges = Vec::new();
    let mut depth = 0usize;
    let mut external_header = None;
    let mut c_names = BTreeSet::new();
    while p.token().kind != Kind::End {
        if depth == 0 && p.token().kind == Kind::Symbol("@") {
            p.collect_need()?;
            continue;
        }
        if depth == 0
            && matches!(
                p.token().kind,
                Kind::Keyword(
                    "fn" | "arena" | "fnshape" | "needs" | "fix" | "extern" | "enum" | "error"
                )
            )
        {
            p.pending_needs.clear();
        }
        if depth == 0 && p.token().kind == Kind::Keyword("extern") {
            p.cursor += 1;
            let Kind::Header(header) = p.token().kind else {
                return Err(p.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("ffi.header", "extern requires a quoted C header name"),
                ));
            };
            external_header = Some(header.to_owned());
            p.cursor += 1;
            p.expect(FixedToken::Symbol("{"))?;
            depth = 1;
            continue;
        }
        if (depth == 0 || (depth == 1 && external_header.is_some()))
            && p.token().kind == Kind::Keyword("struct")
        {
            let start = p.cursor;
            p.cursor += 1;
            let location = p.location();
            let external = if let Some(header) = &external_header {
                let c_name = match p.token().kind {
                    Kind::Ident(name) | Kind::Keyword(name) | Kind::TypeName(name) => {
                        name.to_owned()
                    }
                    _ => {
                        return Err(p.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("ffi.struct", "expected a C struct tag"),
                        ));
                    }
                };
                if c_name.starts_with("suz_") {
                    return Err(p.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("ffi.name", "C names starting with suz_ are reserved"),
                    ));
                }
                if !c_names.insert(c_name.clone()) {
                    return Err(p.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!("ffi.duplicate", "duplicate C struct tag"),
                    ));
                }
                if p.tokens
                    .get(p.cursor + 1)
                    .is_some_and(|t| t.kind == Kind::Keyword("as"))
                {
                    p.cursor += 2;
                }
                Some(crate::ast::ExternStruct {
                    header: header.clone(),
                    c_name,
                })
            } else {
                None
            };
            let Kind::Ident(name) = p.token().kind else {
                return Err(p.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("src.ident", "expected a struct name or Suz alias"),
                ));
            };
            super::names::check(super::names::Shape::Type, name, p.location())?;
            let name = name.to_owned();
            // 列挙型との重複と同じく、構造体の名前の位置で報告する。
            if fnshapes.iter().any(|decl| decl.name == name)
                || enums.iter().any(|decl| decl.name == name)
                || headers.iter().any(|decl| decl.name == name)
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("decl.duplicate", "duplicate type name"),
                ));
            }
            p.cursor += 1;
            let type_parameters = if p.token().kind == Kind::Symbol("[") {
                if external.is_some() {
                    return Err(p.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "generic.parameters",
                            "extern structs cannot have type parameters"
                        ),
                    ));
                }
                p.generic_parameters()?
            } else {
                Vec::new()
            };
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
            requirement_ranges.push(std::mem::take(&mut p.pending_needs));
            headers.push(StructDeclaration {
                symbolic: !type_parameters.is_empty(),
                type_parameters,
                needs: Vec::new(),
                application: None,
                external,
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
        if depth == 0 {
            external_header = None;
        }
        p.cursor += 1;
    }
    p.structs = &headers;
    p.prepare_need_groups()?;
    let mut declarations = headers.clone();
    for (decl, ranges) in declarations.iter_mut().zip(requirement_ranges) {
        p.cursor = decl.start + 2;
        if decl.external.is_some() && p.token().kind == Kind::Keyword("as") {
            p.cursor += 2;
        }
        p.type_parameters = if p.token().kind == Kind::Symbol("[") {
            p.generic_parameters()?
        } else {
            Vec::new()
        };
        p.pending_needs = ranges;
        decl.needs = p.declaration_needs()?;
        decl.fields = p.delimited_list("{", "}", Parser::field_declaration)?;
        if decl.fields.is_empty() {
            return Err(decl.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("struct.nonempty", "struct requires at least one field"),
            ));
        }
        let mut names = BTreeSet::new();
        for field in &decl.fields {
            if decl.external.is_some() && !super::ffi::numeric_type(field.ty) {
                return Err(field.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "ffi.struct",
                        "C struct fields require supported numeric types"
                    ),
                ));
            }
            if !names.insert(&field.name) {
                return Err(field.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("struct.field", "duplicate field name"),
                ));
            }
        }
    }
    validate_layout_range(
        0,
        declarations.len(),
        |id| &declarations[id],
        &p.applications.borrow().arrays,
    )?;
    Ok(declarations)
}

/// 既存の宣言を変更しない具体化では、追加した宣言だけを検査する。
/// 既存部分から追加部分へ新しい辺は生じないため、既に検査した頂点は辿らない。
pub(crate) fn validate_layout_range<'a>(
    start: usize,
    end: usize,
    declaration: impl Fn(usize) -> &'a StructDeclaration,
    arrays: &crate::ast::arrays::Types,
) -> Result<(), Diagnostic> {
    if start == end {
        return Ok(());
    }
    for include_pointers in [false, true] {
        // 先に値だけの循環を調べ、その後に非nullポインタを含む初期化の循環を調べる。
        // nullableの辺は初期化をnullで打ち切れるため、後者の依存から除く。
        let dependencies: Vec<Vec<_>> = (start..end)
            .map(|id| {
                let decl = declaration(id);
                decl.fields
                    .iter()
                    .filter_map(|field| {
                        let mut ty = field.ty;
                        if include_pointers {
                            loop {
                                if let Some(inner) = ty.pointee() {
                                    if ty.is_nullable() {
                                        return None;
                                    }
                                    ty = inner;
                                } else if let Type::Array(id) = ty {
                                    let array = arrays.get(id);
                                    // 一次元はnew []で初期化を打ち切れる。多次元は初期値が必要。
                                    if array.rank == 1 {
                                        return None;
                                    }
                                    ty = array.element;
                                } else {
                                    break;
                                }
                            }
                        }
                        if let Type::Struct(child) = ty {
                            (child >= start).then_some((child, field))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .collect();
        let mut states = vec![0; end - start];
        for root in start..end {
            if states[root - start] != 0 {
                continue;
            }
            states[root - start] = 1;
            let mut stack = vec![(root, 0)];
            while let Some(&mut (id, ref mut next)) = stack.last_mut() {
                if *next == dependencies[id - start].len() {
                    states[id - start] = 2;
                    stack.pop();
                    continue;
                }
                let (child, field) = dependencies[id - start][*next];
                *next += 1;
                if states[child - start] == 1 {
                    if !include_pointers {
                        return Err(field.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "struct.recursion",
                                "struct contains itself by value"
                            ),
                        ));
                    }
                    let cycle_start = stack.iter().position(|&(id, _)| id == child).unwrap();
                    let fields = stack[cycle_start..]
                        .iter()
                        .map(|&(id, next)| {
                            format!(
                                "{}.{}",
                                declaration(id).name,
                                dependencies[id - start][next - 1].1.name
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    return Err(field.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "arena.recursive-struct",
                            "non-null initialization cycle: {}",
                            fields
                        ),
                    ));
                }
                if states[child - start] == 0 {
                    states[child - start] = 1;
                    stack.push((child, 0));
                }
            }
        }
    }
    Ok(())
}
