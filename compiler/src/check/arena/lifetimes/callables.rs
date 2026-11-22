//! 関数値の候補を集め、寿命検査だけに使う呼び出し文脈を作る。
//! Cへ渡すIRは変更しない。fn仮引数は実引数で区別し、static fnは仕様の保存単位で合流する。
use super::*;
use crate::check::ir::{CallHandler, CheckedFunction};

#[derive(Clone)]
pub(crate) struct Input {
    pub base: Option<Rc<Analysis>>,
    pub(super) structs: Rc<crate::check::type_parameters::structs::Structs>,
    pub function_id: usize,
    pub callables: bool,
    pub parameter_metadata: Vec<(Type, Location, bool)>,
    pub end: Location,
    pub region: usize,
    pub arena: Option<usize>,
    pub parameters: Vec<Type>,
    pub name: String,
    pub location: Location,
    pub result: Type,
    pub errors: Vec<crate::ast::ErrorVariant>,
    pub function_types: Rc<std::cell::RefCell<Vec<crate::ast::FunctionType>>>,
    pub statements: Vec<CheckedStatement>,
}

impl Input {
    // 全本体の型検査が終わってポインタの使用が分かるまで、制約やIRの複製を作らない。
    fn with_body(&self, statements: &[CheckedStatement], warn_allocations: bool) -> Rc<Self> {
        let mut input = self.clone();
        let mut analysis = Analysis::with_structs(input.structs.clone());
        analysis.function_id = input.function_id;
        analysis.warn_allocations = warn_allocations;
        (input.region, input.arena) = analysis.parameters(&input.parameter_metadata, input.end);
        input.base = Some(Rc::new(analysis));
        input.statements = statements.to_vec();
        Rc::new(input)
    }
}

