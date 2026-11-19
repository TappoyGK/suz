//! 実行順によらない候補の包含制約を解く。各保存場所は自身の寿命を持ち、
//! 別名経由のメンバーも同じセルへ合流させる。安全でも候補を絞れない保存は拒否する。
mod boundaries;
mod solver;

use super::*;
use std::collections::{BTreeMap, BTreeSet};
type Node = usize;
// 構造体の各ポインタ葉をフィールド番号列で区別する。ポインタ自身の値は空の列。
type Shape = BTreeMap<Vec<usize>, Node>;

// ローカル変数またはnewで作る実体の保存場所。別名から到達しても同じroot/pathを使う。
#[derive(Clone)]
struct Cell {
    root: Node,
    ty: Type,
    path: Vec<usize>,
    region: usize,
    location: Location,
    origin: Origin,
}
#[derive(Clone, Copy)]
enum Origin {
    Allocation,
    Parameter,
    Call,
}
struct Region {
    parent: Option<usize>,
    end: Location,
}
#[derive(Clone)]
enum Place {
    Direct(Node),
    Indirect(Node, Vec<usize>),
}
enum Constraint {
    Copy(Node, Node),
    Load(Node, Place),
    Address(Node, Place),
    Store(Place, Node, Location),
}
struct Analysis<'a> {
    scope: &'a Scope,
    borrowed: BTreeSet<usize>,
    return_region: Option<usize>,
    recovery: Option<Shape>,
    calls: Vec<boundaries::CallCheck>,
    // 各値が指しうる保存場所。実体の領域は参照変数の宣言位置から独立している。
    targets: Vec<BTreeSet<Node>>,
    cells: BTreeMap<Node, Cell>,
    projections: BTreeMap<(Node, Vec<usize>), Node>,
    regions: Vec<Region>,
    constraints: Vec<Constraint>,
    locals: BTreeMap<usize, Node>,
}

impl<'a> Analysis<'a> {
    fn new(scope: &'a Scope) -> Self {
        Self {
            scope,
            borrowed: BTreeSet::new(),
            return_region: None,
            recovery: None,
            calls: Vec::new(),
            targets: vec![],
            cells: BTreeMap::new(),
            projections: BTreeMap::new(),
            regions: vec![Region {
                parent: None,
                end: Location { line: 1, column: 1 },
            }],
            constraints: vec![],
            locals: BTreeMap::new(),
        }
    }

