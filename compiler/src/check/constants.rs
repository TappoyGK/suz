//! 検査済みの定数式を評価し、整数・浮動小数点・文字列の値と評価失敗を求める。
//!
//! 利用元: [`crate::check::declarations`]、[`crate::check::expression`]。
//! 主な対象規則: `[const.evaluation]`。

pub(super) mod generic;
use super::ir::{CheckedExpression, CheckedHandler, IntegerOperation, Value};
use crate::ast::{BinaryOperator, FloatType, IntegerType, Type};
use crate::numeric::Int;
use crate::{Diagnostic, ErrorKind};

// Rustの飽和するfloatからi128への変換は値域の確認だけに使う。
// Suzのすべての整数型の値域はi128の範囲の内側に収まる。
pub(super) fn float_to_integer(value: f64, ty: IntegerType) -> Option<i128> {
    let integer = value.trunc() as i128;
    let (min, max) = ty.bounds();
    (value.is_finite() && (min..=max).contains(&integer)).then_some(integer)
}

pub(super) fn float_to_integer_sat(value: f64, ty: IntegerType) -> i128 {
    if value.is_nan() {
        return 0;
    }
    let (min, max) = ty.bounds();
    (value.trunc() as i128).clamp(min, max)
}

fn integer_result(operator: BinaryOperator, a: i128, b: i128, ty: IntegerType) -> Option<i128> {
    if matches!(operator, BinaryOperator::Divide | BinaryOperator::Remainder)
        && (b == 0 || (a == ty.bounds().0 && b == -1))
    {
        return None;
    }
    if matches!(operator, BinaryOperator::EuclideanRemainder) && b == 0 {
        return None;
    }
    if operator.is_shift() {
        if matches!(operator, BinaryOperator::SaturatingShiftLeft) {
            if b < 0 {
                return None;
            }
            let (min, max) = ty.bounds();
            if a == 0 || b == 0 {
                return Some(a);
            }
            if b >= i128::from(ty.bits()) {
                return Some(if a > 0 { max } else { min });
            }
            let shifted = a.checked_mul(1_i128 << b);
            return Some(match shifted {
                Some(v) => v.clamp(min, max),
                None => {
                    if a > 0 {
                        max
                    } else {
                        min
                    }
                }
            });
        }
        if !(0..i128::from(ty.bits())).contains(&b) {
            return None;
        }
    }
    let value = match operator {
        BinaryOperator::Add => a.checked_add(b),
        BinaryOperator::Subtract => a.checked_sub(b),
        BinaryOperator::Multiply => a.checked_mul(b),
        BinaryOperator::Divide => a.checked_div(b),
        BinaryOperator::Remainder => a.checked_rem(b),
        BinaryOperator::EuclideanRemainder => Int::from_i128(a)
            .rem_euclid(Int::from_i128(b))
            .and_then(Int::to_i128),
        BinaryOperator::ShiftLeft => a.checked_mul(1_i128 << b),
        BinaryOperator::ShiftRight => Some(a >> b),
        BinaryOperator::LogicalShiftRight => {
            let bits = ty.bits();
            let mask = (1_u128 << bits) - 1;
            let shifted = ((a as u128) & mask) >> (b as u32);
            let (min, max) = ty.bounds();
            let value = if min < 0 && shifted > (max as u128) {
                (shifted as i128) - (1_i128 << bits)
            } else {
                shifted as i128
            };
            Some(value)
        }
        _ => unreachable!(),
    }?;
    let (min, max) = ty.bounds();
    (min..=max).contains(&value).then_some(value)
}

fn operation_result(operation: &IntegerOperation, ty: IntegerType) -> Option<i128> {
    match operation {
        IntegerOperation::Exact(e) => {
            let value = super::exact::evaluate(e)?.ok()?;
            let n = value.to_i128()?;
            let (lo, hi) = ty.bounds();
            (lo..=hi).contains(&n).then_some(n)
        }
        IntegerOperation::Binary {
            operator,
            left,
            right,
        } => integer_result(
            *operator,
            constant_integer_raw(left)?,
            constant_integer_raw(right)?,
            ty,
        ),
        IntegerOperation::Negate(operand) => {
            let value = constant_integer_raw(operand)?.checked_neg()?;
            let (min, max) = ty.bounds();
            (min..=max).contains(&value).then_some(value)
        }
        IntegerOperation::NegatedLiteral(value) => Some(*value),
    }
}

