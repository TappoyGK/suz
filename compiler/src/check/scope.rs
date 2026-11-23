//! 名前から束縛・定数・関数宣言を引くスコープを保持する。汎用検査の状態と診断の出力先も共有する。
//!
//! 利用元: [`crate::check::functions`]、[`crate::check::statements`]、[`crate::check::expression`]。

use super::statements::Binding;
use crate::ast::{IntegerType, Type};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone)]
pub(super) struct Signature {
    pub type_parameters: usize,
    // 診断用の名前は汎用宣言だけに保持し、署名を複製するときも共有する。
    pub type_parameter_names: Option<Rc<Vec<String>>>,
    pub needs: Vec<crate::ast::generics::Need>,
    pub method: Option<crate::ast::Method>,
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

/// 要求は宣言順に保持し、操作を検索するときだけ対象の型引数と名前で候補を絞る。
#[derive(Default)]
pub(super) struct Requirements {
    items: Vec<crate::ast::generics::Need>,
    operations: BTreeMap<(usize, bool), BTreeMap<String, Vec<usize>>>,
}
impl From<Vec<crate::ast::generics::Need>> for Requirements {
    fn from(items: Vec<crate::ast::generics::Need>) -> Self {
        // 少数の要求はそのまま照合できる。小さな汎用関数に索引の確保を課さない。
        if items.len() <= crate::ast::generics::SMALL_NEEDS {
            return Self {
                items,
                ..Self::default()
            };
        }
        let mut operations: BTreeMap<_, BTreeMap<_, Vec<_>>> = BTreeMap::new();
        for (id, need) in items.iter().enumerate() {
            let (method, name, _) = need.operation_key();
            operations
                .entry((need.parameter, method))
                .or_default()
                .entry(name.to_owned())
                .or_default()
                .push(id);
        }
        Self { items, operations }
    }
}
impl std::ops::Deref for Requirements {
    type Target = [crate::ast::generics::Need];
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}
impl Requirements {
    pub fn candidates(
        &self,
        parameter: usize,
        method: bool,
        name: &str,
    ) -> impl Iterator<Item = &crate::ast::generics::Need> {
        let small = self
            .items
            .iter()
            .take(if self.operations.is_empty() {
                self.items.len()
            } else {
                0
            })
            .filter(move |need| {
                let (kind, operation, _) = need.operation_key();
                need.parameter == parameter && kind == method && operation == name
            });
        self.operations
            .get(&(parameter, method))
            .and_then(|names| names.get(name))
            .into_iter()
            .flatten()
            .map(|&id| &self.items[id])
            .chain(small)
    }
}

/// 名前付き型引数の検査状態。通常のスコープ複製では一つの参照だけを引き継ぐ。
#[derive(Clone, Default)]
pub(super) struct NamedTypes {
    /// 名前付き型引数の具体化。汎用定義の検査ではNoneとし、type_parametersとneedsを使う。
    pub type_arguments: Option<Rc<Vec<Type>>>,
    pub type_parameters: Rc<Vec<crate::ast::generics::TypeParameter>>,
    pub needs: Rc<Requirements>,
    pub instantiation_path: Rc<Vec<(usize, Vec<Type>)>>,
}

#[derive(Clone)]
pub(super) struct Scope {
    pub array_bounds: super::arrays::bounds::State,
    pub named: Rc<NamedTypes>,

    // 定義のないプログラムでは、演算子用の型ヒント取得を省く。
    pub has_operator_methods: bool,
    // 定数初期値では、演算子構文による関数呼び出しも拒否する。
    pub constant_initializer: bool,
    pub arena_depth: usize,
    pub lifetime_used: Rc<std::cell::Cell<bool>>,
    // 本体にポインタがなくても、呼び出し先の一時確保を蓄積警告へ伝える。
    pub calls_used: Rc<std::cell::Cell<bool>>,
    // ループがないプログラムでは、寿命検査だけを行い蓄積警告の情報を作らない。
    pub program_loops_used: Rc<std::cell::Cell<bool>>,
    pub program_references_used: Rc<std::cell::Cell<bool>>,
    pub callables_used: Rc<std::cell::Cell<bool>>,
    pub trusted_block: bool,
    // 最も内側のerror腕の成功型。関数の戻り値型とは独立し、クロージャ境界で解除する。
    pub recovery: Option<Type>,
    // 式のerror腕からbreak・continueできるかを調べるため、文のループ深さを渡す。
    pub loop_depth: usize,
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
    pub enums: Rc<Vec<crate::ast::EnumDeclaration>>,
    pub structs: Rc<super::type_parameters::structs::Structs>,
    pub functions: Rc<BTreeMap<String, Signature>>,
    pub result: Type,
    pub errors: Rc<Vec<crate::ast::ErrorVariant>>,
}
impl Scope {
    pub fn contains_function(&self, ty: Type) -> bool {
        match ty {
            Type::StaticFunction(_) | Type::Function(_) => true,
            Type::Struct(id) => self
                .structs
                .get(id)
                .fields
                .iter()
                .any(|f| self.contains_function(f.ty)),
            _ => false,
        }
    }
    pub fn intern_function(&self, signature: crate::ast::FunctionType) -> Type {
        Type::StaticFunction(self.structs.shapes.intern(signature))
    }

