//! 関数境界の借用とarena fnの契約を、既存の候補伝播へ落とす。
//! 本体の実装を呼び出しごとに再解析せず、型と契約から保守的な効果を作る。
use super::*;

pub(super) struct CallCheck {
    arguments: Vec<(Type, Shape)>,
    region: usize,
    location: Location,
}

pub(super) struct CallGroup {
    pub region: usize,
    ty: Type,
    reachable: Node,
    local: Node,
}

impl Analysis<'_> {
    fn field_type(&self, mut ty: Type, path: &[usize]) -> Type {
        for &index in path {
            let Type::Struct(id) = ty else {
                unreachable!("checked field path")
            };
            ty = self.scope.structs[id].fields[index].ty;
        }
        ty
    }

    // 再帰構造体でも有限になるよう、型ごとの未知の実体を先に登録してからメンバーを埋める。
    fn parameter_object(
        &mut self,
        ty: Type,
        region: usize,
        location: Location,
        objects: &mut Vec<(Type, Node)>,
    ) -> Node {
        if let Some((_, cell)) = objects.iter().find(|(t, _)| *t == ty) {
            return *cell;
        }
        let cell = self.cell(region, location, ty);
        self.cells.get_mut(&cell).unwrap().origin = Origin::Parameter;
        objects.push((ty, cell));
        // ポインタを介する型の長い連鎖でも、Rustの呼び出しスタックを消費しない。
        let mut index = objects.len() - 1;
        while index < objects.len() {
            let (ty, object) = objects[index];
            for path in self.paths(ty) {
                let pointee = self.field_type(ty, &path).pointee().unwrap();
                let target = if let Some((_, cell)) = objects.iter().find(|(ty, _)| *ty == pointee)
                {
                    *cell
                } else {
                    let cell = self.cell(region, location, pointee);
                    self.cells.get_mut(&cell).unwrap().origin = Origin::Parameter;
                    objects.push((pointee, cell));
                    cell
                };
                let field = self.project(object, &path);
                self.targets[field].insert(target);
            }
            index += 1;
        }
        cell
    }

    pub(super) fn parameters(
        &mut self,
        function: &crate::ast::Function,
        references: &BTreeMap<String, super::super::Binding>,
    ) -> (usize, Option<usize>) {
        self.regions[0].end = function.end;
        // arena fnでは0が呼び出し元のR。ローカル変数の寿命はRより短い。
        let body = if function.arena {
            self.region(0, function.end)
        } else {
            0
        };
        self.return_region = function.arena.then_some(0);
        let mut shared = Vec::new();
        for (id, parameter) in function.parameters.iter().enumerate() {
            let ty = self.scope.resolve_type(parameter.ty);
            if !contains(ty, self.scope) {
                continue;
            }
            let region = if function.arena {
                0
            } else {
                let region = self.regions.len();
                self.regions.push(Region {
                    parent: None,
                    end: parameter.location,
                });
                self.borrowed.insert(region);
                region
            };
            let mut private = Vec::new();
            let objects = if function.arena {
                &mut shared
            } else {
                &mut private
            };
            // 参照捕捉の保存場所は呼び出し元にある。値捕捉・通常の仮引数はローカルなコピー。
            let storage = if references.contains_key(&parameter.name) {
                region
            } else {
                body
            };
            let local = self.cell(storage, parameter.location, ty);
            self.cells.get_mut(&local).unwrap().origin = Origin::Parameter;
            self.locals.insert(id, local);
            for path in self.paths(ty) {
                let pointee = self.field_type(ty, &path).pointee().unwrap();
                let object = self.parameter_object(pointee, region, parameter.location, objects);
                let field = self.project(local, &path);
                self.targets[field].insert(object);
            }
        }
        (body, function.arena.then_some(0))
    }

    pub(super) fn return_value(&mut self, ty: Type, value: Shape, location: Location) {
        if value.is_empty() {
            return;
        }
        let region = self
            .return_region
            .expect("ordinary functions cannot return pointer-containing values");
        let result = self.cell(region, location, ty);
        self.write(Place::Direct(result), value, location);
    }

    pub(super) fn arena_call(
        &mut self,
        arguments: &[(Type, Shape)],
        result_ty: Type,
        region: usize,
        location: Location,
    ) -> Shape {
        let result = self.shape(result_ty);
        for (ty, shape) in arguments {
            for (path, &source) in shape {
                let target = self.field_type(*ty, path).pointee().unwrap();
                let (reachable, _) = self.call_group(region, target, location);
                self.constraints.push(Constraint::Copy(reachable, source));
            }
        }
        for (path, &node) in &result {
            let target = self.field_type(result_ty, path).pointee().unwrap();
            let (_, local) = self.call_group(region, target, location);
            self.constraints.push(Constraint::Copy(node, local));
        }
        self.calls.push(CallCheck {
            arguments: arguments.to_vec(),
            region,
            location,
        });
        result
    }

    // 同じR・型の呼び出し効果は一組だけ作る。R以外に届く実引数は必ず拒否するので、
    // この合流で保存・返却の寿命判定は変わらない。呼び出しごとに同じ密な参照グラフを
    // 複製すると、実体数に対する二乗の伝播をさらに呼び出し回数だけ繰り返してしまう。
    fn add_call_group(&mut self, region: usize, ty: Type) -> (Node, Node) {
        let reachable = self.node();
        let local = self.node();
        self.call_regions.insert(local, region);
        self.constraints.push(Constraint::Copy(local, reachable));
        self.call_groups.push(CallGroup {
            region,
            ty,
            reachable,
            local,
        });
        (reachable, local)
    }

    fn call_group(&mut self, region: usize, ty: Type, location: Location) -> (Node, Node) {
        if let Some(group) = self
            .call_groups
            .iter()
            .find(|g| g.region == region && g.ty == ty)
        {
            return (group.reachable, group.local);
        }
        let start = self.call_groups.len();
        let root = self.add_call_group(region, ty);
        let mut index = start;
        while index < self.call_groups.len() {
            let CallGroup {
                ty,
                reachable,
                local,
                ..
            } = self.call_groups[index];
            let allocated = self.cell(region, location, ty);
            self.cells.get_mut(&allocated).unwrap().origin = Origin::Call;
            self.targets[reachable].insert(allocated);
            for path in self.paths(ty) {
                let target = self.field_type(ty, &path).pointee().unwrap();
                let (other_reachable, other_local) = if let Some(group) = self
                    .call_groups
                    .iter()
                    .find(|g| g.region == region && g.ty == target)
                {
                    (group.reachable, group.local)
                } else {
                    self.add_call_group(region, target)
                };
                // R外に届く入力は検査のためにたどるが、その実体への書き込みや
                // R外の参照の返却を、別の呼び出しへ伝播させてはならない。
                self.constraints.push(Constraint::Load(
                    other_reachable,
                    Place::Indirect(reachable, path.clone()),
                ));
                self.constraints.push(Constraint::Store(
                    Place::Indirect(local, path),
                    other_local,
                    location,
                ));
            }
            index += 1;
        }
        root
    }

    pub(super) fn validate_calls(&self) -> Result<(), Diagnostic> {
        // グループには推移的な参照先まで伝播済み。正常な領域を呼び出しごとにたどらない。
        let invalid: BTreeSet<_> = self
            .call_groups
            .iter()
            .filter_map(|group| {
                self.targets[group.reachable]
                    .iter()
                    .any(|target| self.cells[target].region != group.region)
                    .then_some(group.region)
            })
            .collect();
        let mut secondary = None;
        for call in &self.calls {
            if !invalid.contains(&call.region) {
                continue;
            }
            // 実引数から実体をたどり、違反に至るメンバーの経路を診断にも残す。
            // 同じ実体は一度だけ調べ、循環するグラフでも停止させる。
            let mut pending = std::collections::VecDeque::new();
            for (index, (ty, shape)) in call.arguments.iter().enumerate() {
                for (path, &node) in shape {
                    let target = self.field_type(*ty, path).pointee().unwrap();
                    let label = self.field_label(*ty, path, format!("argument {}", index + 1));
                    pending.push_back((target, node, label));
                }
            }
            let mut visited = BTreeSet::new();
            while let Some((ty, node, label)) = pending.pop_front() {
                for &target in &self.targets[node] {
                    let cell = &self.cells[&target];
                    if cell.region != call.region {
                        let mut error = call.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "arena.fn-call",
                                "{label} may reach an object outside the innermost arena"
                            ),
                        );
                        error.related.push(cell.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!("arena.lifetime", "this reachable object belongs to another arena or a borrowed parameter"),
                        ));
                        error.related.push(self.regions[cell.region].end.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "arena.lifetime",
                                "{}",
                                if self.borrowed.contains(&cell.region) {
                                    "this parameter is borrowed only for the duration of the call"
                                } else {
                                    "its arena ends here"
                                }
                            ),
                        ));
                        // 短命な候補は別の違反から伝わった結果かもしれない。
                        // 外側や借用先へ届く呼び出しを優先し、それがなければこの診断を使う。
                        if self.outlives(cell.region, call.region) {
                            return Err(error);
                        }
                        if secondary.is_none() {
                            secondary = Some(error);
                        }
                    }
                    if !visited.insert(target) {
                        continue;
                    }
                    for path in self.paths(ty) {
                        let mut full_path = cell.path.clone();
                        full_path.extend_from_slice(&path);
                        if let Some(&field) = self.projections.get(&(cell.root, full_path)) {
                            let next = self.field_type(ty, &path).pointee().unwrap();
                            let label = self.field_label(ty, &path, label.clone());
                            pending.push_back((next, field, label));
                        }
                    }
                }
            }
        }
        secondary.map_or(Ok(()), Err)
    }

    fn field_label(&self, mut ty: Type, path: &[usize], mut label: String) -> String {
        for &index in path {
            let Type::Struct(id) = ty else {
                unreachable!("checked field path")
            };
            let field = &self.scope.structs[id].fields[index];
            label.push('.');
            label.push_str(&field.name);
            ty = field.ty;
        }
        label
    }
}
