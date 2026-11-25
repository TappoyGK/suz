//! 正確な整数式の木をCへ変換する。値域に応じた表現、格納時の検査、整数と浮動小数点の比較を扱う。
//!
//! 利用元: [`crate::codegen::expressions`]、[`crate::codegen::arithmetic`]。

mod native;
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
fn can_fail(e: &Exact) -> bool {
    match &e.kind {
        ExactKind::Negate(a) => can_fail(a),
        ExactKind::Binary(op, a, b) => {
            can_fail(a)
                || can_fail(b)
                || (matches!(
                    op,
                    crate::ast::BinaryOperator::Divide
                        | crate::ast::BinaryOperator::Remainder
                        | crate::ast::BinaryOperator::EuclideanRemainder
                ) && b
                    .range
                    .is_none_or(|r| r.min <= Int::ZERO && r.max >= Int::ZERO))
        }
        _ => false,
    }
}
pub(super) fn tree(e: &Exact, c: &mut ExpressionContext) -> String {
    if let Some(ty) = native::ty(e) {
        return lift(ty, &native::emit(e, ty, c));
    }
    match &e.kind {
        ExactKind::Literal(n) => literal(*n),
        ExactKind::Leaf(v) => lift(v.ty, &expression(v, c)),
        ExactKind::Negate(v) => {
            if !can_fail(v) {
                let value = tree(v, c);
                return format!("suz_exact_neg({value})");
            }
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
            let left_fails = can_fail(a);
            let right_fails = can_fail(b);
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
                EuclideanRemainder => format!(
                    "suz_exact_euclid(suz_cmp_{i}, suz_cmp_{j}, {}, {})",
                    e.location.line, e.location.column
                ),
                _ => unreachable!(),
            };
            let result = if let Some(ty) = native::operation_type(e) {
                let left = converted(ty, &format!("suz_cmp_{i}"), c);
                let right = converted(ty, &format!("suz_cmp_{j}"), c);
                let native = if matches!(op, EuclideanRemainder)
                    && ty == Type::Integer(crate::ast::IntegerType::I64)
                {
                    let rem = c.temporaries.len();
                    let rhs = rem + 1;
                    c.temporaries.extend([ty, ty]);
                    lift(
                        ty,
                        &format!(
                            "(suz_cmp_{rhs} = ({right}), suz_cmp_{rem} = ({left}) % suz_cmp_{rhs}, suz_cmp_{rem} < 0 ? suz_cmp_{rem} + (suz_cmp_{rhs} < 0 ? -suz_cmp_{rhs} : suz_cmp_{rhs}) : suz_cmp_{rem})"
                        ),
                    )
                } else {
                    lift(ty, &format!("({left} {} {right})", op.c_symbol()))
                };
                if matches!(op, Divide | Remainder | EuclideanRemainder) {
                    format!("(suz_cmp_{j}.magnitude == 0 ? {result} : {native})")
                } else {
                    native
                }
            } else {
                result
            };
            let right = if right_fails {
                format!("(suz_cmp_{j} = {b}, suz_cmp_{j}.failed ? suz_cmp_{j} : {result})")
            } else {
                format!("(suz_cmp_{j} = {b}, {result})")
            };
            if left_fails {
                format!("(suz_cmp_{i} = {a}, suz_cmp_{i}.failed ? suz_cmp_{i} : {right})")
            } else {
                format!("(suz_cmp_{i} = {a}, {right})")
            }
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
    if let Some(ty) = native::ty(e) {
        c.temporaries.push(ty);
        let name = format!("suz_cmp_{id}");
        let value = native::emit(e, ty, c);
        return (
            format!("{name} = {value}"),
            native::fits(e.range.unwrap(), ty, expr.ty, &name),
            format!("({}){name}", expr.ty.c_name()),
        );
    }
    c.temporaries.push(Type::Exact);
    let name = format!("suz_cmp_{id}");
    let value = tree(e, c);
    (
        format!("{name} = {value}"),
        if e.range
            .is_some_and(|r| matches!(expr.ty, Type::Float(_)) || r.fits_type(expr.ty))
        {
            if can_fail(e) {
                format!("!{name}.failed")
            } else {
                "true".into()
            }
        } else {
            fits(expr.ty, &name)
        },
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
    if condition == "true" {
        return format!("({setup}, {value})");
    }
    let native = c.temporaries[id] != Type::Exact;
    let fallback = match handler {
        Some(CheckedHandler::Value(v)) => expression(v, c),
        Some(CheckedHandler::Return { .. }) => unreachable!(),
        None if native => format!(
            "(suz_panic({}, {}, {}, \"integer arithmetic overflow [rt.int-overflow]\"), ({})0)",
            c_string(&c.source_name),
            location.line,
            location.column,
            expr.ty.c_name()
        ),
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
    if (a == b && matches!(a, Type::Integer(_)))
        || matches!((a, b), (Type::Float(_), Type::Float(_)))
    {
        return format!("{left} {} {right}", op.symbol());
    }
    if let (Type::Integer(a), Type::Integer(b)) = (a, b) {
        let signed_a = a.c_signed();
        let signed_b = b.c_signed();
        // 非負の値同士は符号なしにそろえる。32ビットに収まる比較を
        // 64ビットへ広げず、len_tのint32_tからの不要な符号拡張を避ける。
        if a.bounds().0 == 0 && b.bounds().0 == 0 && signed_a != signed_b {
            let c_type = if a.bounds().1 <= u32::MAX as i128 && b.bounds().1 <= u32::MAX as i128 {
                "uint32_t"
            } else {
                "uint64_t"
            };
            return format!("({c_type}){left} {} ({c_type}){right}", op.symbol());
        }
        if signed_a == signed_b {
            return format!("{left} {} {right}", op.symbol());
        }
        if a.bounds().1 <= i64::MAX as i128 && b.bounds().1 <= i64::MAX as i128 {
            return format!("(int64_t){left} {} (int64_t){right}", op.symbol());
        }
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
