pub(crate) mod visit;

use crate::{Diagnostic, ErrorKind};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Location {
    pub line: usize,
    pub column: usize,
}

impl Location {
    pub fn error(self, kind: ErrorKind, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            kind,
            line: self.line,
            column: self.column,
            message: message.into(),
        }
    }
}

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
        }
    }
    pub fn bits(self) -> u32 {
        match self {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EnumType {
    pub id: usize,
    pub base: IntegerType,
    pub count: usize,
}

#[derive(Clone)]
pub(crate) struct EnumDeclaration {
    pub name: String,
    pub location: Location,
    pub ty: EnumType,
    pub variants: Vec<String>,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Type {
    Struct(usize),
    Enum(EnumType),
    Void,
    Float(FloatType),
    Integer(IntegerType),
    Bool,
    Text,
}

impl Type {
    pub fn integer_bounds(self) -> Option<(i128, i128)> {
        match self {
            Self::Integer(ty) => Some(ty.bounds()),
            Self::Enum(ty) => Some((0, ty.count as i128 - 1)),
            _ => None,
        }
    }
    pub fn integer_storage(self) -> Option<IntegerType> {
        match self {
            Self::Integer(ty) => Some(ty),
            Self::Enum(ty) => Some(ty.base),
            _ => None,
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        if name == "text" {
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
            Self::Integer(ty) => ty.name(),
            Self::Float(ty) => ty.name(),
            Self::Text => "text",
            Self::Bool => "bool",
            Self::Void => "void",
            Self::Enum(_) => "enum",
            Self::Struct(_) => "struct",
        }
    }
    pub fn c_name(self) -> String {
        match self {
            Self::Struct(id) => format!("suz_struct_{id}"),
            Self::Integer(ty) => ty.c_name().into(),
            Self::Float(ty) => ty.c_name().into(),
            Self::Text => "suz_text".into(),
            Self::Bool => "_Bool".into(),
            Self::Void => "void".into(),
            Self::Enum(ty) => ty.base.c_name().into(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct StructField {
    pub name: String,
    pub ty: Type,
    pub location: Location,
}

#[derive(Clone)]
pub(crate) struct StructDeclaration {
    pub name: String,
    pub location: Location,
    pub fields: Vec<StructField>,
    pub start: usize,
    pub end: usize,
}

pub(crate) struct FieldInitializer {
    pub name: String,
    pub value: Expression,
    pub location: Location,
}

pub(crate) struct ConstDeclaration {
    pub name: String,
    pub location: Location,
    pub annotation: Option<Type>,
    pub initializer: Expression,
}

pub(crate) struct Program {
    pub structs: Vec<StructDeclaration>,
    pub enums: Vec<EnumDeclaration>,
    pub constants: Vec<ConstDeclaration>,
    pub functions: Vec<Function>,
}

pub(crate) struct Parameter {
    pub name: String,
    pub ty: Type,
    pub location: Location,
}

pub(crate) struct Function {
    pub keyword_location: Location,
    pub name: String,
    pub location: Location,
    pub parameters: Vec<Parameter>,
    pub result: Type,
    pub statements: Vec<Statement>,
    pub end: Location,
}

pub(crate) struct MatchArm<T = Vec<Statement>> {
    pub pattern: Option<Expression>,
    pub location: Location,
    pub body: T,
}

pub(crate) enum Statement {
    Log {
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
        end: Expression,
        inclusive: bool,
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
        mutable: bool,
        name: String,
        location: Location,
        annotation: Option<Type>,
        initializer: Expression,
    },
    Assign {
        compound: bool,
        name: String,
        fields: Vec<String>,
        location: Location,
        value: Expression,
    },
    Return(Expression),
}

pub(crate) struct Expression {
    pub kind: ExpressionKind,
    pub location: Location,
}

#[derive(Clone)]
pub(crate) enum TemplatePart<T> {
    Text(String),
    Value(T),
}

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
    // Larger levels bind more tightly; addition and subtraction share a level.
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

pub(crate) enum FailureHandler {
    Value(Box<Expression>),
    Return {
        value: Box<Expression>,
        location: Location,
    },
}

pub(crate) enum ExpressionKind {
    Template(Vec<TemplatePart<Expression>>),
    Struct {
        id: usize,
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
        operators: Vec<ComparisonOperator>,
        operands: Vec<Expression>,
    },
    Name(String),
    Group(Box<Expression>),
}
