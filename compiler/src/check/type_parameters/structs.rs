//! 汎用構造体を型引数ごとに展開する。宣言番号を安定させ、再帰参照は同じ番号へ戻す。
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
    declarations: RefCell<Vec<Rc<StructDeclaration>>>,
    applications: Vec<Application>,
    instances: RefCell<BTreeMap<Application, usize>>,
    names: BTreeSet<String>,
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
            applications: program.applications.clone(),
            enums: program.enums.clone(),
            ..Self::from(program.structs.clone())
        }
    }
    pub fn get(&self, id: usize) -> Rc<StructDeclaration> {
        self.declarations.borrow()[id].clone()
    }
    pub fn contains_name(&self, name: &str) -> bool {
        self.names.contains(name)
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
        crate::parser::structs::validate_layout_range(start, declarations.len(), |id| {
            declarations[id].as_ref()
        })
    }
    pub fn prepare(&self) -> Result<(), Diagnostic> {
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
    pub fn symbolic(&self, ty: Type) -> bool {
        match ty.leaf() {
            Type::Parameter(_) => true,
            Type::Struct(id) => self.get(id).symbolic,
            Type::Application(id) => self.applications[id]
                .arguments
                .iter()
                .any(|t| self.symbolic(*t)),
            _ => false,
        }
    }
    pub fn name(&self, ty: Type) -> String {
        if let Some(inner) = ty.pointee() {
            return format!(
                "ptr{} {}",
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
    fn name_with_parameters(&self, ty: Type, parameters: &[TypeParameter]) -> String {
        if let Some(inner) = ty.pointee() {
            return format!(
                "ptr{} {}",
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
        if let Some(inner) = old.pointee() {
            return self.contains_argument(inner, new);
        }
        if let Type::Struct(id) = old
            && let Some(app) = &self.get(id).application
        {
            return app
                .arguments
                .iter()
                .any(|t| self.contains_argument(*t, new));
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
        if !matches!(
            ty,
            Type::Parameter(_) | Type::Application(_) | Type::Struct(_) | Type::Pointer { .. }
        ) {
            return Ok(ty);
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
        let result = resolve().and_then(|ty| {
            self.validate_layout(start)?;
            Ok(ty)
        });
        if result.is_err() {
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
        if let Some(inner) = ty.pointee() {
            let inner = self.resolve_inner(inner, arguments, location, path, parameters)?;
            if matches!(inner.leaf(), Type::Function(_)) {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("arena.ptr-type", "pointer requires a stored value type"),
                ));
            }
            return Ok(inner.pointer().with_nullable(ty.is_nullable()));
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
    fn instantiate(
        &self,
        application: Application,
        location: Location,
        path: &mut Vec<Application>,
        parameters: &[TypeParameter],
    ) -> Result<Type, Diagnostic> {
        for previous in path
            .iter()
            .filter(|a| a.declaration == application.declaration && **a != application)
        {
            let symbolic = previous.arguments.iter().any(|t| self.symbolic(*t));
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
                        "recursive struct instantiation changes [{}] to [{}] without shrinking",
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
        declaration.symbolic = application.arguments.iter().any(|t| self.symbolic(*t));
        declaration.type_parameters.clear();
        declaration.application = Some(application.clone());
        declaration.fields.clear();
        let id = self.declarations.borrow().len();
        self.declarations
            .borrow_mut()
            .push(Rc::new(declaration.clone()));
        self.instances.borrow_mut().insert(application.clone(), id);
        path.push(application.clone());
        for field in &template.fields {
            let mut field = field.clone();
            field.ty = self.resolve_inner(
                field.ty,
                &application.arguments,
                field.location,
                path,
                parameters,
            )?;
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
