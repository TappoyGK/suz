//! 正確な整数式の各オペランドと中間値がCの整数型に収まる場合、Cの演算へ変換する。
//!
//! 利用元: [`crate::codegen::exact`]。

use super::*;
use crate::{
    ast::{BinaryOperator, IntegerType},
    numeric::Range,
};

// すべてのオペランドと中間値が選んだC型に収まる必要がある。
// 特に、剰余が小さくてもINT64_MIN % -1はCの符号付き演算では安全にならない。
pub(super) fn ty(e: &Exact) -> Option<Type> {
    if can_fail(e) {
        return None;
    }
    match &e.kind {
        ExactKind::Negate(a) => {
            ty(a)?;
        }
        ExactKind::Binary(_, a, b) => {
            ty(a)?;
            ty(b)?;
        }
        ExactKind::Leaf(v) if v.ty == Type::Exact => return None,
        _ => {}
    }
    operation_type(e)
}

pub(super) fn operation_type(e: &Exact) -> Option<Type> {
    if matches!(
        e.kind,
        ExactKind::Binary(BinaryOperator::EuclideanRemainder, ..)
    ) {
        return None;
    }
    let range = e.range?;
    let operands: Vec<_> = match &e.kind {
        ExactKind::Negate(a) => vec![a.as_ref()],
        ExactKind::Binary(_, a, b) => vec![a.as_ref(), b.as_ref()],
        _ => vec![],
    };
    for integer in [IntegerType::I64, IntegerType::U64] {
        let (lo, hi) = integer.bounds();
        if !range.contained(lo, hi)
            || operands
                .iter()
                .any(|v| v.range.is_none_or(|r| !r.contained(lo, hi)))
        {
            continue;
        }
        if let ExactKind::Binary(BinaryOperator::Divide | BinaryOperator::Remainder, a, b) = &e.kind
        {
            let a = a.range.unwrap();
            let b = b.range.unwrap();
            if integer == IntegerType::I64
                && a.min == Int::from_i128(i64::MIN as i128)
                && b.min <= Int::from_i128(-1)
                && b.max >= Int::from_i128(-1)
            {
                continue;
            }
        }
        return Some(Type::Integer(integer));
    }
    None
}

fn literal(n: Int) -> String {
    if n == Int::from_i128(i64::MIN as i128) {
        "INT64_MIN".into()
    } else if n.negative {
        format!("(-INT64_C({}))", n.magnitude)
    } else {
        format!("UINT64_C({})", n.magnitude)
    }
}

pub(super) fn emit(e: &Exact, ty: Type, c: &mut ExpressionContext) -> String {
    let cast = ty.c_name();
    match &e.kind {
        ExactKind::Literal(n) => format!("(({cast}){})", literal(*n)),
        ExactKind::Leaf(v) => format!("(({cast})({}))", expression(v, c)),
        ExactKind::Negate(a) => {
            let a = emit(a, self::ty(a).unwrap(), c);
            format!("(-({cast})({a}))")
        }
        ExactKind::Binary(op, a, b) => {
            let id = c.temporaries.len();
            c.temporaries.extend([ty, ty]);
            let next = id + 1;
            let a = emit(a, self::ty(a).unwrap(), c);
            let b = emit(b, self::ty(b).unwrap(), c);
            format!(
                "(suz_cmp_{id} = ({cast})({a}), suz_cmp_{next} = ({cast})({b}), suz_cmp_{id} {} suz_cmp_{next})",
                op.c_symbol()
            )
        }
    }
}

pub(super) fn fits(range: Range, native: Type, target: Type, name: &str) -> String {
    let Some((lo, hi)) = target.integer_bounds() else {
        return "true".into();
    };
    if range.max < Int::from_i128(lo) || range.min > Int::from_i128(hi) {
        return "false".into();
    }
    let c_type = native.c_name();
    let mut checks = Vec::new();
    if range.min < Int::from_i128(lo) {
        checks.push(format!(
            "{name} >= ({c_type}){}",
            literal(Int::from_i128(lo))
        ));
    }
    if range.max > Int::from_i128(hi) {
        checks.push(format!(
            "{name} <= ({c_type}){}",
            literal(Int::from_i128(hi))
        ));
    }
    if checks.is_empty() {
        "true".into()
    } else {
        checks.join(" && ")
    }
}
