//! 構文解析で作るASTと、ソース位置・型・演算子の共通表現を定義する。名前や式の型が未確定の木を保持し、意味検査へ渡す。
//!
//! 利用元: [`crate::parser`]、[`crate::check`]。

pub(crate) mod arrays;
pub(crate) mod generics;
mod pointer_shape;
pub(crate) mod visit;
use pointer_shape::PointerShape;

use crate::{Diagnostic, ErrorKind};

/// ソース内の1始まりの行・列。構文解析後も診断の位置として保持する。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Location {
    pub line: usize,
    pub column: usize,
}

impl Location {
    pub fn error(self, kind: ErrorKind, message: crate::DiagnosticMessage) -> Diagnostic {
        Diagnostic::new(kind, self.line, self.column, message.rule, message.text)
    }
}

/// Suzの具体的な整数型。格納範囲と対応するC型を定める。Lenはビット演算を持たない個数用の型。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum IntegerType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Len,
}

impl IntegerType {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "i8" => Some(Self::I8),
            "i16" => Some(Self::I16),
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "u8" => Some(Self::U8),
            "u16" => Some(Self::U16),
            "u32" => Some(Self::U32),
            "u64" => Some(Self::U64),
            "len_t" => Some(Self::Len),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::Len => "len_t",
        }
    }
    pub fn c_name(self) -> &'static str {
        match self {
            Self::I8 => "int8_t",
            Self::I16 => "int16_t",
            Self::I32 => "int32_t",
            Self::I64 => "int64_t",
            Self::U8 => "uint8_t",
            Self::U16 => "uint16_t",
            Self::U32 => "uint32_t",
            Self::U64 => "uint64_t",
            Self::Len => "int32_t",
        }
    }
    /// 生成Cの格納型の符号。値域が非負でもlen_tのint32_tは符号付き。
    pub fn c_signed(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::Len
        )
    }
    // len_t の31は値域のビット数。言語上のビット幅ではなく、Cではint32_tを使う。
    pub fn bits(self) -> u32 {
        match self {
            Self::Len => 31,
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 => 32,
            Self::I64 | Self::U64 => 64,
        }
    }
    pub fn bounds(self) -> (i128, i128) {
        let bits = self.bits();
        let signed = matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64);
        if signed {
            (-(1_i128 << (bits - 1)), (1_i128 << (bits - 1)) - 1)
        } else {
            (0, (1_i128 << bits) - 1)
        }
    }
}

/// Suzの具体的な浮動小数点型（f32またはf64）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FloatType {
    F32,
    F64,
}
impl FloatType {
    pub fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
    pub fn c_name(self) -> &'static str {
        match self {
            Self::F32 => "float",
            Self::F64 => "double",
        }
    }
}

/// 列挙型の識別番号、整数の基底型、列挙子数。番号はプログラム内の宣言を参照する。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EnumType {
    pub id: usize,
    pub base: IntegerType,
    pub count: usize,
    /// error宣言は整数との変換を持たない。
    pub is_error: bool,
}

/// 列挙型の宣言。列挙子の名前と、宣言を読み飛ばすためのトークン範囲を保持する。
#[derive(Clone)]
pub(crate) struct EnumDeclaration {
    pub name: String,
    pub location: Location,
    pub ty: EnumType,
    pub variants: Vec<String>,
    pub start: usize,
    pub end: usize,
}

/// 汎用数値型を具体化した一つの整数型または浮動小数点型。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScalarType {
    Integer(IntegerType),
    Float(FloatType),
}

// 内部の型関数で扱う、独立したintとfloatの型代入の組み合わせ数。
pub(crate) const NUMERIC_COMBINATIONS: usize = 18;