    fn node(&mut self) -> Node {
        let n = self.targets.len();
        self.targets.push(BTreeSet::new());
        n
    }
    fn cell(&mut self, region: usize, location: Location, ty: Type) -> Node {
        let n = self.node();
        self.cells.insert(
            n,
            Cell {
                origin: Origin::Allocation,
                root: n,
                ty,
                path: vec![],
                region,
                location,
            },
        );
        self.projections.insert((n, vec![]), n);
        n
    }
    fn region(&mut self, parent: usize, end: Location) -> usize {
        let id = self.regions.len();
        self.regions.push(Region {
            parent: Some(parent),
            end,
        });
        id
    }
    fn project(&mut self, cell: Node, path: &[usize]) -> Node {
        let mut metadata = self.cells[&cell].clone();
        metadata.path.extend_from_slice(path);
        let key = (metadata.root, metadata.path.clone());
        if let Some(n) = self.projections.get(&key) {
            return *n;
        }
        let n = self.node();
        self.cells.insert(n, metadata);
        self.projections.insert(key, n);
        n
    }
    fn paths(&self, ty: Type) -> Vec<Vec<usize>> {
        match ty {
            Type::Pointer { .. } => vec![vec![]],
            Type::Struct(id) => self.scope.structs[id]
                .fields
                .iter()
                .enumerate()
                .flat_map(|(i, f)| {
                    self.paths(f.ty).into_iter().map(move |mut path| {
                        path.insert(0, i);
                        path
                    })
                })
                .collect(),
            _ => vec![],
        }
    }
    fn shape(&mut self, ty: Type) -> Shape {
        self.paths(ty)
            .into_iter()
            .map(|p| (p, self.node()))
            .collect()
    }
    fn extended(place: Place, path: &[usize]) -> Place {
        match place {
            Place::Direct(n) if path.is_empty() => Place::Direct(n),
            Place::Direct(_) => unreachable!("direct projection is normalized by at"),
            Place::Indirect(n, mut prefix) => {
                prefix.extend_from_slice(path);
                Place::Indirect(n, prefix)
            }
        }
    }
    fn at(&mut self, place: Place, path: &[usize]) -> Place {
        match place {
            Place::Direct(n) => Place::Direct(self.project(n, path)),
            p => Self::extended(p, path),
        }
    }
    fn read(&mut self, ty: Type, place: Place) -> Shape {
        let result = self.shape(ty);
        for (path, &n) in &result {
            let place = self.at(place.clone(), path);
            self.constraints.push(Constraint::Load(n, place));
        }
        result
    }
    fn write(&mut self, place: Place, value: Shape, location: Location) {
        for (path, source) in value {
            let destination = self.at(place.clone(), &path);
            self.constraints
                .push(Constraint::Store(destination, source, location));
        }
    }
    fn join(&mut self, result: &Shape, value: Shape) {
        for (path, source) in value {
            if let Some(&dst) = result.get(&path) {
                self.constraints.push(Constraint::Copy(dst, source));
            }
        }
    }
    fn place(&mut self, value: &CheckedExpression, region: usize, arena: Option<usize>) -> Place {
        match &value.value {
            Value::Local(id) => Place::Direct(self.locals[id]),
            Value::Field { target, index } => {
                let p = self.place(target, region, arena);
                self.at(p, &[*index])
            }
            Value::Deref(pointer) => {
                let p = self.expr(pointer, region, arena);
                Place::Indirect(p[&vec![]], vec![])
            }
            _ => unreachable!("checked writable place"),
        }
    }
    fn expr(&mut self, value: &CheckedExpression, region: usize, arena: Option<usize>) -> Shape {
        match &value.value {
            Value::Null => self.shape(value.ty),
            Value::New {
                value: initial,
                location,
            } => {
                let ty = initial.ty;
                let initial = self.expr(initial, region, arena);
                let cell = self.cell(arena.expect("new has a lexical arena"), *location, ty);
                self.write(Place::Direct(cell), initial, *location);
                let n = self.node();
                self.targets[n].insert(cell);
                BTreeMap::from([(vec![], n)])
            }
            Value::Local(id) => {
                if contains(value.ty, self.scope) {
                    self.read(value.ty, Place::Direct(self.locals[id]))
                } else {
                    Shape::new()
                }
            }
            Value::Deref(pointer) => {
                let p = self.expr(pointer, region, arena);
                self.read(value.ty, Place::Indirect(p[&vec![]], vec![]))
            }
            Value::Field { target, index } => self
                .expr(target, region, arena)
                .into_iter()
                .filter_map(|(path, n)| {
                    (path.first() == Some(index)).then(|| (path[1..].to_vec(), n))
                })
                .collect(),
            Value::Struct { fields } => {
                let mut result = Shape::new();
                for (index, field) in fields {
                    for (mut path, n) in self.expr(field, region, arena) {
                        path.insert(0, *index);
                        result.insert(path, n);
                    }
                }
                result
            }
            Value::With { target, fields } => {
                let mut result = self.expr(target, region, arena);
                for (index, field) in fields {
                    result.retain(|p, _| p.first() != Some(index));
                    for (mut path, n) in self.expr(field, region, arena) {
                        path.insert(0, *index);
                        result.insert(path, n);
                    }
                }
                result
            }
            Value::Block {
                statements, value, ..
            } => {
                let locals = self.locals.clone();
                let inner = self.region(region, self.regions[region].end);
                self.statements(statements, inner, arena);
                let value = self.expr(value, inner, arena);
                self.locals = locals;
                value
            }
            Value::If {
                branches,
                otherwise,
            } => {
                let result = self.shape(value.ty);
                for (condition, branch) in branches {
                    self.expr(condition, region, arena);
                    let branch = self.expr(branch, region, arena);
                    self.join(&result, branch);
                }
                let other = self.expr(otherwise, region, arena);
                self.join(&result, other);
                result
            }
            Value::Match { target, arms, .. } => {
                self.expr(target, region, arena);
                let result = self.shape(value.ty);
                for (pattern, arm) in arms {
                    if let Some(pattern) = pattern {
                        self.expr(pattern, region, arena);
                    }
                    let arm = self.expr(arm, region, arena);
                    self.join(&result, arm);
                }
                result
            }
            Value::Call {
                arguments,
                arena: is_arena,
                location,
                ..
            } => {
                let shapes: Vec<_> = arguments
                    .iter()
                    .map(|argument| (argument.ty, self.expr(argument, region, arena)))
                    .collect();
                if *is_arena {
                    self.arena_call(
                        &shapes,
                        value.ty,
                        arena.expect("checked arena call"),
                        *location,
                    )
                } else {
                    Shape::new()
                }
            }
            Value::Invoke {
                target, arguments, ..
            } => {
                self.expr(target, region, arena);
                for argument in arguments {
                    self.expr(argument, region, arena);
                }
                Shape::new() // 関数値は通常関数の借用契約だけを持つ。
            }
            Value::HandledCall { call, handler } => {
                let result = self.expr(call, region, arena);
                let alternative = self.handler(handler, call.ty, region, arena);
                self.join(&result, alternative);
                result
            }
            Value::Panic { parts, .. } => {
                for p in parts {
                    if let crate::ast::TemplatePart::Value(v) = p {
                        self.expr(v, region, arena);
                    }
                }
                Shape::new()
            }
            Value::CastReturn {
                return_location: location,
                operand,
                return_value,
                ..
            } => {
                self.expr(operand, region, arena);
                let returned = self.expr(return_value, region, arena);
                self.return_value(return_value.ty, returned, *location);
                Shape::new()
            }
            Value::Closure(_) => Shape::new(), // 本体を独立して検査し、捕捉した参照にも通常関数の借用契約を適用する。
            _ => {
                value.value.visit_constant_children(&mut |child| {
                    self.expr(child, region, arena);
                });
                Shape::new()
            }
        }
    }
    fn call_location(call: &CheckedExpression) -> Location {
        match call.value {
            Value::Call { location, .. } | Value::Invoke { location, .. } => location,
            _ => unreachable!("checked call handler"),
        }
    }
    fn handler(
        &mut self,
        handler: &super::super::ir::CallHandler,
        ty: Type,
        region: usize,
        arena: Option<usize>,
    ) -> Shape {
        use super::super::ir::CallHandler;
        match handler {
            CallHandler::Branches(arms) => {
                let result = self.shape(ty);
                let saved = self.recovery.replace(result.clone());
                for (_, body) in arms {
                    self.block(body, region, arena);
                }
                self.recovery = saved;
                result
            }
            CallHandler::Value(v) => self.expr(v, region, arena),
            CallHandler::Return { value: v, location } => {
                let value = self.expr(v, region, arena);
                self.return_value(v.ty, value, *location);
                Shape::new()
            }
            CallHandler::Fail(Some(v)) => {
                self.expr(v, region, arena);
                Shape::new()
            }
            _ => Shape::new(),
        }
    }
    fn block(&mut self, body: &[CheckedStatement], region: usize, arena: Option<usize>) {
        let locals = self.locals.clone();
        let inner = self.region(region, self.regions[region].end);
        self.statements(body, inner, arena);
        self.locals = locals;
    }
    fn statements(&mut self, statements: &[CheckedStatement], region: usize, arena: Option<usize>) {
        for statement in statements {
            match statement {
                CheckedStatement::Arena { body, end, .. } => {
                    let locals = self.locals.clone();
                    let inner = self.region(region, *end);
                    self.statements(body, inner, Some(inner));
                    self.locals = locals;
                }
                CheckedStatement::Binding {
                    id,
                    initializer,
                    location,
                    ty,
                    ..
                } => {
                    let value = self.expr(initializer, region, arena);
                    let cell = self.cell(region, *location, *ty);
                    self.locals.insert(*id, cell);
                    self.write(Place::Direct(cell), value, *location);
                }
                CheckedStatement::Assign {
                    id,
                    fields,
                    value,
                    location,
                    global,
                    ..
                } => {
                    let value = self.expr(value, region, arena);
                    if !value.is_empty() {
                        assert!(!global, "global pointer types are rejected");
                        let target = self.project(self.locals[id], fields);
                        self.write(Place::Direct(target), value, *location);
                    }
                }
                CheckedStatement::Store {
                    target,
                    id,
                    value,
                    body,
                    location,
                } => {
                    let destination = self.place(target, region, arena);
                    let address = self.node();
                    self.constraints
                        .push(Constraint::Address(address, destination.clone()));
                    let saved = self.cell(region, *location, target.ty.pointer());
                    self.constraints.push(Constraint::Copy(saved, address));
                    let previous = self.locals.insert(*id, saved);
                    self.statements(body, region, arena);
                    let value = self.expr(value, region, arena);
                    self.write(destination, value, *location);
                    if let Some(previous) = previous {
                        self.locals.insert(*id, previous);
                    } else {
                        self.locals.remove(id);
                    }
                }
                CheckedStatement::Block(body) => self.block(body, region, arena),
                CheckedStatement::If {
                    branches,
                    otherwise,
                } => {
                    for (c, b) in branches {
                        self.expr(c, region, arena);
                        self.block(b, region, arena);
                    }
                    if let Some(b) = otherwise {
                        self.block(b, region, arena);
                    }
                }
                CheckedStatement::Loop { condition, body } => {
                    if let Some(c) = condition {
                        self.expr(c, region, arena);
                    }
                    self.block(body, region, arena);
                }
                CheckedStatement::For {
                    id,
                    start,
                    end,
                    body,
                    ..
                } => {
                    self.expr(start, region, arena);
                    if let Some(end) = end {
                        self.expr(end, region, arena);
                    }
                    let locals = self.locals.clone();
                    let cell = self.cell(region, self.regions[region].end, Type::Void);
                    self.locals.insert(*id, cell);
                    self.block(body, region, arena);
                    self.locals = locals;
                }
                CheckedStatement::Match {
                    id, target, arms, ..
                } => {
                    self.expr(target, region, arena);
                    let cell = self.cell(region, self.regions[region].end, Type::Void);
                    self.locals.insert(*id, cell);
                    for (p, b) in arms {
                        if let Some(p) = p {
                            self.expr(p, region, arena);
                        }
                        self.block(b, region, arena);
                    }
                }
                CheckedStatement::Log { parts, .. } => {
                    for p in parts {
                        if let crate::ast::TemplatePart::Value(v) = p {
                            self.expr(v, region, arena);
                        }
                    }
                }
                CheckedStatement::Return { value, location } => {
                    let shape = self.expr(value, region, arena);
                    self.return_value(value.ty, shape, *location);
                }
                CheckedStatement::Recover(v) => {
                    let shape = self.expr(v, region, arena);
                    let result = self.recovery.clone().expect("recover has a handler");
                    self.join(&result, shape);
                }
                CheckedStatement::Fail(v) | CheckedStatement::Expression(v) => {
                    self.expr(v, region, arena);
                }
                CheckedStatement::HandleCall { id, call, handler } => {
                    let value = self.expr(call, region, arena);
                    let alternative = self.handler(handler, call.ty, region, arena);
                    self.join(&value, alternative);
                    let cell = self.cell(region, Self::call_location(call), call.ty);
                    self.locals.insert(*id, cell);
                    self.write(Place::Direct(cell), value, Self::call_location(call));
                }
                CheckedStatement::SliceReturn {
                    location,
                    id,
                    operation,
                    return_value,
                }
                | CheckedStatement::ArithmeticReturn {
                    location,
                    id,
                    operation,
                    return_value,
                } => {
                    self.expr(operation, region, arena);
                    let returned = self.expr(return_value, region, arena);
                    self.return_value(return_value.ty, returned, *location);
                    let cell = self.cell(region, self.regions[region].end, Type::Void);
                    self.locals.insert(*id, cell);
                }
                CheckedStatement::CastReturn {
                    return_location: location,
                    id,
                    operand,
                    return_value,
                    ..
                } => {
                    self.expr(operand, region, arena);
                    let returned = self.expr(return_value, region, arena);
                    self.return_value(return_value.ty, returned, *location);
                    let cell = self.cell(region, self.regions[region].end, Type::Void);
                    self.locals.insert(*id, cell);
                }
                CheckedStatement::Break
                | CheckedStatement::Continue
                | CheckedStatement::Ignore(_) => {}
            }
        }
    }
    fn destinations(&mut self, place: &Place) -> Vec<Node> {
        match place {
            Place::Direct(n) => vec![*n],
            Place::Indirect(n, path) => self.targets[*n]
                .clone()
                .into_iter()
                .map(|cell| self.project(cell, path))
                .collect(),
        }
    }
    fn outlives(&self, source: usize, mut destination: usize) -> bool {
        // 借用先は呼び出し中ずっと有効。ただし借用した実体への参照保存は別途禁止する。
        if self.borrowed.contains(&source) {
            return !self.borrowed.contains(&destination);
        }
        loop {
            if source == destination {
                return true;
            }
            let Some(parent) = self.regions[destination].parent else {
                return false;
            };
            destination = parent;
        }
    }
    fn member_path(&self, cell: Node) -> String {
        let cell = &self.cells[&cell];
        let mut ty = cell.ty;
        let mut names = Vec::new();
        for &index in &cell.path {
            let Type::Struct(id) = ty else {
                unreachable!("checked member path")
            };
            let field = &self.scope.structs[id].fields[index];
            names.push(field.name.as_str());
            ty = field.ty;
        }
        if names.is_empty() {
            String::new()
        } else {
            format!(" in member '{}'", names.join("."))
        }
    }
    fn solve(&mut self) -> Result<(), Diagnostic> {
        // 制約は固定し、候補集合と保存場所だけを更新する。列全体の複製は不要。
        let constraints = std::mem::take(&mut self.constraints);
        solver::propagate(self, &constraints);
        let result = self.validate(&constraints);
        self.constraints = constraints;
        result
    }
    // 候補の収束後に判定し、走査順でまだ見えていない参照先を安全と取り違えない。
    fn validate(&mut self, constraints: &[Constraint]) -> Result<(), Diagnostic> {
        self.validate_calls()?;
        for constraint in constraints {
            let Constraint::Store(place, src, location) = constraint else {
                continue;
            };
            for destination in self.destinations(place) {
                if self.borrowed.contains(&self.cells[&destination].region) {
                    let cell = &self.cells[&destination];
                    let mut error = location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "arena.parameter",
                            "cannot store a pointer into a borrowed caller object{}",
                            self.member_path(destination)
                        ),
                    );
                    error.related.push(cell.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "arena.parameter-regions",
                            "this parameter is borrowed only for the duration of the call"
                        ),
                    ));
                    return Err(error);
                }
                for &source in &self.targets[*src] {
                    let source = &self.cells[&source];
                    if !self.outlives(source.region, self.cells[&destination].region) {
                        let mut error = location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "arena.store",
                                "reference may outlive its arena{}",
                                self.member_path(destination)
                            ),
                        );
                        error.related.push(source.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "arena.lifetime",
                                "{}",
                                match source.origin {
                                    Origin::Allocation => "referenced object is allocated here",
                                    Origin::Call => "this call may allocate the referenced object",
                                    Origin::Parameter =>
                                        "referenced object comes from this parameter",
                                }
                            ),
                        ));
                        error.related.push(self.regions[source.region].end.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("arena.lifetime", "its arena ends here"),
                        ));
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }
}

pub(super) fn check(
    statements: &[CheckedStatement],
    scope: &Scope,
    function: &crate::ast::Function,
    references: &BTreeMap<String, super::Binding>,
) -> Result<(), Diagnostic> {
    let mut analysis = Analysis::new(scope);
    let (region, arena) = analysis.parameters(function, references);
    analysis.statements(statements, region, arena);
    analysis.solve()
}
