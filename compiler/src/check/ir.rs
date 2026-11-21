//! 意味検査で得たIRを定義する。ASTの名前を番号へ解決し、式の型・値域・失敗時の処理を保持してC生成へ渡す。
//!
//! 利用元: [`crate::check`]、[`crate::codegen`]。

use crate::ast::{BinaryOperator, ComparisonOperator, Location, LogicalOperator, Type};

/// [`crate::ast::Program`] の意味検査結果。
/// 型・名前を解決した関数、入口の関数番号、文字列定数と診断候補を保持する。
/// 型宣言はASTの表現を共有し、関数は必要な汎用型の具体化も含めてC生成へ渡す。
pub(crate) struct CheckedProgram {
    pub arrays: crate::ast::arrays::Types,
    pub external_base: usize,
    pub externs: Vec<crate::ast::ExternFunction>,
    pub function_types: Vec<crate::ast::FunctionType>,
    /// 整数除算を含む浮動小数点演算の警告候補。
    pub division_warnings: Vec<Location>,
    /// 反復の外へ残らず、外側のArenaへ蓄積する確保。
    pub arena_warnings: Vec<(Location, String, Vec<crate::Diagnostic>)>,
    /// 定数として決まる比較の位置と結果。
    pub comparison_warnings: Vec<(Location, bool)>,
    /// 実行開始時に登録する文字列定数。
    pub constant_texts: std::collections::BTreeSet<String>,
    /// 起動時の初期化失敗を報告するmainの位置。
    pub main_location: Location,
    pub structs: Vec<crate::ast::StructDeclaration>,
    pub enums: Vec<crate::ast::EnumDeclaration>,
    pub functions: Vec<CheckedFunction>,
    /// Cの入口から呼び出す関数の番号。
    pub main_id: usize,
    /// 明示的な失敗処理のない整数演算の位置。警告設定の適用に使う。
    pub unhandled_arithmetic: Vec<Location>,
}
/// [`crate::ast::Function`] を検査・具体化した関数。
/// 名前を関数番号へ置き換え、仮引数と戻り値の型、本体のIRを保持する。
#[derive(Clone)]
pub(crate) struct CheckedFunction {
    /// 関数群がそろうまで保持する寿命制約。呼び出し要約の検査後に取り除く。
    pub lifetime: Option<super::arena::LifetimeBody>,
    pub mutable_receiver: bool,
    pub id: usize,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub fallible: bool,
    pub statements: Vec<CheckedStatement>,
}
/// [`crate::ast::Statement`] の検査結果。
/// 束縛と参照を番号へ解決し、束縛・代入の型を確定する。
/// 式内の失敗時returnは専用の文へ移し、C生成で制御を組み立てられるようにする。
/// メモリ効率を高めるため、大きな列挙子の式をBoxに置き、全ての文の格納サイズを抑える。
/// 失敗時returnの式は、検査中に作ったBoxをそのまま使う。
#[derive(Clone)]
pub(crate) enum CheckedStatement {
    Arena {
        body: Vec<CheckedStatement>,
        end: Location,
    },
    Store {
        target: Box<CheckedExpression>,
        id: usize,
        value: Box<CheckedExpression>,
        body: Vec<CheckedStatement>,
        location: Location,
    },