/// ASTと意味検査で共有する型表現。
/// 具体的な型に加え、汎用のint・float、型の組み合わせごとの結果、内部の正確な整数を表す。
/// Structの番号は構造体宣言、Functionの番号は共通化した関数値の型を参照する。汎用の型はC生成までに具体化する。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Type {
    Array(usize), // ポインタの参照先専用。配列値は単独では保存しない。
    Pointer {
        leaf: PointerLeaf,
        shape: PointerShape,
    },
    CPointer,
    CInteger(CIntegerType),
    Parameter(usize),
    Application(usize), // 構文中の構造体への型適用。
    StaticShape(usize), // 構文中の保存可能なfnshapeへの型適用。
    Shape(usize),       // 構文中の呼び出し専用fnshapeへの型適用。
    GenericInt,
    GenericFloat,
    Numeric([Option<ScalarType>; NUMERIC_COMBINATIONS]), // intとfloatの内部の型関数。
    Exact, // 内部の数学的な整数。ソースで指定する格納型ではない。
    Struct(usize),
    StaticFunction(usize), // 保存・返却できるトップレベルの関数値。
    Function(usize),       // 呼び出しと転送だけができる仮引数。
    Enum(EnumType),
    Void,
    Float(FloatType),
    Integer(IntegerType),
    Bool,
    Char,
    Symbol,
    Text,
}

/// ポインタの末端の値型。各段のnull許容性はPointerShapeで別に持つ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PointerLeaf {
    Array(usize),
    CPointer,
    CInteger(CIntegerType),
    Parameter(usize),
    Application(usize),
    StaticShape(usize),
    Shape(usize),
    GenericInt,
    GenericFloat,
    Numeric([Option<ScalarType>; NUMERIC_COMBINATIONS]),
    Exact,
    Struct(usize),
    StaticFunction(usize),
    Function(usize),
    Enum(EnumType),
    Void,
    Float(FloatType),
    Integer(IntegerType),
    Bool,
    Char,
    Symbol,
    Text,
}
impl Type {
    pub fn array_id(self) -> Option<usize> {
        match self.pointee() {
            Some(Self::Array(id)) => Some(id),
            _ => None,
        }
    }
    pub fn pointer(self) -> Self {
        let leaf = match self {
            Self::Pointer { leaf, shape } => {
                return Self::Pointer {
                    leaf,
                    shape: shape.push(),
                };
            }
            Self::Array(id) => PointerLeaf::Array(id),
            Self::CPointer => PointerLeaf::CPointer,
            Self::CInteger(v) => PointerLeaf::CInteger(v),
            Self::Parameter(id) => PointerLeaf::Parameter(id),
            Self::Application(id) => PointerLeaf::Application(id),
            Self::StaticShape(id) => PointerLeaf::StaticShape(id),
            Self::Shape(id) => PointerLeaf::Shape(id),
            Self::GenericInt => PointerLeaf::GenericInt,
            Self::GenericFloat => PointerLeaf::GenericFloat,
            Self::Numeric(v) => PointerLeaf::Numeric(v),
            Self::Exact => PointerLeaf::Exact,
            Self::Struct(v) => PointerLeaf::Struct(v),
            Self::StaticFunction(v) => PointerLeaf::StaticFunction(v),
            Self::Function(v) => PointerLeaf::Function(v),
            Self::Enum(v) => PointerLeaf::Enum(v),
            Self::Void => PointerLeaf::Void,
            Self::Float(v) => PointerLeaf::Float(v),
            Self::Integer(v) => PointerLeaf::Integer(v),
            Self::Bool => PointerLeaf::Bool,
            Self::Char => PointerLeaf::Char,
            Self::Symbol => PointerLeaf::Symbol,
            Self::Text => PointerLeaf::Text,
        };
        Self::Pointer {
            leaf,
            shape: PointerShape::new(),
        }
    }
    pub fn is_nullable(self) -> bool {
        matches!(self, Self::Pointer { shape, .. } if shape.is_nullable())
    }
    pub fn is_nonnull(self) -> bool {
        matches!(self, Self::Pointer { .. }) && !self.is_nullable()
    }
    pub fn with_nullable(self, enabled: bool) -> Self {
        match self {
            Self::Pointer { leaf, shape } => Self::Pointer {
                leaf,
                shape: shape.with_nullable(enabled),
            },
            _ => self,
        }
    }
    /// 外側のnull許容性だけを広げる。参照先の型は変更しない。
    pub fn lifts_to(self, to: Self) -> bool {
        self.is_nonnull() && to.is_nullable() && self.pointee() == to.pointee()
    }
    pub fn pointee(self) -> Option<Self> {
        let Self::Pointer { leaf, shape } = self else {
            return None;
        };
        if shape.depth() > 1 {
            return Some(Self::Pointer {
                leaf,
                shape: shape.pop(),
            });
        }
        Some(match leaf {
            PointerLeaf::Array(id) => Self::Array(id),
            PointerLeaf::CPointer => Self::CPointer,
            PointerLeaf::CInteger(v) => Self::CInteger(v),
            PointerLeaf::Parameter(id) => Self::Parameter(id),
            PointerLeaf::Application(id) => Self::Application(id),
            PointerLeaf::StaticShape(id) => Self::StaticShape(id),
            PointerLeaf::Shape(id) => Self::Shape(id),
            PointerLeaf::GenericInt => Self::GenericInt,
            PointerLeaf::GenericFloat => Self::GenericFloat,
            PointerLeaf::Numeric(v) => Self::Numeric(v),
            PointerLeaf::Exact => Self::Exact,
            PointerLeaf::Struct(v) => Self::Struct(v),
            PointerLeaf::StaticFunction(v) => Self::StaticFunction(v),
            PointerLeaf::Function(v) => Self::Function(v),
            PointerLeaf::Enum(v) => Self::Enum(v),
            PointerLeaf::Void => Self::Void,
            PointerLeaf::Float(v) => Self::Float(v),
            PointerLeaf::Integer(v) => Self::Integer(v),
            PointerLeaf::Bool => Self::Bool,
            PointerLeaf::Char => Self::Char,
            PointerLeaf::Symbol => Self::Symbol,
            PointerLeaf::Text => Self::Text,
        })
    }
}

