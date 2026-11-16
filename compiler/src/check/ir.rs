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
// rustfmt does not format this macro body; keep its layout consistent manually.
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

// Keep the read-only and mutable constant grammar in one definition. The
// mutability and visitor methods are the only differences between them.
// rustfmt does not format this macro body; keep its layout consistent manually.
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
                Self::Cast(v) | Self::Negate(v) | Self::Truncate(v) | Self::BitNot(v)
                | Self::Not(v) | Self::TextView(v) | Self::Intern { operand: v, .. }
                | Self::Field { target: v, .. } => visit(v),
                // constant_struct is a derived, concrete cache of parsing the
                // operand, not a source operand. It cannot contain generic types:
                // struct fields have concrete types ([num.places]).
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
                Self::GenericFloatLiteral(_) | Self::Integer { .. } | Self::Float(_)
                | Self::Bool(_) | Self::Text { .. } => {}
                Self::Local(_) | Self::Void | Self::Call { .. } | Self::Block { .. }
                | Self::Match { .. } | Self::If { .. } | Self::CastReturn { .. } => return false,
            }
            true
        }
    };
}

impl Value {
    // Exact leaves and value handlers are children too; return handlers are
    // outside the constant-expression grammar.
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
