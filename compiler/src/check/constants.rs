use super::ir::{CheckedExpression, CheckedHandler, IntegerOperation, Value};
use crate::ast::{BinaryOperator, FloatType, IntegerType, Type};
use crate::{Diagnostic, ErrorKind};

// Rust's saturating float-to-i128 conversion is used only for the range
// check; every Suz integer range is strictly inside i128's bounds.
pub(super) fn float_to_integer(value: f64, ty: IntegerType) -> Option<i128> {
    let integer = value.trunc() as i128;
    let (min, max) = ty.bounds();
    (value.is_finite() && (min..=max).contains(&integer)).then_some(integer)
}

fn integer_result(operator: BinaryOperator, a: i128, b: i128, ty: IntegerType) -> Option<i128> {
    if matches!(operator, BinaryOperator::Divide | BinaryOperator::Remainder)
        && (b == 0 || (a == ty.bounds().0 && b == -1))
    {
        return None;
    }
    if operator.is_shift() && !(0..i128::from(ty.bits())).contains(&b) {
        return None;
    }
    let value = match operator {
        BinaryOperator::Add => a.checked_add(b),
        BinaryOperator::Subtract => a.checked_sub(b),
        BinaryOperator::Multiply => a.checked_mul(b),
        BinaryOperator::Divide => a.checked_div(b),
        BinaryOperator::Remainder => a.checked_rem(b),
        BinaryOperator::ShiftLeft => a.checked_mul(1_i128 << b),
        BinaryOperator::ShiftRight => Some(a >> b),
        _ => unreachable!(),
    }?;
    let (min, max) = ty.bounds();
    (min..=max).contains(&value).then_some(value)
}

