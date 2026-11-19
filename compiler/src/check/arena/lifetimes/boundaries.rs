//! 関数境界の借用とarena fnの契約を、既存の候補伝播へ落とす。
//! 本体の実装を呼び出しごとに再解析せず、型と契約から保守的な効果を作る。
use super::*;

pub(super) struct CallCheck {
    arguments: Vec<(Type, Shape)>,
    region: usize,
    location: Location,
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
        // 各ポインタの指す型について、既存実体と呼び出し中の確保の候補をまとめる。
        let mut groups: Vec<(Type, Node)> = Vec::new();
        let mut add_type = |ty: Type, this: &mut Self| {
            for path in this.paths(ty) {
                let target = this.field_type(ty, &path).pointee().unwrap();
                if !groups.iter().any(|(ty, _)| *ty == target) {
                    groups.push((target, this.node()));
                }
            }
        };
        for (ty, _) in arguments {
            add_type(*ty, self);
        }
        add_type(result_ty, self);
        let mut index = 0;
        while index < groups.len() {
            let (ty, _) = groups[index];
            for path in self.paths(ty) {
                let target = self.field_type(ty, &path).pointee().unwrap();
                if !groups.iter().any(|(ty, _)| *ty == target) {
                    groups.push((target, self.node()));
                }
            }
            index += 1;
        }
        let group = |ty| groups.iter().find(|(t, _)| *t == ty).unwrap().1;
        for &(ty, node) in &groups {
            let allocated = self.cell(region, location, ty);
            self.cells.get_mut(&allocated).unwrap().origin = Origin::Call;
            self.targets[node].insert(allocated);
            for path in self.paths(ty) {
                let target = self.field_type(ty, &path).pointee().unwrap();
                let other = group(target);
                let place = Place::Indirect(node, path);
                // 引数から推移的に到達する候補を読み、書き換えうる同型の候補を保存する。
                // 新たな辺が見つかるたびに既存のsolverが伝播するため、循環や後からの更新も含む。
                self.constraints
                    .push(Constraint::Load(other, place.clone()));
                self.constraints
                    .push(Constraint::Store(place, other, location));
            }
        }
        for (ty, shape) in arguments {
            for (path, &source) in shape {
                let target = self.field_type(*ty, path).pointee().unwrap();
                self.constraints
                    .push(Constraint::Copy(group(target), source));
            }
        }
        for (path, &node) in &result {
            let target = self.field_type(result_ty, path).pointee().unwrap();
            self.constraints.push(Constraint::Copy(node, group(target)));
        }
        self.calls.push(CallCheck {
            arguments: arguments.to_vec(),
            region,
            location,
        });
        result
    }

    pub(super) fn validate_calls(&self) -> Result<(), Diagnostic> {
        for call in &self.calls {
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
                        return Err(error);
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
        Ok(())
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
