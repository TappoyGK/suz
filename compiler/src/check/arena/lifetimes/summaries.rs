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
    ReachableParameter(usize, Type),
    // 書き込みメソッドの受け手、または参照捕捉された変数の保存場所。
    Slot(usize),
    Allocation(usize, Node),
    // 同じ再帰成分が現在のArenaへ確保する、同じ型の実体。
    RecursiveAllocation(usize, Type),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) struct CallNote {
    pub location: Location,
    pub message: String,
}
#[derive(Clone)]
pub(super) struct Call {
    pub id: usize,
    pub arguments: Vec<(Type, Leaves)>,
    pub result: Leaves,
    pub receiver: Option<Place>,
    pub captures: BTreeMap<usize, Place>,
    pub region: usize,
    pub location: Location,
    pub notes: BTreeSet<CallNote>,
    pub indirect: bool,
    pub loop_region: Option<usize>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct CallKey<'a> {
    id: usize,
    indirect_site: Option<Location>,
    result_places: Vec<Node>,
    region: usize,
    arguments: Vec<(usize, Type, &'a [usize], Node)>,
    receiver: Option<Place>,
    captures: BTreeMap<usize, Place>,
}
impl Call {
    fn result_node(&self, path: &[usize]) -> Option<Node> {
        self.result
            .binary_search_by(|(candidate, _)| candidate.as_slice().cmp(path))
            .ok()
            .map(|index| self.result[index].1)
    }
    fn key(&self, representative: &[Node]) -> CallKey<'_> {
        CallKey {
            id: self.id,
            indirect_site: self.indirect.then_some(self.location),
            result_places: Vec::new(),
            region: self.region,
            arguments: self
                .arguments
                .iter()
                .enumerate()
                .flat_map(|(index, (ty, shape))| {
                    shape.iter().map(move |(path, node)| {
                        (index, *ty, path.as_slice(), representative[*node])
                    })
                })
                .collect(),
            captures: self.captures.clone(),
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
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ObjectInfo {
    ty: Type,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct StoreInfo {
    function: usize,
    location: Location,
    objection: bool,
    nonnull: bool,
}
#[derive(Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StoreEffect {
    destinations: BTreeSet<Endpoint>,
    values: BTreeSet<Endpoint>,
}
#[derive(Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Summary {
    objects: BTreeMap<Object, ObjectInfo>,
    returned: BTreeMap<Vec<usize>, BTreeSet<Endpoint>>,
    stores: BTreeMap<StoreInfo, StoreEffect>,
    notes: BTreeMap<StoreInfo, BTreeSet<CallNote>>,
    allocations: BTreeMap<Object, accumulation::Trace>,
    // 呼び出し元へ参照を返さない一時確保。診断用の代表位置だけを伝播する。
    discarded: Option<accumulation::Trace>,
    kept: BTreeSet<Endpoint>,
}

impl Summary {
    pub(super) fn external(function: &crate::ast::ExternFunction) -> Self {
        let mut summary = Self::default();
        for (index, parameter) in function.parameters.iter().enumerate() {
            if parameter.keep
                && let Some(ty) = parameter.ty.pointee()
            {
                let object = Object::Parameter(index, ty);
                summary.objects.insert(object, ObjectInfo { ty });
                summary.kept.insert(Endpoint {
                    object,
                    path: vec![],
                    limited: false,
                });
            }
        }
        summary
    }
    // 使われない仮引数の到達先を呼び出しごとに読み直さない。
    // 到達先を要約が実際に使う仮引数だけは、そこまでの経路も保持する。
    fn retain_used_objects(&mut self) {
        let used: BTreeSet<_> = self
            .returned
            .values()
            .flatten()
            .chain(
                self.stores
                    .values()
                    .flat_map(|effect| effect.destinations.iter().chain(&effect.values)),
            )
            .chain(&self.kept)
            .map(|endpoint| endpoint.object)
            .collect();
        let traversed: BTreeSet<_> = used
            .iter()
            .filter_map(|object| match object {
                Object::ReachableParameter(index, _) => Some(*index),
                _ => None,
            })
            .collect();
        self.objects.retain(|object, _| {
            used.contains(object)
                || match object {
                    Object::Parameter(index, _) | Object::ReachableParameter(index, _) => {
                        traversed.contains(index)
                    }
                    _ => false,
                }
        });
        self.allocations
            .retain(|object, _| self.objects.contains_key(object));
    }
    // 再帰を一周するたびに、同じArena・型の確保箇所を全呼び出し元へ展開しない。
    // 参照先の領域は共通なので、寿命検査では一つの候補にまとめられる。
    // 仮引数と受け手はまとめず、外側の実体への保存を引き続き区別する。
    fn compact_recursive(&mut self, component: usize) {
        let remap: BTreeMap<_, _> = self
            .objects
            .iter()
            .map(|(&object, info)| {
                let target = match object {
                    Object::Allocation(..) | Object::RecursiveAllocation(..) => {
                        Object::RecursiveAllocation(component, info.ty)
                    }
                    Object::Parameter(..) | Object::ReachableParameter(..) | Object::Slot(..) => {
                        object
                    }
                };
                (object, target)
            })
            .collect();
        self.objects = std::mem::take(&mut self.objects)
            .into_iter()
            .map(|(object, info)| (remap[&object], info))
            .collect();
        let endpoints = |set: &mut BTreeSet<Endpoint>| {
            *set = std::mem::take(set)
                .into_iter()
                .map(|mut endpoint| {
                    endpoint.object = remap[&endpoint.object];
                    endpoint
                })
                .collect();
        };
        endpoints(&mut self.kept);
        let mut allocations = BTreeMap::new();
        for (object, location) in std::mem::take(&mut self.allocations) {
            allocations
                .entry(remap[&object])
                .and_modify(|previous: &mut accumulation::Trace| {
                    if location.origin() < previous.origin() {
                        *previous = location.clone();
                    }
                })
                .or_insert(location);
        }
        self.allocations = allocations;
        for values in self.returned.values_mut() {
            endpoints(values);
        }
        let mut combined: BTreeMap<_, (StoreInfo, StoreEffect)> = BTreeMap::new();
        for (info, mut effect) in std::mem::take(&mut self.stores) {
            endpoints(&mut effect.destinations);
            endpoints(&mut effect.values);
            // objectionでは確実な違反と不確かな違反の区別が必要なので、
            // 保存値の候補が異なる保存同士を合流しない。
            let key = (
                effect.destinations.clone(),
                info.objection,
                info.nonnull,
                info.objection.then(|| effect.values.clone()),
            );
            combined
                .entry(key)
                .and_modify(|(_, previous)| previous.values.extend(effect.values.clone()))
                .or_insert((info, effect));
        }
        self.stores = combined.into_values().collect();
    }