// 検査器が受理する整数の定数式を評価する。ローカル束縛と値ブロックは定数式に含めない。
fn constant_integer_raw(expr: &CheckedExpression) -> Option<i128> {
    if expr.ty == Type::Exact {
        return super::exact::constant(expr)?.to_i128();
    }
    if let Value::Field { target, index } = &expr.value {
        return constant_integer_raw(&constant_field(target, *index)?);
    }
    if let Value::Slice { .. } = &expr.value {
        return constant_integer_raw(&slice_value(expr)?);
    }
    let ty = expr.ty.integer_storage()?;
    let wrap = |value: i128| {
        let (min, max) = ty.bounds();
        (value - min).rem_euclid(max - min + 1) + min
    };
    match &expr.value {
        Value::IntegerArithmetic {
            operation, handler, ..
        } => operation_result(operation, ty).or_else(|| match handler {
            Some(CheckedHandler::Value(fallback)) => constant_integer_raw(fallback),
            _ => None,
        }),
        Value::Integer { value, .. } => Some(*value),
        Value::Cast(inner) => match inner.ty {
            Type::Exact => super::exact::constant(inner)?.to_i128(),
            Type::Float(_) => float_to_integer(constant_float_raw(inner)?, ty),
            _ => constant_integer_raw(inner),
        },
        Value::CheckedCast {
            operand,
            fallback,
            kind,
            ..
        } => {
            if operand.ty.is_text() {
                return text_cast(operand, expr.ty, *kind)
                    .and_then(|value| constant_integer_raw(&value))
                    .or_else(|| constant_integer_raw(fallback));
            }
            // 両辺とも定数式の文法に従っていなければならない。
            if matches!(operand.ty, Type::Float(_)) {
                let value = constant_float_raw(operand)?;
                if *kind == crate::ast::CastKind::Saturating {
                    return if value.is_nan() {
                        constant_integer_raw(fallback)
                    } else {
                        Some(float_to_integer_sat(value, ty))
                    };
                }
                return float_to_integer(value, ty).or_else(|| constant_integer_raw(fallback));
            }
            if operand.ty == Type::Exact {
                let value = super::exact::constant(operand)?;
                return if crate::numeric::Range::point(value).fits_type(expr.ty) {
                    value.to_i128()
                } else {
                    constant_integer_raw(fallback)
                };
            }
            let value = constant_integer_raw(operand)?;
            if crate::numeric::Range::point(Int::from_i128(value)).fits_type(expr.ty) {
                Some(value)
            } else {
                constant_integer_raw(fallback)
            }
        }
        Value::SaturatingCast(inner) => {
            let (min, max) = ty.bounds();
            if inner.ty == Type::Exact {
                let n = super::exact::constant(inner)?;
                if n < Int::from_i128(min) {
                    Some(min)
                } else if n > Int::from_i128(max) {
                    Some(max)
                } else {
                    n.to_i128()
                }
            } else if matches!(inner.ty, Type::Float(_)) {
                Some(float_to_integer_sat(constant_float_raw(inner)?, ty))
            } else {
                Some(constant_integer_raw(inner)?.clamp(min, max))
            }
        }
        Value::Truncate(inner) => {
            if inner.ty == Type::Exact {
                let n = super::exact::constant(inner)?;
                let bits = ty.bits();
                let mask = (1u128 << bits) - 1;
                let low = if n.negative {
                    0u128.wrapping_sub(n.magnitude)
                } else {
                    n.magnitude
                } & mask;
                let value = low as i128;
                Some(if ty.bounds().0 < 0 && low & (1u128 << (bits - 1)) != 0 {
                    value - (1i128 << bits)
                } else {
                    value
                })
            } else {
                Some(wrap(constant_integer_raw(inner)?))
            }
        }
        Value::Len(inner) => match evaluate_raw(inner)?.value {
            Value::Text { text } => Some(text.chars().count() as i128),
            Value::Array { lengths, .. } => constant_integer_raw(&lengths[0]),
            _ => None,
        },
        Value::BitNot(inner) => Some(wrap(!constant_integer_raw(inner)?)),
        Value::WrapNegate(inner) => Some(wrap(-constant_integer_raw(inner)?)),
        Value::SaturatingNegate(inner) => {
            let (min, max) = ty.bounds();
            Some((-constant_integer_raw(inner)?).clamp(min, max))
        }
        Value::Binary {
            operator,
            left,
            right,
        } => {
            let left = constant_integer_raw(left)?;
            let right = constant_integer_raw(right)?;
            let (min, max) = ty.bounds();
            match operator {
                BinaryOperator::SaturatingAdd => return Some((left + right).clamp(min, max)),
                BinaryOperator::SaturatingSubtract => return Some((left - right).clamp(min, max)),
                BinaryOperator::SaturatingMultiply => {
                    return Some(match left.checked_mul(right) {
                        Some(v) => v.clamp(min, max),
                        None => {
                            if (left < 0) != (right < 0) {
                                min
                            } else {
                                max
                            }
                        }
                    });
                }
                BinaryOperator::SaturatingShiftLeft => {
                    return integer_result(BinaryOperator::SaturatingShiftLeft, left, right, ty);
                }
                _ => {}
            }
            Some(wrap(match operator {
                BinaryOperator::WrapAdd => left + right,
                BinaryOperator::WrapSubtract => left - right,
                // 整数のラップ変換の結果に影響するのは下位64ビットだけ。
                BinaryOperator::WrapMultiply => (left as u64).wrapping_mul(right as u64) as i128,
                BinaryOperator::WrapShiftLeft => {
                    let shift = (right as u64) as u32 & (ty.bits() - 1);
                    ((left as u64) << shift) as i128
                }
                BinaryOperator::WrapShiftRight => {
                    let shift = (right as u64) as u32 & (ty.bits() - 1);
                    if ty.bounds().0 < 0 {
                        left >> shift
                    } else {
                        ((left as u64) >> shift) as i128
                    }
                }
                BinaryOperator::WrapLogicalShiftRight => {
                    let shift = (right as u64) as u32 & (ty.bits() - 1);
                    let mask = (1_u128 << ty.bits()) - 1;
                    (((left as u128) & mask) >> shift) as i128
                }
                BinaryOperator::And => left & right,
                BinaryOperator::Or => left | right,
                BinaryOperator::Xor => left ^ right,
                _ => return None,
            }))
        }
        _ => None,
    }
}