fn operation_result(operation: &IntegerOperation, ty: IntegerType) -> Option<i128> {
    match operation {
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

// Evaluate the integer constant-expression forms supported by the checker.
// Local bindings and value blocks are not constant expressions.
fn constant_integer_raw(expr: &CheckedExpression) -> Option<i128> {
    if let Value::Field { target, index } = &expr.value {
        return constant_integer_raw(&constant_field(target, *index)?);
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
            Type::Float(_) => float_to_integer(constant_float_raw(inner)?, ty),
            _ => constant_integer_raw(inner),
        },
        Value::CheckedCast {
            operand,
            fallback,
            wrap,
            ..
        } => {
            if operand.ty == Type::Text {
                return text_cast(operand, expr.ty, *wrap)
                    .and_then(|value| constant_integer_raw(&value))
                    .or_else(|| constant_integer_raw(fallback));
            }
            // Both sides must belong to the constant-expression grammar.
            if matches!(operand.ty, Type::Float(_)) {
                let value = constant_float_raw(operand)?;
                return float_to_integer(value, ty).or_else(|| constant_integer_raw(fallback));
            }
            let value = constant_integer_raw(operand)?;
            let (min, max) = expr.ty.integer_bounds()?;
            if (min..=max).contains(&value) {
                Some(value)
            } else {
                constant_integer_raw(fallback)
            }
        }
        Value::Truncate(inner) => Some(wrap(constant_integer_raw(inner)?)),
        Value::BitNot(inner) => Some(wrap(!constant_integer_raw(inner)?)),
        Value::Binary {
            operator,
            left,
            right,
        } => {
            let left = constant_integer_raw(left)?;
            let right = constant_integer_raw(right)?;
            Some(wrap(match operator {
                BinaryOperator::WrapAdd => left + right,
                BinaryOperator::WrapSubtract => left - right,
                // Only the low 64 bits can affect an integer wrapping result.
                BinaryOperator::WrapMultiply => (left as u64).wrapping_mul(right as u64) as i128,
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
    // Convert directly to f32, avoiding a second rounding through f64.
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
        Value::Float(bits) => {
            return Some(match ty {
                FloatType::F32 => f32::from_bits(*bits as u32) as f64,
                FloatType::F64 => f64::from_bits(*bits),
            });
        }
        Value::Negate(inner) => -constant_float_raw(inner)?,
        Value::Cast(inner) => match inner.ty {
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
            wrap,
            ..
        } => {
            debug_assert!(!wrap);
            if operand.ty == Type::Text {
                return text_cast(operand, expr.ty, false)
                    .and_then(|value| constant_float_raw(&value))
                    .or_else(|| constant_float_raw(fallback));
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
            // Evaluate f32 operations at their own precision, without an
            // intermediate f64 rounding (notably for division).
            match ty {
                FloatType::F32 => {
                    let (left, right) = (left as f32, right as f32);
                    return Some(match operator {
                        BinaryOperator::Add => left + right,
                        BinaryOperator::Subtract => left - right,
                        BinaryOperator::Multiply => left * right,
                        BinaryOperator::Divide => left / right,
                        _ => return None,
                    } as f64);
                }
                FloatType::F64 => match operator {
                    BinaryOperator::Add => left + right,
                    BinaryOperator::Subtract => left - right,
                    BinaryOperator::Multiply => left * right,
                    BinaryOperator::Divide => left / right,
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
            wrap,
            ..
        } if operand.ty == Type::Text => text_cast(operand, Type::Bool, *wrap)
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
        } => {
            let mut left = evaluate_raw(operands.first()?)?;
            for (operator, operand) in operators.iter().zip(operands.iter().skip(1)) {
                let right = evaluate_raw(operand)?;
                let order = match left.ty {
                    Type::Text => {
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
                    Type::Void => return None,
                    Type::Integer(_) | Type::Enum(_) => {
                        Some(constant_integer_raw(&left)?.cmp(&constant_integer_raw(&right)?))
                    }
                    Type::Float(_) => {
                        constant_float_raw(&left)?.partial_cmp(&constant_float_raw(&right)?)
                    }
                    Type::Bool => Some(constant_bool_raw(&left)?.cmp(&constant_bool_raw(&right)?)),
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

fn evaluate_raw(expr: &CheckedExpression) -> Option<CheckedExpression> {
    if let Value::Field { target, index } = &expr.value {
        return constant_field(target, *index);
    }
    let value = match expr.ty {
        Type::Text => match &expr.value {
            Value::Text { .. } => expr.value.clone(),
            Value::Template {
                constant: Some(text),
                location,
                ..
            } => Value::Text {
                text: text.clone(),
                location: *location,
            },
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
        Type::Void => return None,
        Type::Integer(_) | Type::Enum(_) => Value::Integer {
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

// Grammar and type checking cover unselected operands too; evaluation below
// visits only the selected path and reports the first failing operation.
pub(super) fn is_constant(expr: &CheckedExpression) -> bool {
    match &expr.value {
        Value::Struct { fields } => fields.iter().all(|(_, value)| is_constant(value)),
        Value::Field { target, .. } => is_constant(target),
        Value::Text { .. } => true,
        Value::Template { parts, .. } => parts.iter().all(|part| match part {
            crate::ast::TemplatePart::Text(_) => true,
            crate::ast::TemplatePart::Value(value) => is_constant(value),
        }),
        Value::Integer { .. } | Value::Float(_) | Value::Bool(_) => true,
        Value::Cast(inner)
        | Value::Negate(inner)
        | Value::Truncate(inner)
        | Value::BitNot(inner)
        | Value::Not(inner) => is_constant(inner),
        Value::CheckedCast {
            operand, fallback, ..
        } => is_constant(operand) && is_constant(fallback),
        Value::IntegerArithmetic {
            operation, handler, ..
        } => {
            operation.operands().all(is_constant)
                && match handler {
                    None => true,
                    Some(CheckedHandler::Value(value)) => is_constant(value),
                    Some(CheckedHandler::Return(_)) => false,
                }
        }
        Value::Binary { left, right, .. } | Value::Logical { left, right, .. } => {
            is_constant(left) && is_constant(right)
        }
        Value::Comparison { operands, .. } => operands.iter().all(is_constant),
        _ => false,
    }
}

fn cast_succeeds(operand: &CheckedExpression, target: Type, wrap: bool) -> bool {
    if operand.ty == Type::Text {
        return text_cast(operand, target, wrap).is_some();
    }
    if operand.ty.integer_bounds().is_some()
        && let Some((min, max)) = target.integer_bounds()
    {
        return (min..=max).contains(&constant_integer_raw(operand).unwrap());
    }
    match (operand.ty, target) {
        (Type::Float(_), Type::Integer(to)) => {
            float_to_integer(constant_float_raw(operand).unwrap(), to).is_some()
        }
        (Type::Float(_), Type::Float(FloatType::F32)) => {
            let value = constant_float_raw(operand).unwrap();
            !value.is_finite() || (value as f32).is_finite()
        }
        _ => true,
    }
}

fn validate_evaluation(expr: &CheckedExpression) -> Result<(), Diagnostic> {
    match &expr.value {
        Value::Template { parts, .. } => {
            for part in parts {
                if let crate::ast::TemplatePart::Value(value) = part {
                    validate_evaluation(value)?;
                }
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
                    "constant integer arithmetic failed [const.evaluation]",
                ));
            }
        }
        Value::Cast(inner)
        | Value::Negate(inner)
        | Value::Truncate(inner)
        | Value::BitNot(inner)
        | Value::Not(inner) => validate_evaluation(inner)?,
        Value::CheckedCast {
            operand,
            fallback,
            wrap,
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
                cast_succeeds(operand, expr.ty, *wrap)
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
        } => {
            validate_evaluation(&operands[0])?;
            for (operator, pair) in operators.iter().zip(operands.windows(2)) {
                validate_evaluation(&pair[1])?;
                let pair = CheckedExpression {
                    ty: Type::Bool,
                    value: Value::Comparison {
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
    if !is_constant(expr) {
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

fn text_cast(operand: &CheckedExpression, target: Type, wrap: bool) -> Option<CheckedExpression> {
    let Value::Text { text, .. } = evaluate_raw(operand)?.value else {
        return None;
    };
    super::text_parse::parse(&text, target, wrap)
}