impl Type {
    /// ポインタの深さを除いた型。汎用数値型と関数型の宣言を調べるときに使う。
    pub fn leaf(mut self) -> Self {
        while let Some(inner) = self.pointee() {
            self = inner;
        }
        self
    }
    pub fn generic(self) -> bool {
        matches!(self, Self::GenericInt | Self::GenericFloat)
    }
    pub fn symbolic(self) -> bool {
        self.generic() || matches!(self, Self::Numeric(_))
    }
    /// 取りうる結果がすべて整数である記号的な型も含む。
    pub fn is_integer(self) -> bool {
        match self {
            Self::Integer(_) | Self::GenericInt => true,
            Self::Numeric(types) => types
                .iter()
                .flatten()
                .all(|ty| matches!(ty, ScalarType::Integer(_))),
            _ => false,
        }
    }
    /// 取りうる結果がすべて浮動小数点型である記号的な型も含む。
    pub fn is_float(self) -> bool {
        match self {
            Self::Float(_) | Self::GenericFloat => true,
            Self::Numeric(types) => types
                .iter()
                .flatten()
                .all(|ty| matches!(ty, ScalarType::Float(_))),
            _ => false,
        }
    }
    /// 具体型と記号的な整数・浮動小数点型を含む。
    pub fn is_numeric(self) -> bool {
        matches!(self, Self::Numeric(_)) || self.is_integer() || self.is_float()
    }
    pub fn is_text(self) -> bool {
        matches!(self, Self::Symbol | Self::Text)
    }
    pub fn integer_bounds(self) -> Option<(i128, i128)> {
        match self {
            Self::Char => Some((0, 0x10ffff)),
            Self::Integer(ty) => Some(ty.bounds()),
            Self::Enum(ty) => Some((0, ty.count as i128 - 1)),
            _ => None,
        }
    }
    pub fn integer_storage(self) -> Option<IntegerType> {
        match self {
            Self::Char => Some(IntegerType::U32),
            Self::Integer(ty) => Some(ty),
            Self::Enum(ty) => Some(ty.base),
            _ => None,
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        if let Some(ty) = CIntegerType::from_name(name) {
            Some(Self::CInteger(ty))
        } else if name == "int" {
            Some(Self::GenericInt)
        } else if name == "float" {
            Some(Self::GenericFloat)
        } else if name == "char" {
            Some(Self::Char)
        } else if name == "symbol" {
            Some(Self::Symbol)
        } else if name == "text" {
            Some(Self::Text)
        } else if name == "void" {
            Some(Self::Void)
        } else if name == "bool" {
            Some(Self::Bool)
        } else if name == "f32" {
            Some(Self::Float(FloatType::F32))
        } else if name == "f64" {
            Some(Self::Float(FloatType::F64))
        } else {
            IntegerType::from_name(name).map(Self::Integer)
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Pointer { .. } => {
                if self.is_nullable() {
                    "ptr?"
                } else {
                    "ptr"
                }
            }
            Self::Array(_) => "array",
            Self::CPointer => "*const void",
            Self::CInteger(ty) => ty.name(),
            Self::Parameter(_) => "type parameter",
            Self::StaticShape(_) => "static fn",
            Self::Shape(_) => "fn",
            Self::Application(_) => "generic struct",
            Self::Exact => "exact integer",
            Self::GenericInt => "int",
            Self::GenericFloat => "float",
            Self::Numeric(_) => {
                if self.is_integer() {
                    "int"
                } else {
                    "float"
                }
            }
            Self::Integer(ty) => ty.name(),
            Self::Float(ty) => ty.name(),
            Self::Char => "char",
            Self::Symbol => "symbol",
            Self::Text => "text",
            Self::Bool => "bool",
            Self::Void => "void",
            Self::Enum(_) => "enum",
            Self::Struct(_) => "struct",
            Self::StaticFunction(_) | Self::Function(_) => "function",
        }
    }
    pub fn c_name(self) -> String {
        match self {
            Self::Pointer { .. } if self.array_id().is_some() => {
                format!("suz_array_{}", self.array_id().unwrap())
            }
            Self::Array(_) => unreachable!("array storage has no standalone C value type"),
            Self::Pointer { .. } => format!("typeof({} *)", self.pointee().unwrap().c_name()),
            Self::CPointer => "suz_ffi_bytes".into(),
            Self::CInteger(ty) => ty.c_name().into(),
            Self::Exact => "suz_exact".into(),
            Self::Parameter(_)
            | Self::StaticShape(_)
            | Self::Shape(_)
            | Self::Application(_)
            | Self::GenericInt
            | Self::GenericFloat
            | Self::Numeric(_) => {
                unreachable!("generic types are resolved before code generation")
            }
            Self::Struct(id) => format!("suz_struct_{id}"),
            Self::StaticFunction(id) => format!("suz_fntype_{id}"),
            Self::Function(id) => format!("suz_callback_{id}"),
            Self::Integer(ty) => ty.c_name().into(),
            Self::Float(ty) => ty.c_name().into(),
            Self::Char => "uint32_t".into(),
            Self::Symbol => "suz_text".into(),
            Self::Text => "suz_text_view".into(),
            Self::Bool => "_Bool".into(),
            Self::Void => "void".into(),
            Self::Enum(ty) => ty.base.c_name().into(),
        }
    }
}

/// 構造体のフィールド宣言。名前・型と、その宣言のソース位置。
#[derive(Clone)]
pub(crate) struct StructField {
    pub name: String,
    pub ty: Type,
    pub location: Location,
}

/// 構造体の宣言。宣言順のフィールドとトークン範囲を持つ。
#[derive(Clone)]
pub(crate) struct StructDeclaration {
    pub type_parameters: Vec<generics::TypeParameter>,
    pub needs: Vec<generics::Need>,
    pub application: Option<generics::Application>,
    pub symbolic: bool,
    pub external: Option<ExternStruct>,
    pub name: String,
    pub location: Location,
    pub fields: Vec<StructField>,
    pub start: usize,
    pub end: usize,
}

/// ヘッダーのstructタグとの照合に使うC側の名前。
#[derive(Clone)]
pub(crate) struct ExternStruct {
    pub header: String,
    pub c_name: String,
}

/// 構造体リテラルに書いたフィールド名・初期化式・位置。意味検査でフィールド番号へ解決する。
#[derive(Clone)]
pub(crate) struct FieldInitializer {
    pub name: String,
    pub value: Expression,
    pub location: Location,
}

/// トップレベルのfixの宣言。省略可能な型注釈と、まだ評価していない初期化式。
pub(crate) struct TopLevelBinding {
    pub name: String,
    pub location: Location,
    pub annotation: Option<Type>,
    pub initializer: Expression,
}

/// エラー集合の要素。型番号とバリアント番号をソートして関数の形を比較する。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ErrorVariant {
    pub type_id: usize,
    pub variant: usize,
}

