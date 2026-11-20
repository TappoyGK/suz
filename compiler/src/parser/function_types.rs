//! fnshapeの前方宣言と型引数・要求を解析する。関数型の同一性は具体化後に決める。
use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{EnumDeclaration, FnShapeDeclaration, FunctionType, StructDeclaration, Type},
    lexer::{Kind, Token},
};

/// 型引数などの情報が増えても、名前の解決で宣言を毎回全件走査しない。
pub(super) fn name_index(
    declarations: &[FnShapeDeclaration],
) -> std::rc::Rc<std::collections::BTreeMap<String, usize>> {
    std::rc::Rc::new(
        declarations
            .iter()
            .enumerate()
            .map(|(id, d)| (d.name.clone(), id))
            .collect(),
    )
}

fn parser<'a, 's>(
    tokens: &'a [Token<'s>],
    enums: &'a [EnumDeclaration],
    structs: &'a [StructDeclaration],
    declarations: &'a [FnShapeDeclaration],
    applications: crate::ast::generics::Applications,
    need_groups: std::rc::Rc<super::need_groups::Groups>,
) -> Parser<'a, 's> {
    Parser {
        need_groups,
        applications,
        type_parameters: Vec::new(),
        pending_needs: Vec::new(),
        generic_functions: Default::default(),
        allows: Vec::new(),
        tokens,
        enums,
        structs,
        fnshapes: declarations,
        fnshape_names: name_index(declarations),
        restricted_condition: false,
        cursor: 0,
        constant_expression: false,
        in_value: false,
        depth: 0,
        limit_location: None,
    }
}

pub(super) fn headers(tokens: &[Token<'_>]) -> Result<Vec<FnShapeDeclaration>, Diagnostic> {
    let mut result = Vec::new();
    let mut p = parser(
        tokens,
        &[],
        &[],
        &[],
        Default::default(),
        Default::default(),
    );
    let mut depth = 0usize;
    while p.token().kind != Kind::End {
        if depth == 0 && p.token().kind == Kind::Keyword("fnshape") {
            let start = p.cursor;
            p.cursor += 1;
            let location = p.location();
            let Kind::Ident(name) = p.token().kind else {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("fnshape.decl", "expected a function type name"),
                ));
            };
            super::names::check(super::names::Shape::Type, name, location)?;
            p.cursor += 1;
            let type_parameters = if p.token().kind == Kind::Symbol("[") {
                p.generic_parameters()?
            } else {
                Vec::new()
            };
            result.push(FnShapeDeclaration {
                plain_application: None,
                name: name.into(),
                location,
                start,
                end: 0,
                type_id: result.len(),
                type_parameters,
                needs: Vec::new(),
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
    Ok(result)
}

pub(super) fn resolve(
    tokens: &[Token<'_>],
    enums: &[EnumDeclaration],
    structs: &mut [StructDeclaration],
    declarations: &mut [FnShapeDeclaration],
    applications: crate::ast::generics::Applications,
    need_groups: std::rc::Rc<super::need_groups::Groups>,
) -> Result<Vec<FunctionType>, Diagnostic> {
    if declarations.is_empty() {
        return Ok(Vec::new());
    }
    let mut names = std::collections::BTreeSet::new();
    for d in declarations.iter() {
        if !names.insert(&d.name)
            || enums.iter().any(|e| e.name == d.name)
            || structs.iter().any(|s| s.name == d.name)
        {
            return Err(d.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("decl.duplicate", "duplicate type name"),
            ));
        }
    }
    let mut p = parser(
        tokens,
        enums,
        structs,
        declarations,
        applications.clone(),
        need_groups,
    );
    let mut ranges = std::collections::BTreeMap::new();
    // @needsを持たないソースでは、要求の位置を探す二度目の構文走査を行わない。
    if tokens.iter().any(|token| token.kind == Kind::Symbol("@")) {
        let mut depth = 0usize;
        while p.token().kind != Kind::End {
            if depth == 0 && p.token().kind == Kind::Symbol("@") {
                p.collect_need()?;
                continue;
            }
            if depth == 0 && p.token().kind == Kind::Keyword("fnshape") {
                ranges.insert(p.cursor, std::mem::take(&mut p.pending_needs));
            } else if depth == 0 && matches!(p.token().kind, Kind::Keyword(_)) {
                p.pending_needs.clear();
            }
            match p.token().kind {
                Kind::Symbol("{" | "(" | "[") => depth += 1,
                Kind::Symbol("}" | ")" | "]") => depth = depth.saturating_sub(1),
                _ => {}
            }
            p.cursor += 1;
        }
    }
    let mut raw = Vec::new();
    let mut metadata = Vec::new();
    for d in declarations.iter() {
        p.cursor = d.start + 2;
        p.type_parameters = if p.token().kind == Kind::Symbol("[") {
            p.generic_parameters()?
        } else {
            Vec::new()
        };
        p.pending_needs = ranges.remove(&d.start).unwrap_or_default();
        let needs = p.declaration_needs()?;
        let parameters = p.list(Parser::function_parameter_type)?;
        p.expect(FixedToken::Symbol("->"))?;
        let result = p.parse_type()?;
        let errors = p.error_spec()?;
        if parameters
            .iter()
            .chain([&result])
            .any(|ty| ty.leaf().generic())
        {
            return Err(d.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "num.places",
                    "fnshape requires concrete types; int and float are not allowed"
                ),
            ));
        }
        if parameters.contains(&Type::Void) {
            return Err(d.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("type.void", "parameters cannot have type void"),
            ));
        }
        raw.push(FunctionType {
            parameters,
            result,
            errors,
        });
        metadata.push((p.cursor, needs));
    }
    // 構造体を経由しない関数型の循環は、型引数の有無によらず拒否する。
    let applications = applications.borrow();
    let mut pending = vec![0; raw.len()];
    let mut parents = vec![Vec::new(); raw.len()];
    for (id, signature) in raw.iter().enumerate() {
        let mut types: Vec<_> = signature
            .parameters
            .iter()
            .copied()
            .chain([signature.result])
            .collect();
        while let Some(ty) = types.pop() {
            if let Type::StaticShape(app) | Type::Shape(app) = ty.leaf() {
                let app = &applications.entries[app];
                pending[id] += 1;
                parents[app.declaration].push(id);
                types.extend(&app.arguments);
            }
        }
    }
    let mut ready: std::collections::VecDeque<_> =
        (0..raw.len()).filter(|id| pending[*id] == 0).collect();
    let mut visited = vec![false; raw.len()];
    let mut order = Vec::new();
    while let Some(id) = ready.pop_front() {
        visited[id] = true;
        order.push(id);
        for &parent in &parents[id] {
            pending[parent] -= 1;
            if pending[parent] == 0 {
                ready.push_back(parent);
            }
        }
    }
    if let Some(id) = visited.iter().position(|done| !done) {
        return Err(declarations[id].location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fnshape.recursion",
                "function value type contains itself without passing through a struct"
            ),
        ));
    }
    for (d, (end, needs)) in declarations.iter_mut().zip(metadata) {
        d.end = end;
        d.needs = needs;
    }
    for (type_id, &declaration) in order.iter().enumerate() {
        declarations[declaration].type_id = type_id;
    }
    Ok(order.into_iter().map(|id| raw[id].clone()).collect())
}
