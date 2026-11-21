//! 実行順によらない候補の包含制約を解く。各保存場所は自身の寿命を持ち、
//! 別名経由のメンバーも同じセルへ合流させる。安全でも候補を絞れない保存は拒否する。
//!
//! **[impl.lifetime-flow]** ポインタを含みうる値の流れは、すべて制約（Copy・Limit・Load・Store・Address）へ
//! 変換してから検査し、構文ごとに個別の寿命検査を書かない。式・文・失敗処理で分岐する
//! match には `_` を使わず全種類を列挙する。
mod boundaries;
mod callables;
mod solver;
mod summaries;

use super::*;
use crate::check::CheckedHandler;
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
type Node = usize;
// 実体の所属はCellに残す。関数の戻り値に課す利用期間だけを参照側へ付け、
// 短く借りた外側の実体を、内側に確保した実体と取り違えない。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Reference {
    target: Node,
    limit: Option<usize>,
}
impl Reference {
    fn object(target: Node) -> Self {
        Self {
            target,
            limit: None,
        }
    }
}
// 構造体の各ポインタ葉をフィールド番号列で区別する。ポインタ自身の値は空の列。
type Shape = BTreeMap<Vec<usize>, Node>;
// 型検査後の呼び出し境界では葉を追加・削除しないため、並びだけを保持する。
type Leaves = Vec<(Vec<usize>, Node)>;

/// 呼び出し先を待つ本体と、検査・要約が済んだ本体を区別する。
/// 呼び出しのない関数の解析グラフを、全具体化が終わるまで保持しない。
#[derive(Clone)]
pub(crate) enum Body {
    Input(Rc<callables::Input>),
    Pending(Rc<Analysis>),
    Resolved(Rc<summaries::Summary>),
}

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
#[derive(Clone)]
struct Region {
    parent: Option<usize>,
    end: Location,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Direct(Node),
    Indirect(Node, Vec<usize>),
}
#[derive(Clone)]
enum Constraint {
    Copy(Node, Node),
    // 参照先を変えずに利用期間を制限する関数境界。
    Limit(Node, Node, usize),
    Load(Node, Place),
    Address(Node, Place),
    Store(Place, Node, Location),
}
#[derive(Clone)]
/// 型検査で作った寿命制約。具体化済みの呼び出し先がそろうまで保持する。
pub(crate) struct Analysis {
    structs: Rc<crate::check::type_parameters::structs::Structs>,
    function_id: usize,
    resolved: bool,
    objects: BTreeMap<Node, summaries::Object>,
    returned: Shape,
    direct_calls: Vec<summaries::Call>,
    effect_calls: BTreeMap<usize, Location>,
    effect_notes: BTreeMap<usize, BTreeSet<summaries::CallNote>>,
    candidate_notes: BTreeMap<(usize, Location), BTreeSet<summaries::CallNote>>,
    effect_stores: BTreeMap<usize, summaries::StoreInfo>,
    objection: bool,
    // 字句上のobjection内で作ったStoreの添字。呼出し契約由来の制約は含めない。
    objections: BTreeSet<usize>,
    // 候補集合はnullを表さないため、非nullの証明を値と不変の保存場所に別途保持する。
    nonnull_values: BTreeSet<Node>,
    return_region: Option<usize>,
    recovery: Option<Shape>,
    // 各値が指しうる保存場所。実体の領域は参照変数の宣言位置から独立している。
    targets: Vec<BTreeSet<Reference>>,
    cells: BTreeMap<Node, Cell>,
    projections: BTreeMap<(Node, Vec<usize>), Node>,
    regions: Vec<Region>,
    constraints: Vec<Constraint>,
    locals: BTreeMap<usize, Node>,
}