/// 関数値の型。同じ仮引数・戻り値の組を一つの番号へまとめる。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FunctionType {
    pub parameters: Vec<Type>,
    pub result: Type,
    pub errors: Vec<ErrorVariant>,
}

/// fnshapeの名前と宣言位置。型の同一性は名前ではなく仮引数と戻り値の型で決まる。
#[derive(Clone)]
pub(crate) struct FnShapeDeclaration {
    /// 型引数のない名前の型適用番号。使用位置での再登録を省く。
    pub plain_application: Option<usize>,
    pub type_parameters: Vec<generics::TypeParameter>,
    pub needs: Vec<generics::Need>,
    pub name: String,
    pub location: Location,
    pub start: usize,
    pub end: usize,
    /// Program.function_typesの署名番号。直接の依存先から順に並ぶ。
    pub type_id: usize,
}

/// 構文解析の結果全体。型宣言・トップレベル束縛・関数とallowの範囲を保持する。
/// [`crate::check`] が検査して [`crate::check::ir::CheckedProgram`] へ変換する。
pub(crate) struct Program {
    pub arrays: arrays::Types,
    pub need_groups: Vec<generics::NeedGroup>,
    pub applications: Vec<generics::Application>,
    pub externs: Vec<ExternFunction>,
    pub fnshapes: Vec<FnShapeDeclaration>,
    pub function_types: Vec<FunctionType>,
    pub allows: Vec<crate::warnings::AllowScope>,
    pub structs: Vec<StructDeclaration>,
    pub enums: Vec<EnumDeclaration>,
    pub bindings: Vec<TopLevelBinding>,
    pub functions: Vec<Function>,
}