fn integer_as_float(value: i128, ty: FloatType) -> f64 {
    // f64を経由した二重丸めを避け、直接f32へ変換する。
    match ty {
        FloatType::F32 => (value as f32) as f64,
        FloatType::F64 => value as f64,
    }
}

fn constant_float_raw(expr: &CheckedExpression) -> Option<f64> {
    if let Value::Field { target, index } = &expr.value {
        return constant_float_raw(&constant_field(target, *index)?);
    }
    let Type::Float(ty) = expr.ty else {
        return None;
    };
    let value = match &expr.value {
        Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            handler,
            ..
        } => match super::exact::evaluate(e)? {
            Ok(n) => n.float(ty == FloatType::F32),
            Err(_) => match handler {
                Some(CheckedHandler::Value(v)) => constant_float_raw(v)?,
                _ => return None,
            },
        },
        Value::Float(bits) => {
            return Some(match ty {
                FloatType::F32 => f32::from_bits(*bits as u32) as f64,
                FloatType::F64 => f64::from_bits(*bits),
            });
        }
        Value::Negate(inner) => -constant_float_raw(inner)?,
        Value::Cast(inner) => match inner.ty {
            Type::Exact => super::exact::constant(inner)?.float(ty == FloatType::F32),
            Type::Integer(_) => {
                let integer = constant_integer_raw(inner)?;
                return Some(integer_as_float(integer, ty));
            }
            Type::Float(_) => constant_float_raw(inner)?,
            _ => return None,
        },
        Value::CheckedCast {
            operand,
            fallback,
            kind,
            ..
        } => {
            debug_assert_eq!(*kind, crate::ast::CastKind::Checked);
            if operand.ty.is_text() {
                return text_cast(operand, expr.ty, *kind)
                    .and_then(|value| constant_float_raw(&value))
                    .or_else(|| constant_float_raw(fallback));
            }
            if operand.ty == Type::Exact {
                return Some(super::exact::constant(operand)?.float(ty == FloatType::F32));
            }
            if matches!(operand.ty, Type::Integer(_)) {
                let integer = constant_integer_raw(operand)?;
                return Some(integer_as_float(integer, ty));
            }
            let value = constant_float_raw(operand)?;
            if ty == FloatType::F32 && value.is_finite() && (value as f32).is_infinite() {
                constant_float_raw(fallback)?
            } else {
                value
            }
        }
        Value::Binary {
            operator,
            left,
            right,
        } => {
            let left = constant_float_raw(left)?;
            let right = constant_float_raw(right)?;
            // f32の演算はその精度で評価する。特に除算で、途中にf64の丸めを挟まない。
            match ty {
                FloatType::F32 => {
                    let (left, right) = (left as f32, right as f32);
                    return Some(match operator {
                        BinaryOperator::Add => left + right,
                        BinaryOperator::Subtract => left - right,
                        BinaryOperator::Multiply => left * right,
                        BinaryOperator::Divide => left / right,
                        BinaryOperator::Remainder => left % right,
                        BinaryOperator::EuclideanRemainder => {
                            if !right.is_finite() {
                                f32::NAN
                            } else {
                                let r = left % right;
                                let abs_b = right.abs();
                                if r < 0.0 {
                                    let adjusted = r + abs_b;
                                    if adjusted >= abs_b { 0.0 } else { adjusted }
                                } else if r == 0.0 {
                                    0.0
                                } else {
                                    r
                                }
                            }
                        }
                        _ => return None,
                    } as f64);
                }
                FloatType::F64 => match operator {
                    BinaryOperator::Add => left + right,
                    BinaryOperator::Subtract => left - right,
                    BinaryOperator::Multiply => left * right,
                    BinaryOperator::Divide => left / right,
                    BinaryOperator::Remainder => left % right,
                    BinaryOperator::EuclideanRemainder => {
                        if !right.is_finite() {
                            f64::NAN
                        } else {
                            let r = left % right;
                            let abs_b = right.abs();
                            if r < 0.0 {
                                let adjusted = r + abs_b;
                                if adjusted >= abs_b { 0.0 } else { adjusted }
                            } else if r == 0.0 {
                                0.0
                            } else {
                                r
                            }
                        }
                    }
                    _ => return None,
                },
            }
        }
        _ => return None,
    };
    Some(match ty {
        FloatType::F32 => (value as f32) as f64,
        FloatType::F64 => value,
    })
}

