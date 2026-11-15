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

#[derive(Clone)]
pub(super) struct Scope {
    pub generic_result: Option<Type>,
    pub generic_types: [Option<Type>; 2],
    pub probing_generic: bool,
    pub missing_cast_handler: Rc<std::cell::Cell<Option<crate::ast::Location>>>,
    pub instances: Rc<std::cell::RefCell<super::generic::Instances>>,
    pub type_hints: Rc<std::cell::RefCell<BTreeMap<usize, Option<Type>>>>,
    pub float_hints: Rc<std::cell::RefCell<BTreeMap<usize, bool>>>,
    pub hint_depth: Rc<std::cell::Cell<usize>>,
    pub division_warnings: Rc<std::cell::RefCell<std::collections::BTreeSet<(usize, usize)>>>,
    pub comparison_warnings: Rc<std::cell::RefCell<BTreeMap<(usize, usize), bool>>>,
    pub constant_texts: Rc<std::cell::RefCell<std::collections::BTreeSet<String>>>,
    pub locals: BTreeMap<String, Binding>,
    pub enums: Rc<Vec<crate::ast::EnumDeclaration>>,
    pub structs: Rc<Vec<crate::ast::StructDeclaration>>,
    pub functions: Rc<BTreeMap<String, Signature>>,
    pub result: Type,
}
impl Scope {
    pub fn resolve_type(&self, ty: Type) -> Type {
        match ty {
            Type::GenericInt => self.generic_types[0].expect("declared generic int"),
            Type::GenericFloat => self.generic_types[1].expect("declared generic float"),
            _ => ty,
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
            generic_result: None,
            generic_types: [None, None],
            probing_generic: false,
            missing_cast_handler: Rc::default(),
            instances: Rc::default(),
            type_hints: Rc::default(),
            hint_depth: Rc::default(),
            float_hints: Rc::default(),
            division_warnings: Rc::default(),
            comparison_warnings: Rc::default(),
            constant_texts: Rc::default(),
            locals: BTreeMap::new(),
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
