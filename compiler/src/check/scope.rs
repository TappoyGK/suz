use super::statements::Binding;
use crate::ast::{IntegerType, Type};
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    rc::Rc,
};

#[derive(Clone)]
pub(super) struct Signature {
    pub id: usize,
    pub parameters: Vec<Type>,
    pub result: Type,
}

impl Signature {
    pub fn declares(&self, family: super::generic::Family) -> bool {
        self.parameters
            .iter()
            .chain([&self.result])
            .any(|ty| super::generic::Family::of(*ty) == Some(family))
    }
    pub fn is_generic(&self) -> bool {
        self.result.generic() || self.parameters.iter().any(|t| t.generic())
    }
}

#[derive(Clone, Default)]
pub(super) struct TemplateAnalysis {
    pub errors: Rc<std::cell::RefCell<Vec<crate::Diagnostic>>>,
    pub casts:
        Rc<std::cell::RefCell<Vec<(crate::ast::Location, super::expression::casts::CastRisk)>>>,
}

#[derive(Clone)]
pub(super) struct Scope {
    pub generic_types: super::generic::GenericTypes,
    pub generic_calls: Option<Rc<super::generic::CallTrace>>,
    pub analyzing_generic: bool,
    // Template observations are shared by nested scopes, isolated by speculation.
    pub analysis: TemplateAnalysis,
    pub instances: Rc<std::cell::RefCell<super::generic::Instances>>,
    // Expression-pointer caches live for one hint session and one substitution.
    pub type_hints: Rc<std::cell::RefCell<BTreeMap<usize, Option<Type>>>>,
    pub float_hints: Rc<std::cell::RefCell<BTreeMap<usize, bool>>>,
    pub hint_depth: Rc<std::cell::Cell<usize>>,
    // Collected outputs for diagnostics and constant registration.
    pub division_warnings: Rc<std::cell::RefCell<std::collections::BTreeSet<(usize, usize)>>>,
    pub comparison_warnings: Rc<std::cell::RefCell<BTreeMap<(usize, usize), bool>>>,
    pub constant_texts: Rc<std::cell::RefCell<std::collections::BTreeSet<String>>>,
    pub locals: BTreeMap<String, Binding>,
    // Immutable declarations are shared; only local binding state is copied.
    pub constants: Rc<BTreeMap<String, Binding>>,
    pub enums: Rc<Vec<crate::ast::EnumDeclaration>>,
    pub structs: Rc<Vec<crate::ast::StructDeclaration>>,
    pub functions: Rc<BTreeMap<String, Signature>>,
    pub result: Type,
}
impl Scope {
    pub fn get(&self, name: &str) -> Option<&Binding> {
        self.locals.get(name).or_else(|| self.constants.get(name))
    }
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
    pub fn resolve_type(&self, ty: Type) -> Type {
        self.generic_types.resolve(ty)
    }
    pub fn in_generic(&self) -> bool {
        !self.generic_types.is_empty()
    }
    // Isolate outputs while preserving caches for the same type substitution.
    pub fn speculative(&self) -> Self {
        Self {
            analysis: Default::default(),
            instances: Default::default(),
            division_warnings: Default::default(),
            comparison_warnings: Default::default(),
            constant_texts: Default::default(),
            ..self.clone()
        }
    }

    pub fn type_name(&self, ty: Type) -> &str {
        match ty {
            Type::Enum(ty) => &self.enums[ty.id].name,
            Type::Struct(id) => &self.structs[id].name,
            _ => ty.name(),
        }
    }
    pub fn is_type_name(&self, name: &str) -> bool {
        self.enums.iter().any(|decl| decl.name == name)
            || self.structs.iter().any(|decl| decl.name == name)
    }
}
impl Default for Scope {
    fn default() -> Self {
        Self {
            generic_types: Default::default(),
            generic_calls: None,
            analyzing_generic: false,
            analysis: Default::default(),
            instances: Rc::default(),
            type_hints: Rc::default(),
            hint_depth: Rc::default(),
            float_hints: Rc::default(),
            division_warnings: Rc::default(),
            comparison_warnings: Rc::default(),
            constant_texts: Rc::default(),
            locals: BTreeMap::new(),
            constants: Rc::default(),
            enums: Rc::default(),
            structs: Rc::default(),
            functions: Rc::default(),
            result: Type::Integer(IntegerType::U8),
        }
    }
}
impl Deref for Scope {
    type Target = BTreeMap<String, Binding>;
    fn deref(&self) -> &Self::Target {
        &self.locals
    }
}
impl DerefMut for Scope {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.locals
    }
}

impl std::ops::Index<&str> for Scope {
    type Output = Binding;
    fn index(&self, name: &str) -> &Binding {
        self.get(name).expect("checked binding name")
    }
}
