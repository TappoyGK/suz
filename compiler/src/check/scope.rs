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
    pub locals: BTreeMap<String, Binding>,
    pub enums: Rc<Vec<crate::ast::EnumDeclaration>>,
    pub structs: Rc<Vec<crate::ast::StructDeclaration>>,
    pub functions: Rc<BTreeMap<String, Signature>>,
    pub result: Type,
}
impl Scope {
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
