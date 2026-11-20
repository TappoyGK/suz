//! 本体の制約から保存・返却の由来を求め、実引数の参照先へ適用する。
//! 再帰する呼び出しも、確保箇所と仮引数由来の有限な集合が増えなくなるまで解く。
use super::*;
use crate::check::ir::CheckedFunction;
use std::collections::VecDeque;

/// 仮引数から届く実体は仮引数と型の組、確保は具体化した関数と確保箇所で区別する。
/// 再帰型の無限な参照経路を列挙せず、同じ仮引数から同じ型へ届く候補を合流させる。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Object {
    Parameter(usize, Type),
    // 書き込みメソッドの受け手、または参照捕捉された変数の保存場所。
    Slot(usize),
    Allocation(usize, Node),
}

#[derive(Clone)]
pub(super) struct Call {
    pub id: usize,
    pub arguments: Vec<(Type, Shape)>,
    pub result_ty: Type,
    pub result: Shape,
    pub receiver: Option<Place>,
    pub region: usize,
    pub location: Location,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct CallKey {
    id: usize,
    region: usize,
    arguments: Vec<(Type, Shape)>,
    receiver: Option<Place>,
}
impl Call {
    fn key(&self, representative: &[Node]) -> CallKey {
        CallKey {
            id: self.id,
            region: self.region,
            arguments: self
                .arguments
                .iter()
                .map(|(ty, shape)| {
                    (
                        *ty,
                        shape
                            .iter()
                            .map(|(path, &node)| (path.clone(), representative[node]))
                            .collect(),
                    )
                })
                .collect(),
            receiver: self.receiver.as_ref().map(|place| match place {
                Place::Direct(node) => Place::Direct(*node),
                Place::Indirect(node, path) => Place::Indirect(representative[*node], path.clone()),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Endpoint {
    object: Object,
    path: Vec<usize>,
    // 関数境界を通った参照。適用時の呼び出し先Arenaまでに利用期間を制限する。
    limited: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ObjectInfo {
    ty: Type,
    location: Location,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct StoreInfo {
    function: usize,
    location: Location,
    objection: bool,
    nonnull: bool,
}
#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct StoreEffect {
    destinations: BTreeSet<Endpoint>,
    values: BTreeSet<Endpoint>,
}
#[derive(Clone, Default, Debug, PartialEq, Eq)]
struct Summary {
    objects: BTreeMap<Object, ObjectInfo>,
    returned: BTreeMap<Vec<usize>, BTreeSet<Endpoint>>,
    stores: BTreeMap<StoreInfo, StoreEffect>,
}

impl Analysis {
    // 構文の移行までは従来のarena fn契約も検査する。要約用の制約には混ぜない。
    pub(super) fn legacy_calls(&mut self) {
        for call in self.direct_calls.clone() {
            if let Some(receiver) = call.receiver {
                let ty = call.arguments[0].0;
                let updated = self.arena_call(&call.arguments, ty, call.region, call.location);
                self.write(receiver, updated, call.location);
            }
            let value =
                self.arena_call(&call.arguments, call.result_ty, call.region, call.location);
            self.join(&call.result, value);
        }
    }

    fn endpoint(&self, reference: Reference) -> Option<Endpoint> {
        let cell = &self.cells[&reference.target];
        let object = *self.objects.get(&cell.root)?;
        // 関数内の明示的なArenaの実体は、呼び出し元へ残せない。
        if matches!(object, Object::Allocation(..)) && cell.region != 0 {
            return None;
        }
        Some(Endpoint {
            object,
            path: cell.path.clone(),
            limited: reference.limit.is_some(),
        })
    }

    fn summary(&mut self) -> Summary {
        let mut result = Summary::default();
        for (&node, &object) in &self.objects {
            let cell = &self.cells[&node];
            if matches!(object, Object::Allocation(..)) && cell.region != 0 {
                continue;
            }
            result.objects.insert(
                object,
                ObjectInfo {
                    ty: cell.ty,
                    location: cell.location,
                },
            );
        }
        for (path, &node) in &self.returned {
            result.returned.insert(
                path.clone(),
                self.targets[node]
                    .iter()
                    .filter_map(|&reference| self.endpoint(reference))
                    .collect(),
            );
        }
        // 初期状態にある仮引数のフィールドを「書いた」とは扱わない。
        // Storeの保存先だけを列挙し、別名による保存は収束後の参照先で解決する。
        for (index, constraint) in self.constraints.clone().into_iter().enumerate() {
            let Constraint::Store(place, source, location) = constraint else {
                continue;
            };
            let values: BTreeSet<_> = self.targets[source]
                .iter()
                .filter_map(|&reference| self.endpoint(reference))
                .collect();
            let info = self
                .effect_stores
                .get(&index)
                .copied()
                .unwrap_or(StoreInfo {
                    function: self.function_id,
                    location,
                    objection: self.objections.contains(&index),
                    nonnull: self.nonnull_values.contains(&source),
                });
            let destinations: BTreeSet<_> = self
                .destinations(&place)
                .into_iter()
                .filter_map(|destination| self.endpoint(Reference::object(destination)))
                .collect();
            if destinations.is_empty() {
                continue;
            }
            let effect = result.stores.entry(info).or_default();
            effect.destinations.extend(destinations);
            effect.values.extend(values);
        }
        result
    }

    /// 要約中の実体を指す値を作る。仮引数由来は実引数からたどり、確保箇所は
    /// この呼び出しのArenaに置く。利用期間を短くしても実体の領域は変えない。
    fn call_objects(&mut self, call: &Call, summary: &Summary) -> BTreeMap<Object, Node> {
        let mut mapped = BTreeMap::new();
        for (&object, info) in &summary.objects {
            let node = self.node();
            mapped.insert(object, node);
            match object {
                Object::Parameter(..) => {}
                Object::Slot(index) => {
                    if index == 0
                        && let Some(receiver) = &call.receiver
                    {
                        self.constraints
                            .push(Constraint::Address(node, receiver.clone()));
                    }
                }
                Object::Allocation(..) => {
                    let cell = self.cell(call.region, info.location, info.ty);
                    self.cells.get_mut(&cell).unwrap().origin = Origin::Call;
                    self.objects.insert(cell, object);
                    self.targets[node].insert(Reference::object(cell));
                }
            }
        }
        for (index, (ty, shape)) in call.arguments.iter().enumerate() {
            for (path, &source) in shape {
                let ty = self.field_type(*ty, path).pointee().unwrap();
                if let Some(&node) = mapped.get(&Object::Parameter(index, ty)) {
                    self.constraints.push(Constraint::Copy(node, source));
                }
            }
        }
        // 読み出しの辺を残すことで、別名経由の後続の書き込みも実引数の到達先に反映する。
        for (&object, &node) in &mapped {
            let Object::Parameter(index, ty) = object else {
                continue;
            };
            for path in self.paths(ty) {
                let target = self.field_type(ty, &path).pointee().unwrap();
                if let Some(&next) = mapped.get(&Object::Parameter(index, target)) {
                    self.constraints
                        .push(Constraint::Load(next, Place::Indirect(node, path)));
                }
            }
        }
        mapped
    }

    fn call_value(
        &mut self,
        endpoint: &Endpoint,
        objects: &BTreeMap<Object, Node>,
        region: usize,
    ) -> Node {
        let root = objects[&endpoint.object];
        let value = if endpoint.path.is_empty() {
            root
        } else {
            let value = self.node();
            self.constraints.push(Constraint::Address(
                value,
                Place::Indirect(root, endpoint.path.clone()),
            ));
            value
        };
        if endpoint.limited {
            let limited = self.node();
            self.constraints
                .push(Constraint::Limit(limited, value, region));
            limited
        } else {
            value
        }
    }

    fn apply_summary(&mut self, call: &Call, summary: &Summary) {
        let objects = self.call_objects(call, summary);
        for (info, effect) in &summary.stores {
            let destination = self.node();
            for value in &effect.destinations {
                let value = self.call_value(value, &objects, call.region);
                self.constraints.push(Constraint::Copy(destination, value));
            }
            let place = Place::Indirect(destination, vec![]);
            // 一つの保存に複数の候補があることを保つ。候補ごとにStoreを分割すると、
            // objection内の不確かな保存を「必ず違反する保存」と取り違える。
            let source = self.node();
            for value in &effect.values {
                let value = self.call_value(value, &objects, call.region);
                self.constraints.push(Constraint::Copy(source, value));
            }
            if info.nonnull {
                self.nonnull_values.insert(source);
            }
            if info.objection {
                self.objections.insert(self.constraints.len());
            }
            self.effect_calls
                .insert(self.constraints.len(), call.location);
            self.effect_stores.insert(self.constraints.len(), *info);
            self.constraints
                .push(Constraint::Store(place, source, info.location));
        }
        for (path, values) in &summary.returned {
            let Some(&destination) = call.result.get(path) else {
                continue;
            };
            for source in values {
                let source = self.call_value(source, &objects, call.region);
                self.constraints
                    .push(Constraint::Limit(destination, source, call.region));
            }
        }
    }

    fn with_summaries(&self, summaries: &BTreeMap<usize, Summary>) -> Result<Self, Diagnostic> {
        let mut analysis = self.clone();
        let representative = solver::representatives(self, &self.constraints);
        let mut applied: BTreeMap<CallKey, Shape> = BTreeMap::new();
        for call in &self.direct_calls {
            let Some(summary) = summaries.get(&call.id) else {
                // 未知の呼び出し先を「効果なし」と扱うと検査が欠落する。
                return Err(call.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "arena.summary",
                        "missing lifetime summary for called function"
                    ),
                ));
            };
            // 同じ参照入力・同じ領域の呼び出しは、候補の包含解析では同じ効果になる。
            // 読み出し用の一時ノード番号ではなく、無条件コピーの代表で入力を比べる。
            // 同じ変数を何度も渡すコードで、密な到達グラフを呼び出し回数分作らない。
            let key = call.key(&representative);
            if let Some(result) = applied.get(&key) {
                analysis.join(&call.result, result.clone());
                continue;
            }
            analysis.apply_summary(call, summary);
            applied.insert(key, call.result.clone());
        }
        let constraints = std::mem::take(&mut analysis.constraints);
        solver::propagate(&mut analysis, &constraints);
        analysis.constraints = constraints;
        Ok(analysis)
    }
}

fn infer(bodies: &BTreeMap<usize, Rc<Analysis>>) -> Result<BTreeMap<usize, Summary>, Diagnostic> {
    let mut summaries: BTreeMap<_, _> = bodies.keys().map(|&id| (id, Summary::default())).collect();
    let mut callers: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (&id, body) in bodies {
        for call in &body.direct_calls {
            callers.entry(call.id).or_default().insert(id);
        }
    }
    let mut pending: VecDeque<_> = bodies.keys().copied().collect();
    let mut queued: BTreeSet<_> = bodies.keys().copied().collect();
    while let Some(id) = pending.pop_front() {
        queued.remove(&id);
        let summary = bodies[&id].with_summaries(&summaries)?.summary();
        if summaries[&id] == summary {
            continue;
        }
        summaries.insert(id, summary);
        if let Some(dependents) = callers.get(&id) {
            for &caller in dependents {
                if queued.insert(caller) {
                    pending.push_back(caller);
                }
            }
        }
    }
    Ok(summaries)
}

/// 全具体化の要約を収束させて呼び出しを検査し、C生成には不要な解析本体を取り除く。
pub(in crate::check) fn check_program(functions: &mut [CheckedFunction]) -> Result<(), Diagnostic> {
    let bodies: BTreeMap<_, _> = functions
        .iter_mut()
        .filter_map(|function| function.lifetime.take().map(|body| (function.id, body)))
        .collect();
    let summaries = infer(&bodies)?;
    let mut pending: Vec<_> = bodies.values().map(Rc::as_ref).collect();
    while let Some(body) = pending.pop() {
        pending.extend(body.closures.iter().map(Rc::as_ref));
        let mut analysis = body.with_summaries(&summaries)?;
        let constraints = std::mem::take(&mut analysis.constraints);
        analysis.validate(&constraints)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
