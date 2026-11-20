//! 型引数の宣言と、定義時検査に使う操作の要求。
use super::{ErrorVariant, Location, Type};

/// 宣言内で有効な型引数の名前と位置。型には宣言内の添字を保存する。
#[derive(Clone)]
pub(crate) struct TypeParameter {
    pub name: String,
    pub location: Location,
}

/// 要求する演算子、または読み取り・書き換えとArenaの区別を持つメソッド。
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum RequiredOperation {
    Operator(&'static str),
    Method {
        name: String,
        mutable: bool,
        arena: bool,
    },
}

/// 型引数が満たすべき操作の署名。対象の型引数は宣言内の添字で示す。
#[derive(Clone)]
pub(crate) struct Need {
    pub parameter: usize,
    pub operation: RequiredOperation,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub errors: Vec<ErrorVariant>,
    pub location: Location,
}

impl Need {
    /// 演算子は右辺の型ごとに定義できるが、通常メソッドは名前による多重定義を持たない。
    pub fn same_operation(&self, other: &Self) -> bool {
        if self.parameter != other.parameter {
            return false;
        }
        match (&self.operation, &other.operation) {
            (RequiredOperation::Operator(a), RequiredOperation::Operator(b)) => {
                a == b && self.parameters == other.parameters
            }
            (
                RequiredOperation::Method { name: a, .. },
                RequiredOperation::Method { name: b, .. },
            ) => a == b,
            _ => false,
        }
    }
    /// 重複した要求をまとめるときに、操作の修飾・引数・結果・エラーを照合する。
    pub fn same_signature(&self, other: &Self) -> bool {
        self.operation == other.operation
            && self.parameters == other.parameters
            && self.result == other.result
            && self.errors == other.errors
    }
}

/// 構造体またはfnshapeへの型引数の適用。宣言の種類は参照元のTypeで区別する。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Application {
    pub declaration: usize,
    pub arguments: Vec<Type>,
}

/// 構文解析中の型適用表。fnshapeを多数使っても既存の適用を全件走査しない。
#[derive(Default)]
pub(crate) struct ApplicationTable {
    pub entries: Vec<Application>,
    ids: std::collections::BTreeMap<Application, usize>,
}
impl ApplicationTable {
    pub fn intern(&mut self, application: Application) -> usize {
        if let Some(&id) = self.ids.get(&application) {
            return id;
        }
        let id = self.entries.len();
        self.entries.push(application.clone());
        self.ids.insert(application, id);
        id
    }
}
pub(crate) type Applications = std::rc::Rc<std::cell::RefCell<ApplicationTable>>;

/// 名前付き要求一覧。展開後の直接要求だけを意味検査へ渡す。
pub(crate) struct NeedGroup {
    pub name: String,
    pub location: Location,
    pub type_parameters: Vec<TypeParameter>,
    pub needs: Vec<Need>,
}
