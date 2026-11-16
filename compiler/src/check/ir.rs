use crate::ast::{BinaryOperator, ComparisonOperator, Location, LogicalOperator, Type};

pub(crate) struct CheckedProgram {
    pub division_warnings: Vec<Location>,
    pub comparison_warnings: Vec<(Location, bool)>,
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
    SliceReturn {
        id: usize,
        operation: CheckedExpression,
        return_value: CheckedExpression,
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
        end: Option<CheckedExpression>,
        inclusive: bool,
        reverse: bool,
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
    Exact(Box<super::exact::Exact>),
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
            Self::NegatedLiteral(_) | Self::Exact(_) => (None, None),
        };
        first.into_iter().chain(second)
    }
}
#[derive(Clone)]
pub(crate) enum Value {
    GenericFloatLiteral(String), // Decimal spelling retained during generic checking only.
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

impl Value {
    // Visit the children admitted by the constant-expression grammar. Exact
    // leaves and value handlers are children too; a return handler is not a
    // constant expression. The caller combines properties in one traversal.
    pub fn visit_constant_children(&self, visit: &mut impl FnMut(&CheckedExpression)) -> bool {
        let handler = |handler: &Option<CheckedHandler>,
                       visit: &mut dyn FnMut(&CheckedExpression)| {
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
            Self::Exact(exact) => exact.leaves(visit),
            Self::IntegerArithmetic {
                operation,
                handler: h,
                ..
            } => {
                match operation {
                    IntegerOperation::Exact(exact) => exact.leaves(visit),
                    _ => operation.operands().for_each(&mut *visit),
                }
                return handler(h, visit);
            }
            Self::Cast(v)
            | Self::Negate(v)
            | Self::Truncate(v)
            | Self::BitNot(v)
            | Self::Not(v)
            | Self::TextView(v)
            | Self::Intern { operand: v, .. }
            | Self::Field { target: v, .. } => visit(v),
            Self::CheckedCast {
                operand,
                fallback,
                constant_struct,
                ..
            } => {
                visit(operand);
                visit(fallback);
                if let Some(value) = constant_struct {
                    visit(value);
                }
            }
            Self::Binary { left, right, .. } | Self::Logical { left, right, .. } => {
                visit(left);
                visit(right);
            }
            Self::Comparison { operands, .. } => operands.iter().for_each(visit),
            Self::Template { parts, .. } => {
                for part in parts {
                    if let crate::ast::TemplatePart::Value(value) = part {
                        visit(value);
                    }
                }
            }
            Self::Struct { fields } => fields.iter().for_each(|(_, v)| visit(v)),
            Self::Slice {
                target,
                start,
                end,
                handler: h,
                ..
            } => {
                visit(target);
                for value in [start, end].into_iter().flatten() {
                    visit(value);
                }
                return handler(h, visit);
            }
            Self::GenericFloatLiteral(_)
            | Self::Integer { .. }
            | Self::Float(_)
            | Self::Bool(_)
            | Self::Text { .. } => {}
            Self::Local(_)
            | Self::Void
            | Self::Call { .. }
            | Self::Block { .. }
            | Self::Match { .. }
            | Self::If { .. }
            | Self::CastReturn { .. } => return false,
        }
        true
    }

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