    fn same_effects(&self, other: &Self) -> bool {
        if self.objects != other.objects
            || self.returned != other.returned
            || self.kept != other.kept
            || self.discarded.is_some() != other.discarded.is_some()
            || !self.allocations.keys().eq(other.allocations.keys())
        {
            return false;
        }
        // 同じ効果の診断位置の選び方だけでは、呼び出し元を再計算しない。
        let effects = |summary: &Self| {
            summary
                .stores
                .iter()
                .map(|(info, effect)| (info.objection, info.nonnull, effect.clone()))
                .collect::<BTreeSet<_>>()
        };
        effects(self) == effects(other)
    }
}

impl Analysis {
    // 参照をそのまま返す、または読み取り専用の呼び出しへ転送する本体では、
    // 既知の由来を仮引数へ代入するだけで要約が得られる。間接読み出し・保存、
    // アドレス取得、確保、内側のArenaを含む本体は通常の制約解決へ回す。
    pub(super) fn forwarded_summary(
        &self,
        summaries: &BTreeMap<usize, Rc<Summary>>,
    ) -> Option<Summary> {
        if self.return_region != Some(0)
            || !self.root_references_only()
            || self.direct_calls.len() > 1
            || self.objects.values().any(|object| {
                !matches!(
                    object,
                    Object::Parameter(..) | Object::ReachableParameter(..)
                )
            })
            || self.constraints.iter().any(|constraint| match constraint {
                Constraint::Copy(..) | Constraint::Load(_, Place::Direct(_)) => false,
                Constraint::Store(Place::Direct(cell), _, _) => {
                    self.objects.contains_key(&self.cells[cell].root)
                }
                _ => true,
            })
        {
            return None;
        }
        let representative = solver::representatives(self, &self.constraints);
        let mut result = Summary::default();
        for (&node, &object) in &self.objects {
            result.objects.insert(
                object,
                ObjectInfo {
                    ty: self.cells[&node].ty,
                },
            );
        }
        if let Some(call) = self.direct_calls.first() {
            let callee = summaries.get(&call.id)?;
            if !callee.stores.is_empty()
                || !callee.kept.is_empty()
                || callee.discarded.is_some()
                || !callee.allocations.is_empty()
            {
                return None;
            }
            let mut inputs = BTreeMap::new();
            for &object in callee.objects.keys() {
                // 仮引数の初期グラフには、返却に使わない到達先も含まれる。
                // 直接の参照の転送には不要なので、戻り値が参照する実体だけを代入する。
                if !callee
                    .returned
                    .values()
                    .any(|values| values.iter().any(|value| value.object == object))
                {
                    continue;
                }
                let Object::Parameter(index, ty) = object else {
                    return None;
                };
                let (argument_ty, shape) = call.arguments.get(index)?;
                if argument_ty.pointee() != Some(ty) {
                    return None;
                }
                let source = representative[shape.iter().find(|(path, _)| path.is_empty())?.1];
                let targets = &self.targets[source];
                if targets.len() != 1 {
                    return None;
                }
                let endpoint = self.endpoint(*targets.first()?)?;
                if !endpoint.path.is_empty() || endpoint.limited {
                    return None;
                }
                inputs.insert(object, endpoint.object);
            }
            for (path, &node) in &self.returned {
                if representative[node] != representative[call.result_node(path)?] {
                    return None;
                }
                let values = callee
                    .returned
                    .get(path)?
                    .iter()
                    .map(|endpoint| {
                        Some(Endpoint {
                            object: *inputs.get(&endpoint.object)?,
                            path: endpoint.path.clone(),
                            limited: true,
                        })
                    })
                    .collect::<Option<_>>()?;
                result.returned.insert(path.clone(), values);
            }
        } else {
            for (path, &node) in &self.returned {
                let values = &self.targets[representative[node]];
                if values.is_empty() {
                    return None;
                }
                result.returned.insert(
                    path.clone(),
                    values
                        .iter()
                        .map(|&reference| self.endpoint(reference))
                        .collect::<Option<_>>()?,
                );
            }
        }
        result.retain_used_objects();
        Some(result)
    }

