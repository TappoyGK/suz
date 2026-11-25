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
    if power_of_two_euclid(e).is_some() {
        return Some(Type::Integer(IntegerType::I64));
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

/// 2のべき乗 M (1 <= M <= 2^63) に対する `a %% M` は、a が加減乗・単項マイナスと
/// 固定幅整数葉だけからなる場合、Z -> Z/2^64Z -> Z/MZ の環準同型により
/// uint64_t 上のラップ演算の下位ビットマスクと数学的に一致する。
fn power_of_two_euclid(e: &Exact) -> Option<u64> {
    let ExactKind::Binary(BinaryOperator::EuclideanRemainder, a, b) = &e.kind else {
        return None;
    };
    let ExactKind::Literal(m) = b.kind else {
        return None;
    };
    if m.negative || !m.magnitude.is_power_of_two() || m.magnitude > (1u128 << 63) {
        return None;
    }
    mod64_compatible(a).then_some(m.magnitude as u64)
}

fn mod64_compatible(e: &Exact) -> bool {
    match &e.kind {
        ExactKind::Literal(_) => true,
        ExactKind::Leaf(v) => v.ty != Type::Exact,
        ExactKind::Negate(a) => mod64_compatible(a),
        ExactKind::Binary(
            BinaryOperator::Add | BinaryOperator::Subtract | BinaryOperator::Multiply,
            a,
            b,
        ) => mod64_compatible(a) && mod64_compatible(b),
        _ => false,
    }
}

fn emit_mod64(e: &Exact, c: &mut ExpressionContext) -> String {
    match &e.kind {
        ExactKind::Literal(n) => {
            let low = if n.negative {
                0u64.wrapping_sub(n.magnitude as u64)
            } else {
                n.magnitude as u64
            };
            format!("UINT64_C({low})")
        }
        ExactKind::Leaf(v) => {
            let expr = expression(v, c);
            if v.ty.integer_bounds().is_some_and(|(min, _)| min < 0) {
                format!("((uint64_t)(int64_t)({expr}))")
            } else {
                format!("((uint64_t)({expr}))")
            }
        }
        ExactKind::Negate(a) => format!("(UINT64_C(0) - {})", emit_mod64(a, c)),
        ExactKind::Binary(op, a, b) => {
            let id = c.temporaries.len();
            c.temporaries.extend([
                Type::Integer(IntegerType::U64),
                Type::Integer(IntegerType::U64),
            ]);
            let next = id + 1;
            let a = emit_mod64(a, c);
            let b = emit_mod64(b, c);
            format!(
                "(suz_cmp_{id} = {a}, suz_cmp_{next} = {b}, suz_cmp_{id} {} suz_cmp_{next})",
                op.c_symbol()
            )
        }
    }
}

pub(super) fn operation_type(e: &Exact) -> Option<Type> {
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
        if let ExactKind::Binary(
            op @ (BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::EuclideanRemainder),
            a,
            b,
        ) = &e.kind
        {
            let a = a.range.unwrap();
            let b = b.range.unwrap();
            if integer == IntegerType::I64
                && ((a.min == Int::from_i128(i64::MIN as i128)
                    && b.min <= Int::from_i128(-1)
                    && b.max >= Int::from_i128(-1))
                    || (matches!(op, BinaryOperator::EuclideanRemainder)
                        && b.min == Int::from_i128(i64::MIN as i128)))
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
    if let Some(modulus) = power_of_two_euclid(e) {
        let ExactKind::Binary(_, a, _) = &e.kind else {
            unreachable!()
        };
        let mask = modulus - 1;
        return format!("(({cast})({} & UINT64_C({mask})))", emit_mod64(a, c));
    }
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
            if matches!(op, BinaryOperator::EuclideanRemainder)
                && ty == Type::Integer(IntegerType::I64)
            {
                let rem = c.temporaries.len();
                c.temporaries.push(ty);
                format!(
                    "(suz_cmp_{id} = ({cast})({a}), suz_cmp_{next} = ({cast})({b}), suz_cmp_{rem} = suz_cmp_{id} % suz_cmp_{next}, suz_cmp_{rem} < 0 ? suz_cmp_{rem} + (suz_cmp_{next} < 0 ? -suz_cmp_{next} : suz_cmp_{next}) : suz_cmp_{rem})"
                )
            } else {
                format!(
                    "(suz_cmp_{id} = ({cast})({a}), suz_cmp_{next} = ({cast})({b}), suz_cmp_{id} {} suz_cmp_{next})",
                    op.c_symbol()
                )
            }
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