fn constant_bool_raw(expr: &CheckedExpression) -> Option<bool> {
    if let Value::Field { target, index } = &expr.value {
        return constant_bool_raw(&constant_field(target, *index)?);
    }
    use crate::ast::{ComparisonOperator as Op, LogicalOperator};
    use std::cmp::Ordering::{Equal, Greater, Less};
    match &expr.value {
        Value::Bool(value) => Some(*value),
        Value::CheckedCast {
            operand,
            fallback,
            kind,
            ..
        } if operand.ty.is_text() => text_cast(operand, Type::Bool, *kind)
            .and_then(|value| constant_bool_raw(&value))
            .or_else(|| constant_bool_raw(fallback)),
        Value::Not(inner) => Some(!constant_bool_raw(inner)?),
        Value::Logical {
            operator,
            left,
            right,
        } => {
            let left = constant_bool_raw(left)?;
            Some(match operator {
                LogicalOperator::And => left && constant_bool_raw(right)?,
                LogicalOperator::Or => left || constant_bool_raw(right)?,
            })
        }
        Value::Comparison {
            operators,
            operands,
            methods,
            ..
        } => {
            if methods.iter().any(Option::is_some) {
                return None;
            }
            let mut left = evaluate_raw(operands.first()?)?;
            for (operator, operand) in operators.iter().zip(operands.iter().skip(1)) {
                let right = evaluate_raw(operand)?;
                let numeric = matches!(left.ty, Type::Integer(_) | Type::Float(_) | Type::Exact)
                    && matches!(right.ty, Type::Integer(_) | Type::Float(_) | Type::Exact);
                let order = if numeric {
                    match (left.ty, right.ty) {
                        (Type::Float(_), Type::Float(_)) => {
                            constant_float_raw(&left)?.partial_cmp(&constant_float_raw(&right)?)
                        }
                        (Type::Float(_), _) => super::exact::constant(&right)?
                            .compare_float(constant_float_raw(&left)?)
                            .map(|o| o.reverse()),
                        (_, Type::Float(_)) => super::exact::constant(&left)?
                            .compare_float(constant_float_raw(&right)?),
                        _ => Some(
                            super::exact::constant(&left)?.cmp(&super::exact::constant(&right)?),
                        ),
                    }
                } else {
                    match left.ty {
                        Type::StaticFunction(_) | Type::Function(_) => return None,
                        Type::ArrayRef { .. }
                        | Type::Array(_)
                        | Type::Pointer { .. }
                        | Type::CPointer
                        | Type::CInteger(_)
                        | Type::Application(_)
                        | Type::StaticShape(_)
                        | Type::Shape(_)
                        | Type::Parameter(_)
                        | Type::GenericInt
                        | Type::GenericFloat
                        | Type::Numeric(_) => return None,
                        Type::Symbol | Type::Text => {
                            let (Value::Text { text: a, .. }, Value::Text { text: b, .. }) =
                                (&left.value, &right.value)
                            else {
                                return None;
                            };
                            Some(a.cmp(b))
                        }
                        Type::Struct(_) => {
                            let equal = equal_structs(&left, &right)?;
                            let matches = match operator {
                                Op::Equal => equal,
                                Op::NotEqual => !equal,
                                _ => return None,
                            };
                            if !matches {
                                return Some(false);
                            }
                            left = right;
                            continue;
                        }
                        Type::Void | Type::Exact => return None,
                        Type::Integer(_) | Type::Range(_) | Type::Enum(_) | Type::Char => {
                            Some(constant_integer_raw(&left)?.cmp(&constant_integer_raw(&right)?))
                        }
                        Type::Float(_) => {
                            constant_float_raw(&left)?.partial_cmp(&constant_float_raw(&right)?)
                        }
                        Type::Bool => {
                            Some(constant_bool_raw(&left)?.cmp(&constant_bool_raw(&right)?))
                        }
                    }
                };
                let matches = match operator {
                    Op::Equal => order == Some(Equal),
                    Op::NotEqual => order != Some(Equal),
                    Op::Less => order == Some(Less),
                    Op::LessEqual => matches!(order, Some(Less | Equal)),
                    Op::Greater => order == Some(Greater),
                    Op::GreaterEqual => matches!(order, Some(Greater | Equal)),
                };
                if !matches {
                    return Some(false);
                }
                left = right;
            }
            Some(true)
        }
        _ => None,
    }
}

fn constant_field(target: &CheckedExpression, index: usize) -> Option<CheckedExpression> {
    let Value::Struct { fields } = evaluate_raw(target)?.value else {
        return None;
    };
    fields
        .into_iter()
        .find(|(id, _)| *id == index)
        .map(|(_, value)| value)
}

fn equal_structs(left: &CheckedExpression, right: &CheckedExpression) -> Option<bool> {
    let (Value::Struct { fields: left }, Value::Struct { fields: right }) =
        (&left.value, &right.value)
    else {
        return None;
    };
    for (index, left) in left {
        let right = &right.iter().find(|(id, _)| id == index)?.1;
        let equal = constant_bool_raw(&CheckedExpression {
            ty: Type::Bool,
            value: Value::Comparison {
                locations: Vec::new(),
                methods: Vec::new(),
                operators: vec![crate::ast::ComparisonOperator::Equal],
                operands: vec![left.clone(), right.clone()],
            },
        })?;
        if !equal {
            return Some(false);
        }
    }
    Some(true)
}