impl Analysis {
    fn new(scope: &Scope) -> Self {
        Self {
            structs: scope.structs.clone(),
            function_id: 0,
            resolved: false,
            objects: BTreeMap::new(),
            returned: Shape::new(),
            direct_calls: Vec::new(),
            effect_calls: BTreeMap::new(),
            effect_notes: BTreeMap::new(),
            candidate_notes: BTreeMap::new(),
            effect_stores: BTreeMap::new(),
            objection: false,
            objections: BTreeSet::new(),
            nonnull_values: BTreeSet::new(),
            return_region: None,
            recovery: None,
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
            Type::Struct(id) => self
                .structs
                .get(id)
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
            .map(|p| {
                let n = self.node();
                if self.field_type(ty, &p).is_nonnull() {
                    self.nonnull_values.insert(n);
                }
                (p, n)
            })
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
            if self.objection {
                self.objections.insert(self.constraints.len());
            }
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
    // 分岐で候補を合流する。非null性は分岐の結果型で決まる。
    fn alternatives(&mut self, ty: Type, alternatives: Vec<Shape>) -> Shape {
        let result = self.shape(ty);
        for shape in alternatives {
            self.join(&result, shape);
        }
        result
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
            // 最外側の暗黙変換は実体と、保存直前の非nullの証明を変えない。
            Value::Cast(inner) if value.ty.pointee().is_some() => self.expr(inner, region, arena),
            Value::New {
                value: initial,
                location,
            } => {
                let ty = initial.ty;
                let initial = self.expr(initial, region, arena);
                let cell = self.cell(arena.expect("new has a lexical arena"), *location, ty);
                self.objects
                    .insert(cell, summaries::Object::Allocation(self.function_id, cell));
                self.write(Place::Direct(cell), initial, *location);
                let n = self.node();
                self.targets[n].insert(Reference::object(cell));
                self.nonnull_values.insert(n);
                BTreeMap::from([(vec![], n)])
            }
            Value::Local(id) => {
                if contains_in(value.ty, &self.structs) {
                    let cell = self.locals[id];
                    self.read(value.ty, Place::Direct(cell))
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
                let typed = self.shape(value.ty);
                self.join(&typed, result);
                typed
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
                let typed = self.shape(value.ty);
                self.join(&typed, result);
                typed
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
                let mut alternatives = Vec::new();
                for (condition, branch) in branches {
                    self.expr(condition, region, arena);
                    alternatives.push(self.expr(branch, region, arena));
                }
                alternatives.push(self.expr(otherwise, region, arena));
                self.alternatives(value.ty, alternatives)
            }
            Value::Match { target, arms, .. } => {
                self.expr(target, region, arena);
                let mut alternatives = Vec::new();
                for (pattern, arm) in arms {
                    if let Some(pattern) = pattern {
                        self.expr(pattern, region, arena);
                    }
                    alternatives.push(self.expr(arm, region, arena));
                }
                self.alternatives(value.ty, alternatives)
            }
            Value::Call {
                arguments,
                location,
                id,
            } => {
                let shapes: Vec<_> = arguments
                    .iter()
                    .map(|argument| (argument.ty, self.expr(argument, region, arena)))
                    .collect();
                let receiver = if let Some(CheckedExpression {
                    value:
                        Value::Receiver {
                            target,
                            mutable: true,
                        },
                    ..
                }) = arguments.first()
                {
                    Some(self.place(target, region, arena))
                } else {
                    None
                };
                let result = self.shape(value.ty);
                let captures = arguments
                    .iter()
                    .enumerate()
                    .filter_map(|(i, arg)| match &arg.value {
                        Value::Receiver {
                            target,
                            mutable: true,
                        } => Some((i, self.place(target, region, arena))),
                        _ => None,
                    })
                    .collect();
                self.direct_calls.push(summaries::Call {
                    id: *id,
                    arguments: shapes
                        .into_iter()
                        .map(|(ty, shape)| (ty, shape.into_iter().collect()))
                        .collect(),
                    result: result
                        .iter()
                        .map(|(path, &node)| (path.clone(), node))
                        .collect(),
                    receiver,
                    captures,
                    region: arena.expect("checked arena call"),
                    location: *location,
                    notes: self
                        .candidate_notes
                        .get(&(*id, *location))
                        .cloned()
                        .unwrap_or_default(),
                });
                result
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
            Value::Closure(closure) => {
                // 本体は呼び出し文脈ごとに別の要約へ移した。ここでは捕捉値の評価だけを行う。
                for capture in &closure.captures {
                    self.expr(&capture.value, region, arena);
                }
                Shape::new()
            }
            Value::Receiver { target, .. } => self.expr(target, region, arena),
            Value::Cast(v)
            | Value::NonNull(v)
            | Value::CBytes(v)
            | Value::CByteLen { operand: v, .. }
            | Value::FunctionBorrow(v)
            | Value::Len(v)
            | Value::Negate(v)
            | Value::Truncate(v)
            | Value::BitNot(v)
            | Value::Not(v)
            | Value::TextView(v)
            | Value::Intern { operand: v, .. } => {
                self.expr(v, region, arena);
                Shape::new()
            }
            Value::CheckedCast {
                operand, fallback, ..
            } => {
                self.expr(operand, region, arena);
                self.expr(fallback, region, arena);
                Shape::new()
            }
            Value::Binary { left, right, .. } | Value::Logical { left, right, .. } => {
                self.expr(left, region, arena);
                self.expr(right, region, arena);
                Shape::new()
            }
            Value::Comparison {
                operands, methods, ..
            } => {
                let values: Vec<_> = operands
                    .iter()
                    .map(|operand| {
                        (
                            operand.ty,
                            self.expr(operand, region, arena).into_iter().collect(),
                        )
                    })
                    .collect();
                for (index, method) in methods.iter().enumerate() {
                    if let Some(id) = method {
                        self.direct_calls.push(summaries::Call {
                            id: *id,
                            arguments: vec![values[index].clone(), values[index + 1].clone()],
                            result: Vec::new(),
                            receiver: None,
                            captures: BTreeMap::new(),
                            region: arena.expect("functions have a current arena"),
                            location: self.regions[region].end,
                            notes: BTreeSet::new(),
                        });
                    }
                }
                Shape::new()
            }
            Value::Template { parts, .. } => {
                for part in parts {
                    if let crate::ast::TemplatePart::Value(v) = part {
                        self.expr(v, region, arena);
                    }
                }
                Shape::new()
            }
            Value::Slice {
                target,
                start,
                end,
                handler,
                ..
            } => {
                self.expr(target, region, arena);
                for v in [start, end].into_iter().flatten() {
                    self.expr(v, region, arena);
                }
                self.checked_handler(handler.as_ref(), region, arena);
                Shape::new()
            }
            Value::IntegerArithmetic { handler, .. } => {
                value.value.visit_constant_children(&mut |child| {
                    self.expr(child, region, arena);
                });
                self.checked_handler(handler.as_ref(), region, arena);
                Shape::new()
            }
            Value::Exact(_) => {
                value.value.visit_constant_children(&mut |child| {
                    self.expr(child, region, arena);
                });
                Shape::new()
            }
            Value::CIntegerLiteral { .. }
            | Value::GenericNumericLiteral { .. }
            | Value::Integer { .. }
            | Value::Float(_)
            | Value::Function(_)
            | Value::Bool(_)
            | Value::Text { .. }
            | Value::Void => Shape::new(),
        }
    }
    fn checked_handler(
        &mut self,
        handler: Option<&CheckedHandler>,
        region: usize,
        arena: Option<usize>,
    ) {
        match handler {
            None => {}
            Some(CheckedHandler::Value(v)) => {
                self.expr(v, region, arena);
            }
            Some(CheckedHandler::Return { value, location }) => {
                let returned = self.expr(value, region, arena);
                self.return_value(value.ty, returned, *location);
            }
        }
    }
    fn call_location(call: &CheckedExpression) -> Location {
        match &call.value {
            Value::Call { location, .. } | Value::Invoke { location, .. } => *location,
            Value::If { otherwise, .. } => Self::call_location(otherwise),
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
            CallHandler::Fail(None) | CallHandler::Ignore(_) => Shape::new(),
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
                    ..
                } => {
                    let value = self.expr(value, region, arena);
                    if !value.is_empty() {
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
                CheckedStatement::Objection(body) => {
                    let saved = self.objection;
                    self.objection = true;
                    self.block(body, region, arena);
                    self.objection = saved;
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
                .map(|reference| self.project(reference.target, path))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }
    }
    fn outlives(&self, source: usize, mut destination: usize) -> bool {
        // 借用先は呼び出し中ずっと有効。ただし借用した実体への参照保存は別途禁止する。

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
            let declaration = self.structs.get(id);
            let field = &declaration.fields[index];
            names.push(field.name.clone());
            ty = field.ty;
        }
        if names.is_empty() {
            String::new()
        } else {
            format!(" in member '{}'", names.join("."))
        }
    }
    fn reference_outlives(&self, source: Reference, destination: usize) -> bool {
        self.outlives(self.cells[&source.target].region, destination)
            && source
                .limit
                .is_none_or(|limit| self.outlives(limit, destination))
    }
    fn solve(&mut self) -> Result<(), Diagnostic> {
        // 制約は固定し、候補集合と保存場所だけを更新する。列全体の複製は不要。
        let constraints = std::mem::take(&mut self.constraints);
        solver::propagate(self, &constraints);
        let result = self.validate(&constraints);
        self.constraints = constraints;
        result
    }
    fn call_diagnostic(&self, index: usize, mut error: Diagnostic) -> Diagnostic {
        if let Some(notes) = self.effect_notes.get(&index) {
            for note in notes {
                error.related.push(note.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("arena.call-diagnostic", "{}", note.message),
                ));
            }
        }
        if let Some(location) = self.effect_calls.get(&index) {
            error.related.push(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "arena.call-diagnostic",
                    "this call applies the store to its actual arguments"
                ),
            ));
        }
        error
    }
    // 候補の収束後に判定し、走査順でまだ見えていない参照先を安全と取り違えない。
    fn root_references_only(&self) -> bool {
        // 全参照が呼び出し元のArenaに属し、より短い利用期間もなければ、
        // 本体内の保存先より長命である。局所変数のアドレスや内側のArena、
        // 独立した借用先を含む場合は通常の検査に回す。
        self.regions
            .iter()
            .skip(1)
            .all(|region| region.parent.is_some())
            && self.direct_calls.iter().all(|call| call.region == 0)
            && self.targets.iter().flatten().all(|source| {
                self.cells[&source.target].region == 0
                    && source.limit.is_none_or(|limit| limit == 0)
            })
    }

    fn validate(&mut self, constraints: &[Constraint]) -> Result<(), Diagnostic> {
        if self.root_references_only() {
            return Ok(());
        }
        for (index, constraint) in constraints.iter().enumerate() {
            let Constraint::Store(place, src, location) = constraint else {
                continue;
            };
            let destinations = self.destinations(place);
            // 実行される保存について、非nullで全候補対が違反する場合だけを証明済みとする。
            // 候補の空集合は証明に使わない。集合が変わらない状態で全保存先をまとめて判定する。
            let proven = self.nonnull_values.contains(src)
                && !destinations.is_empty()
                && !self.targets[*src].is_empty()
                && destinations.iter().all(|dst| {
                    self.targets[*src]
                        .iter()
                        .all(|source| !self.reference_outlives(*source, self.cells[dst].region))
                });
            // 戻り値の利用期間は明示的な契約であり、objectionでは延長できない。
            // 実体の所属は変えず、外側の実体でも参照値そのものの持ち出しを拒否する。
            for &destination in &destinations {
                for source in &self.targets[*src] {
                    if let Some(limit) = source.limit
                        && !self.outlives(limit, self.cells[&destination].region)
                    {
                        if !self.outlives(
                            self.cells[&source.target].region,
                            self.cells[&destination].region,
                        ) {
                            continue;
                        }
                        let message = crate::diagnostic!(
                            "arena.return-lifetime",
                            "returned reference cannot be stored beyond its permitted lifetime"
                        );
                        let mut error = location.error(ErrorKind::InvalidSource, message);
                        error.related.push(self.regions[limit].end.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "arena.lifetime",
                                "its permitted lifetime ends here"
                            ),
                        ));
                        return Err(self.call_diagnostic(index, error));
                    }
                }
            }
            let limited = destinations.iter().any(|dst| {
                self.targets[*src].iter().any(|source| {
                    source
                        .limit
                        .is_some_and(|limit| !self.outlives(limit, self.cells[dst].region))
                })
            });
            if self.objections.contains(&index) && !proven && !limited {
                continue;
            }
            for destination in destinations {
                for &source in &self.targets[*src] {
                    if !self.reference_outlives(source, self.cells[&destination].region) {
                        let lifetime = self.cells[&source.target].region;
                        let source = &self.cells[&source.target];
                        let mut error = location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "arena.store",
                                "{}{}",
                                if proven {
                                    "proven lifetime violation: reference outlives its arena"
                                } else {
                                    "reference may outlive its arena"
                                },
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
                        error.related.push(self.regions[lifetime].end.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("arena.lifetime", "its arena ends here"),
                        ));
                        return Err(self.call_diagnostic(index, error));
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
    id: usize,
) -> Result<Body, Diagnostic> {
    let mut analysis = Analysis::new(scope);
    analysis.function_id = id;
    let (region, arena) = analysis.parameters(function, references, scope);
    Ok(Body::Input(Rc::new(callables::Input {
        base: analysis,
        region,
        arena,
        parameters: function
            .parameters
            .iter()
            .map(|p| scope.resolve_type_at(p.ty, p.location).unwrap())
            .collect(),
        name: function.name.clone(),
        location: function.location,
        result: scope.result,
        errors: function.errors.clone(),
        function_types: scope.function_types.clone(),
        statements: statements.to_vec(),
    })))
}

pub(in crate::check) use callables::check_program;
