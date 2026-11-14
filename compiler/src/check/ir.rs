use crate::ast::{BinaryOperator, ComparisonOperator, Location, LogicalOperator, Type};

pub(crate) struct CheckedProgram {
    pub constant_texts: std::collections::BTreeSet<String>,
    pub main_location: Location,
    pub structs: Vec<crate::ast::StructDeclaration>,
    pub enums: Vec<crate::ast::EnumDeclaration>,
    pub functions: Vec<CheckedFunction>,
    pub main_id: usize,
    pub unhandled_arithmetic: Vec<Location>,
}
pub(crate) struct CheckedFunction {
    pub id: usize,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub statements: Vec<CheckedStatement>,
}
#[derive(Clone)]
pub(crate) enum CheckedStatement {
    Log {
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
        end: CheckedExpression,
        inclusive: bool,
        body: Vec<CheckedStatement>,
    },
    ArithmeticReturn {
        id: usize,
        operation: CheckedExpression,
        return_value: CheckedExpression,
    },
    CastReturn {
        location: Location,
        wrap: bool,
        id: usize,
        ty: Type,
        operand: CheckedExpression,
        return_value: CheckedExpression,
    },
    Loop {
        condition: Option<CheckedExpression>,
        body: Vec<CheckedStatement>,
        unconditional: bool,
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
        fields: Vec<usize>,
        id: usize,
        ty: Type,
        value: CheckedExpression,
    },
    Return(CheckedExpression),
}
#[derive(Clone)]
pub(crate) struct CheckedExpression {
    pub ty: Type,
    pub value: Value,
}
#[derive(Clone)]
pub(crate) enum CheckedHandler {
    Value(Box<CheckedExpression>),
    Return(Box<CheckedExpression>),
}
#[derive(Clone)]
pub(crate) enum IntegerOperation {
    Binary {
        operator: BinaryOperator,
        left: Box<CheckedExpression>,
        right: Box<CheckedExpression>,
    },
    Negate(Box<CheckedExpression>),
    // Directly negated literals are range-checked with their sign. Their
    // positive magnitude need not fit the type (for example, -128 as i8).
    NegatedLiteral(i128),
}
impl IntegerOperation {
    pub fn operands(&self) -> impl Iterator<Item = &CheckedExpression> {
        let (first, second) = match self {
            Self::Binary { left, right, .. } => (Some(left.as_ref()), Some(right.as_ref())),
            Self::Negate(operand) => (Some(operand.as_ref()), None),
            Self::NegatedLiteral(_) => (None, None),
        };
        first.into_iter().chain(second)
    }
}
#[derive(Clone)]
pub(crate) enum Value {
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
}
