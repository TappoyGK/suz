//! 型引数の宣言と、定義時検査に使う操作の要求。
use super::{ErrorVariant, Location, Type};

/// 宣言内で有効な型引数の名前と位置。型には宣言内の添字を保存する。
#[derive(Clone)]
pub(crate) struct TypeParameter {
    pub name: String,
    pub location: Location,
}

/// 要求する演算子、または読み取り・書き換えとArenaの区別を持つメソッド。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
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
    /// 一覧を展開しても、直接要求を書いた行を診断に残す。
    pub definition: Location,
}

impl Need {
    /// 演算子は右辺型も照合する。通常メソッドは名前だけで重複を判定する。
    pub fn operation_key(&self) -> (bool, &str, &[Type]) {
        match &self.operation {
            RequiredOperation::Operator(op) => (false, op, &self.parameters),
            RequiredOperation::Method { name, .. } => (true, name, &[]),
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

/// 少数の要求は直接比較し、表の確保とキーの複製を省く上限。
pub(crate) const SMALL_NEEDS: usize = 8;

/// 構文解析時の重複除去。出現順と最初の定義位置を保ち、署名は表で照合する。
#[derive(Default)]
pub(crate) struct NeedList {
    items: Vec<Need>,
    signatures: std::collections::BTreeSet<NeedSignature>,
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct NeedSignature {
    parameter: usize,
    operation: RequiredOperation,
    parameters: Vec<Type>,
    result: Type,
    errors: Vec<ErrorVariant>,
}
impl From<&Need> for NeedSignature {
    fn from(need: &Need) -> Self {
        Self {
            parameter: need.parameter,
            operation: need.operation.clone(),
            parameters: need.parameters.clone(),
            result: need.result,
            errors: need.errors.clone(),
        }
    }
}
impl NeedList {
    pub fn insert(&mut self, need: Need) {
        if self.items.len() < SMALL_NEEDS {
            if !self
                .items
                .iter()
                .any(|n| n.parameter == need.parameter && n.same_signature(&need))
            {
                self.items.push(need);
            }
            return;
        }
        if self.signatures.is_empty() {
            self.signatures
                .extend(self.items.iter().map(NeedSignature::from));
        }
        let key = NeedSignature::from(&need);
        if self.signatures.insert(key) {
            self.items.push(need);
        }
    }
    pub fn into_vec(self) -> Vec<Need> {
        self.items
    }
}