    Ignore(Box<crate::ast::Ignore>),
    Recover(CheckedExpression),
    Fail(CheckedExpression),
    HandleCall {
        id: usize,
        call: Box<CheckedExpression>,
        handler: Box<CallHandler>,
    },
    SliceReturn {
        location: Location,
        id: usize,
        operation: CheckedExpression,
        return_value: Box<CheckedExpression>,
    },
    Log {
        level: crate::ast::LogLevel,
        parts: Vec<crate::ast::TemplatePart<CheckedExpression>>,
        location: Location,
    },
    Expression(CheckedExpression),
    Match {
        id: usize,
        target: CheckedExpression,
        arms: Vec<(Option<CheckedExpression>, Vec<CheckedStatement>)>,
        pattern_warnings: Vec<Location>,
    },
    For {
        id: usize,
        start: CheckedExpression,
        end: Option<Box<CheckedExpression>>,
        inclusive: bool,
        reverse: bool,
        body: Vec<CheckedStatement>,
    },
    ArithmeticReturn {
        location: Location,
        id: usize,
        operation: CheckedExpression,
        return_value: Box<CheckedExpression>,
    },
    CastReturn {
        return_location: Location,
        location: Location,
        wrap: bool,
        id: usize,
        ty: Type,
        operand: Box<CheckedExpression>,
        return_value: Box<CheckedExpression>,
    },
    Loop {
        condition: Option<CheckedExpression>,
        body: Vec<CheckedStatement>,
    },
    Break,
    Continue,
    If {
        branches: Vec<(CheckedExpression, Vec<CheckedStatement>)>,
        otherwise: Option<Vec<CheckedStatement>>,
    },
    Block(Vec<CheckedStatement>),
    Objection(Vec<CheckedStatement>),
    Binding {
        location: Location,
        mutable: bool,
        id: usize,
        ty: Type,
        // 型とソース位置の追加で全ての文が肥大化しないよう、初期値を別に保持する。
        initializer: Box<CheckedExpression>,
    },
    Assign {
        location: Location,
        // フィールド経路の各段で、通常の構造体かC構造体かを判定するための元変数の型。
        root_type: Type,
        fields: Vec<usize>,
        id: usize,
        ty: Type,
        value: Box<CheckedExpression>,
    },
    Return {
        value: CheckedExpression,
        location: Location,
    },
}
/// [`crate::ast::Expression`] の検査結果。
/// 推論・検査した型と、参照先を解決した [`Value`] を対にする。
/// 汎用検査中は型が記号的な場合もあり、C生成に渡す前に具体化する。
#[derive(Clone)]
pub(crate) struct CheckedExpression {
    pub ty: Type,
    pub value: Value,
}
/// [`crate::ast::FailureHandler`] の検査結果。代替値または戻り値を型付きの式として保持する。
#[derive(Clone)]
pub(crate) enum CheckedHandler {
    Value(Box<CheckedExpression>),
    Return {
        value: Box<CheckedExpression>,
        location: Location,
    },
}
/// 呼び出しの失敗処理。脱出するものはC生成前に文へ持ち上げる。
#[derive(Clone)]
pub(crate) enum CallHandler {
    Branches(Vec<(Option<crate::ast::ErrorVariant>, Vec<CheckedStatement>)>),
    Value(CheckedExpression),
    Return {
        value: CheckedExpression,
        location: Location,
    },
    Fail(Option<CheckedExpression>),
    Ignore(Box<crate::ast::Ignore>),
}

/// クロージャ本体と、評価位置で順に保持する値・参照。
/// functionの仮引数列の末尾にキャプチャーを並べ、本体で使うローカル番号を割り当てる。
#[derive(Clone)]
pub(crate) struct CheckedClosure {
    pub function: CheckedFunction,
    pub captures: Vec<CheckedCapture>,
}

/// 環境へ保持する値。参照キャプチャーではvalueが外側のローカル変数を指す。
#[derive(Clone)]
pub(crate) struct CheckedCapture {
    pub value: CheckedExpression,
    pub by_reference: bool,
}

/// [`crate::ast::ExpressionKind`] の整数演算を検査した表現。
/// 正確な整数式、型付きの二項演算・符号反転、範囲を確認済みの負のリテラルを区別する。
#[derive(Clone)]
pub(crate) enum IntegerOperation {
    Exact(Box<super::exact::Exact>),
    Binary {
        operator: BinaryOperator,
        left: Box<CheckedExpression>,
        right: Box<CheckedExpression>,
    },
    Negate(Box<CheckedExpression>),
    // 直接符号を反転したリテラルは、符号付きの値で範囲を検査する。
    // 正の絶対値が型に収まる必要はない（例: i8の-128）。
    NegatedLiteral(i128),
}
// rustfmtはこのマクロ本体を整形しないため、同じ書式を手動で保つ。
macro_rules! integer_operands {
    ($name:ident, ($($mutable:tt)?), $borrow:ident) => {
        pub fn $name(&$($mutable)? self) -> impl Iterator<Item = &$($mutable)? CheckedExpression> {
            let (first, second) = match self {
                Self::Binary { left, right, .. } => (Some(left.$borrow()), Some(right.$borrow())),
                Self::Negate(operand) => (Some(operand.$borrow()), None),
                Self::NegatedLiteral(_) | Self::Exact(_) => (None, None),
            };
            first.into_iter().chain(second)
        }
    };
}
impl IntegerOperation {
    integer_operands!(operands, (), as_ref);
    integer_operands!(operands_mut, (mut), as_mut);
}

/// [`crate::ast::ExpressionKind`] に対応する検査済みの値・演算。
/// ローカルと関数は番号、フィールドは添字で参照する。型は外側の [`CheckedExpression`] に持つ。
/// 正確な整数式には値域があり、変換・演算・切り出しには必要な失敗処理を保持する。
/// 値ブロックには捕捉した変数を、汎用リテラルには具体化まで必要な綴りを残す。
#[derive(Clone)]
pub(crate) enum Value {
    /// 呼び出しの第1引数。型は受け手の値の型だが、更新時は実体の位置を渡す。
    Receiver {
        target: Box<CheckedExpression>,
        mutable: bool,
    },
    Array {
        values: Vec<CheckedExpression>,
        lengths: Vec<CheckedExpression>,
        repeated: bool,
        // 定数で決まる内側からの要素数。対象C環境のsizeofでもコンパイル時に検査する。
        constant_counts: Vec<u64>,
    },
    /// 要素の住所。添字は書き込み先の確定時に一度だけ評価する。
    ArrayElement {
        target: Box<CheckedExpression>,
        indices: Vec<(CheckedExpression, Location)>,
    },
    ArrayRead {
        address: Box<CheckedExpression>,
        handler: Option<CheckedHandler>,
    },
    New {
        value: Box<CheckedExpression>,
        location: Location,
    },
    Deref(Box<CheckedExpression>),
    NonNull(Box<CheckedExpression>),
    Null,

