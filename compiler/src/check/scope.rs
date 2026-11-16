use super::statements::Binding;
use crate::ast::{IntegerType, Type};
use std::{collections::BTreeMap, rc::Rc};

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

// Diagnostics and runtime constants share their destinations across lexical scopes.
#[derive(Clone, Default)]
pub(super) struct Outputs {
    pub divisions: Rc<std::cell::RefCell<std::collections::BTreeSet<(usize, usize)>>>,
    pub comparisons: Rc<std::cell::RefCell<BTreeMap<(usize, usize), bool>>>,
    pub constant_texts: Rc<std::cell::RefCell<std::collections::BTreeSet<String>>>,
}

#[derive(Clone, Copy, Default)]
pub(super) enum Mode {
    #[default]
    Ordinary,
    GenericCheck,
    Instance(super::generic::GenericTypes),
}

#[derive(Clone)]
pub(super) struct Scope {
    pub mode: Mode,
    pub deferred: Option<Rc<std::cell::RefCell<Vec<super::generic::Deferred>>>>,
    // Persistent path from the initiating call to this instance.
    pub generic_calls: Option<Rc<super::generic::CallTrace>>,
    pub instances: Rc<std::cell::RefCell<super::generic::Instances>>,
    // Expression-pointer caches live for one hint session and one substitution.
    pub type_hints: Rc<std::cell::RefCell<BTreeMap<usize, Option<Type>>>>,
    pub float_hints: Rc<std::cell::RefCell<BTreeMap<usize, bool>>>,
    pub hint_depth: Rc<std::cell::Cell<usize>>,
    pub outputs: Outputs,
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
        match self.mode {
            Mode::Instance(types) => types.resolve(ty),
            Mode::Ordinary | Mode::GenericCheck => ty,
        }
    }
    pub fn generic_check(&self) -> bool {
        matches!(self.mode, Mode::GenericCheck)
    }
    pub fn instance(&self) -> bool {
        matches!(self.mode, Mode::Instance(_))
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
            mode: Default::default(),
            deferred: None,
            generic_calls: None,
            instances: Rc::default(),
            type_hints: Rc::default(),
            hint_depth: Rc::default(),
            float_hints: Rc::default(),
            outputs: Default::default(),
            locals: BTreeMap::new(),
            constants: Rc::default(),
            enums: Rc::default(),
            structs: Rc::default(),
            functions: Rc::default(),
            result: Type::Integer(IntegerType::U8),
        }
    }
}
impl std::ops::Index<&str> for Scope {
    type Output = Binding;
    fn index(&self, name: &str) -> &Binding {
        self.get(name).expect("checked binding name")
    }
}