// 連続する添字・切り出しの1段の結果。orのない途中の段の範囲外は、外側の段のorへ伝える。
enum ArrayStep {
    Value(CheckedExpression),
    OutOfRange,
}

fn array_access(expr: &CheckedExpression) -> Option<CheckedExpression> {
    match array_step(expr)? {
        ArrayStep::Value(value) => Some(value),
        ArrayStep::OutOfRange => None,
    }
}

fn chain_target(target: &CheckedExpression) -> Option<ArrayStep> {
    if matches!(
        target.value,
        Value::ArrayRead { handler: None, .. } | Value::ArrayView { handler: None, .. }
    ) {
        array_step(target)
    } else {
        evaluate_raw(target).map(ArrayStep::Value)
    }
}

fn chain_fallback(handler: &Option<CheckedHandler>) -> Option<ArrayStep> {
    match handler {
        Some(CheckedHandler::Value(v)) => evaluate_raw(v).map(ArrayStep::Value),
        None => Some(ArrayStep::OutOfRange),
        _ => None,
    }
}

// 定数配列の部分参照を求める。反復初期化は展開せず、一つの初期値と寸法を保つ。
fn array_step(expr: &CheckedExpression) -> Option<ArrayStep> {
    if let Value::ArrayView {
        target,
        start,
        end,
        index,
        handler,
        location,
        ..
    } = &expr.value
    {
        // 対象がすでに配列の値なら、全要素を評価し直さず、取り出す範囲だけを評価する。
        let owned;
        let array = if matches!(target.value, Value::Array { .. }) {
            target.as_ref()
        } else {
            owned = match chain_target(target)? {
                ArrayStep::Value(value) => value,
                ArrayStep::OutOfRange => return chain_fallback(handler),
            };
            &owned
        };
        let Value::Array {
            metadata,
            values,
            lengths,
            repeated,
        } = &array.value
        else {
            return None;
        };
        let repeated = *repeated;
        let mut lengths = lengths
            .iter()
            .map(evaluate_raw)
            .collect::<Option<Vec<_>>>()?;
        let mut origins = metadata.origins.clone();
        let length = constant_integer_raw(&lengths[0])?;
        let first = start
            .as_ref()
            .map_or(Some(0), |v| constant_integer_raw(v))?;
        let last = if *index {
            first.checked_add(1)?
        } else {
            end.as_ref()
                .map_or(Some(length), |v| constant_integer_raw(v))?
        };
        if first < 0 || last < first || last > length {
            return chain_fallback(handler);
        }
        let stride = lengths[1..].iter().try_fold(1usize, |n, v| {
            n.checked_mul(constant_integer_raw(v)? as usize)
        })?;
        let values = if repeated {
            values.as_slice()
        } else {
            values.get(first as usize * stride..last as usize * stride)?
        };
        let values = values
            .iter()
            .map(evaluate_raw)
            .collect::<Option<Vec<_>>>()?;
        if *index {
            origins.remove(0);
            lengths.remove(0);
        } else {
            if start.is_some() || end.is_some() {
                origins[0] = (*location, 0);
            }
            lengths[0] = CheckedExpression {
                ty: Type::Integer(IntegerType::Len),
                value: Value::Integer {
                    value: last - first,
                    minus: None,
                },
            };
        }
        let mut count = 1u64;
        let mut constant_counts = Vec::new();
        for length in lengths.iter().rev() {
            count = count.checked_mul(constant_integer_raw(length)? as u64)?;
            constant_counts.push(count);
        }
        return Some(ArrayStep::Value(CheckedExpression {
            ty: expr.ty,
            value: Value::Array {
                metadata: Box::new(super::ir::ArrayMetadata {
                    origins,
                    constant_counts,
                    slots: Vec::new(),
                }),
                values,
                lengths,
                repeated,
            },
        }));
    }
    if let Value::ArrayRead { address, handler } = &expr.value {
        let Value::ArrayElement { target, indices } = &address.value else {
            return None;
        };
        let owned;
        let array = if matches!(target.value, Value::Array { .. }) {
            target.as_ref()
        } else {
            owned = match chain_target(target)? {
                ArrayStep::Value(value) => value,
                ArrayStep::OutOfRange => return chain_fallback(handler),
            };
            &owned
        };
        let Value::Array {
            values,
            lengths,
            repeated,
            ..
        } = &array.value
        else {
            return None;
        };
        let mut offset = 0usize;
        for ((index, _, _), length) in indices.iter().zip(lengths) {
            let index = constant_integer_raw(index)?;
            let length = constant_integer_raw(&evaluate_raw(length)?)?;
            if !(0..length).contains(&index) {
                return chain_fallback(handler);
            }
            offset = offset
                .checked_mul(length as usize)?
                .checked_add(index as usize)?;
        }
        return values
            .get(if *repeated { 0 } else { offset })
            .and_then(evaluate_raw)
            .map(ArrayStep::Value);
    }
    None
}

