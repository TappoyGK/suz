//! 意味検査で得たIRを定義する。ASTの名前を番号へ解決し、式の型・値域・失敗時の処理を保持してC生成へ渡す。
//!
//! 利用元: [`crate::check`]、[`crate::codegen`]。

use crate::ast::{BinaryOperator, ComparisonOperator, Location, LogicalOperator, Type};

/// [`crate::ast::Program`] の意味検査結果。
/// 型・名前を解決した関数、入口の関数番号、文字列定数と診断候補を保持する。
/// 型宣言はASTの表現を共有し、関数は必要な汎用型の具体化も含めてC生成へ渡す。
pub(crate) struct CheckedProgram {
    pub function_types: Vec<crate::ast::FunctionType>,
    /// 整数除算を含む浮動小数点演算の警告候補。
    pub division_warnings: Vec<Location>,
    /// 定数として決まる比較の位置と結果。
    pub comparison_warnings: Vec<(Location, bool)>,
    /// 実行開始時に登録する文字列定数。
    pub constant_texts: std::collections::BTreeSet<String>,
    /// 静的な格納領域へmainの前に設定する、評価済みの大域変数の初期値。
    pub globals: Vec<CheckedExpression>,
    pub global_warnings: Vec<Location>,
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
    Fail(CheckedExpression),
    HandleCall {
        id: usize,
        call: Box<CheckedExpression>,
        handler: Box<CallHandler>,
    },
    SliceReturn {
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
        id: usize,
        operation: CheckedExpression,
        return_value: Box<CheckedExpression>,
    },
    CastReturn {
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
    Binding {
        mutable: bool,
        id: usize,
        ty: Type,
        initializer: CheckedExpression,
    },
    Assign {
        global: bool,
        fields: Vec<usize>,
        id: usize,
        ty: Type,
        value: CheckedExpression,
    },
    Return(CheckedExpression),
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
    Return(Box<CheckedExpression>),
}
/// 呼び出しの失敗処理。脱出するものはC生成前に文へ持ち上げる。
#[derive(Clone)]
pub(crate) enum CallHandler {
    Value(CheckedExpression),
    Return(CheckedExpression),
    Fail(Option<CheckedExpression>),
    Ignore,
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
    Panic {
        parts: Vec<crate::ast::TemplatePart<CheckedExpression>>,
        location: Location,
    },
    HandledCall {
        call: Box<CheckedExpression>,
        handler: Box<CallHandler>,
    },
    Function(usize),
    FunctionBorrow(Box<CheckedExpression>),
    Closure(Box<CheckedClosure>),
    Invoke {
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
        operators: Vec<ComparisonOperator>,
        operands: Vec<CheckedExpression>,
    },
    Local(usize),
    Global(usize),
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
                    Some(CheckedHandler::Return(_)) => false,
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
                Self::FunctionBorrow(v) | Self::Len(v) | Self::Cast(v) | Self::Negate(v) | Self::Truncate(v) | Self::BitNot(v)
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
                Self::Comparison { operands, .. } => {
                    for value in operands {
                        visit(value);
                    }
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
                Self::Slice { target, start, end, handler: h, .. } => {
                    visit(target);
                    for value in [start, end].into_iter().flatten() {
                        visit(value);
                    }
                    return handler(h, visit);
                }
                Self::GenericNumericLiteral { .. } | Self::Integer { .. } | Self::Float(_)
                | Self::Function(_) | Self::Bool(_) | Self::Text { .. } => {}
                Self::Panic { .. } | Self::HandledCall { .. } | Self::Closure(_) | Self::Local(_) | Self::Global(_) | Self::Void | Self::Invoke { .. } | Self::Call { .. } | Self::Block { .. }
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