    CBytes(Box<CheckedExpression>),
    CByteLen {
        operand: Box<CheckedExpression>,
        location: Location,
    },
    /// 値は既知だが対象C環境の範囲は未確定。生成Cで静的に検査する。
    CIntegerLiteral {
        value: crate::numeric::Int,
        location: Location,
    },
    Panic {
        parts: Vec<crate::ast::TemplatePart<CheckedExpression>>,
        location: Location,
    },
    HandledCall {
        call: Box<CheckedExpression>,
        handler: Box<CallHandler>,
    },
    Function {
        id: usize,
        location: Location,
    },
    FunctionBorrow(Box<CheckedExpression>),
    Closure(Box<CheckedClosure>),
    Invoke {
        location: Location,
        target: Box<CheckedExpression>,
        arguments: Vec<CheckedExpression>,
    },
    // スカラー型によって表現が決まるまで、元の綴りを保持する。
    GenericNumericLiteral {
        text: String,
        negative: bool,
        floating: bool,
        location: Location,
    },
    Exact(Box<super::exact::Exact>),
    TextView(Box<CheckedExpression>),
    Intern {
        constant: Option<String>,
        operand: Box<CheckedExpression>,
        location: Location,
    },
    Slice {
        target: Box<CheckedExpression>,
        start: Option<Box<CheckedExpression>>,
        end: Option<Box<CheckedExpression>>,
        index: bool,
        handler: Option<CheckedHandler>,
    },
    Text {
        text: String,
    },
    Template {
        parts: Vec<crate::ast::TemplatePart<CheckedExpression>>,
        location: Location,
        constant: Option<String>,
    },
    Struct {
        fields: Vec<(usize, CheckedExpression)>,
    },
    With {
        target: Box<CheckedExpression>,
        fields: Vec<(usize, CheckedExpression)>,
    },
    Field {
        target: Box<CheckedExpression>,
        index: usize,
    },
    Void,
    Call {
        location: Location,
        id: usize,
        arguments: Vec<CheckedExpression>,
    },
    Block {
        statements: Vec<CheckedStatement>,
        value: Box<CheckedExpression>,
        captures: Vec<(usize, Type, bool)>,
    },
    Match {
        target: Box<CheckedExpression>,
        arms: Vec<(Option<CheckedExpression>, CheckedExpression)>,
        pattern_warnings: Vec<Location>,
    },
    IntegerArithmetic {
        operation: IntegerOperation,
        location: Location,
        handler: Option<CheckedHandler>,
    },
    CastReturn {
        return_location: Location,
        location: Location,
        wrap: bool,
        operand: Box<CheckedExpression>,
        return_value: Box<CheckedExpression>,
    },
    Cast(Box<CheckedExpression>),
    CheckedCast {
        location: Location,
        constant_struct: Option<Box<CheckedExpression>>,
        wrap: bool,
        operand: Box<CheckedExpression>,
        fallback: Box<CheckedExpression>,
    },
    Len(Box<CheckedExpression>),
    Negate(Box<CheckedExpression>),
    Float(u64),
    If {
        branches: Vec<(CheckedExpression, CheckedExpression)>,
        otherwise: Box<CheckedExpression>,
    },
    Truncate(Box<CheckedExpression>),
    BitNot(Box<CheckedExpression>),
    Binary {
        operator: BinaryOperator,
        left: Box<CheckedExpression>,
        right: Box<CheckedExpression>,
    },
    Integer {
        value: i128,
        minus: Option<Location>,
    },
    Bool(bool),
    Not(Box<CheckedExpression>),
    Logical {
        operator: LogicalOperator,
        left: Box<CheckedExpression>,
        right: Box<CheckedExpression>,
    },
    Comparison {
        // 比較メソッドの呼び出しによる診断を、各演算子の位置へ対応させる。
        locations: Vec<Location>,
        // 空なら全て組み込み比較。Some(id)は比較メソッドの具体化済み関数ID。
        methods: Vec<Option<usize>>,
        operators: Vec<ComparisonOperator>,
        operands: Vec<CheckedExpression>,
    },
    Local(usize),
}

