//! 関数値の候補を集め、寿命検査だけに使う呼び出し文脈を作る。
//! Cへ渡すIRは変更しない。fn仮引数は実引数で区別し、static fnは仕様の保存単位で合流する。
use super::*;
use crate::check::ir::{CallHandler, CheckedFunction};

#[derive(Clone)]
pub(crate) struct Input {
    pub base: Analysis,
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
    let before = value_key(into);
    for (path, values) in from {
        into.entry(path).or_default().extend(values);
    }
    before != value_key(into)
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
    templates: BTreeMap<usize, Rc<Input>>,
    closures: BTreeMap<usize, usize>,
    contexts: BTreeMap<usize, Context>,
    keys: BTreeMap<ContextKey, usize>,
    // static fnは呼び出し文脈を分けず、宣言の仮引数・戻り値・構造体フィールドごとに合流。
    parameters: BTreeMap<(usize, usize), Values>,
    returns: BTreeMap<usize, Values>,
    allocations: BTreeMap<(usize, Location), usize>,
    heap: BTreeMap<usize, Values>,
    fields: BTreeMap<(usize, usize), Values>,
    next: usize,
    empty: usize,
    changed: bool,
    failure: Option<Diagnostic>,
    context_counts: BTreeMap<usize, usize>,
}
impl Program {
    fn request(&mut self, function: usize, arguments: &[Values]) -> usize {
        let Some(input) = self.templates.get(&function) else {
            return function;
        };
        let mut bound = vec![Values::new(); input.parameters.len()];
        for (index, (&ty, values)) in input.parameters.iter().zip(arguments).enumerate() {
            if matches!(ty, Type::Function(_)) {
                bound[index] = values.clone();
            } else {
                self.changed |= merge(
                    self.parameters.entry((function, index)).or_default(),
                    values.clone(),
                );
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
        let id = self.next;
        self.next += 1;
        self.keys.insert(key, id);
        self.contexts.insert(
            id,
            Context {
                function,
                arguments: bound,
            },
        );
        self.changed = true;
        id
    }
    fn closure(&mut self, input: Rc<Input>) -> usize {
        let key = Rc::as_ptr(&input) as usize;
        if let Some(&id) = self.closures.get(&key) {
            return id;
        }
        let id = self.next;
        self.next += 1;
        self.closures.insert(key, id);
        self.templates.insert(id, input);
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
    site: Location,
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
                    for path in self.analysis.paths(ty) {
                        let target = self.analysis.field_type(ty, &path).pointee().unwrap();
                        let object =
                            self.analysis
                                .parameter_object(target, 0, location, &mut objects);
                        let field = self.analysis.project(cell, &path);
                        self.analysis.targets[field].insert(Reference::object(object));
                    }
                    for (ty, object) in objects {
                        self.analysis
                            .objects
                            .insert(object, summaries::Object::Parameter(index, ty));
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
    fn field_key(&self, owner: usize, index: usize) -> (usize, usize) {
        let decl = self.analysis.structs.get(owner);
        (
            decl.application.as_ref().map_or(owner, |a| a.declaration),
            index,
        )
    }
    fn field_values(&self, ty: Type) -> Values {
        let Some(id) = (match ty {
            Type::Struct(id) => Some(id),
            _ => None,
        }) else {
            return Values::new();
        };
        let mut result = Values::new();
        for index in 0..self.analysis.structs.get(id).fields.len() {
            if let Some(values) = self.program.fields.get(&self.field_key(id, index)) {
                for (path, set) in values {
                    let mut path = path.clone();
                    path.insert(0, index);
                    result.insert(path, set.clone());
                }
            }
        }
        result
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
            self.program.changed |= merge(self.program.fields.entry(key).or_default(), selected);
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
            Value::Function(id) => single(Bound {
                function: *id,
                site: self.site,
                captures: Vec::new(),
            }),
            Value::Local(id) => {
                let mut values = self.locals.get(id).cloned().unwrap_or_default();
                merge(&mut values, self.field_values(ty));
                values
            }
            Value::FunctionBorrow(value)
            | Value::Cast(value)
            | Value::Receiver { target: value, .. } => self.expr(value),
            Value::Closure(closure) => {
                let Some(Body::Input(input)) = &closure.function.lifetime else {
                    unreachable!("closure input retained until lifetime checking")
                };
                let function = self.program.closure(input.clone());
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
                    site: self.site,
                    captures,
                })
            }
            Value::Field { target, index } => {
                let values = self.expr(target);
                values
                    .into_iter()
                    .filter_map(|(path, set)| {
                        (path.first() == Some(index)).then(|| (path[1..].to_vec(), set))
                    })
                    .collect()
            }
            Value::New { value, location } => {
                let values = self.expr(value);
                let key = (self.function, *location);
                let id = if let Some(&id) = self.program.allocations.get(&key) {
                    id
                } else {
                    let id = self.program.next;
                    self.program.next += 1;
                    self.program.allocations.insert(key, id);
                    id
                };
                self.program.changed |= merge(self.program.heap.entry(id).or_default(), values);
                single(Bound {
                    function: id,
                    site: *location,
                    captures: Vec::new(),
                })
            }
            Value::Deref(target) => {
                let pointers = self.expr(target);
                let mut values = self.field_values(ty);
                for pointer in pointers.values().flat_map(|set| set.values()) {
                    merge(
                        &mut values,
                        self.program
                            .heap
                            .get(&pointer.function)
                            .cloned()
                            .unwrap_or_default(),
                    );
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
                self.program
                    .returns
                    .get(&function)
                    .cloned()
                    .unwrap_or_default()
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
                                    message:
                                        "this initialization or assignment supplies the call target"
                                            .into(),
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
                        self.program
                            .returns
                            .get(&bound.function)
                            .cloned()
                            .unwrap_or_default(),
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
        merge(&mut values, self.field_values(ty));
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
            match &*statement {
                CheckedStatement::Binding { location, .. }
                | CheckedStatement::Assign { location, .. }
                | CheckedStatement::Return { location, .. }
                | CheckedStatement::Store { location, .. } => self.site = *location,
                _ => {}
            }
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
                        self.program.changed |= merge(
                            self.program.heap.entry(pointer.function).or_default(),
                            values.clone(),
                        );
                    }
                    self.save_fields(target.ty, &values);
                    // フィールドへのポインタ経由の保存も、所有する構造体宣言の集合へ加える。
                    if let Value::Field { target, index } = &target.value
                        && let Type::Struct(owner) = target.ty
                    {
                        let key = self.field_key(owner, *index);
                        self.program.changed |=
                            merge(self.program.fields.entry(key).or_default(), values);
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
                    start, end, body, ..
                } => {
                    self.expr(start);
                    if let Some(e) = end {
                        self.expr(e);
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
                    operation,
                    return_value,
                    ..
                }
                | CheckedStatement::ArithmeticReturn {
                    operation,
                    return_value,
                    ..
                } => {
                    self.expr(operation);
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
) -> Result<(), Diagnostic> {
    let mut program = Program {
        next: functions.iter().map(|f| f.id).max().unwrap_or(0) + 1,
        ..Program::default()
    };
    program.next = program.next.max(external.end);
    program.empty = program.next;
    program.next += 1;
    let mut checked = Vec::new();
    for id in external.chain(std::iter::once(program.empty)) {
        checked.push(CheckedFunction {
            id,
            lifetime: None,
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
                program.templates.insert(function.id, input);
            }
            other => {
                let mut function = function.clone();
                function.lifetime = other;
                checked.push(function);
            }
        }
    }
    let roots: Vec<_> = program.templates.keys().copied().collect();
    for id in roots {
        program.request(id, &[]);
    }
    let mut finished = BTreeMap::new();
    loop {
        program.changed = false;
        let contexts: Vec<_> = program
            .contexts
            .iter()
            .map(|(&id, context)| (id, context.clone()))
            .collect();
        for (id, context) in contexts {
            let input = program.templates[&context.function].clone();
            let mut analysis = input.base.clone();
            analysis.function_id = id;
            let mut walk = Walk {
                program: &mut program,
                analysis,
                function: context.function,
                locals: BTreeMap::new(),
                returned: Values::new(),
                recoveries: Vec::new(),
                site: input.location,
                next_capture: input.parameters.len(),
                function_types: input.function_types.clone(),
            };
            for (index, (&ty, values)) in
                input.parameters.iter().zip(&context.arguments).enumerate()
            {
                let values = if matches!(ty, Type::Function(_)) {
                    walk.bind(values)
                } else {
                    walk.program
                        .parameters
                        .get(&(context.function, index))
                        .cloned()
                        .unwrap_or_default()
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
            walk.program.changed |= merge(
                walk.program.returns.entry(context.function).or_default(),
                walk.returned,
            );
            if let Some(error) = walk.program.failure.take() {
                return Err(error);
            }
            let mut analysis = walk.analysis;
            analysis.statements(&statements, input.region, input.arena);
            finished.insert(id, analysis);
        }
        if !program.changed {
            break;
        }
    }
    for (id, mut analysis) in finished {
        let body = if analysis.direct_calls.is_empty() {
            let summary = if let Some(summary) = analysis.forwarded_summary(&BTreeMap::new()) {
                summary
            } else {
                analysis.solve()?;
                analysis.summary()
            };
            Body::Resolved(Rc::new(summary))
        } else {
            Body::Pending(Rc::new(analysis))
        };
        checked.push(CheckedFunction {
            id,
            lifetime: Some(body),
            mutable_receiver: false,
            parameters: vec![],
            result: Type::Void,
            fallible: false,
            statements: vec![],
        });
    }
    summaries::check_program(&mut checked)
}
