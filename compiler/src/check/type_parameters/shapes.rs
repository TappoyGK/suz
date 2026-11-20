//! fnshapeの型引数を展開し、具体化後の署名で関数型をまとめる。
use crate::{
    Diagnostic,
    ast::{FnShapeDeclaration, FunctionType, Type, generics::Application},
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

#[derive(Default)]
pub(in crate::check) struct Shapes {
    names: BTreeSet<String>,
    pub has_generics: bool,
    pub plain_types: RefCell<Vec<Option<usize>>>,
    pub types: Rc<RefCell<Vec<FunctionType>>>,
    pub declarations: Vec<FnShapeDeclaration>,
    pub templates: Vec<FunctionType>,
    pub active: RefCell<Vec<Application>>,
    instances: RefCell<BTreeMap<Application, (usize, usize)>>,
    registrations: Cell<usize>,
    origins: RefCell<BTreeMap<usize, Application>>,
    canonical: RefCell<BTreeMap<FunctionType, usize>>,
    aliases: RefCell<BTreeMap<usize, usize>>,
    unfinished: RefCell<BTreeSet<usize>>,
    backreferences: RefCell<BTreeSet<usize>>,
    revision: Cell<usize>,
}
impl Shapes {
    pub fn new(program: &crate::ast::Program) -> Self {
        Self {
            names: program.fnshapes.iter().map(|d| d.name.clone()).collect(),
            plain_types: RefCell::new(vec![None; program.fnshapes.len()]),
            has_generics: program
                .fnshapes
                .iter()
                .any(|d| !d.type_parameters.is_empty()),
            declarations: program.fnshapes.clone(),
            templates: program.function_types.clone(),
            ..Self::default()
        }
    }
    pub fn contains_name(&self, name: &str) -> bool {
        self.names.contains(name)
    }
    /// 型適用を含まない通常の署名には予約枠も具体化表も不要。
    /// 構造体名は既に宣言番号を持つので、相互参照でもその番号を使える。
    pub fn prepare_plain(&self) {
        for (id, declaration) in self.declarations.iter().enumerate() {
            let signature = &self.templates[declaration.type_id];
            if !declaration.type_parameters.is_empty()
                || signature
                    .parameters
                    .iter()
                    .chain([&signature.result])
                    .any(|ty| {
                        matches!(
                            ty.leaf(),
                            Type::Application(_)
                                | Type::StaticShape(_)
                                | Type::Shape(_)
                                | Type::Parameter(_)
                        )
                    })
            {
                continue;
            }
            let type_id = self.intern(signature.clone());
            self.plain_types.borrow_mut()[id] = Some(type_id);
            self.origins
                .borrow_mut()
                .entry(type_id)
                .or_insert_with(|| Application {
                    declaration: id,
                    arguments: Vec::new(),
                });
        }
    }
    pub fn origin(&self, id: usize) -> Option<Application> {
        self.origins.borrow().get(&id).cloned()
    }
    pub fn canonical_id(&self, mut id: usize) -> usize {
        while let Some(&next) = self.aliases.borrow().get(&id) {
            id = next;
        }
        id
    }
    pub fn normalize(&self, ty: Type) -> Type {
        if let Some(inner) = ty.pointee() {
            return self
                .normalize(inner)
                .pointer()
                .with_nullable(ty.is_nullable());
        }
        match ty {
            Type::StaticFunction(id) => Type::StaticFunction(self.canonical_id(id)),
            Type::Function(id) => Type::Function(self.canonical_id(id)),
            _ => ty,
        }
    }
    pub fn intern(&self, signature: FunctionType) -> usize {
        if let Some(&id) = self.canonical.borrow().get(&signature) {
            return id;
        }
        let id = self.types.borrow().len();
        self.types.borrow_mut().push(signature.clone());
        self.canonical.borrow_mut().insert(signature, id);
        id
    }
    // 構造体を介する再帰に備え、署名を展開する前に番号を予約する。
    pub fn instantiate(
        &self,
        app: Application,
        resolve: impl FnOnce(&FunctionType) -> Result<FunctionType, Diagnostic>,
    ) -> Result<usize, Diagnostic> {
        if let Some(&(id, _)) = self.instances.borrow().get(&app) {
            if self.unfinished.borrow().contains(&id) {
                self.backreferences.borrow_mut().insert(id);
            }
            return Ok(self.canonical_id(id));
        }
        let id = self.types.borrow().len();
        self.types.borrow_mut().push(FunctionType {
            parameters: Vec::new(),
            result: Type::Void,
            errors: Vec::new(),
        });
        let registration = self.registrations.get();
        self.registrations.set(registration + 1);
        self.instances
            .borrow_mut()
            .insert(app.clone(), (id, registration));
        self.origins.borrow_mut().insert(id, app.clone());
        self.active.borrow_mut().push(app.clone());
        self.unfinished.borrow_mut().insert(id);
        let result = resolve(&self.templates[self.declarations[app.declaration].type_id]);
        self.active.borrow_mut().pop();
        self.unfinished.borrow_mut().remove(&id);
        let mut signature = result?;
        for ty in signature
            .parameters
            .iter_mut()
            .chain([&mut signature.result])
        {
            *ty = self.normalize(*ty);
        }
        self.types.borrow_mut()[id] = signature.clone();
        let existing = self.canonical.borrow().get(&signature).copied();
        if let Some(existing) = existing {
            self.instances
                .borrow_mut()
                .insert(app, (existing, registration));
            let referenced = self.backreferences.borrow_mut().remove(&id);
            if !referenced && id + 1 == self.types.borrow().len() {
                // 外へ参照が出ていない末尾の予約枠は、そのまま取り除ける。
                self.types.borrow_mut().pop();
                self.origins.borrow_mut().remove(&id);
            } else {
                self.aliases.borrow_mut().insert(id, existing);
                if referenced {
                    self.revision.set(self.revision.get() + 1);
                }
            }
            Ok(existing)
        } else {
            self.canonical.borrow_mut().insert(signature, id);
            Ok(id)
        }
    }
    /// 再帰先へ渡した予約番号の変更回数。通常の別名では表全体の更新を不要にする。
    pub fn revision(&self) -> usize {
        self.revision.get()
    }
    pub fn alias_count(&self) -> usize {
        self.aliases.borrow().len()
    }
    pub fn recanonicalize(&self, normalize: impl Fn(Type) -> Type) {
        {
            let mut types = self.types.borrow_mut();
            for signature in types.iter_mut() {
                for ty in signature
                    .parameters
                    .iter_mut()
                    .chain([&mut signature.result])
                {
                    *ty = normalize(*ty);
                }
            }
        }
        let mut canonical = self.canonical.borrow_mut();
        canonical.clear();
        for (id, signature) in self.types.borrow().iter().enumerate() {
            if self.aliases.borrow().contains_key(&id) {
                continue;
            }
            if let Some(&existing) = canonical.get(signature) {
                self.aliases.borrow_mut().insert(id, existing);
            } else {
                canonical.insert(signature.clone(), id);
            }
        }
    }
    pub fn checkpoint(&self) -> (usize, usize) {
        (self.types.borrow().len(), self.registrations.get())
    }
    pub fn rollback(&self, (start, registration): (usize, usize)) {
        self.types.borrow_mut().truncate(start);
        // 既存の関数型と同一になった適用も戻す。型引数が取り消した構造体を指しうる。
        self.instances
            .borrow_mut()
            .retain(|_, (_, created)| *created < registration);
        self.registrations.set(registration);
        self.origins.borrow_mut().retain(|id, _| *id < start);
        self.canonical.borrow_mut().retain(|_, id| *id < start);
        self.aliases.borrow_mut().retain(|id, _| *id < start);
        self.backreferences.borrow_mut().retain(|id| *id < start);
    }
}