// 関数値の由来と、クロージャが保持する値・保存場所を別々に持つ。
// 文脈のキーには関数の候補だけを入れ、呼び出し元のローカル番号は入れない。
#[derive(Clone)]
struct Bound {
    function: usize,
    site: Location,
    captures: Vec<Capture>,
}
#[derive(Clone)]
struct Capture {
    value: CheckedExpression,
    reference: bool,
    functions: Values,
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BoundKey(usize, Vec<ValueKey>);
type ValueKey = Vec<(Vec<usize>, Vec<BoundKey>)>;
type Values = BTreeMap<Vec<usize>, BTreeMap<BoundKey, Bound>>;
impl Bound {
    fn key(&self) -> BoundKey {
        BoundKey(
            self.function,
            self.captures
                .iter()
                .map(|c| value_key(&c.functions))
                .collect(),
        )
    }
}
fn value_key(values: &Values) -> ValueKey {
    values
        .iter()
        .map(|(path, set)| (path.clone(), set.keys().cloned().collect()))
        .collect()
}
fn single(bound: Bound) -> Values {
    BTreeMap::from([(vec![], BTreeMap::from([(bound.key(), bound)]))])
}
fn merge(into: &mut Values, from: Values) -> bool {
    let mut changed = false;
    for (path, values) in from {
        let set = into.entry(path).or_default();
        for (key, value) in values {
            changed |= set.insert(key, value).is_none();
        }
    }
    changed
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Flow {
    Parameter(usize, usize),
    Return(usize),
    Heap(usize),
    Field(usize, usize, Type),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ContextKey(usize, Vec<ValueKey>);
#[derive(Clone)]
struct Context {
    function: usize,
    arguments: Vec<Values>,
}
#[derive(Default)]
struct Program {
    warn_allocations: bool,
    templates: BTreeMap<usize, Rc<Input>>,
    closures: BTreeMap<usize, usize>,
    contexts: BTreeMap<usize, Context>,
    keys: BTreeMap<ContextKey, usize>,
    // static fnは呼び出し文脈を分けず、宣言の仮引数・戻り値・構造体フィールドごとに合流。
    values: BTreeMap<Flow, Values>,
    readers: BTreeMap<Flow, BTreeSet<usize>>,
    pending: std::collections::VecDeque<usize>,
    queued: BTreeSet<usize>,
    current: usize,
    allocations: BTreeMap<(usize, Location), usize>,
    next: usize,
    empty: usize,
    failure: Option<Diagnostic>,
    context_counts: BTreeMap<usize, usize>,
}
impl Program {
    fn enqueue(&mut self, id: usize) {
        if self.queued.insert(id) {
            self.pending.push_back(id);
        }
    }
    // 候補を読んだ文脈だけを、その候補が増えたときに再計算する。
    // 名前付き呼び出しの長い連鎖で全本体を繰り返し走査しない。
    fn read(&mut self, key: Flow) -> Values {
        self.readers
            .entry(key.clone())
            .or_default()
            .insert(self.current);
        self.values.get(&key).cloned().unwrap_or_default()
    }
    fn write(&mut self, key: Flow, values: Values) {
        if values.is_empty() {
            return;
        }
        if merge(self.values.entry(key.clone()).or_default(), values) {
            for id in self.readers.get(&key).cloned().unwrap_or_default() {
                self.enqueue(id);
            }
        }
    }

    fn request(&mut self, function: usize, arguments: &[Values]) -> usize {
        let Some(input) = self.templates.get(&function).cloned() else {
            return function;
        };
        if !input.callables {
            return function;
        }
        let mut bound = vec![Values::new(); input.parameters.len()];
        for (index, (&ty, values)) in input.parameters.iter().zip(arguments).enumerate() {
            if matches!(ty, Type::Function(_)) {
                bound[index] = values.clone();
            } else {
                self.write(Flow::Parameter(function, index), values.clone());
            }
        }
        // 再帰のたびに前のクロージャを捕捉する形では、環境の木が無限に深くなる。
        // 資源上限では検査を省略せず、ソース位置を付けて停止する。
        fn depth(values: &Values, path: &mut BTreeSet<usize>) -> usize {
            let mut maximum = 0;
            for bound in values.values().flat_map(|set| set.values()) {
                if !bound.captures.is_empty() && !path.insert(bound.function) {
                    return 65;
                }
                for capture in &bound.captures {
                    maximum = maximum.max(1 + depth(&capture.functions, path));
                }
                if !bound.captures.is_empty() {
                    path.remove(&bound.function);
                }
            }
            maximum
        }
        if bound
            .iter()
            .any(|values| depth(values, &mut BTreeSet::new()) > 64)
        {
            self.failure = Some(input.location.error(ErrorKind::InvalidSource, crate::diagnostic!("arena.analysis-limit", "recursive or deeply nested closure environments exceed the lifetime analysis limit")));
            return self.empty;
        }
        let key = ContextKey(function, bound.iter().map(value_key).collect());
        if let Some(&id) = self.keys.get(&key) {
            return id;
        }
        let count = self.context_counts.entry(function).or_default();
        if *count >= 256 {
            self.failure = Some(input.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "arena.analysis-limit",
                    "function exceeds the lifetime analysis limit of 256 callback contexts"
                ),
            ));
            return self.empty;
        }
        *count += 1;
        // 関数値による特殊化のない文脈は元の関数番号を保つ。
        // 関数値を扱わない呼び出し元の要約からも、そのまま参照できる。
        let id = if bound.iter().all(Values::is_empty) {
            function
        } else {
            let id = self.next;
            self.next += 1;
            id
        };
        self.keys.insert(key, id);
        self.contexts.insert(
            id,
            Context {
                function,
                arguments: bound,
            },
        );
        self.enqueue(id);
        id
    }
    fn closure(&mut self, input: Rc<Input>, statements: &[CheckedStatement]) -> usize {
        let key = Rc::as_ptr(&input) as usize;
        if let Some(&id) = self.closures.get(&key) {
            return id;
        }
        let id = self.next;
        self.next += 1;
        self.closures.insert(key, id);
        self.templates
            .insert(id, input.with_body(statements, self.warn_allocations));
        id
    }
}

struct Walk<'a> {
    program: &'a mut Program,
    analysis: Analysis,
    function: usize,
    locals: BTreeMap<usize, Values>,
    returned: Values,
    recoveries: Vec<Values>,
    next_capture: usize,
    function_types: Rc<std::cell::RefCell<Vec<crate::ast::FunctionType>>>,
}
impl Walk<'_> {
    // 環境の値も通常の仮引数として寿命要約へ渡す。参照捕捉はSlotとして保存場所を保つ。
    // これは検査用の番号であり、生成関数の引数や実行時の呼出し規約を増やすものではない。
    fn bind(&mut self, values: &Values) -> Values {
        let mut result = Values::new();
        for (path, set) in values {
            for bound in set.values() {
                let mut bound = bound.clone();
                for capture in &mut bound.captures {
                    let index = self.next_capture;
                    self.next_capture += 1;
                    let local = usize::MAX - index;
                    let ty = capture.value.ty;
                    let location = self.analysis.regions[0].end;
                    let storage = if capture.reference { 0 } else { 1 };
                    let cell = self.analysis.cell(storage, location, ty);
                    self.analysis.locals.insert(local, cell);
                    if capture.reference {
                        self.analysis
                            .objects
                            .insert(cell, summaries::Object::Slot(index));
                    }
                    let mut objects = Vec::new();
                    let mut roots = BTreeMap::new();
                    for path in self.analysis.paths(ty) {
                        let target = reference_target(self.analysis.field_type(ty, &path)).unwrap();
                        let object = self.analysis.parameter_root(
                            index,
                            target,
                            0,
                            location,
                            &mut roots,
                            &mut objects,
                        );
                        let field = self.analysis.project(cell, &path);
                        self.analysis.targets[field].insert(Reference::object(object));
                    }
                    for (ty, object) in objects {
                        self.analysis
                            .objects
                            .insert(object, summaries::Object::ReachableParameter(index, ty));
                    }
                    capture.value = CheckedExpression {
                        ty,
                        value: Value::Local(local),
                    };
                    capture.functions = self.bind(&capture.functions);
                    self.locals.insert(local, capture.functions.clone());
                }
                result
                    .entry(path.clone())
                    .or_default()
                    .insert(bound.key(), bound);
            }
        }
        result
    }
    fn append_captures(arguments: &mut Vec<CheckedExpression>, values: &Values) {
        for set in values.values() {
            for bound in set.values() {
                for capture in &bound.captures {
                    arguments.push(if capture.reference {
                        CheckedExpression {
                            ty: capture.value.ty,
                            value: Value::Receiver {
                                target: Box::new(capture.value.clone()),
                                mutable: true,
                            },
                        }
                    } else {
                        capture.value.clone()
                    });
                    Self::append_captures(arguments, &capture.functions);
                }
            }
        }
    }
    fn field_key(&self, owner: usize, index: usize) -> Flow {
        let decl = self.analysis.structs.get(owner);
        Flow::Field(
            decl.application.as_ref().map_or(owner, |a| a.declaration),
            index,
            // 関数葉は宣言単位で合流し、呼び出しで具体型を照合する。
            // 複合型の経路は具体型で区別し、Box[T]のTへ別のTの経路を注入しない。
            match decl.fields[index].ty {
                Type::StaticFunction(_) => Type::Void,
                ty => ty,
            },
        )
    }
    fn save_fields(&mut self, ty: Type, values: &Values) {
        let Type::Struct(id) = ty else { return };
        for index in 0..self.analysis.structs.get(id).fields.len() {
            let selected = values
                .iter()
                .filter(|(path, _)| path.first() == Some(&index))
                .map(|(path, set)| (path[1..].to_vec(), set.clone()))
                .collect();
            let key = self.field_key(id, index);
            self.program.write(key, selected);
        }
    }
    fn call(
        &mut self,
        function: usize,
        arguments: &mut Vec<CheckedExpression>,
        values: &[Values],
    ) -> usize {
        let id = self.program.request(function, values);
        if let Some(template) = self.program.templates.get(&function) {
            for (&ty, values) in template.parameters.iter().zip(values) {
                if matches!(ty, Type::Function(_)) {
                    Self::append_captures(arguments, values);
                }
            }
        }
        id
    }
    fn expr(&mut self, expr: &mut CheckedExpression) -> Values {
        let ty = expr.ty;
        let mut values = match &mut expr.value {
            Value::Function { id, location } => single(Bound {
                function: *id,
                site: *location,
                captures: Vec::new(),
            }),
            Value::Array {
                values, lengths, ..
            } => {
                let mut result = Values::new();
                for v in values.iter_mut().chain(lengths) {
                    merge(&mut result, self.expr(v));
                }
                result
            }
            Value::ArrayElement { target, indices } => {
                let result = self.expr(target);
                for (index, _, _) in indices {
                    self.expr(index);
                }
                result
            }
            Value::ArrayView {
                target,
                start,
                end,
                handler,
                ..
            } => {
                let mut result = self.expr(target);
                for v in [start, end].into_iter().flatten() {
                    self.expr(v);
                }
                if let Some(handler) = handler {
                    match handler {
                        CheckedHandler::Value(v) => {
                            merge(&mut result, self.expr(v));
                        }
                        CheckedHandler::Return { value, .. } => {
                            self.expr(value);
                        }
                    }
                }
                result
            }
            Value::ArrayRead { address, handler } => {
                let pointers = self.expr(address);
                let mut result = Values::new();
                for pointer in pointers.values().flat_map(|s| s.values()) {
                    merge(&mut result, self.program.read(Flow::Heap(pointer.function)));
                }
                if let Some(handler) = handler {
                    match handler {
                        CheckedHandler::Value(v) => {
                            merge(&mut result, self.expr(v));
                        }
                        CheckedHandler::Return { value, .. } => {
                            self.expr(value);
                        }
                    }
                }
                result
            }
            Value::Local(id) => self.locals.get(id).cloned().unwrap_or_default(),
            Value::FunctionBorrow(value)
            | Value::Cast(value)
            | Value::Receiver { target: value, .. } => self.expr(value),
            Value::Closure(closure) => {
                let Some(Body::Input(input)) = &closure.function.lifetime else {
                    unreachable!("closure input retained until lifetime checking")
                };
                let function = self
                    .program
                    .closure(input.clone(), &closure.function.statements);
                let captures = closure
                    .captures
                    .iter_mut()
                    .map(|capture| {
                        let functions = self.expr(&mut capture.value);
                        Capture {
                            value: capture.value.clone(),
                            reference: capture.by_reference,
                            functions,
                        }
                    })
                    .collect();
                single(Bound {
                    function,
                    site: input.location,
                    captures,
                })
            }
            Value::Field { target, index } => {
                let values = self.expr(target);
                let mut values = values
                    .into_iter()
                    .filter_map(|(path, set)| {
                        (path.first() == Some(index)).then(|| (path[1..].to_vec(), set))
                    })
                    .collect();
                if let Type::Struct(owner) = target.ty {
                    merge(
                        &mut values,
                        self.program.read(self.field_key(owner, *index)),
                    );
                }
                values
            }
            Value::New { value, location } => {
                let values = self.expr(value);
                if !super::super::callable_type(value.ty, &self.analysis.structs) {
                    return Values::new();
                }
                let key = (self.function, *location);
                let id = if let Some(&id) = self.program.allocations.get(&key) {
                    id
                } else {
                    let id = self.program.next;
                    self.program.next += 1;
                    self.program.allocations.insert(key, id);
                    id
                };
                self.program.write(Flow::Heap(id), values);
                single(Bound {
                    function: id,
                    site: *location,
                    captures: Vec::new(),
                })
            }
            Value::Deref(target) => {
                let pointers = self.expr(target);
                let mut values = Values::new();
                for pointer in pointers.values().flat_map(|set| set.values()) {
                    merge(&mut values, self.program.read(Flow::Heap(pointer.function)));
                }
                values
            }
            Value::Struct { fields } => {
                let mut values = Values::new();
                for (index, value) in fields {
                    for (mut path, set) in self.expr(value) {
                        path.insert(0, *index);
                        values.insert(path, set);
                    }
                }
                values
            }
            Value::With { target, fields } => {
                let mut values = self.expr(target);
                for (index, value) in fields {
                    values.retain(|p, _| p.first() != Some(index));
                    for (mut path, set) in self.expr(value) {
                        path.insert(0, *index);
                        values.insert(path, set);
                    }
                }
                values
            }
            Value::Call { id, arguments, .. } => {
                let function = *id;
                let values: Vec<_> = arguments.iter_mut().map(|a| self.expr(a)).collect();
                *id = self.call(function, arguments, &values);
                self.program.read(Flow::Return(function))
            }
            Value::Invoke {
                target,
                arguments,
                location,
            } => {
                let candidates = self.expr(target);
                let values: Vec<_> = arguments.iter_mut().map(|a| self.expr(a)).collect();
                let mut calls = Vec::new();
                let mut returned = Values::new();
                for bound in candidates.values().flat_map(|set| set.values()) {
                    let mut args = arguments.clone();
                    let mut vals = values.clone();
                    for capture in &bound.captures {
                        args.push(if capture.reference {
                            CheckedExpression {
                                ty: capture.value.ty,
                                value: Value::Receiver {
                                    target: Box::new(capture.value.clone()),
                                    mutable: true,
                                },
                            }
                        } else {
                            capture.value.clone()
                        });
                        vals.push(capture.functions.clone());
                    }
                    let id = self.call(bound.function, &mut args, &vals);
                    if let Some(input) = self.program.templates.get(&bound.function) {
                        self.analysis
                            .candidate_notes
                            .entry((id, *location))
                            .or_default()
                            .extend([
                                summaries::CallNote {
                                    location: input.location,
                                    message: format!("possible call target: {}", input.name),
                                },
                                summaries::CallNote {
                                    location: bound.site,
                                    message: "this function value supplies the call target".into(),
                                },
                            ]);
                    }
                    calls.push(CheckedExpression {
                        ty,
                        value: Value::Call {
                            id,
                            arguments: args,
                            location: *location,
                        },
                    });
                    merge(
                        &mut returned,
                        self.program.read(Flow::Return(bound.function)),
                    );
                }
                // 候補は全て検査する。分岐は寿命検査専用で、生成Cの間接呼び出しはそのまま。
                expr.value = if let Some(last) = calls.pop() {
                    if calls.is_empty() {
                        last.value
                    } else {
                        Value::If {
                            branches: calls
                                .into_iter()
                                .map(|call| {
                                    (
                                        CheckedExpression {
                                            ty: Type::Bool,
                                            value: Value::Bool(true),
                                        },
                                        call,
                                    )
                                })
                                .collect(),
                            otherwise: Box::new(last),
                        }
                    }
                } else {
                    Value::Call {
                        id: self.program.empty,
                        arguments: Vec::new(),
                        location: *location,
                    }
                };
                returned
            }
            Value::Block {
                statements, value, ..
            } => {
                self.statements(statements);
                self.expr(value)
            }
            Value::If {
                branches,
                otherwise,
            } => {
                let mut values = self.expr(otherwise);
                for (condition, value) in branches {
                    self.expr(condition);
                    merge(&mut values, self.expr(value));
                }
                values
            }
            Value::Match { target, arms, .. } => {
                self.expr(target);
                let mut values = Values::new();
                for (pattern, value) in arms {
                    if let Some(p) = pattern {
                        self.expr(p);
                    }
                    merge(&mut values, self.expr(value));
                }
                values
            }
            Value::Comparison {
                operands, methods, ..
            } => {
                let values: Vec<_> = operands.iter_mut().map(|v| self.expr(v)).collect();
                for (index, method) in methods.iter_mut().enumerate() {
                    if let Some(id) = method {
                        *id = self.program.request(*id, &values[index..index + 2]);
                    }
                }
                Values::new()
            }
            Value::HandledCall { call, handler } => {
                let mut values = self.expr(call);
                merge(&mut values, self.handler(handler));
                values
            }
            Value::CastReturn {
                operand,
                return_value,
                ..
            } => {
                self.expr(operand);
                let values = self.expr(return_value);
                merge(&mut self.returned, values);
                Values::new()
            }
            Value::Panic { parts, .. } => {
                for p in parts {
                    if let crate::ast::TemplatePart::Value(v) = p {
                        self.expr(v);
                    }
                }
                Values::new()
            }
            _ => {
                expr.value.visit_constant_parts_mut(
                    &mut |child| {
                        self.expr(child);
                    },
                    &mut |_| {},
                );
                // return付き算術・切り出しハンドラーは定数式の走査には含まれない。
                match &mut expr.value {
                    Value::IntegerArithmetic {
                        handler: Some(CheckedHandler::Return { value, .. }),
                        ..
                    }
                    | Value::Slice {
                        handler: Some(CheckedHandler::Return { value, .. }),
                        ..
                    } => {
                        let values = self.expr(value);
                        merge(&mut self.returned, values);
                    }
                    _ => {}
                }
                Values::new()
            }
        };
        if let Type::StaticFunction(shape) = ty {
            let types = self.function_types.borrow();
            let required = &types[shape];
            for set in values.values_mut() {
                set.retain(|_, bound| {
                    self.program
                        .templates
                        .get(&bound.function)
                        .is_none_or(|input| {
                            input.parameters == required.parameters
                                && input.result == required.result
                                && input.errors == required.errors
                        })
                });
            }
        }
        self.save_fields(ty, &values);
        values
    }
    fn handler(&mut self, handler: &mut CallHandler) -> Values {
        match handler {
            CallHandler::Value(value) => self.expr(value),
            CallHandler::Return { value, .. } => {
                let values = self.expr(value);
                merge(&mut self.returned, values);
                Values::new()
            }
            CallHandler::Branches(branches) => {
                self.recoveries.push(Values::new());
                for (_, body) in branches {
                    self.statements(body);
                }
                self.recoveries.pop().unwrap()
            }
            CallHandler::Fail(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
                Values::new()
            }
            CallHandler::Ignore(_) => Values::new(),
        }
    }
    fn statements(&mut self, statements: &mut [CheckedStatement]) {
        for statement in statements {
            match statement {
                CheckedStatement::Binding {
                    id, initializer, ..
                } => {
                    let values = self.expr(initializer);
                    merge(self.locals.entry(*id).or_default(), values);
                }
                CheckedStatement::Assign {
                    id,
                    root_type,
                    fields,
                    value,
                    ..
                } => {
                    let values = self.expr(value);
                    let values = values
                        .into_iter()
                        .map(|(path, set)| {
                            let mut full = fields.clone();
                            full.extend(path);
                            (full, set)
                        })
                        .collect();
                    self.save_fields(*root_type, &values);
                    merge(self.locals.entry(*id).or_default(), values);
                }
                CheckedStatement::Store {
                    target,
                    value,
                    body,
                    ..
                } => {
                    let indirect = if let Value::Deref(pointer) = &mut target.value {
                        self.expr(pointer)
                    } else {
                        self.expr(target);
                        Values::new()
                    };
                    self.statements(body);
                    let values = self.expr(value);
                    for pointer in indirect.values().flat_map(|set| set.values()) {
                        self.program
                            .write(Flow::Heap(pointer.function), values.clone());
                    }
                    self.save_fields(target.ty, &values);
                    // フィールドへのポインタ経由の保存も、所有する構造体宣言の集合へ加える。
                    if let Value::Field { target, index } = &target.value
                        && let Type::Struct(owner) = target.ty
                    {
                        let key = self.field_key(owner, *index);
                        self.program.write(key, values);
                    }
                }
                CheckedStatement::Return { value, .. } => {
                    let values = self.expr(value);
                    merge(&mut self.returned, values);
                }
                CheckedStatement::Expression(v) | CheckedStatement::Fail(v) => {
                    self.expr(v);
                }
                CheckedStatement::Recover(v) => {
                    let values = self.expr(v);
                    if let Some(recovery) = self.recoveries.last_mut() {
                        merge(recovery, values);
                    }
                }
                CheckedStatement::Arena { body, .. }
                | CheckedStatement::Block(body)
                | CheckedStatement::Objection(body) => self.statements(body),
                CheckedStatement::If {
                    branches,
                    otherwise,
                } => {
                    for (condition, body) in branches {
                        self.expr(condition);
                        self.statements(body);
                    }
                    if let Some(body) = otherwise {
                        self.statements(body);
                    }
                }
                CheckedStatement::Loop { condition, body } => {
                    if let Some(c) = condition {
                        self.expr(c);
                    }
                    self.statements(body);
                }
                CheckedStatement::For {
                    id,
                    start,
                    end,
                    body,
                    ..
                } => {
                    let mut values = self.expr(start);
                    if let Some(e) = end {
                        self.expr(e);
                    }
                    if let Some(array) = start.ty.array_id() {
                        if self.analysis.structs.arrays.borrow().get(array).rank == 1 {
                            let pointers = values;
                            values = Values::new();
                            for pointer in pointers.values().flat_map(|s| s.values()) {
                                merge(&mut values, self.program.read(Flow::Heap(pointer.function)));
                            }
                        }
                        merge(self.locals.entry(*id).or_default(), values);
                    }
                    self.statements(body);
                }
                CheckedStatement::Match { target, arms, .. } => {
                    self.expr(target);
                    for (pattern, body) in arms {
                        if let Some(p) = pattern {
                            self.expr(p);
                        }
                        self.statements(body);
                    }
                }
                CheckedStatement::HandleCall { id, call, handler } => {
                    let mut values = self.expr(call);
                    merge(&mut values, self.handler(handler));
                    merge(self.locals.entry(*id).or_default(), values);
                }
                CheckedStatement::SliceReturn {
                    id,
                    operation,
                    return_value,
                    ..
                }
                | CheckedStatement::ArithmeticReturn {
                    id,
                    operation,
                    return_value,
                    ..
                } => {
                    let values = self.expr(operation);
                    merge(self.locals.entry(*id).or_default(), values);
                    let values = self.expr(return_value);
                    merge(&mut self.returned, values);
                }
                CheckedStatement::CastReturn {
                    operand,
                    return_value,
                    ..
                } => {
                    self.expr(operand);
                    let values = self.expr(return_value);
                    merge(&mut self.returned, values);
                }
                CheckedStatement::Log { parts, .. } => {
                    for p in parts {
                        if let crate::ast::TemplatePart::Value(v) = p {
                            self.expr(v);
                        }
                    }
                }
                CheckedStatement::Break
                | CheckedStatement::Continue
                | CheckedStatement::Ignore(_) => {}
            }
        }
    }
}

