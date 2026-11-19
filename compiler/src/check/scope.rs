//! 名前から束縛・定数・関数宣言を引くスコープを保持する。汎用検査の状態と診断の出力先も共有する。
//!
//! 利用元: [`crate::check::functions`]、[`crate::check::statements`]、[`crate::check::expression`]。

use super::statements::Binding;
use crate::ast::{IntegerType, Type};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone)]
pub(super) struct Signature {
    pub arena: bool,
    pub external: bool,
    pub id: usize,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub errors: Vec<crate::ast::ErrorVariant>,
}

impl Signature {
    pub fn declares(&self, family: super::generic::Family) -> bool {
        self.parameters
            .iter()
            .chain([&self.result])
            .any(|ty| super::generic::Family::of(ty.leaf()) == Some(family))
    }
    pub fn is_generic(&self) -> bool {
        self.result.leaf().generic() || self.parameters.iter().any(|t| t.leaf().generic())
    }
}

// 診断とランタイム定数の出力先を、字句スコープ間で共有する。
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
    pub arena_depth: usize,
    pub arena_used: Rc<std::cell::Cell<bool>>,
    pub nonnull: std::collections::BTreeSet<usize>,
    pub unsafe_block: bool,
    // 最も内側のerror腕の成功型。関数の戻り値型とは独立し、クロージャ境界で解除する。
    pub recovery: Option<Type>,
    // 式のerror腕からbreak・continueできるかを調べるため、文のループ深さを渡す。
    pub loop_depth: usize,
    pub fnshapes: Rc<Vec<crate::ast::FnShapeDeclaration>>,
    pub function_types: Rc<std::cell::RefCell<Vec<crate::ast::FunctionType>>>,
    pub mode: Mode,
    pub deferred: Option<Rc<std::cell::RefCell<Vec<super::generic::Deferred>>>>,
    // 起点の呼び出しから、この具体化までの共有可能な経路。
    pub generic_calls: Option<Rc<super::generic::CallTrace>>,
    pub instances: Rc<std::cell::RefCell<super::generic::Instances>>,
    // 式のポインターを鍵とするキャッシュは、一回のヒント取得と一つの型代入の間だけ保持する。
    pub type_hints: Rc<std::cell::RefCell<BTreeMap<usize, Option<Type>>>>,
    pub float_hints: Rc<std::cell::RefCell<BTreeMap<usize, bool>>>,
    pub hint_depth: Rc<std::cell::Cell<usize>>,
    pub outputs: Outputs,
    pub locals: BTreeMap<String, Binding>,
    pub value_captures: std::collections::BTreeSet<String>,
    pub uncaptured: std::collections::BTreeSet<String>,
    // 不変の宣言は共有し、ローカル束縛の状態だけを複製する。
    pub constants: Rc<BTreeMap<String, Binding>>,
    // 大域変数の型・格納先と代入済みフラグを、全関数の検査で共有する。
    pub globals: Rc<BTreeMap<String, Binding>>,
    pub enums: Rc<Vec<crate::ast::EnumDeclaration>>,
    pub structs: Rc<Vec<crate::ast::StructDeclaration>>,
    pub functions: Rc<BTreeMap<String, Signature>>,
    pub result: Type,
    pub errors: Rc<Vec<crate::ast::ErrorVariant>>,
}
impl Scope {
    pub fn contains_function(&self, ty: Type) -> bool {
        match ty {
            Type::StaticFunction(_) | Type::Function(_) => true,
            Type::Struct(id) => self.structs[id]
                .fields
                .iter()
                .any(|f| self.contains_function(f.ty)),
            _ => false,
        }
    }
    pub fn intern_function(&self, signature: crate::ast::FunctionType) -> Type {
        let mut types = self.function_types.borrow_mut();
        let id = types
            .iter()
            .position(|t| t == &signature)
            .unwrap_or_else(|| {
                types.push(signature);
                types.len() - 1
            });
        Type::StaticFunction(id)
    }
    pub fn get(&self, name: &str) -> Option<&Binding> {
        self.locals
            .get(name)
            .or_else(|| self.constants.get(name))
            .or_else(|| self.globals.get(name))
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

    pub fn type_name(&self, ty: Type) -> String {
        match ty {
            Type::Pointer { .. } => format!("ptr {}", self.type_name(ty.pointee().unwrap())),
            Type::Enum(ty) => self.enums[ty.id].name.clone(),
            Type::Struct(id) => self.structs[id].name.clone(),
            Type::StaticFunction(id) | Type::Function(id) => {
                let prefix = if matches!(ty, Type::StaticFunction(_)) {
                    "static fn"
                } else {
                    "fn"
                };
                if let Some(declaration) = self.fnshapes.iter().find(|d| d.type_id == id) {
                    return format!("{prefix} {}", declaration.name);
                }
                // 名前のない推論型では、違いが分かるよう仮引数と戻り値を表示する。
                let types = self.function_types.borrow();
                let signature = &types[id];
                let parameters = signature
                    .parameters
                    .iter()
                    .map(|&ty| self.type_name(ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{prefix}({parameters}) -> {}",
                    self.type_name(signature.result)
                )
            }
            _ => ty.name().into(),
        }
    }
    pub fn is_type_name(&self, name: &str) -> bool {
        self.fnshapes.iter().any(|d| d.name == name)
            || self.enums.iter().any(|decl| decl.name == name)
            || self.structs.iter().any(|decl| decl.name == name)
    }
}
impl Default for Scope {
    fn default() -> Self {
        Self {
            arena_depth: 0,
            arena_used: Default::default(),
            nonnull: Default::default(),
            unsafe_block: false,
            recovery: None,
            loop_depth: 0,
            fnshapes: Rc::default(),
            function_types: Rc::default(),
            mode: Default::default(),
            deferred: None,
            generic_calls: None,
            instances: Rc::default(),
            type_hints: Rc::default(),
            hint_depth: Rc::default(),
            float_hints: Rc::default(),
            outputs: Default::default(),
            locals: BTreeMap::new(),
            value_captures: Default::default(),
            uncaptured: Default::default(),
            constants: Rc::default(),
            globals: Rc::default(),
            enums: Rc::default(),
            structs: Rc::default(),
            functions: Rc::default(),
            result: Type::Integer(IntegerType::U8),
            errors: Rc::default(),
        }
    }
}
impl std::ops::Index<&str> for Scope {
    type Output = Binding;
    fn index(&self, name: &str) -> &Binding {
        self.get(name).expect("checked binding name")
    }
}
