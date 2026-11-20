//! fnshapeの前方参照を解決し、同じ形の関数値の型を一つにまとめる。
//! 構造体は名前で区別するため、関数値の型の循環検査は構造体の内部へ進まない。

use super::{FixedToken, Parser};
use crate::{
    Diagnostic, ErrorKind,
    ast::{EnumDeclaration, FnShapeDeclaration, FunctionType, StructDeclaration, Type},
    lexer::{Kind, Token},
};

pub(super) fn headers(tokens: &[Token<'_>]) -> Result<Vec<FnShapeDeclaration>, Diagnostic> {
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut cursor = 0;
    while tokens[cursor].kind != Kind::End {
        if depth == 0 && tokens[cursor].kind == Kind::Keyword("fnshape") {
            let start = cursor;
            cursor += 1;
            let location = crate::ast::Location {
                line: tokens[cursor].line,
                column: tokens[cursor].column,
            };
            let Kind::Ident(name) = tokens[cursor].kind else {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("fnshape.decl", "expected a function type name"),
                ));
            };
            super::names::check(super::names::Shape::Type, name, location)?;
            let type_id = result.len();
            result.push(FnShapeDeclaration {
                name: name.into(),
                location,
                start,
                end: 0,
                type_id,
            });
        }
        match tokens[cursor].kind {
            Kind::Symbol("{" | "(" | "[") => depth += 1,
            Kind::Symbol("}" | ")" | "]") => depth = depth.saturating_sub(1),
            _ => {}
        }
        cursor += 1;
    }
    Ok(result)
}

pub(super) fn resolve(
    tokens: &[Token<'_>],
    enums: &[EnumDeclaration],
    structs: &mut [StructDeclaration],
    declarations: &mut [FnShapeDeclaration],
) -> Result<Vec<FunctionType>, Diagnostic> {
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
    let mut p = Parser {
        type_parameters: Vec::new(),
        pending_needs: Vec::new(),
        generic_functions: Default::default(),
        allows: Vec::new(),
        tokens,
        enums,
        structs,
        fnshapes: declarations,
        restricted_condition: false,
        cursor: 0,
        constant_expression: false,
        in_value: false,
        depth: 0,
        limit_location: None,
    };
    let mut raw = Vec::new();
    let mut ends = Vec::new();
    for d in declarations.iter() {
        p.cursor = d.start + 2;
        let parameters = p.list(Parser::function_parameter_type)?;
        p.expect(FixedToken::Symbol("->"))?;
        let result = p.parse_type()?;
        let errors = p.error_spec()?;
        for ty in parameters.iter().chain([&result]) {
            if ty.leaf().generic() {
                return Err(d.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "num.places",
                        "fnshape requires concrete types; int and float are not allowed"
                    ),
                ));
            }
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
        ends.push(p.cursor);
    }
    // 依存先から順に解決する。長い前方参照の連鎖でもRustの再帰スタックを使わない。
    let mut pending = vec![0; raw.len()];
    let mut parents = vec![Vec::new(); raw.len()];
    for (id, signature) in raw.iter().enumerate() {
        for ty in signature.parameters.iter().chain([&signature.result]) {
            if let Type::StaticFunction(child) | Type::Function(child) = ty.leaf() {
                pending[id] += 1;
                parents[child].push(id);
            }
        }
    }
    let mut ready: std::collections::VecDeque<_> =
        (0..raw.len()).filter(|&id| pending[id] == 0).collect();
    let mut ids = vec![None; raw.len()];
    let mut types = Vec::new();
    while let Some(id) = ready.pop_front() {
        let remap = |ty| remap(ty, &ids);
        let signature = FunctionType {
            parameters: raw[id].parameters.iter().copied().map(remap).collect(),
            result: remap(raw[id].result),
            errors: raw[id].errors.clone(),
        };
        let canonical = types
            .iter()
            .position(|t| t == &signature)
            .unwrap_or_else(|| {
                types.push(signature);
                types.len() - 1
            });
        ids[id] = Some(canonical);
        for &parent in &parents[id] {
            pending[parent] -= 1;
            if pending[parent] == 0 {
                ready.push_back(parent);
            }
        }
    }
    if let Some(id) = ids.iter().position(Option::is_none) {
        return Err(declarations[id].location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "fnshape.recursion",
                "function value type contains itself without passing through a struct"
            ),
        ));
    }
    for (id, d) in declarations.iter_mut().enumerate() {
        d.type_id = ids[id].unwrap();
        d.end = ends[id];
    }
    for field in structs.iter_mut().flat_map(|s| &mut s.fields) {
        field.ty = remap(field.ty, &ids);
    }
    Ok(types)
}

// ptrで包まれた関数型にも、宣言番号から正規化後の番号への置き換えを適用する。
fn remap(ty: Type, ids: &[Option<usize>]) -> Type {
    match ty {
        Type::Pointer { .. } => remap(ty.pointee().unwrap(), ids)
            .pointer()
            .with_nullable(ty.is_nullable()),
        Type::StaticFunction(id) => Type::StaticFunction(ids[id].unwrap()),
        Type::Function(id) => Type::Function(ids[id].unwrap()),
        _ => ty,
    }
}
