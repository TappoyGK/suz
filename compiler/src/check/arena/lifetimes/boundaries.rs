//! 関数の仮引数と返却先を寿命要約の境界へ写す。
use super::*;

impl Analysis {
    pub(super) fn field_type(&self, mut ty: Type, path: &[usize]) -> Type {
        if let Type::Array(id) = ty {
            ty = self.structs.arrays.borrow().get(id).element;
        }
        for &index in path {
            let Type::Struct(id) = ty else {
                unreachable!("checked field path")
            };
            ty = self.structs.get(id).fields[index].ty;
        }
        ty
    }

    // 再帰構造体でも有限になるよう、型ごとの未知の実体を先に登録してからメンバーを埋める。
    pub(super) fn parameter_object(
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
                self.targets[field].insert(Reference::object(target));
            }
            index += 1;
        }
        cell
    }

    // 仮引数自身の実体と、そのフィールドから到達する実体は区別する。
    // 同じ型というだけでまとめると、n.nextへの保存がn自身への保存先候補を増やす。
    pub(super) fn parameter_root(
        &mut self,
        index: usize,
        ty: Type,
        region: usize,
        location: Location,
        roots: &mut BTreeMap<Type, Node>,
        objects: &mut Vec<(Type, Node)>,
    ) -> Node {
        if let Some(&root) = roots.get(&ty) {
            return root;
        }
        let root = self.cell(region, location, ty);
        self.cells.get_mut(&root).unwrap().origin = Origin::Parameter;
        self.objects
            .insert(root, summaries::Object::Parameter(index, ty));
        roots.insert(ty, root);
        for path in self.paths(ty) {
            let target = self.field_type(ty, &path).pointee().unwrap();
            let object = self.parameter_object(target, region, location, objects);
            let field = self.project(root, &path);
            self.targets[field].insert(Reference::object(object));
        }
        root
    }

    pub(super) fn parameters(
        &mut self,
        parameters: &[(Type, Location, bool)],
        end: Location,
    ) -> (usize, Option<usize>) {
        self.regions[0].end = end;
        // 0が呼び出し時の現在のArena。引数の実体は要約を適用するときに
        // 実引数の由来へ置き換える。ローカル変数の保存場所は関数の終了まで。
        let body = self.region(0, end);
        self.return_region = Some(0);
        for (id, &(ty, location, reference)) in parameters.iter().enumerate() {
            let region = 0;
            let mut private = Vec::new();
            let objects = &mut private;
            let mut roots = BTreeMap::new();
            // 参照捕捉の保存場所は呼び出し元にある。値捕捉・通常の仮引数はローカルなコピー。
            let storage = if reference { region } else { body };
            let local = self.cell(storage, location, ty);
            self.cells.get_mut(&local).unwrap().origin = Origin::Parameter;
            self.locals.insert(id, local);
            if reference {
                self.objects.insert(local, summaries::Object::Slot(id));
            }
            for path in self.paths(ty) {
                let pointee = self.field_type(ty, &path).pointee().unwrap();
                let object =
                    self.parameter_root(id, pointee, region, location, &mut roots, objects);
                let field = self.project(local, &path);
                self.targets[field].insert(Reference::object(object));
            }
            for &(ty, object) in objects.iter() {
                self.objects
                    .insert(object, summaries::Object::ReachableParameter(id, ty));
            }
        }
        (body, Some(0))
    }

    pub(super) fn return_value(&mut self, ty: Type, value: Shape, location: Location) {
        if value.is_empty() {
            return;
        }
        let region = self
            .return_region
            .expect("ordinary functions cannot return pointer-containing values");
        if self.returned.is_empty() {
            let result = self.cell(region, location, ty);
            self.returned = self
                .paths(ty)
                .into_iter()
                .map(|path| {
                    let cell = self.project(result, &path);
                    (path, cell)
                })
                .collect();
        }
        // 返却先の検査と要約で同じ保存場所を使い、同じ値を別のノードへ写さない。
        for (path, source) in value {
            if self.objection {
                self.objections.insert(self.constraints.len());
            }
            self.constraints.push(Constraint::Store(
                Place::Direct(self.returned[&path]),
                source,
                location,
            ));
        }
    }
}
