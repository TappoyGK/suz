//! 構造体とfnshapeを型引数ごとに展開する。両者の相互参照も同じ具体化へ戻す。
use crate::{
    Diagnostic, ErrorKind,
    ast::{
        Location, Program, StructDeclaration, Type,
        generics::{Application, TypeParameter},
    },
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

/// スコープ間で共有する構造体表。登録済み宣言はRcで読み、展開中に借用を持ち越さない。
#[derive(Default)]
pub(in crate::check) struct Structs {
    pub shapes: super::shapes::Shapes,
    pub arrays: RefCell<crate::ast::arrays::Types>,
    declarations: RefCell<Vec<Rc<StructDeclaration>>>,
    applications: Vec<Application>,
    instances: RefCell<BTreeMap<Application, usize>>,
    aliases: RefCell<BTreeMap<usize, usize>>,
    names: BTreeSet<String>,
    need_names: BTreeSet<String>,
    original_count: usize,
    enums: Vec<crate::ast::EnumDeclaration>,
}
impl From<Vec<StructDeclaration>> for Structs {
    fn from(declarations: Vec<StructDeclaration>) -> Self {
        Self {
            names: declarations.iter().map(|d| d.name.clone()).collect(),
            original_count: declarations.len(),
            declarations: RefCell::new(declarations.into_iter().map(Rc::new).collect()),
            ..Self::default()
        }
    }
}
impl Structs {
    pub fn new(program: &Program) -> Self {
        Self {
            need_names: program.need_groups.iter().map(|g| g.name.clone()).collect(),
            arrays: RefCell::new(program.arrays.clone()),
            applications: program.applications.clone(),
            shapes: super::shapes::Shapes::new(program),
            enums: program.enums.clone(),
            ..Self::from(program.structs.clone())
        }
    }
    pub fn get(&self, id: usize) -> Rc<StructDeclaration> {
        self.declarations.borrow()[self.canonical_id(id)].clone()
    }
    fn canonical_id(&self, mut id: usize) -> usize {
        while let Some(&next) = self.aliases.borrow().get(&id) {
            id = next;
        }
        id
    }
    fn normalize(&self, ty: Type) -> Type {
        if let Some(inner) = ty.pointee() {
            return self
                .normalize(inner)
                .pointer()
                .with_readonly(ty.is_readonly_pointer())
                .with_nullable(ty.is_nullable());
        }
        match ty {
            Type::Array(id) => {
                let array = self.arrays.borrow().get(id);
                let element = self.normalize(array.element);
                self.arrays.borrow_mut().intern(element, array.rank)
            }
            Type::ArrayRef {
                id,
                mutable,
                nullable,
            } => {
                let array = self.arrays.borrow().get(id);
                let element = self.normalize(array.element);
                let Type::Array(id) = self.arrays.borrow_mut().intern(element, array.rank) else {
                    unreachable!()
                };
                Type::ArrayRef {
                    id,
                    mutable,
                    nullable,
                }
            }
            Type::Struct(id) => Type::Struct(self.canonical_id(id)),
            _ => self.shapes.normalize(ty),
        }
    }
    // 再帰の途中で予約番号を参照したフィールドも、確定した関数型へ揃える。
    // 型引数に関数型を含む構造体も同じキーで再利用できるよう、双方の表を更新する。
    fn normalize_instances(&self) {
        loop {
            let before = self.shapes.alias_count() + self.aliases.borrow().len();
            let mut instances = self.instances.borrow_mut();
            instances.clear();
            let count = self.declarations.borrow().len();
            for id in 0..count {
                if self.aliases.borrow().contains_key(&id) {
                    continue;
                }
                let mut declaration = self.get(id).as_ref().clone();
                for field in &mut declaration.fields {
                    field.ty = self.normalize(field.ty);
                }
                if let Some(app) = &mut declaration.application {
                    for argument in &mut app.arguments {
                        *argument = self.normalize(*argument);
                    }
                    declaration.symbolic = app.arguments.iter().any(|ty| self.unresolved(*ty));
                    if let Some(&existing) = instances.get(app) {
                        self.aliases.borrow_mut().insert(id, existing);
                    } else {
                        instances.insert(app.clone(), id);
                    }
                }
                self.declarations.borrow_mut()[id] = Rc::new(declaration);
            }
            drop(instances);
            self.shapes.recanonicalize(|ty| self.normalize(ty));
            if before == self.shapes.alias_count() + self.aliases.borrow().len() {
                break;
            }
        }
    }
    pub fn contains_name(&self, name: &str) -> bool {
        self.names.contains(name) || self.need_names.contains(name)
    }
    pub fn snapshot(&self) -> Vec<StructDeclaration> {
        self.declarations
            .borrow()
            .iter()
            .map(|d| d.as_ref().clone())
            .collect()
    }
    fn validate_layout(&self, start: usize) -> Result<(), Diagnostic> {
        let declarations = self.declarations.borrow();
        crate::parser::structs::validate_layout_range(
            start,
            declarations.len(),
            |id| declarations[id].as_ref(),
            &self.arrays.borrow(),
        )
    }
    pub fn prepare(&self) -> Result<(), Diagnostic> {
        self.shapes.prepare_plain();
        // 直接参照する関数型を先に登録し、長い宣言列を再帰でたどらない。
        let mut order: Vec<_> = (0..self.shapes.declarations.len()).collect();
        order.sort_by_key(|id| self.shapes.declarations[*id].type_id);
        for id in order {
            if self.shapes.plain_types.borrow()[id].is_some() {
                continue;
            }
            let declaration = &self.shapes.declarations[id];
            let app = Application {
                declaration: id,
                arguments: (0..declaration.type_parameters.len())
                    .map(Type::Parameter)
                    .collect(),
            };
            let ty = self.transaction(|| {
                self.instantiate_shape(
                    app,
                    declaration.location,
                    &mut Vec::new(),
                    &declaration.type_parameters,
                )
                .map(Type::StaticFunction)
            })?;
            if declaration.type_parameters.is_empty() {
                let Type::StaticFunction(type_id) = ty else {
                    unreachable!()
                };
                self.shapes.plain_types.borrow_mut()[id] = Some(type_id);
            }
        }
        for id in 0..self.original_count {
            let declaration = self.get(id);
            if !declaration.type_parameters.is_empty() {
                continue;
            }
            let mut resolved = declaration.as_ref().clone();
            for field in &mut resolved.fields {
                field.ty = self.resolve(field.ty, &[], field.location)?;
            }
            self.declarations.borrow_mut()[id] = Rc::new(resolved);
        }
        self.validate_layout(0)
    }
    /// 型引数のないfnshapeはprepareで解決済みなので、使用ごとに具体化表を引かない。
    pub fn plain_shape(&self, ty: Type) -> Option<Type> {
        let (id, persistent) = match ty {
            Type::StaticShape(id) => (id, true),
            Type::Shape(id) => (id, false),
            _ => return None,
        };
        let declaration = self.applications[id].declaration;
        let id = self.shapes.plain_types.borrow()[declaration]?;
        let id = self.shapes.canonical_id(id);
        Some(if persistent {
            Type::StaticFunction(id)
        } else {
            Type::Function(id)
        })
    }
    pub fn application(&self, id: usize) -> &Application {
        &self.applications[id]
    }
    #[inline]
    pub fn symbolic(&self, ty: Type) -> bool {
        match ty.leaf() {
            Type::Array(id) | Type::ArrayRef { id, .. } => {
                self.symbolic(self.arrays.borrow().get(id).element)
            }
            Type::Parameter(_) => true,
            Type::Struct(id) => self.get(id).symbolic,
            Type::Application(id) | Type::StaticShape(id) | Type::Shape(id) => self.applications
                [id]
                .arguments
                .iter()
                .any(|ty| self.symbolic(*ty)),
            Type::StaticFunction(_) | Type::Function(_) => {
                self.symbolic_inner(ty, &mut BTreeSet::new())
            }
            _ => false,
        }
    }
    fn symbolic_inner(&self, ty: Type, seen: &mut BTreeSet<usize>) -> bool {
        match ty.leaf() {
            Type::Array(id) | Type::ArrayRef { id, .. } => {
                self.symbolic_inner(self.arrays.borrow().get(id).element, seen)
            }
            Type::Parameter(_) => true,
            Type::Struct(id) => self.get(id).symbolic,
            Type::StaticFunction(id) | Type::Function(id) => {
                if !seen.insert(id) {
                    return false;
                }
                let signature = self.shapes.types.borrow()[id].clone();
                signature
                    .parameters
                    .iter()
                    .chain([&signature.result])
                    .any(|t| self.symbolic_inner(*t, seen))
            }
            Type::Application(id) | Type::StaticShape(id) | Type::Shape(id) => self.applications
                [id]
                .arguments
                .iter()
                .any(|t| self.symbolic_inner(*t, seen)),
            _ => false,
        }
    }
    /// コード生成前に解決する型。名前付き型引数と、既存のint・floatの両方を含む。
    pub fn unresolved(&self, ty: Type) -> bool {
        if let Type::Array(id) | Type::ArrayRef { id, .. } = ty.leaf() {
            return self.unresolved(self.arrays.borrow().get(id).element);
        }
        ty.leaf().symbolic() || self.symbolic(ty)
    }
    pub fn name(&self, ty: Type) -> String {
        if let Some(inner) = ty.pointee() {
            return format!(
                "{}{} {}",
                if ty.is_readonly_pointer() {
                    "ref"
                } else {
                    "mut"
                },
                if ty.is_nullable() { "?" } else { "" },
                self.name(inner)
            );
        }
        match ty {
            Type::Struct(id) => self.get(id).name.clone(),
            Type::Enum(ty) => self.enums[ty.id].name.clone(),
            Type::Parameter(id) => format!("T{}", id + 1),
            Type::Application(id) => {
                let app = &self.applications[id];
                format!(
                    "{}[{}]",
                    self.get(app.declaration).name,
                    app.arguments
                        .iter()
                        .map(|t| self.name(*t))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            _ => ty.name().into(),
        }
    }
    pub fn name_with_parameters(&self, ty: Type, parameters: &[TypeParameter]) -> String {
        if let Some(inner) = ty.pointee() {
            return format!(
                "{}{} {}",
                if ty.is_readonly_pointer() {
                    "ref"
                } else {
                    "mut"
                },
                if ty.is_nullable() { "?" } else { "" },
                self.name_with_parameters(inner, parameters)
            );
        }
        match ty {
            Type::Parameter(id) => parameters
                .get(id)
                .map_or_else(|| self.name(ty), |p| p.name.clone()),
            Type::Struct(id) if self.get(id).application.is_some() => {
                let declaration = self.get(id);
                let app = declaration.application.as_ref().unwrap();
                format!(
                    "{}[{}]",
                    self.get(app.declaration).name,
                    app.arguments
                        .iter()
                        .map(|t| self.name_with_parameters(*t, parameters))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            _ => self.name(ty),
        }
    }
    pub fn contains_argument(&self, old: Type, new: Type) -> bool {
        if old == new {
            return true;
        }
        let mut pending = vec![old];
        let mut seen = BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if ty == new {
                return true;
            }
            if !seen.insert(ty) {
                continue;
            }
            if let Some(inner) = ty.pointee() {
                pending.push(inner);
                continue;
            }
            match ty {
                Type::Array(id) | Type::ArrayRef { id, .. } => {
                    pending.push(self.arrays.borrow().get(id).element)
                }
                Type::Struct(id) => {
                    if let Some(app) = &self.get(id).application {
                        pending.extend(&app.arguments);
                    }
                }
                Type::StaticFunction(id) | Type::Function(id) => {
                    let signature = &self.shapes.types.borrow()[id];
                    pending.extend(&signature.parameters);
                    pending.push(signature.result);
                }
                _ => {}
            }
        }
        false
    }
    pub fn apply(
        &self,
        declaration: usize,
        arguments: Vec<Type>,
        location: Location,
    ) -> Result<Type, Diagnostic> {
        self.transaction(|| {
            self.instantiate(
                Application {
                    declaration,
                    arguments,
                },
                location,
                &mut Vec::new(),
                &self.get(declaration).type_parameters,
            )
        })
    }
    pub fn resolve(
        &self,
        ty: Type,
        arguments: &[Type],
        location: Location,
    ) -> Result<Type, Diagnostic> {
        self.resolve_with_parameters(ty, arguments, location, &[])
    }
    pub fn resolve_with_parameters(
        &self,
        ty: Type,
        arguments: &[Type],
        location: Location,
        parameters: &[TypeParameter],
    ) -> Result<Type, Diagnostic> {
        // 型引数自体の置換では新しい型を展開しない。確定した実引数をそのまま使う。
        if let Type::Parameter(id) = ty {
            return Ok(self.normalize(arguments.get(id).copied().unwrap_or(ty)));
        }
        if let Some(ty) = self.plain_shape(ty) {
            return Ok(ty);
        }
        match ty.leaf() {
            Type::Array(_)
            | Type::ArrayRef { .. }
            | Type::Application(_)
            | Type::StaticShape(_)
            | Type::Shape(_) => {}
            Type::Parameter(_) | Type::Struct(_) | Type::StaticFunction(_) | Type::Function(_) => {
                // 既に解決した型へ置き換える型引数がなければ、展開表に触れない。
                if arguments.is_empty() {
                    return Ok(self.normalize(ty));
                }
            }
            _ => return Ok(ty),
        }
        self.transaction(|| {
            self.resolve_inner(ty, arguments, location, &mut Vec::new(), parameters)
        })
    }
    // 型ヒントは診断を捨てて再検査することがある。失敗した展開を表に残すと、
    // 再検査が未完成の宣言を成功済みとして使うため、追加分をまとめて戻す。
    fn transaction(
        &self,
        resolve: impl FnOnce() -> Result<Type, Diagnostic>,
    ) -> Result<Type, Diagnostic> {
        let start = self.declarations.borrow().len();
        let array_start = self.arrays.borrow().entries.len();
        // 関数型のないプログラムでは、関数型表の巻き戻しや予約番号の更新は不要。
        let shape_state = (!self.shapes.declarations.is_empty()
            || !self.shapes.types.borrow().is_empty())
        .then(|| (self.shapes.checkpoint(), self.shapes.revision()));
        let result = resolve().and_then(|ty| {
            if shape_state.is_some() {
                // 予約中の関数型はまだ署名が空なので、展開完了後に記号型の有無を確定する。
                let count = self.declarations.borrow().len();
                for id in start..count {
                    let declaration = self.get(id);
                    if let Some(app) = &declaration.application {
                        let symbolic = app.arguments.iter().any(|ty| self.unresolved(*ty));
                        if symbolic != declaration.symbolic {
                            let mut resolved = declaration.as_ref().clone();
                            resolved.symbolic = symbolic;
                            self.declarations.borrow_mut()[id] = Rc::new(resolved);
                        }
                    }
                }
            }
            self.validate_layout(start)?;
            if let Some((_, alias_start)) = shape_state {
                if self.shapes.revision() != alias_start {
                    self.normalize_instances();
                }
                Ok(self.normalize(ty))
            } else {
                Ok(ty)
            }
        });
        if result.is_err() {
            self.arrays.borrow_mut().truncate(array_start);
            if let Some((shape_start, _)) = shape_state {
                self.shapes.rollback(shape_start);
            }
            let mut declarations = self.declarations.borrow_mut();
            let mut instances = self.instances.borrow_mut();
            for declaration in &declarations[start..] {
                if let Some(application) = &declaration.application {
                    instances.remove(application);
                }
            }
            declarations.truncate(start);
        }
        result
    }
    fn resolve_inner(
        &self,
        ty: Type,
        arguments: &[Type],
        location: Location,
        path: &mut Vec<Application>,
        parameters: &[TypeParameter],
    ) -> Result<Type, Diagnostic> {
        if let Some(ty) = self.plain_shape(ty) {
            return Ok(ty);
        }
        if let Some(inner) = ty.pointee() {
            let inner = self.resolve_inner(inner, arguments, location, path, parameters)?;
            if matches!(inner.leaf(), Type::Function(_)) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("arena.ptr-type", "pointer requires a stored value type"),
                ));
            }
            return Ok(inner
                .pointer()
                .with_readonly(ty.is_readonly_pointer())
                .with_nullable(ty.is_nullable()));
        }
        if matches!(
            ty,
            Type::StaticShape(_) | Type::Shape(_) | Type::StaticFunction(_) | Type::Function(_)
        ) {
            return self.resolve_shape(ty, arguments, location, path, parameters);
        }
        if let Type::Array(id) = ty {
            let array = self.arrays.borrow().get(id);
            let element =
                self.resolve_inner(array.element, arguments, location, path, parameters)?;
            if element == Type::Void || matches!(element, Type::Function(_)) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.type", "array elements require stored value types"),
                ));
            }
            return Ok(self.arrays.borrow_mut().intern(element, array.rank));
        }
        if let Type::ArrayRef {
            id,
            mutable,
            nullable,
        } = ty
        {
            let array = self.arrays.borrow().get(id);
            let element =
                self.resolve_inner(array.element, arguments, location, path, parameters)?;
            if element == Type::Void || matches!(element, Type::Function(_)) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("array.type", "array elements require stored value types"),
                ));
            }
            let Type::Array(id) = self.arrays.borrow_mut().intern(element, array.rank) else {
                unreachable!()
            };
            return Ok(Type::ArrayRef {
                id,
                mutable,
                nullable,
            });
        }
        let application = match ty {
            Type::Parameter(id) => return Ok(arguments.get(id).copied().unwrap_or(ty)),
            Type::Application(id) => self.applications[id].clone(),
            Type::Struct(id) => match self.get(id).application.clone() {
                Some(app) => app,
                None => return Ok(ty),
            },
            _ => return Ok(ty),
        };
        let arguments = application
            .arguments
            .iter()
            .map(|t| self.resolve_inner(*t, arguments, location, path, parameters))
            .collect::<Result<Vec<_>, _>>()?;
        self.instantiate(
            Application {
                arguments,
                ..application
            },
            location,
            path,
            parameters,
        )
    }
    fn resolve_shape(
        &self,
        ty: Type,
        arguments: &[Type],
        location: Location,
        path: &mut Vec<Application>,
        parameters: &[TypeParameter],
    ) -> Result<Type, Diagnostic> {
        if arguments.is_empty() && matches!(ty, Type::StaticFunction(_) | Type::Function(_)) {
            return Ok(self.normalize(ty));
        }
        let (app, static_type) = match ty {
            Type::StaticShape(id) => (Some(self.applications[id].clone()), true),
            Type::Shape(id) => (Some(self.applications[id].clone()), false),
            Type::StaticFunction(id) => (self.shapes.origin(id), true),
            Type::Function(id) => (self.shapes.origin(id), false),
            _ => unreachable!(),
        };
        let id = if let Some(app) = app {
            if app.arguments.is_empty() && matches!(ty, Type::StaticFunction(_) | Type::Function(_))
            {
                return Ok(self.normalize(ty));
            }
            let resolved = app
                .arguments
                .iter()
                .map(|t| self.resolve_inner(*t, arguments, location, path, parameters))
                .collect::<Result<Vec<_>, _>>()?;
            let application = Application {
                declaration: app.declaration,
                arguments: resolved.clone(),
            };
            self.instantiate_shape(application, location, path, parameters)?
        } else {
            let id = match ty {
                Type::StaticFunction(id) | Type::Function(id) => id,
                _ => unreachable!(),
            };
            let mut signature = self.shapes.types.borrow()[id].clone();
            for ty in signature
                .parameters
                .iter_mut()
                .chain([&mut signature.result])
            {
                *ty = self.resolve_inner(*ty, arguments, location, path, parameters)?;
            }
            self.shapes.intern(signature)
        };
        Ok(if static_type {
            Type::StaticFunction(id)
        } else {
            Type::Function(id)
        })
    }
    fn instantiate_shape(
        &self,
        application: Application,
        location: Location,
        path: &mut Vec<Application>,
        parameters: &[TypeParameter],
    ) -> Result<usize, Diagnostic> {
        let resolved = application.arguments.clone();
        self.validate_recursion(
            &application,
            location,
            &self.shapes.active.borrow(),
            parameters,
        )?;
        self.shapes.instantiate(application, |template| {
            let mut signature = template.clone();
            for ty in signature
                .parameters
                .iter_mut()
                .chain([&mut signature.result])
            {
                *ty = self.resolve_inner(*ty, &resolved, location, path, parameters)?;
            }
            if matches!(signature.result, Type::Function(_))
                || signature.parameters.contains(&Type::Void)
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.storage",
                        "function types require non-void parameters and a stored return type"
                    ),
                ));
            }
            Ok(signature)
        })
    }
    fn validate_recursion(
        &self,
        application: &Application,
        location: Location,
        path: &[Application],
        parameters: &[TypeParameter],
    ) -> Result<(), Diagnostic> {
        for previous in path
            .iter()
            .filter(|a| a.declaration == application.declaration && *a != application)
        {
            let symbolic = previous.arguments.iter().any(|t| self.unresolved(*t));
            // 記号から具体型への置換は、その具体化で判定する。定義時には型を包み続ける
            // 拡大だけを止め、固定された型への遷移や型引数の入れ替えを展開できるようにする。
            let invalid = if symbolic {
                previous
                    .arguments
                    .iter()
                    .zip(&application.arguments)
                    .any(|(old, new)| old != new && self.contains_argument(*new, *old))
            } else {
                !previous
                    .arguments
                    .iter()
                    .zip(&application.arguments)
                    .all(|(old, new)| self.contains_argument(*old, *new))
            };
            if invalid {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.recursion",
                        "recursive type instantiation changes [{}] to [{}] without shrinking",
                        previous
                            .arguments
                            .iter()
                            .map(|t| self.name_with_parameters(*t, parameters))
                            .collect::<Vec<_>>()
                            .join(", "),
                        application
                            .arguments
                            .iter()
                            .map(|t| self.name_with_parameters(*t, parameters))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
        Ok(())
    }
    fn instantiate(
        &self,
        application: Application,
        location: Location,
        path: &mut Vec<Application>,
        parameters: &[TypeParameter],
    ) -> Result<Type, Diagnostic> {
        self.validate_recursion(&application, location, path, parameters)?;
        if let Some(&id) = self.instances.borrow().get(&application) {
            return Ok(Type::Struct(id));
        }
        let template = self.get(application.declaration);
        let mut declaration = template.as_ref().clone();
        declaration.name = format!(
            "{}[{}]",
            template.name,
            application
                .arguments
                .iter()
                .map(|t| self.name(*t))
                .collect::<Vec<_>>()
                .join(", ")
        );
        declaration.symbolic = application.arguments.iter().any(|t| self.unresolved(*t));
        declaration.type_parameters.clear();
        declaration.application = Some(application.clone());
        declaration.fields.clear();
        // 具体化の入口を本文に、展開先で起きた違反を注記に残す。
        // 記号的な定義検査では、違反した定義位置をそのまま報告する。
        let report_use = path.is_empty() && !declaration.symbolic;
        let id = self.declarations.borrow().len();
        self.declarations
            .borrow_mut()
            .push(Rc::new(declaration.clone()));
        self.instances.borrow_mut().insert(application.clone(), id);
        path.push(application.clone());
        for field in &template.fields {
            let mut field = field.clone();
            field.ty = self
                .resolve_inner(
                    field.ty,
                    &application.arguments,
                    field.location,
                    path,
                    parameters,
                )
                .map_err(|error| {
                    if !report_use {
                        return error;
                    }
                    let mut diagnostic = location.error(
                        error.kind,
                        crate::diagnostic!(
                            "generic.check",
                            "generic struct '{}' is invalid for these type arguments",
                            declaration.name
                        ),
                    );
                    diagnostic.related.push(error);
                    diagnostic
                })?;
            if matches!(field.ty, Type::Function(_) | Type::Void) {
                return Err(field.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.storage",
                        "struct fields require stored value types"
                    ),
                ));
            }
            declaration.fields.push(field);
        }
        path.pop();
        self.declarations.borrow_mut()[id] = Rc::new(declaration);
        Ok(Type::Struct(id))
    }
}