pub(in crate::check) fn check_program(
    functions: &mut [CheckedFunction],
    external: std::ops::Range<usize>,
    externs: &[crate::ast::ExternFunction],
    warn_allocations: bool,
) -> Result<Vec<(Location, String, Vec<Diagnostic>)>, Diagnostic> {
    if functions.iter().all(|function| function.lifetime.is_none()) {
        return Ok(Vec::new());
    }
    let mut program = Program {
        warn_allocations,
        next: functions.iter().map(|f| f.id).max().unwrap_or(0) + 1,
        ..Program::default()
    };
    program.next = program.next.max(external.end);
    program.empty = program.next;
    program.next += 1;
    let mut checked = Vec::new();
    let mut warnings = Vec::new();
    let external_start = external.start;
    for id in external.chain(std::iter::once(program.empty)) {
        checked.push(CheckedFunction {
            name: String::new(),
            id,
            lifetime: externs
                .get(id - external_start)
                .filter(|_| warn_allocations)
                .map(|function| Body::Resolved(Rc::new(summaries::Summary::external(function)))),
            mutable_receiver: false,
            parameters: vec![],
            result: Type::Void,
            fallible: false,
            statements: vec![],
        });
    }
    for function in functions.iter_mut() {
        match function.lifetime.take() {
            Some(Body::Input(input)) => {
                let input = input.with_body(&function.statements, warn_allocations);
                if !input.callables {
                    let mut analysis = input.base.as_deref().expect("input has its body").clone();
                    analysis.statements(&input.statements, input.region, input.arena);
                    let mut function = CheckedFunction {
                        name: String::new(),
                        id: function.id,
                        lifetime: None,
                        mutable_receiver: false,
                        parameters: vec![],
                        result: Type::Void,
                        fallible: false,
                        statements: vec![],
                    };
                    function.lifetime = Some(Body::Pending(Rc::new(analysis)));
                    checked.push(function);
                }
                program.templates.insert(function.id, input);
            }
            other => {
                checked.push(CheckedFunction {
                    name: String::new(),
                    id: function.id,
                    lifetime: other,
                    mutable_receiver: false,
                    parameters: vec![],
                    result: Type::Void,
                    fallible: false,
                    statements: vec![],
                });
            }
        }
    }
    let roots: Vec<_> = program.templates.keys().copied().collect();
    for id in roots {
        program.request(id, &[]);
    }
    let mut finished = BTreeMap::new();
    while let Some(id) = program.pending.pop_front() {
        program.queued.remove(&id);
        program.current = id;
        let context = program.contexts[&id].clone();
        let input = program.templates[&context.function].clone();
        let mut analysis = input.base.as_deref().expect("input has its body").clone();
        analysis.function_id = id;
        let mut walk = Walk {
            program: &mut program,
            analysis,
            function: context.function,
            locals: BTreeMap::new(),
            returned: Values::new(),
            recoveries: Vec::new(),
            next_capture: input.parameters.len(),
            function_types: input.function_types.clone(),
        };
        for (index, (&ty, values)) in input.parameters.iter().zip(&context.arguments).enumerate() {
            let values = if matches!(ty, Type::Function(_)) {
                walk.bind(values)
            } else {
                walk.program.read(Flow::Parameter(context.function, index))
            };
            walk.locals.insert(index, values);
        }
        let statements = loop {
            let before: Vec<_> = walk
                .locals
                .iter()
                .map(|(id, v)| (*id, value_key(v)))
                .collect();
            let mut statements = input.statements.clone();
            walk.statements(&mut statements);
            let after: Vec<_> = walk
                .locals
                .iter()
                .map(|(id, v)| (*id, value_key(v)))
                .collect();
            if before == after {
                break statements;
            }
        };
        walk.program
            .write(Flow::Return(context.function), walk.returned);
        if let Some(error) = walk.program.failure.take() {
            return Err(error);
        }
        // 関数候補の収束中はIRだけを更新する。寿命の制約と要約は収束後に一度作る。
        finished.insert(id, (walk.analysis, statements, input.region, input.arena));
    }
    for (id, (mut analysis, statements, region, arena)) in finished {
        analysis.statements(&statements, region, arena);
        let body = if analysis.direct_calls.is_empty() {
            let summary = if let Some(summary) = analysis.forwarded_summary(&BTreeMap::new()) {
                summary
            } else {
                analysis.solve()?;
                analysis.summary()
            };
            warnings.extend(analysis.loop_warnings());
            Body::Resolved(Rc::new(summary))
        } else {
            Body::Pending(Rc::new(analysis))
        };
        checked.push(CheckedFunction {
            name: String::new(),
            id,
            lifetime: Some(body),
            mutable_receiver: false,
            parameters: vec![],
            result: Type::Void,
            fallible: false,
            statements: vec![],
        });
    }
    warnings.extend(summaries::check_program(&mut checked)?);
    warnings.sort_by_key(|warning| warning.0);
    warnings.dedup_by_key(|warning| warning.0);
    Ok(warnings)
}