/// 関数の仮引数。ソース上の名前・型注釈・位置を保持する。
#[derive(Clone)]
pub(crate) struct Parameter {
    pub name: String,
    pub ty: Type,
    pub location: Location,
}

/// 関数の構文。仮引数・戻り値型・本体の文と、宣言の開始・終了位置を保持する。
#[derive(Clone)]
pub(crate) struct Function {
    pub type_parameters: Vec<generics::TypeParameter>,
    pub needs: Vec<generics::Need>,
    pub method: Option<Method>,
    pub keyword_location: Location,
    pub name: String,
    pub location: Location,
    pub parameters: Vec<Parameter>,
    pub result: Type,
    pub errors: Vec<ErrorVariant>,
    pub statements: Vec<Statement>,
    pub end: Location,
}

/// メソッドの受け手。暗黙の第1仮引数selfの型と更新の可否を保持する。
#[derive(Clone, Copy)]
pub(crate) struct Method {
    pub operator: Option<&'static str>,
    pub owner: Type,
    pub mutable: bool,
}

impl Function {
    pub fn declares_numeric(&self, ty: Type) -> bool {
        self.result.leaf() == ty
            || self
                .parameters
                .iter()
                .any(|parameter| parameter.ty.leaf() == ty)
    }
}

/// matchの一つの分岐。patternがNoneならそれ以外の分岐。
/// 本体は文の列または値の式であり、Tで区別する。
#[derive(Clone)]
pub(crate) struct MatchArm<T = Vec<Statement>> {
    pub pattern: Option<Expression>,
    pub location: Location,
    pub body: T,
}

/// 出力文のレベル。Panicはpanic文でのみ使い、出力後に終了する。
#[derive(Clone, Copy, Debug)]
pub(crate) enum LogLevel {
    Debug,
    Error,
    Info,
    Warn,
    Panic,
}

/// 構文解析済みの文。束縛やフィールドの参照はまだソース上の名前で保持する。
#[derive(Clone)]
pub(crate) enum Statement {
    Arena {
        body: Vec<Statement>,
        end: Location,
    },
    Let {
        name: String,
        location: Location,
        value: Box<Expression>,
        body: Vec<Statement>,
        otherwise: Vec<Statement>,
    },
    Store {
        target: Box<Expression>,
        value: Box<Expression>,
        operator: Option<BinaryOperator>,
        location: Location,
    },