fn evaluate_raw(expr: &CheckedExpression) -> Option<CheckedExpression> {
    if matches!(
        expr.value,
        Value::ArrayRead { .. } | Value::ArrayView { .. }
    ) {
        return array_access(expr);
    }
    if let Value::With { target, fields } = &expr.value {
        let mut result = evaluate_raw(target)?;
        let Value::Struct { fields: saved } = &mut result.value else {
            return None;
        };
        // 差し替え前の値も含めて左辺を評価した後、記述順に初期値を評価する。
        for (index, value) in fields {
            let value = evaluate_raw(value)?;
            saved.iter_mut().find(|(field, _)| field == index)?.1 = value;
        }
        return Some(result);
    }
    if let Value::Field { target, index } = &expr.value {
        return constant_field(target, *index);
    }
    if let Value::TextView(inner) | Value::Intern { operand: inner, .. } = &expr.value {
        let mut value = evaluate_raw(inner)?;
        value.ty = expr.ty;
        return Some(value);
    }
    if let Value::Slice { .. } = &expr.value {
        return slice_value(expr);
    }
    let value = match expr.ty {
        Type::StaticFunction(_) | Type::Function(_) => match expr.value {
            Value::Function { .. } => expr.value.clone(),
            _ => return None,
        },
        Type::ArrayRef { mutable: false, .. } => match &expr.value {
            Value::Nil => Value::Nil,
            Value::Cast(inner) => {
                let mut value = evaluate_raw(inner)?;
                value.ty = expr.ty;
                return Some(value);
            }
            Value::Array {
                metadata,
                values,
                lengths,
                repeated,
            } => {
                let mut evaluated: Vec<_> =
                    values.iter().map(evaluate_raw).collect::<Option<_>>()?;
                let mut metadata = metadata.clone();
                if !metadata.slots.is_empty() {
                    let mut ordered = vec![None; evaluated.len()];
                    for (value, &slot) in evaluated.into_iter().zip(&metadata.slots) {
                        ordered[slot] = Some(value);
                    }
                    evaluated = ordered.into_iter().map(Option::unwrap).collect();
                    metadata.slots.clear();
                }
                Value::Array {
                    metadata,
                    values: evaluated,
                    lengths: lengths.iter().map(evaluate_raw).collect::<Option<_>>()?,
                    repeated: *repeated,
                }
            }
            _ => return None,
        },
        Type::ArrayRef {
            mutable: true,
            nilable: true,
            ..
        } => match &expr.value {
            Value::Nil => Value::Nil,
            Value::Cast(inner) => {
                let mut value = evaluate_raw(inner)?;
                value.ty = expr.ty;
                return Some(value);
            }
            _ => return None,
        },
        Type::Pointer { .. } if expr.ty.is_nilable() => match &expr.value {
            Value::Nil => Value::Nil,
            Value::Cast(inner) => {
                let mut value = evaluate_raw(inner)?;
                value.ty = expr.ty;
                return Some(value);
            }
            _ => return None,
        },
        Type::ArrayRef { .. }
        | Type::Array(_)
        | Type::Pointer { .. }
        | Type::CPointer
        | Type::CInteger(_)
        | Type::Application(_)
        | Type::StaticShape(_)
        | Type::Shape(_)
        | Type::Parameter(_)
        | Type::GenericInt
        | Type::GenericFloat
        | Type::Numeric(_) => {
            return None;
        }
        Type::Symbol | Type::Text => match &expr.value {
            Value::Text { .. } => expr.value.clone(),
            Value::Template {
                constant: Some(text),
                ..
            } => Value::Text { text: text.clone() },
            _ => return None,
        },
        Type::Struct(_) => {
            if let Value::CheckedCast {
                constant_struct,
                fallback,
                ..
            } = &expr.value
            {
                return constant_struct
                    .as_deref()
                    .cloned()
                    .or_else(|| evaluate_raw(fallback));
            }
            let Value::Struct { fields } = &expr.value else {
                return None;
            };
            Value::Struct {
                fields: fields
                    .iter()
                    .map(|(index, value)| Some((*index, evaluate_raw(value)?)))
                    .collect::<Option<_>>()?,
            }
        }
        Type::Exact => {
            let Value::Exact(e) = &expr.value else {
                return None;
            };
            let n = super::exact::evaluate(e)?.ok()?;
            Value::Exact(Box::new(super::exact::Exact {
                domains: None,
                arithmetic: false,
                kind: super::exact::ExactKind::Literal(n),
                range: Some(crate::numeric::Range::point(n)),
                location: e.location,
            }))
        }
        Type::Void => return None,
        Type::Integer(_) | Type::Range(_) | Type::Enum(_) | Type::Char => Value::Integer {
            value: constant_integer_raw(expr)?,
            minus: None,
        },
        Type::Float(ty) => {
            let value = constant_float_raw(expr)?;
            Value::Float(match ty {
                FloatType::F32 => u64::from((value as f32).to_bits()),
                FloatType::F64 => value.to_bits(),
            })
        }
        Type::Bool => Value::Bool(constant_bool_raw(expr)?),
    };
    Some(CheckedExpression { ty: expr.ty, value })
}

// 文法と型は選択されないオペランドも検査する。以下の評価は選択された経路だけをたどり、
// 最初に失敗した演算を報告する。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConstantKind {
    Nonconstant,
    Scalar,
    Generic,
}

