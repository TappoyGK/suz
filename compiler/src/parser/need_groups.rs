//! 名前付きの要求一覧を定義位置で解析し、型引数を置換して直接要求へ展開する。
use super::{FixedToken, Parser, names};
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        Location, Type,
        generics::{Need, NeedGroup, NeedList, TypeParameter},
    },
    lexer::{Kind, Token},
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(super) struct Declaration {
    name: String,
    location: Location,
    parameters: Vec<TypeParameter>,
    body: Vec<(usize, usize)>,
    pub end: usize,
}

/// 前方参照のための宣言表。展開済みの一覧は定義側の型引数番号で保存する。
#[derive(Default)]
pub(super) struct Groups {
    pub names: BTreeMap<String, usize>,
    pub starts: BTreeMap<usize, usize>,
    pub declarations: Vec<Declaration>,
    expanded: RefCell<BTreeMap<usize, Vec<Need>>>,
    order: Vec<usize>,
}

pub(super) fn collect(tokens: &[Token<'_>]) -> Result<Rc<Groups>, Diagnostic> {
    let mut groups = Groups::default();
    if !tokens.iter().enumerate().any(|(i, t)| {
        t.kind == Kind::Keyword("needs") && (i == 0 || tokens[i - 1].kind != Kind::Symbol("@"))
    }) {
        return Ok(Rc::new(groups));
    }
    let mut p = Parser {
        applications: Default::default(),
        type_parameters: Vec::new(),
        pending_needs: Vec::new(),
        need_groups: Default::default(),
        generic_functions: Default::default(),
        allows: Vec::new(),
        tokens,
        enums: &[],
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
    let mut depth = 0usize;
    while p.token().kind != Kind::End {
        if depth == 0
            && p.token().kind == Kind::Keyword("needs")
            && (p.cursor == 0 || tokens[p.cursor - 1].kind != Kind::Symbol("@"))
        {
            let start = p.cursor;
            let location = p.location();
            p.cursor += 1;
            let Kind::Ident(name) = p.token().kind else {
                return Err(p.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("generic.needs-group", "expected a requirement list name"),
                ));
            };
            names::check(names::Shape::Type, name, p.location())?;
            let name = name.to_owned();
            p.cursor += 1;
            let parameters = p.generic_parameters()?;
            p.expect(FixedToken::Symbol("{"))?;
            if !p.token().kind.is_newline() {
                return Err(p.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.needs-group",
                        "requirements must start on the next line"
                    ),
                ));
            }
            p.newlines();
            let mut body = Vec::new();
            while !matches!(p.token().kind, Kind::Symbol("}") | Kind::End) {
                let first = p.cursor;
                let line = p.token().line;
                while !p.token().kind.is_newline()
                    && p.token().kind != Kind::End
                    && p.token().line == line
                {
                    p.cursor += 1;
                }
                if !p.token().kind.is_newline() {
                    return Err(p.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "generic.needs-group",
                            "each requirement must occupy one line"
                        ),
                    ));
                }
                body.push((first, p.cursor));
                p.newlines();
            }
            p.expect(FixedToken::Symbol("}"))?;
            if body.is_empty() {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.needs-group",
                        "a requirement list must not be empty"
                    ),
                ));
            }
            let id = groups.declarations.len();
            if groups.names.insert(name.clone(), id).is_some() {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("decl.duplicate", "duplicate requirement list name"),
                ));
            }
            groups.starts.insert(start, id);
            groups.declarations.push(Declaration {
                name,
                location,
                parameters,
                body,
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
    // 依存先から展開する。長い前方参照列でもRustの呼び出しスタックを増やさない。
    let mut pending = vec![0; groups.declarations.len()];
    let mut parents = vec![Vec::new(); groups.declarations.len()];
    for (id, d) in groups.declarations.iter().enumerate() {
        for &(start, _) in &d.body {
            if tokens[start + 1].kind != Kind::Symbol(":")
                && let Kind::Ident(name) = tokens[start].kind
                && let Some(&child) = groups.names.get(name)
            {
                pending[id] += 1;
                parents[child].push(id);
            }
        }
    }
    let mut ready: std::collections::VecDeque<_> =
        (0..pending.len()).filter(|&id| pending[id] == 0).collect();
    while let Some(id) = ready.pop_front() {
        groups.order.push(id);
        for &parent in &parents[id] {
            pending[parent] -= 1;
            if pending[parent] == 0 {
                ready.push_back(parent);
            }
        }
    }
    if let Some(id) = pending.iter().position(|&n| n != 0) {
        let d = &groups.declarations[id];
        return Err(d.location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "generic.needs-expand",
                "cyclic requirement list dependency at '{}'",
                d.name
            ),
        ));
    }
    Ok(Rc::new(groups))
}