// 参照用と変更用の定数式の文法を一つの定義で保つ。違いは可変性と訪問メソッドだけ。
// rustfmtはこのマクロ本体を整形しないため、同じ書式を手動で保つ。
macro_rules! constant_children_visitor {
    ($name:ident, ($($mutable:tt)?), $walk:ident, $operands:ident) => {
        pub fn $name(
            &$($mutable)? self,
            visit: &mut impl FnMut(&$($mutable)? CheckedExpression),
            exact_node: &mut impl FnMut(&$($mutable)? super::exact::Exact),
        ) -> bool {
            let handler = |handler: &$($mutable)? Option<CheckedHandler>,
                           visit: &mut dyn FnMut(&$($mutable)? CheckedExpression)| {
                match handler {
                    None => true,
                    Some(CheckedHandler::Value(value)) => {
                        visit(value);
                        true
                    }
                    Some(CheckedHandler::Return { .. }) => false,
                }
            };
            match self {
                Self::Exact(exact) => exact.$walk(exact_node, visit),
                Self::IntegerArithmetic { operation, handler: h, .. } => {
                    match operation {
                        IntegerOperation::Exact(exact) => exact.$walk(exact_node, visit),
                        _ => operation.$operands().for_each(&mut *visit),
                    }
                    return handler(h, visit);
                }
                Self::Receiver { target: v, .. } | Self::New { value: v, .. } | Self::Deref(v) | Self::NonNull(v) | Self::CBytes(v) | Self::CByteLen { operand: v, .. } | Self::FunctionBorrow(v) | Self::Len(v) | Self::Cast(v) | Self::Negate(v) | Self::Truncate(v) | Self::BitNot(v)
                | Self::Not(v) | Self::TextView(v) | Self::Intern { operand: v, .. }
                | Self::Field { target: v, .. } => visit(v),
                // constant_structはオペランドの解析から得た具体的なキャッシュであり、ソースのオペランドではない。
                // 構造体のフィールドは具体型なので、汎用型を含まない（[num.places]）。
                Self::CheckedCast { operand, fallback, .. } => {
                    visit(operand);
                    visit(fallback);
                }
                Self::Binary { left, right, .. } | Self::Logical { left, right, .. } => {
                    visit(left);
                    visit(right);
                }
                Self::Comparison { operands, methods, .. } => {
                    for value in operands {
                        visit(value);
                    }
                    return !methods.iter().any(Option::is_some);
                }
                Self::Template { parts, .. } => {
                    for part in parts {
                        if let crate::ast::TemplatePart::Value(value) = part {
                            visit(value);
                        }
                    }
                }
                Self::With { target, fields } => {
                    visit(target);
                    for (_, value) in fields {
                        visit(value);
                    }
                }
                Self::Struct { fields } => {
                    for (_, value) in fields {
                        visit(value);
                    }
                }
                Self::Array { values, lengths, .. } => {
                    for v in values { visit(v); }
                    for v in lengths { visit(v); }
                    return true;
                }
                Self::ArrayElement { target, indices } => {
                    visit(target);
                    for (v, _) in indices { visit(v); }
                    return true;
                }
                Self::ArrayRead { address, handler: h } => {
                    visit(address);
                    return handler(h, visit);
                }
                Self::Slice { target, start, end, handler: h, .. } => {
                    visit(target);
                    for value in [start, end].into_iter().flatten() {
                        visit(value);
                    }
                    return handler(h, visit);
                }
                Self::Null | Self::CIntegerLiteral { .. } | Self::GenericNumericLiteral { .. } | Self::Integer { .. } | Self::Float(_)
                | Self::Function { .. } | Self::Bool(_) | Self::Text { .. } => {}
                Self::Panic { .. } | Self::HandledCall { .. } | Self::Closure(_) | Self::Local(_) | Self::Void | Self::Invoke { .. } | Self::Call { .. } | Self::Block { .. }
                | Self::Match { .. } | Self::If { .. } | Self::CastReturn { .. } => return false,
            }
            true
        }
    };
}

impl Value {
    // 正確な整数式の葉と値ハンドラーも子に含める。returnハンドラーは定数式の文法に含まれない。
    constant_children_visitor!(visit_constant_parts, (), visit, operands);
    pub fn visit_constant_children(&self, visit: &mut impl FnMut(&CheckedExpression)) -> bool {
        self.visit_constant_parts(visit, &mut |_| {})
    }
    constant_children_visitor!(visit_constant_parts_mut, (mut), visit_mut, operands_mut);

    pub fn symbolic_range(&self) -> bool {
        match self {
            Self::Exact(e)
            | Self::IntegerArithmetic {
                operation: IntegerOperation::Exact(e),
                ..
            } => e.domains.is_some(),
            _ => false,
        }
    }
}