// 定数性と型への依存を同時に分類する。通常の定数評価では各ノードを一度だけ訪問し、
// 汎用型を調べるための再走査をしない。
fn constant_kind(expr: &CheckedExpression) -> ConstantKind {
    let mut kind = if expr.ty.symbolic() || expr.value.symbolic_range() {
        ConstantKind::Generic
    } else {
        ConstantKind::Scalar
    };
    let admitted = expr.value.visit_constant_children(&mut |child| {
        if kind == ConstantKind::Nonconstant {
            return;
        }
        match constant_kind(child) {
            ConstantKind::Nonconstant => kind = ConstantKind::Nonconstant,
            ConstantKind::Generic => kind = ConstantKind::Generic,
            ConstantKind::Scalar => {}
        }
    });
    if admitted {
        kind
    } else {
        ConstantKind::Nonconstant
    }
}

pub(super) fn is_constant(expr: &CheckedExpression) -> bool {
    constant_kind(expr) != ConstantKind::Nonconstant
}

// 結果の型・値域または定数の部分式を通じて、値は汎用型に依存する。途中の具体型への変換では依存は消えない。
// 型・値域の直接検査は、引数やそれを含む正確な整数式などの非定数にも適用する。
// constant_kindは、型が代入に依存していても、これらを意図的にNonconstantへ分類する。
pub(super) fn depends_on_generic(expr: &CheckedExpression) -> bool {
    expr.ty.symbolic()
        || expr.value.symbolic_range()
        || constant_kind(expr) == ConstantKind::Generic
}

fn cast_succeeds(operand: &CheckedExpression, target: Type, kind: crate::ast::CastKind) -> bool {
    if operand.ty == Type::Exact {
        return super::exact::constant(operand).is_some_and(|n| {
            if matches!(target, Type::Float(_)) {
                return true;
            }
            crate::numeric::Range::point(n).fits_type(target)
        });
    }

    if operand.ty.is_text() {
        return text_cast(operand, target, kind).is_some();
    }
    if operand.ty.integer_bounds().is_some() && target.integer_bounds().is_some() {
        let value = constant_integer_raw(operand).unwrap();
        return crate::numeric::Range::point(Int::from_i128(value)).fits_type(target);
    }
    match (operand.ty, target) {
        (Type::Float(_), Type::Integer(to)) => {
            let value = constant_float_raw(operand).unwrap();
            if kind == crate::ast::CastKind::Saturating {
                !value.is_nan()
            } else {
                float_to_integer(value, to).is_some()
            }
        }
        (Type::Float(_), Type::Float(FloatType::F32)) => {
            let value = constant_float_raw(operand).unwrap();
            !value.is_finite() || (value as f32).is_finite()
        }
        _ => true,
    }
}

// 演算失敗のハンドラーは、格納済みオペランド内の失敗を捕捉しない。
// 正確な整数式が既に失敗していれば、それ以上たどらない。
fn validate_exact_leaves(e: &super::exact::Exact) -> Result<(), Diagnostic> {
    use super::exact::ExactKind;
    match &e.kind {
        ExactKind::Leaf(v) => validate_evaluation(v)?,
        ExactKind::Negate(v) => validate_exact_leaves(v)?,
        ExactKind::Binary(_, a, b) => {
            validate_exact_leaves(a)?;
            if !matches!(super::exact::evaluate(a), Some(Err(_))) {
                validate_exact_leaves(b)?;
            }
        }
        ExactKind::Literal(_) => {}
    }
    Ok(())
}