impl Parser<'_, '_> {
    pub(super) fn prepare_need_groups(&mut self) -> Result<(), Diagnostic> {
        let groups = self.need_groups.clone();
        for &id in &groups.order {
            self.expand_group(id)?;
        }
        Ok(())
    }

    pub(super) fn validate_need_groups(&mut self) -> Result<Vec<NeedGroup>, Diagnostic> {
        let groups = self.need_groups.clone();
        let mut result = Vec::new();
        for (id, d) in groups.declarations.iter().enumerate() {
            if self.enums.iter().any(|e| e.name == d.name)
                || self.structs.iter().any(|s| s.name == d.name)
                || self.fnshape_names.contains_key(&d.name)
            {
                return Err(d.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "decl.duplicate",
                        "requirement list name conflicts with a type name"
                    ),
                ));
            }
            result.push(NeedGroup {
                name: d.name.clone(),
                location: d.location,
                type_parameters: d.parameters.clone(),
                needs: self.expand_group(id)?,
            });
        }
        Ok(result)
    }

    pub(super) fn named_need(&mut self, end: usize) -> Result<Vec<Need>, Diagnostic> {
        let location = self.location();
        let Kind::Ident(name) = self.token().kind else {
            return Err(self.error(ErrorKind::InvalidSource, crate::diagnostic!("generic.needs-group", "expected a direct requirement or a requirement list; do not use @needs inside a list")));
        };
        let id = self.need_groups.names.get(name).copied().ok_or_else(|| {
            self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("generic.needs-group", "unknown requirement list '{}'", name),
            )
        })?;
        self.cursor += 1;
        let arguments = self.delimited_list("[", "]", Self::type_argument)?;
        let expected = self.need_groups.declarations[id].parameters.len();
        if expected != arguments.len() {
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.needs-group",
                    "expected {} type arguments, found {}",
                    expected,
                    arguments.len()
                ),
            ));
        }
        if self.cursor != end {
            return Err(self.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "generic.needs-group",
                    "unexpected tokens after requirement list"
                ),
            ));
        }
        let template = self.need_groups.expanded.borrow()[&id].clone();
        let mut needs = NeedList::default();
        for mut need in template {
            let Type::Parameter(parameter) = arguments[need.parameter] else {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.needs",
                        "an expanded requirement must target a type parameter of this declaration"
                    ),
                ));
            };
            need.parameter = parameter;
            for ty in need.parameters.iter_mut().chain([&mut need.result]) {
                *ty = self.replace_need_type(*ty, &arguments);
            }
            // 使用時の矛盾は一覧を使用した行へ結び付ける。
            need.location = location;
            needs.insert(need);
        }
        Ok(needs.into_vec())
    }

    fn expand_group(&mut self, id: usize) -> Result<Vec<Need>, Diagnostic> {
        let groups = self.need_groups.clone();
        if let Some(needs) = groups.expanded.borrow().get(&id) {
            return Ok(needs.clone());
        }
        let d = &groups.declarations[id];
        let saved = self.cursor;
        let parameters = std::mem::replace(&mut self.type_parameters, d.parameters.clone());
        // 型引数の名前も、一覧の定義位置で全トップレベル宣言と照合する。
        for parameter in &d.parameters {
            if self.enums.iter().any(|e| e.name == parameter.name)
                || self.structs.iter().any(|s| s.name == parameter.name)
                || self.fnshape_names.contains_key(&parameter.name)
                || groups.names.contains_key(&parameter.name)
            {
                return Err(parameter.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("decl.shadowing", "type parameter shadows a top-level name"),
                ));
            }
        }
        let mut needs = NeedList::default();
        for &(start, end) in &d.body {
            self.cursor = start;
            self.need_item(end, &mut needs)?;
        }
        self.cursor = saved;
        self.type_parameters = parameters;
        let needs = needs.into_vec();
        groups.expanded.borrow_mut().insert(id, needs.clone());
        Ok(needs)
    }

    /// 型適用の中にも置換を行う。具体化せず、定義位置で解決した宣言番号を保つ。
    fn replace_need_type(&self, ty: Type, arguments: &[Type]) -> Type {
        if let Some(inner) = ty.pointee() {
            return self
                .replace_need_type(inner, arguments)
                .pointer()
                .with_readonly(ty.is_readonly_pointer())
                .with_nilable(ty.is_nilable());
        }
        match ty {
            Type::Parameter(id) => arguments[id],
            Type::Application(id) | Type::StaticShape(id) | Type::Shape(id) => {
                let mut app = self.applications.borrow().entries[id].clone();
                for ty in &mut app.arguments {
                    *ty = self.replace_need_type(*ty, arguments);
                }
                let id = self.applications.borrow_mut().intern(app);
                match ty {
                    Type::Application(_) => Type::Application(id),
                    Type::StaticShape(_) => Type::StaticShape(id),
                    _ => Type::Shape(id),
                }
            }
            _ => ty,
        }
    }
}