    pub fn get(&self, name: &str) -> Option<&Binding> {
        self.locals.get(name).or_else(|| self.constants.get(name))
    }
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
    pub fn substitute(
        &self,
        ty: Type,
        arguments: &[Type],
        location: crate::ast::Location,
    ) -> Result<Type, crate::Diagnostic> {
        self.structs
            .resolve_with_parameters(ty, arguments, location, &self.named.type_parameters)
    }
    pub fn method_arguments(&self, ty: Type) -> Vec<Type> {
        if let Type::Struct(id) = ty.pointee().unwrap_or(ty)
            && let Some(app) = &self.structs.get(id).application
        {
            return app.arguments.clone();
        }
        Vec::new()
    }
    pub fn method_owner_name(&self, ty: Type) -> String {
        if let Type::Struct(id) = ty.pointee().unwrap_or(ty)
            && let Some(app) = &self.structs.get(id).application
        {
            return self.structs.get(app.declaration).name.clone();
        }
        self.type_name(ty.pointee().unwrap_or(ty))
    }
    #[inline]
    pub fn resolve_type_at(
        &self,
        ty: Type,
        location: crate::ast::Location,
    ) -> Result<Type, crate::Diagnostic> {
        if !matches!(
            ty,
            Type::Parameter(_)
                | Type::Application(_)
                | Type::Struct(_)
                | Type::Pointer { .. }
                | Type::ArrayRef { .. }
                | Type::StaticShape(_)
                | Type::Shape(_)
                | Type::StaticFunction(_)
                | Type::Function(_)
        ) {
            return Ok(match self.mode {
                Mode::Instance(types) => types.resolve(ty),
                Mode::Ordinary | Mode::GenericCheck => ty,
            });
        }
        if let Some(ty) = self.structs.plain_shape(ty) {
            return Ok(ty);
        }
        if self.structs.shapes.has_generics {
            super::type_parameters::shape_requirements(ty, self, location)?;
        }
        let ty = self.substitute(
            ty,
            self.named
                .type_arguments
                .as_deref()
                .map_or(&[], Vec::as_slice),
            location,
        )?;
        let ty = match self.mode {
            Mode::Instance(types) => types.resolve(ty),
            Mode::Ordinary | Mode::GenericCheck => ty,
        };
        super::type_parameters::struct_requirements(ty, self, location)?;
        Ok(ty)
    }
    pub fn generic_check(&self) -> bool {
        matches!(self.mode, Mode::GenericCheck)
    }
    pub fn instance(&self) -> bool {
        matches!(self.mode, Mode::Instance(_))
    }

    pub fn type_name(&self, ty: Type) -> String {
        match ty {
            Type::Array(id) => {
                let array = self.structs.arrays.borrow().get(id);
                let element = self.type_name(array.element);
                if array.rank == 1 {
                    format!("[{element}]")
                } else {
                    format!("[{element}; {}]", vec!["_"; array.rank].join(", "))
                }
            }
            Type::ArrayRef {
                id,
                mutable,
                nullable,
            } => {
                let array = self.structs.arrays.borrow().get(id);
                let element = self.type_name(array.element);
                let prefix = if mutable { "#" } else { "." };
                let suffix = if nullable { "?" } else { "" };
                if array.rank == 1 {
                    format!("{prefix}[{element}]{suffix}")
                } else {
                    format!(
                        "{prefix}[{element}; {}]{suffix}",
                        vec!["_"; array.rank].join(", ")
                    )
                }
            }
            Type::Parameter(id) => self
                .named
                .type_parameters
                .get(id)
                .map_or_else(|| "type parameter".into(), |p| p.name.clone()),
            Type::Pointer { .. } => format!(
                "{}{} {}",
                if ty.is_readonly_pointer() {
                    "ref"
                } else {
                    "mut"
                },
                if ty.is_nullable() { "?" } else { "" },
                self.type_name(ty.pointee().unwrap())
            ),
            Type::Enum(ty) => self.enums[ty.id].name.clone(),
            Type::Struct(id) => {
                let declaration = self.structs.get(id);
                if declaration.symbolic {
                    self.structs
                        .name_with_parameters(ty, &self.named.type_parameters)
                } else {
                    declaration.name.clone()
                }
            }
            Type::StaticFunction(id) | Type::Function(id) => {
                let prefix = if matches!(ty, Type::StaticFunction(_)) {
                    "static fn"
                } else {
                    "fn"
                };
                if let Some(app) = self.structs.shapes.origin(id) {
                    let declaration = &self.structs.shapes.declarations[app.declaration];
                    let arguments = if app.arguments.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "[{}]",
                            app.arguments
                                .iter()
                                .map(|ty| self.type_name(*ty))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    return format!("{prefix} {}{arguments}", declaration.name);
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
        self.structs.shapes.contains_name(name)
            || self.enums.iter().any(|decl| decl.name == name)
            || self.structs.contains_name(name)
    }
}
impl Default for Scope {
    fn default() -> Self {
        let structs = Rc::new(super::type_parameters::structs::Structs::default());
        Self {
            named: Rc::default(),
            has_operator_methods: false,
            constant_initializer: false,
            arena_depth: 0,
            lifetime_used: Default::default(),
            calls_used: Default::default(),
            program_loops_used: Default::default(),
            program_references_used: Default::default(),
            callables_used: Default::default(),
            trusted_block: false,
            recovery: None,
            loop_depth: 0,
            function_types: structs.shapes.types.clone(),
            mode: Default::default(),
            deferred: None,
            generic_calls: None,
            instances: Rc::default(),
            array_bounds: Default::default(),
            type_hints: Rc::default(),
            hint_depth: Rc::default(),
            float_hints: Rc::default(),
            outputs: Default::default(),
            locals: BTreeMap::new(),
            value_captures: Default::default(),
            uncaptured: Default::default(),
            constants: Rc::default(),
            enums: Rc::default(),
            structs,
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