    Trusted(Vec<Statement>),
    Objection(Vec<Statement>),
    Log {
        level: LogLevel,
        parts: Vec<TemplatePart<Expression>>,
        location: Location,
    },
    Expression(Expression),
    Match {
        target: Expression,
        arms: Vec<MatchArm>,
        location: Location,
    },
    For {
        name: String,
        location: Location,
        start: Expression,
        // メモリ効率を高めるため、終了式をBoxに置き、Forを含まない文の格納サイズも抑える。
        end: Option<Box<Expression>>,
        inclusive: bool,
        reverse: bool,
        body: Vec<Statement>,
    },
    Loop {
        condition: Option<Expression>,
        body: Vec<Statement>,
    },
    Break(Location),
    Continue(Location),
    If {
        branches: Vec<(Expression, Vec<Statement>)>,
        otherwise: Option<Vec<Statement>>,
    },
    Block(Vec<Statement>),
    Binding {
        pointer: bool,
        mutable: bool,
        name: String,
        location: Location,
        annotation: Option<Type>,
        // 型とソース位置の追加で全ての文が肥大化しないよう、初期値を別に保持する。
        initializer: Box<Expression>,
    },
    Assign {
        compound: bool,
        name: String,
        fields: Vec<String>,
        location: Location,
        value: Expression,
    },
    Return(Expression),
    Fail(Expression),
    Recover(Expression),
    Ignore(Box<Ignore>),
}

/// 構文解析済みの式とソース位置。型は意味検査で決定する。
#[derive(Clone)]
pub(crate) struct Expression {
    pub kind: ExpressionKind,
    pub location: Location,
    /// 構文上の高さ。後置・左結合の演算を追加する前の上限検査に使う。
    pub height: usize,
}

/// 文字列テンプレートの文字列部分、または補間する値。
/// TはASTでは [`Expression`]、IRでは [`crate::check::ir::CheckedExpression`]。
#[derive(Clone)]
pub(crate) enum TemplatePart<T> {
    Text(String),
    Value(T),
}

/// 短絡評価する論理演算子（&&または||）。
#[derive(Clone, Copy)]
pub(crate) enum LogicalOperator {
    And,
    Or,
}

impl LogicalOperator {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::And => "&&",
            Self::Or => "||",
        }
    }
}

/// 等値・順序比較の演算子。比較の連鎖では各境界の演算子として保持する。
#[derive(Clone, Copy)]
pub(crate) enum ComparisonOperator {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

impl ComparisonOperator {
    pub fn from_symbol(symbol: &str) -> Option<Self> {
        match symbol {
            "==" => Some(Self::Equal),
            "!=" => Some(Self::NotEqual),
            "<" => Some(Self::Less),
            "<=" => Some(Self::LessEqual),
            ">" => Some(Self::Greater),
            ">=" => Some(Self::GreaterEqual),
            _ => None,
        }
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        }
    }
    pub fn direction(self) -> Option<bool> {
        match self {
            Self::Less | Self::LessEqual => Some(true),
            Self::Greater | Self::GreaterEqual => Some(false),
            _ => None,
        }
    }
    pub fn is_equality(self) -> bool {
        matches!(self, Self::Equal | Self::NotEqual)
    }
}

