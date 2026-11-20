//! 汎用構造体を型引数ごとに展開する。宣言番号を安定させ、再帰参照は同じ番号へ戻す。
use crate::{
    Diagnostic, ErrorKind,
    ast::{Location, Program, StructDeclaration, Type, generics::Application},
};
use std::{cell::RefCell, rc::Rc};

/// スコープ間で共有する構造体表。登録済み宣言はRcで読み、展開中に借用を持ち越さない。
#[derive(Default)]
pub(in crate::check) struct Structs {
    declarations: RefCell<Vec<Rc<StructDeclaration>>>,
    applications: Vec<Application>,
    original_count: usize,
    enums: Vec<crate::ast::EnumDeclaration>,
}
impl From<Vec<StructDeclaration>> for Structs {
    fn from(declarations: Vec<StructDeclaration>) -> Self {
        Self {
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
    pub fn iter(&self) -> std::vec::IntoIter<Rc<StructDeclaration>> {
        self.declarations.borrow().clone().into_iter()
    }
    pub fn snapshot(&self) -> Vec<StructDeclaration> {
        self.iter().map(|d| d.as_ref().clone()).collect()
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
        crate::parser::structs::validate_layout(&self.snapshot())
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
            )
        })
    }
    pub fn resolve(
        &self,
        ty: Type,
        arguments: &[Type],
        location: Location,
    ) -> Result<Type, Diagnostic> {
        self.transaction(|| self.resolve_inner(ty, arguments, location, &mut Vec::new()))
    }
    // 型ヒントは診断を捨てて再検査することがある。失敗した展開を表に残すと、
    // 再検査が未完成の宣言を成功済みとして使うため、追加分をまとめて戻す。
    fn transaction(
        &self,
        resolve: impl FnOnce() -> Result<Type, Diagnostic>,
    ) -> Result<Type, Diagnostic> {
        let start = self.declarations.borrow().len();
        let result = resolve();
        if result.is_err() {
            self.declarations.borrow_mut().truncate(start);
        }
        result
    }
    fn resolve_inner(
        &self,
        ty: Type,
        arguments: &[Type],
        location: Location,
        path: &mut Vec<Application>,
    ) -> Result<Type, Diagnostic> {
        if let Some(inner) = ty.pointee() {
            let inner = self.resolve_inner(inner, arguments, location, path)?;
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
            .map(|t| self.resolve_inner(*t, arguments, location, path))
            .collect::<Result<Vec<_>, _>>()?;
        self.instantiate(
            Application {
                arguments,
                ..application
            },
            location,
            path,
        )
    }
    fn instantiate(
        &self,
        application: Application,
        location: Location,
        path: &mut Vec<Application>,
    ) -> Result<Type, Diagnostic> {
        for previous in path
            .iter()
            .filter(|a| a.declaration == application.declaration && **a != application)
        {
            if !previous
                .arguments
                .iter()
                .zip(&application.arguments)
                .all(|(old, new)| self.contains_argument(*old, *new))
            {
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "generic.recursion",
                        "recursive struct instantiation changes [{}] to [{}] without shrinking",
                        previous
                            .arguments
                            .iter()
                            .map(|t| self.name(*t))
                            .collect::<Vec<_>>()
                            .join(", "),
                        application
                            .arguments
                            .iter()
                            .map(|t| self.name(*t))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
        if let Some(id) = self
            .iter()
            .position(|d| d.application.as_ref() == Some(&application))
        {
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
        path.push(application.clone());
        for field in &template.fields {
            let mut field = field.clone();
            field.ty =
                self.resolve_inner(field.ty, &application.arguments, field.location, path)?;
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
        if path.is_empty() {
            crate::parser::structs::validate_layout(&self.snapshot())?;
        }
        Ok(Type::Struct(id))
    }
}