    fn endpoint(&self, reference: Reference) -> Option<Endpoint> {
        let cell = &self.cells[&reference.target];
        let object = *self.objects.get(&cell.root)?;
        // 関数内の明示的なArenaの実体は、呼び出し元へ残せない。
        if matches!(
            object,
            Object::Allocation(..) | Object::RecursiveAllocation(..)
        ) && cell.region != 0
        {
            return None;
        }
        Some(Endpoint {
            object,
            path: cell.path.clone(),
            limited: reference.limit.is_some(),
        })
    }

    pub(super) fn summary(&mut self) -> Summary {
        let mut result = Summary {
            discarded: self.discarded_allocation(),
            ..Summary::default()
        };
        for &node in &self.kept {
            result.kept.extend(
                self.targets[node]
                    .iter()
                    .filter_map(|&value| self.endpoint(value)),
            );
        }
        for (&node, &object) in &self.objects {
            let cell = &self.cells[&node];
            if matches!(
                object,
                Object::Allocation(..) | Object::RecursiveAllocation(..)
            ) && cell.region != 0
            {
                continue;
            }
            result.objects.insert(object, ObjectInfo { ty: cell.ty });
            if let Some(allocations) = self.allocations.get(&node)
                && let Some(allocation) = allocations.iter().min_by_key(|a| a.trace.origin())
            {
                result.allocations.insert(object, allocation.trace.clone());
            }
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
        let constraints = std::mem::take(&mut self.constraints);
        for (index, constraint) in constraints.iter().enumerate() {
            let Constraint::Store(place, source, location) = constraint else {
                continue;
            };
            let values: BTreeSet<_> = self.targets[*source]
                .iter()
                .filter_map(|&reference| self.endpoint(reference))
                .collect();
            let info = self
                .effect_stores
                .get(&index)
                .copied()
                .unwrap_or(StoreInfo {
                    function: self.function_id,
                    location: *location,
                    objection: self.objections.contains(&index),
                    nonnull: self.nonnull_values.contains(source),
                });
            let destinations: BTreeSet<_> = self
                .destinations(place)
                .into_iter()
                .filter_map(|destination| self.endpoint(Reference::object(destination)))
                .collect();
            if destinations.is_empty() {
                continue;
            }
            if let Some(notes) = self.effect_notes.get(&index) {
                result
                    .notes
                    .entry(info)
                    .or_default()
                    .extend(notes.iter().cloned());
            }
            let effect = result.stores.entry(info).or_default();
            effect.destinations.extend(destinations);
            effect.values.extend(values);
        }
        result.retain_used_objects();
        self.constraints = constraints;
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
                Object::Parameter(..) | Object::ReachableParameter(..) => {}
                Object::Slot(index) => {
                    if let Some(receiver) = call
                        .captures
                        .get(&index)
                        .or_else(|| (index == 0).then_some(call.receiver.as_ref()).flatten())
                    {
                        self.constraints
                            .push(Constraint::Address(node, receiver.clone()));
                    }
                }
                Object::Allocation(..) | Object::RecursiveAllocation(..) => {
                    // 一つの間接呼び出しで選ばれる関数の確保先は、同じArenaに属する。
                    // 型ごとに合流し、候補N個が返すN個の同寿命の実体を、次の反復で
                    // N個の候補すべてへ渡す完全グラフを作らない。異なる呼び出し位置は分ける。
                    let key = (call.location, call.region, info.ty);
                    let cell = if call.indirect {
                        if let Some(&cell) = self.candidate_allocations.get(&key) {
                            cell
                        } else {
                            let cell = self.cell(call.region, call.location, info.ty);
                            self.cells.get_mut(&cell).unwrap().origin = Origin::Call;
                            self.objects
                                .insert(cell, Object::Allocation(self.function_id, cell));
                            self.candidate_allocations.insert(key, cell);
                            cell
                        }
                    } else {
                        let cell = self.cell(call.region, call.location, info.ty);
                        self.cells.get_mut(&cell).unwrap().origin = Origin::Call;
                        self.objects.insert(cell, object);
                        cell
                    };
                    self.targets[node].insert(Reference::object(cell));
                }
            }
        }
        for (index, (ty, shape)) in call.arguments.iter().enumerate() {
            for (path, source) in shape {
                let source = *source;
                let ty = self.field_type(*ty, path).pointee().unwrap();
                if let Some(&node) = mapped.get(&Object::Parameter(index, ty)) {
                    self.constraints.push(Constraint::Copy(node, source));
                }
            }
        }
        // 読み出しの辺を残すことで、別名経由の後続の書き込みも実引数の到達先に反映する。
        for (&object, &node) in &mapped {
            let (Object::Parameter(index, ty) | Object::ReachableParameter(index, ty)) = object
            else {
                continue;
            };
            for path in self.paths(ty) {
                let target = self.field_type(ty, &path).pointee().unwrap();
                if let Some(&next) = mapped.get(&Object::ReachableParameter(index, target)) {
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

    fn apply_summary(
        &mut self,
        call: &Call,
        summary: &Summary,
    ) -> Vec<(Node, accumulation::Trace)> {
        let objects = self.call_objects(call, summary);
        let allocations: Vec<_> = summary
            .allocations
            .iter()
            .flat_map(|(object, trace)| {
                self.targets[objects[object]]
                    .iter()
                    .map(|r| (self.cells[&r.target].root, trace.clone()))
            })
            .collect();
        self.record_allocations(call, &allocations, summary.discarded.as_ref());
        for value in &summary.kept {
            let value = self.call_value(value, &objects, call.region);
            self.kept.push(value);
        }
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
            // 呼び出し先の字句上のobjectionは呼び出し元へ持ち出さない。
            // 実引数を代入して初めて分かる保存先の寿命は、この境界で必ず検査する。
            self.effect_calls
                .insert(self.constraints.len(), call.location);
            self.effect_stores.insert(self.constraints.len(), *info);
            let mut notes = summary.notes.get(info).cloned().unwrap_or_default();
            notes.extend(call.notes.iter().cloned());
            self.effect_notes.insert(self.constraints.len(), notes);
            self.constraints
                .push(Constraint::Store(place, source, info.location));
        }
        for (path, values) in &summary.returned {
            let Some(destination) = call.result_node(path) else {
                continue;
            };
            for source in values {
                let source = self.call_value(source, &objects, call.region);
                self.constraints
                    .push(Constraint::Limit(destination, source, call.region));
            }
        }
        allocations
    }

    fn record_allocations(
        &mut self,
        call: &Call,
        allocations: &[(Node, accumulation::Trace)],
        discarded: Option<&accumulation::Trace>,
    ) {
        for (node, trace) in allocations {
            self.allocations
                .entry(*node)
                .or_default()
                .push(accumulation::Allocation {
                    site: call.location,
                    trace: trace.through(call),
                    loop_region: call.loop_region,
                });
        }
        if let Some(trace) = discarded {
            self.discarded_calls.push(accumulation::Discarded {
                site: call.location,
                trace: trace.through(call),
                region: call.region,
                loop_region: call.loop_region,
            });
        }
    }

    fn with_summaries(&self, summaries: &BTreeMap<usize, Rc<Summary>>) -> Result<Self, Diagnostic> {
        self.clone().resolve_summaries(summaries)
    }

    fn resolve_summaries(
        mut self,
        summaries: &BTreeMap<usize, Rc<Summary>>,
    ) -> Result<Self, Diagnostic> {
        if self.resolved {
            return Ok(self);
        }
        let representative = solver::representatives(&self, &self.constraints);
        // 同じ保存場所へ結果を渡す確保は、警告に必要な区別を失わずに共有できる。
        // 戻り値と一時変数は別の保存場所なので、片方だけ残す場合は合流しない。
        let original_constraints = self.constraints.len();
        let mut destinations = None;
        let calls = std::mem::take(&mut self.direct_calls);
        let mut analysis = self;
        let mut allocations_by_call: Vec<Vec<(Node, accumulation::Trace)>> = Vec::new();
        let mut applied: BTreeMap<CallKey<'_>, usize> = BTreeMap::new();
        for (index, call) in calls.iter().enumerate() {
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
            let mut key = call.key(&representative);
            if !summary.allocations.is_empty() {
                let destinations = destinations.get_or_insert_with(|| {
                    let mut destinations: BTreeMap<Node, Option<Node>> = BTreeMap::new();
                    for constraint in analysis.constraints.iter().take(original_constraints) {
                        if let Constraint::Store(Place::Direct(destination), source, _) = constraint
                        {
                            destinations
                                .entry(*source)
                                .and_modify(|known| {
                                    if *known != Some(*destination) {
                                        *known = None;
                                    }
                                })
                                .or_insert(Some(*destination));
                        }
                    }
                    destinations
                });
                let places: Option<Vec<_>> = call
                    .result
                    .iter()
                    .map(|(_, node)| destinations.get(node).copied().flatten())
                    .collect();
                if let Some(places) = places.filter(|places| !places.is_empty()) {
                    key.result_places = places;
                } else {
                    key.indirect_site = Some(call.location);
                }
            }
            // 同じ要約は推論時に共有済み。関数名が違っても同じ効果なら再展開しない。
            key.id = Rc::as_ptr(summary) as usize;
            if let Some(&previous) = applied.get(&key) {
                let allocations: Vec<(Node, accumulation::Trace)> =
                    allocations_by_call[previous].clone();
                analysis.record_allocations(call, &allocations, summary.discarded.as_ref());
                allocations_by_call.push(allocations);
                for (path, destination) in &call.result {
                    if let Some(source) = calls[previous].result_node(path) {
                        analysis
                            .constraints
                            .push(Constraint::Copy(*destination, source));
                    }
                }
                continue;
            }
            allocations_by_call.push(analysis.apply_summary(call, summary));
            applied.insert(key, index);
        }
        let constraints = std::mem::take(&mut analysis.constraints);
        solver::propagate_with(&mut analysis, &constraints, representative);
        analysis.constraints = constraints;
        drop(applied);
        analysis.direct_calls = calls;
        Ok(analysis)
    }
}

// 呼び出し先の成分を先に並べる。再帰しない連鎖は各関数を一度だけ解けばよい。
fn components(bodies: &BTreeMap<usize, Rc<Analysis>>) -> Vec<Vec<usize>> {
    let ids: Vec<_> = bodies.keys().copied().collect();
    let mut graph = vec![Vec::new(); ids.len()];
    let mut reverse = vec![Vec::new(); ids.len()];
    for (index, body) in bodies.values().enumerate() {
        for call in &body.direct_calls {
            if let Ok(callee) = ids.binary_search(&call.id) {
                graph[index].push(callee);
                reverse[callee].push(index);
            }
        }
    }
    let mut seen = vec![false; ids.len()];
    let mut order = Vec::with_capacity(ids.len());
    let mut stack = Vec::new();
    for index in 0..ids.len() {
        if seen[index] {
            continue;
        }
        seen[index] = true;
        stack.push((index, 0));
        while let Some((node, next)) = stack.last_mut() {
            if let Some(&callee) = graph[*node].get(*next) {
                *next += 1;
                if !seen[callee] {
                    seen[callee] = true;
                    stack.push((callee, 0));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    seen.fill(false);
    let mut result = Vec::new();
    let mut pending = Vec::new();
    for index in order.into_iter().rev() {
        if seen[index] {
            continue;
        }
        seen[index] = true;
        let mut component = Vec::new();
        pending.push(index);
        while let Some(node) = pending.pop() {
            component.push(ids[node]);
            for &caller in &reverse[node] {
                if !seen[caller] {
                    seen[caller] = true;
                    pending.push(caller);
                }
            }
        }
        result.push(component);
    }
    result.reverse();
    result
}

struct Inferred {
    #[cfg(test)]
    summaries: BTreeMap<usize, Rc<Summary>>,
    checked: BTreeMap<usize, Analysis>,
    #[cfg(test)]
    evaluations: usize,
}

fn infer_checked(
    mut bodies: BTreeMap<usize, Rc<Analysis>>,
    export_unused: bool,
    mut summaries: BTreeMap<usize, Rc<Summary>>,
) -> Result<Inferred, Diagnostic> {
    let empty = Rc::new(Summary::default());
    let mut shared: BTreeMap<Summary, Rc<Summary>> = BTreeMap::new();
    for summary in summaries.values_mut() {
        *summary = shared
            .entry((**summary).clone())
            .or_insert_with(|| summary.clone())
            .clone();
    }
    summaries.extend(bodies.keys().map(|&id| (id, empty.clone())));
    let mut checked = BTreeMap::new();
    #[cfg(test)]
    let mut evaluations = 0;
    let mut callers: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (&id, body) in &bodies {
        for call in &body.direct_calls {
            callers.entry(call.id).or_default().insert(id);
        }
    }
    let exported: BTreeSet<_> = callers.keys().copied().collect();
    let mut pending = VecDeque::new();
    for component in components(&bodies) {
        let recursive = component.len() > 1
            || bodies[&component[0]]
                .direct_calls
                .iter()
                .any(|call| call.id == component[0]);
        let members: BTreeSet<_> = if recursive {
            component.iter().copied().collect()
        } else {
            BTreeSet::new()
        };
        pending.extend(component.iter().copied());
        let mut queued = members.clone();
        while let Some(id) = pending.pop_front() {
            #[cfg(test)]
            {
                evaluations += 1;
            }
            queued.remove(&id);
            let (mut analysis, forwarded) = if recursive {
                (bodies[&id].with_summaries(&summaries)?, None)
            } else {
                let mut body = Rc::unwrap_or_clone(bodies.remove(&id).unwrap());
                if let Some(summary) = body.forwarded_summary(&summaries) {
                    body.resolved = true;
                    (body, Some(summary))
                } else {
                    (body.resolve_summaries(&summaries)?, None)
                }
            };
            let mut summary = if export_unused || exported.contains(&id) {
                forwarded.unwrap_or_else(|| analysis.summary())
            } else {
                Summary::default()
            };
            if recursive {
                summary.compact_recursive(component[0]);
            }
            checked.insert(id, analysis);
            let changed = if recursive {
                !summaries[&id].same_effects(&summary)
            } else {
                *summaries[&id] != summary
            };
            if !changed {
                continue;
            }
            let summary = shared
                .entry(summary.clone())
                .or_insert_with(|| Rc::new(summary))
                .clone();
            summaries.insert(id, summary);
            if recursive && let Some(dependents) = callers.get(&id) {
                for &caller in dependents {
                    if members.contains(&caller) && queued.insert(caller) {
                        pending.push_back(caller);
                    }
                }
            }
        }
    }
    Ok(Inferred {
        #[cfg(test)]
        summaries,
        checked,
        #[cfg(test)]
        evaluations,
    })
}

/// 全具体化の要約を収束させて呼び出しを検査し、C生成には不要な解析本体を取り除く。
pub(in crate::check) fn check_program(
    functions: &mut [CheckedFunction],
) -> Result<Vec<(Location, String, Vec<Diagnostic>)>, Diagnostic> {
    let mut bodies = BTreeMap::new();
    let mut ready = BTreeMap::new();
    for function in functions {
        match function.lifetime.take() {
            Some(Body::Pending(body)) => {
                bodies.insert(function.id, body);
            }
            Some(Body::Resolved(summary)) => {
                ready.insert(function.id, summary);
            }
            Some(Body::Input(_)) => unreachable!("callable contexts are resolved first"),
            None => {
                ready.insert(function.id, Rc::new(Summary::default()));
            }
        }
    }
    let Inferred { checked, .. } = infer_checked(bodies, false, ready)?;
    let mut warnings = Vec::new();
    for mut analysis in checked.into_values() {
        warnings.extend(analysis.loop_warnings());
        if analysis.resolved || analysis.root_references_only() {
            continue;
        }
        let constraints = std::mem::take(&mut analysis.constraints);
        analysis.validate(&constraints)?;
    }
    warnings.sort_by_key(|warning| warning.0);
    warnings.dedup_by_key(|warning| warning.0);
    Ok(warnings)
}

#[cfg(test)]
fn infer(
    bodies: &BTreeMap<usize, Rc<Analysis>>,
) -> Result<BTreeMap<usize, Rc<Summary>>, Diagnostic> {
    Ok(infer_checked(bodies.clone(), true, BTreeMap::new())?.summaries)
}

#[cfg(test)]
mod tests;