/// 算術・ラップ・シフト・ビット演算の二項演算子。構文の優先順位もここで定める。
#[derive(Clone, Copy)]
pub(crate) enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    ShiftLeft,
    ShiftRight,
    WrapAdd,
    WrapSubtract,
    WrapMultiply,
    Or,
    Xor,
    And,
}
impl BinaryOperator {
    pub fn is_shift(self) -> bool {
        matches!(self, Self::ShiftLeft | Self::ShiftRight)
    }
    pub fn is_arithmetic(self) -> bool {
        matches!(
            self,
            Self::Add
                | Self::Subtract
                | Self::Multiply
                | Self::Divide
                | Self::Remainder
                | Self::ShiftLeft
                | Self::ShiftRight
        )
    }
    pub fn from_symbol(symbol: &str) -> Option<Self> {
        match symbol {
            "+" => Some(Self::Add),
            "-" => Some(Self::Subtract),
            "*" => Some(Self::Multiply),
            "/" => Some(Self::Divide),
            "%" => Some(Self::Remainder),
            "<<" => Some(Self::ShiftLeft),
            ">>" => Some(Self::ShiftRight),
            "|" => Some(Self::Or),
            "^" => Some(Self::Xor),
            "&" => Some(Self::And),
            "+%" => Some(Self::WrapAdd),
            "-%" => Some(Self::WrapSubtract),
            "*%" => Some(Self::WrapMultiply),
            _ => None,
        }
    }
    // 値が大きいほど強く結合する。加算と減算は同じ優先順位。
    pub fn precedence(self) -> u8 {
        match self {
            Self::Or => 0,
            Self::Xor => 1,
            Self::And => 2,
            Self::ShiftLeft | Self::ShiftRight => 3,
            Self::WrapAdd | Self::WrapSubtract | Self::Add | Self::Subtract => 4,
            Self::WrapMultiply | Self::Multiply | Self::Divide | Self::Remainder => 5,
        }
    }
    pub fn c_symbol(self) -> &'static str {
        match self {
            Self::Divide => "/",
            Self::Remainder => "%",
            Self::ShiftLeft => "<<",
            Self::ShiftRight => ">>",
            Self::Or => "|",
            Self::Xor => "^",
            Self::And => "&",
            Self::WrapAdd | Self::Add => "+",
            Self::WrapSubtract | Self::Subtract => "-",
            Self::WrapMultiply | Self::Multiply => "*",
        }
    }
}

/// errorハンドラーの腕。ブロックだけがrecoverを文として使える。
#[derive(Clone)]
pub(crate) enum ErrorArmBody {
    Value(Box<Expression>),
    Statements { body: Vec<Statement>, block: bool },
}

#[derive(Clone)]
pub(crate) struct ErrorHandler {
    pub arms: Vec<MatchArm<ErrorArmBody>>,
    pub location: Location,
}

/// 失敗時に値を返す処理、または関数からreturnする処理の構文。
#[derive(Clone)]
pub(crate) enum FailureHandler {
    // 腕のない通常の式まで大きくしないよう、分岐の内容をまとめて保持する。
    Errors(Box<ErrorHandler>),
    Fail {
        value: Option<Box<Expression>>,
        location: Location,
    },
    Ignore(Box<Ignore>),
    Value(Box<Expression>),
    Return {
        value: Box<Expression>,
        location: Location,
    },
}

/// 明示した値・参照のキャプチャーと、独立した仮引数・戻り先を持つクロージャ。
#[derive(Clone)]
pub(crate) struct Closure {
    pub captures: Vec<Capture>,
    pub function: Function,
}

/// キャプチャー一覧の名前と診断位置。型と参照先は外側のスコープで解決する。
#[derive(Clone)]
pub(crate) struct Capture {
    pub by_reference: bool,
    pub name: String,
    pub location: Location,
}

