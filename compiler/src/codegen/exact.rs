use super::{
    ExpressionContext,
    expressions::{c_string, expression},
};
use crate::{
    ast::Type,
    check::{
        CheckedExpression, CheckedHandler, IntegerOperation, Value,
        exact::{Exact, ExactKind},
    },
    numeric::Int,
};
pub(super) fn literal(n: Int) -> String {
    format!(
        "suz_exact_make({}, ((unsigned _BitInt(128))UINT64_C({}) << 64) | UINT64_C({}))",
        n.negative,
        (n.magnitude >> 64) as u64,
        n.magnitude as u64
    )
}
pub(super) fn lift(ty: Type, name: &str) -> String {
    if ty == Type::Exact {
        name.into()
    } else if ty.integer_bounds().is_some_and(|(min, _)| min < 0) {
        format!("suz_exact_i64({name})")
    } else {
        format!("suz_exact_make(false, {name})")
    }
}
pub(super) fn tree(e: &Exact, c: &mut ExpressionContext) -> String {
    match &e.kind {
        ExactKind::Literal(n) => literal(*n),
        ExactKind::Leaf(v) => lift(v.ty, &expression(v, c)),
        ExactKind::Negate(v) => {
            let id = c.temporaries.len();
            c.temporaries.push(Type::Exact);
            let value = tree(v, c);
            format!(
                "(suz_cmp_{id} = {value}, suz_cmp_{id}.failed ? suz_cmp_{id} : suz_exact_neg(suz_cmp_{id}))"
            )
        }
        ExactKind::Binary(op, a, b) => {
            let i = c.temporaries.len();
            c.temporaries.extend([Type::Exact, Type::Exact]);
            let j = i + 1;
            let a = tree(a, c);
            let b = tree(b, c);
            use crate::ast::BinaryOperator::*;
            let result = match op {
                Add | Subtract | Multiply => format!(
                    "suz_exact_{}(suz_cmp_{i}, suz_cmp_{j})",
                    match op {
                        Add => "add",
                        Subtract => "sub",
                        _ => "mul",
                    }
                ),
                Divide | Remainder => format!(
                    "suz_exact_divrem(suz_cmp_{i}, suz_cmp_{j}, {}, {}, {})",
                    matches!(op, Remainder),
                    e.location.line,
                    e.location.column
                ),
                _ => unreachable!(),
            };
            format!(
                "(suz_cmp_{i} = {a}, suz_cmp_{i}.failed ? suz_cmp_{i} : (suz_cmp_{j} = {b}, suz_cmp_{j}.failed ? suz_cmp_{j} : {result}))"
            )
        }
    }
}
pub(super) fn converted(ty: Type, name: &str, c: &mut ExpressionContext) -> String {
    match ty {
        Type::Float(f) => {
            c.float_types.insert(f);
            format!("suz_exact_{}({name})", f.name())
        }
        _ if ty.integer_bounds().is_some_and(|(min, _)| min < 0) => {
            format!("({})suz_exact_signed({name})", ty.c_name())
        }
        _ => format!("({})suz_exact_bits({name})", ty.c_name()),
    }
}
pub(super) fn fits(ty: Type, name: &str) -> String {
    match ty {
        Type::Float(_) => format!("!{name}.failed"),
        Type::Integer(t) => format!("suz_exact_fits({name}, {}, {})", t.bounds().0 < 0, t.bits()),
        Type::Enum(t) => format!(
            "(!{name}.failed && !{name}.negative && {name}.magnitude < {})",
            t.count
        ),
        Type::Char => format!(
            "(!{name}.failed && !{name}.negative && {name}.magnitude <= 0x10ffff && ({name}.magnitude < 0xd800 || {name}.magnitude > 0xdfff))"
        ),
        _ => unreachable!(),
    }
}
pub(super) fn parts(
    expr: &CheckedExpression,
    c: &mut ExpressionContext,
) -> (String, String, String) {
    let Value::IntegerArithmetic {
        operation: IntegerOperation::Exact(e),
        ..
    } = &expr.value
    else {
        unreachable!()
    };
    let id = c.temporaries.len();
    c.temporaries.push(Type::Exact);
    let name = format!("suz_cmp_{id}");
    let value = tree(e, c);
    (
        format!("{name} = {value}"),
        fits(expr.ty, &name),
        converted(expr.ty, &name, c),
    )
}
pub(super) fn stored(expr: &CheckedExpression, c: &mut ExpressionContext) -> String {
    let Value::IntegerArithmetic {
        handler, location, ..
    } = &expr.value
    else {
        unreachable!()
    };
    let id = c.temporaries.len();
    let (setup, condition, value) = parts(expr, c);
    let fallback = match handler {
        Some(CheckedHandler::Value(v)) => expression(v, c),
        Some(CheckedHandler::Return(_)) => unreachable!(),
        None => format!(
            "(suz_panic({}, suz_cmp_{id}.failed ? suz_cmp_{id}.line : {}, suz_cmp_{id}.failed ? suz_cmp_{id}.column : {}, suz_cmp_{id}.failed ? \"integer division by zero [rt.div-zero]\" : \"integer arithmetic overflow [rt.int-overflow]\"), ({})0)",
            c_string(&c.source_name),
            location.line,
            location.column,
            expr.ty.c_name()
        ),
    };
    format!("({setup}, {condition} ? {value} : ({fallback}))")
}

pub(super) fn compare(
    a: Type,
    left: &str,
    b: Type,
    right: &str,
    op: crate::ast::ComparisonOperator,
) -> String {
    use crate::ast::ComparisonOperator::*;
    if matches!((a, b), (Type::Float(_), Type::Float(_))) {
        return format!("{left} {} {right}", op.symbol());
    }
    let (call, reverse) = match (a, b) {
        (Type::Float(_), _) => (
            format!("suz_exact_compare_float({}, {left})", lift(b, right)),
            true,
        ),
        (_, Type::Float(_)) => (
            format!("suz_exact_compare_float({}, {right})", lift(a, left)),
            false,
        ),
        _ => (
            format!("suz_exact_compare({}, {})", lift(a, left), lift(b, right)),
            false,
        ),
    };
    let op = if reverse {
        match op {
            Less => Greater,
            LessEqual => GreaterEqual,
            Greater => Less,
            GreaterEqual => LessEqual,
            op => op,
        }
    } else {
        op
    };
    match op {
        Equal => format!("{call} == 0"),
        NotEqual => format!("{call} != 0"),
        Less => format!("{call} == -1"),
        Greater => format!("{call} == 1"),
        LessEqual => format!("{call} <= 0"),
        GreaterEqual => format!("(unsigned){call} <= 1"),
    }
}