fn validate_evaluation(expr: &CheckedExpression) -> Result<(), Diagnostic> {
    if let Value::Exact(e)
    | Value::IntegerArithmetic {
        operation: IntegerOperation::Exact(e),
        ..
    } = &expr.value
    {
        validate_exact_leaves(e)?;
        let failed = match super::exact::evaluate(e) {
            Some(Err(l)) => Some(l),
            Some(Ok(n)) => {
                if let Some((lo, hi)) = expr.ty.integer_bounds() {
                    (!crate::numeric::Range::point(n).contained(lo, hi)).then_some(e.location)
                } else {
                    None
                }
            }
            None => None,
        };
        if let Some(location) = failed {
            if let Value::IntegerArithmetic {
                handler: Some(CheckedHandler::Value(v)),
                ..
            } = &expr.value
            {
                return validate_evaluation(v);
            }
            return Err(location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!("const.evaluation", "constant integer arithmetic failed"),
            ));
        }
        return Ok(());
    }

    match &expr.value {
        Value::Len(inner) | Value::TextView(inner) | Value::Intern { operand: inner, .. } => {
            validate_evaluation(inner)?
        }
        Value::Slice {
            target,
            start,
            end,
            handler,
            ..
        } => {
            validate_evaluation(target)?;
            for bound in [start, end].into_iter().flatten() {
                validate_evaluation(bound)?;
            }
            if slice_result(expr).is_none()
                && let Some(CheckedHandler::Value(value)) = handler
            {
                validate_evaluation(value)?;
            }
        }
        Value::Template { parts, .. } => {
            for part in parts {
                if let crate::ast::TemplatePart::Value(value) = part {
                    validate_evaluation(value)?;
                }
            }
        }
        Value::With { target, fields } => {
            validate_evaluation(target)?;
            for (_, value) in fields {
                validate_evaluation(value)?;
            }
        }
        Value::Struct { fields } => {
            for (_, value) in fields {
                validate_evaluation(value)?;
            }
        }
        Value::Field { target, .. } => validate_evaluation(target)?,
        Value::IntegerArithmetic {
            operation,
            location,
            handler,
        } => {
            for operand in operation.operands() {
                validate_evaluation(operand)?;
            }
            let Type::Integer(ty) = expr.ty else {
                unreachable!()
            };
            if operation_result(operation, ty).is_none() {
                if let Some(CheckedHandler::Value(fallback)) = handler {
                    return validate_evaluation(fallback);
                }
                return Err(location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!("const.evaluation", "constant integer arithmetic failed"),
                ));
            }
        }
        Value::Cast(inner)
        | Value::Negate(inner)
        | Value::Truncate(inner)
        | Value::SaturatingCast(inner)
        | Value::BitNot(inner)
        | Value::WrapNegate(inner)
        | Value::SaturatingNegate(inner)
        | Value::Not(inner) => validate_evaluation(inner)?,
        Value::CheckedCast {
            operand,
            fallback,
            kind,
            ..
        } => {
            validate_evaluation(operand)?;
            let succeeds = if matches!(expr.ty, Type::Struct(_)) {
                let Value::CheckedCast {
                    constant_struct, ..
                } = &expr.value
                else {
                    unreachable!()
                };
                constant_struct.is_some()
            } else {
                cast_succeeds(operand, expr.ty, *kind)
            };
            if !succeeds {
                validate_evaluation(fallback)?;
            }
        }
        Value::Binary { left, right, .. } => {
            validate_evaluation(left)?;
            validate_evaluation(right)?;
        }
        Value::Logical {
            operator,
            left,
            right,
        } => {
            validate_evaluation(left)?;
            let left = constant_bool_raw(left).unwrap();
            if matches!(operator, crate::ast::LogicalOperator::And) == left {
                validate_evaluation(right)?;
            }
        }
        Value::Comparison {
            operators,
            operands,
            ..
        } => {
            validate_evaluation(&operands[0])?;
            for (operator, pair) in operators.iter().zip(operands.windows(2)) {
                validate_evaluation(&pair[1])?;
                let pair = CheckedExpression {
                    ty: Type::Bool,
                    value: Value::Comparison {
                        locations: Vec::new(),
                        methods: Vec::new(),
                        operators: vec![*operator],
                        operands: pair.to_vec(),
                    },
                };
                if constant_bool_raw(&pair) == Some(false) {
                    break;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn evaluate(expr: &CheckedExpression) -> Result<Option<CheckedExpression>, Diagnostic> {
    // 記号的な定数は通常の評価前に具体化する。結果が具体型でも、子に記号的な値がある場合を含む。
    if constant_kind(expr) != ConstantKind::Scalar {
        return Ok(None);
    }
    validate_evaluation(expr)?;
    Ok(evaluate_raw(expr))
}

pub(super) fn constant_integer(expr: &CheckedExpression) -> Result<Option<i128>, Diagnostic> {
    Ok(evaluate(expr)?.and_then(|value| constant_integer_raw(&value)))
}
pub(super) fn constant_float(expr: &CheckedExpression) -> Result<Option<f64>, Diagnostic> {
    Ok(evaluate(expr)?.and_then(|value| constant_float_raw(&value)))
}

fn text_cast(
    operand: &CheckedExpression,
    target: Type,
    kind: crate::ast::CastKind,
) -> Option<CheckedExpression> {
    let Value::Text { text, .. } = evaluate_raw(operand)?.value else {
        return None;
    };
    super::text_parse::parse(&text, target, kind)
}

fn slice_result(expr: &CheckedExpression) -> Option<CheckedExpression> {
    let Value::Slice {
        target,
        start,
        end,
        index,
        ..
    } = &expr.value
    else {
        unreachable!()
    };
    let Value::Text { text } = evaluate_raw(target)?.value else {
        return None;
    };
    let positions: Vec<_> = text
        .char_indices()
        .map(|(pos, _)| pos)
        .chain(std::iter::once(text.len()))
        .collect();
    let bound = |value: &Option<Box<CheckedExpression>>, default: usize| -> Option<usize> {
        match value {
            Some(value) => usize::try_from(constant_integer_raw(value)?).ok(),
            None => Some(default),
        }
    };
    let first = bound(start, 0)?;
    let value = if *index {
        let byte = *positions.get(first)?;
        Value::Integer {
            value: i128::from(text[byte..].chars().next()? as u32),
            minus: None,
        }
    } else {
        let last = bound(end, positions.len() - 1)?;
        if first > last {
            return None;
        }
        Value::Text {
            text: text[*positions.get(first)?..*positions.get(last)?].to_owned(),
        }
    };
    Some(CheckedExpression { ty: expr.ty, value })
}
fn slice_value(expr: &CheckedExpression) -> Option<CheckedExpression> {
    slice_result(expr).or_else(|| {
        let Value::Slice {
            handler: Some(CheckedHandler::Value(value)),
            ..
        } = &expr.value
        else {
            return None;
        };
        evaluate_raw(value)
    })
}