/// 式の構文ごとの内容。名前・リテラルの綴り・子の式を保持し、意味検査で [`crate::check::ir::Value`] へ変換する。
#[derive(Clone)]
pub(crate) enum ExpressionKind {
    MethodCall {
        target: Box<Expression>,
        name: String,
        mutable: bool,
        arguments: Vec<Expression>,
    },
    /// メソッド呼び出しを通常の呼び出しへ渡す際の受け手。ソースからは直接作らない。
    Receiver {
        target: Box<Expression>,
        mutable: bool,
    },
    ArrayInit {
        values: Vec<Expression>,
        lengths: Vec<Expression>,
        repeated: bool,
    },
    New(Box<Expression>),
    Deref(Box<Expression>),
    Null,
    /// or panicの内部表現。通常の値式としては構文から作らない。
    Panic(Vec<TemplatePart<Expression>>),
    Closure(Box<Closure>),
    FunctionValue {
        name: String,
        type_arguments: Vec<Type>,
    },
    Invoke {
        target: Box<Expression>,
        arguments: Vec<Expression>,
    },
    Slice {
        target: Box<Expression>,
        start: Option<Box<Expression>>,
        end: Option<Box<Expression>>,
        index: bool,
        fallback: Option<FailureHandler>,
    },
    Char(u32),
    Template(Vec<TemplatePart<Expression>>),
    Struct {
        ty: Type,
        fields: Vec<FieldInitializer>,
    },
    With {
        target: Box<Expression>,
        fields: Vec<FieldInitializer>,
    },
    Field {
        target: Box<Expression>,
        name: String,
    },
    Variant {
        enum_name: String,
        variant: String,
    },
    Void,
    Call {
        type_arguments: Vec<Type>,
        name: String,
        arguments: Vec<Expression>,
    },
    Block {
        statements: Vec<Statement>,
        value: Box<Expression>,
    },
    Match {
        target: Box<Expression>,
        arms: Vec<MatchArm<Expression>>,
    },
    ArithmeticHandler {
        operand: Box<Expression>,
        fallback: Option<FailureHandler>,
    },
    Cast {
        wrap: bool,
        operand: Box<Expression>,
        target: Type,
        fallback: Option<FailureHandler>,
    },
    CBytes(Box<Expression>),
    CByteLen(Box<Expression>),
    Len(Box<Expression>),
    Negate(Box<Expression>),
    Float(String),
    If {
        branches: Vec<(Expression, Expression)>,
        otherwise: Box<Expression>,
    },
    BitNot(Box<Expression>),
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Integer {
        digits: String,
        negative: bool,
    },
    Bool(bool),
    Not(Box<Expression>),
    Logical {
        operator: LogicalOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Comparison {
        locations: Vec<Location>,
        operators: Vec<ComparisonOperator>,
        operands: Vec<Expression>,
    },
    Name(String),
    Group(Box<Expression>),
}

/// ignoreの実行位置と、補間・評価しない理由。記録を有効にした場合だけCへ出力する。
#[derive(Clone, Debug)]
pub(crate) struct Ignore {
    pub location: Location,
    pub reason: String,
}

/// C環境で幅や符号が決まる整数型。Suzの演算型と分けて保持する。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CIntegerType {
    Int,
    Uint,
    Long,
    Ulong,
    Size,
    Char,
}
impl CIntegerType {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "_int" => Self::Int,
            "_uint" => Self::Uint,
            "_long" => Self::Long,
            "_ulong" => Self::Ulong,
            "_size_t" => Self::Size,
            "_char" => Self::Char,
            _ => return None,
        })
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Int => "_int",
            Self::Uint => "_uint",
            Self::Long => "_long",
            Self::Ulong => "_ulong",
            Self::Size => "_size_t",
            Self::Char => "_char",
        }
    }
    pub fn c_name(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Uint => "unsigned int",
            Self::Long => "long",
            Self::Ulong => "unsigned long",
            Self::Size => "size_t",
            Self::Char => "char",
        }
    }
    pub fn limits(self) -> (&'static str, &'static str) {
        match self {
            Self::Int => ("INT_MIN", "INT_MAX"),
            Self::Uint => ("0", "UINT_MAX"),
            Self::Long => ("LONG_MIN", "LONG_MAX"),
            Self::Ulong => ("0", "ULONG_MAX"),
            Self::Size => ("0", "SIZE_MAX"),
            Self::Char => ("CHAR_MIN", "CHAR_MAX"),
        }
    }
}
/// 本体をCヘッダーとリンカーから得る、ファイル内の関数宣言。
#[derive(Clone)]
pub(crate) struct ExternFunction {
    pub header: String,
    pub c_name: String,
    pub name: String,
    pub parameters: Vec<ExternParameter>,
    pub result: Type,
    pub location: Location,
}

/// 外部関数の仮引数。型付きCポインタはSuz側では非nullのptr Tとして検査する。
#[derive(Clone)]
pub(crate) struct ExternParameter {
    /// Cが呼び出し後も参照を保持する。蓄積警告の逃避先として使う。
    pub keep: bool,
    pub name: String,
    pub ty: Type,
    pub location: Location,
    /// 型付きポインタの参照先にCのconstを付ける。Suzのptr型自体は変更しない。
    pub read_only: bool,
}
