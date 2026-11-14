use super::super::constants::{constant_float, constant_integer, float_to_integer};
use super::super::ir::{CheckedExpression, Value};
use super::expression;
use crate::ast::{FailureHandler, FloatType};
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
};

pub(super) fn check(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    Ok(match &expr.kind {
        ExpressionKind::Cast {
            operand,
            target,
            fallback,
        } => {
            let operand = expression(operand, None, bindings)?;
            let safe = match (operand.ty, *target) {
                (Type::Integer(from), Type::Integer(to)) => {
                    let (from_min, from_max) = from.bounds();
                    let (to_min, to_max) = to.bounds();
                    from_min >= to_min && from_max <= to_max
                }
                (Type::Enum(from), Type::Integer(to)) => (from.count as i128 - 1) <= to.bounds().1,
                (Type::Integer(_), Type::Enum(_)) => false,
                (Type::Integer(_), Type::Float(_)) => true,
                (Type::Float(from), Type::Float(to)) => from == to || from == FloatType::F32,
                (Type::Float(_), Type::Integer(_)) => false,
                _ => {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "invalid source and target types for as [expr.as]",
                    ));
                }
            };
            if let Some(FailureHandler::Return { value, .. }) = fallback {
                let return_value = crate::check::functions::return_value(value, bindings)?;
                let checked = CheckedExpression {
                    ty: *target,
                    value: Value::CastReturn {
                        operand: Box::new(operand),
                        return_value: Box::new(return_value),
                    },
                };
                if expected.is_some_and(|ty| ty != *target) {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "cast result does not match the expected type [type.expected]",
                    ));
                }
                return Ok(checked);
            }
            let fallback = match fallback {
                Some(FailureHandler::Value(value)) => {
                    Some(expression(value, Some(*target), bindings)?)
                }
                None => None,
                _ => unreachable!(),
            };
            if operand.ty.integer_bounds().is_some() && target.integer_bounds().is_some() {
                if let Some(fallback) = fallback {
                    CheckedExpression {
                        ty: *target,
                        value: Value::CheckedCast {
                            operand: Box::new(operand),
                            fallback: Box::new(fallback),
                        },
                    }
                } else {
                    if !safe {
                        let (min, max) = target.integer_bounds().unwrap();
                        match constant_integer(&operand)? {
                            Some(value) if (min..=max).contains(&value) => {}
                            Some(_) => return Err(expr.location.error(
                                ErrorKind::InvalidSource,
                                "constant cast is out of range and requires or [const.cast-check]",
                            )),
                            None => {
                                return Err(expr.location.error(
                                    ErrorKind::InvalidSource,
                                    "this cast requires or [expr.cast-handler]",
                                ));
                            }
                        }
                    }
                    CheckedExpression {
                        ty: *target,
                        value: Value::Cast(Box::new(operand)),
                    }
                }
            } else if let (Type::Float(_), Type::Integer(to)) = (operand.ty, *target) {
                if let Some(fallback) = fallback {
                    CheckedExpression {
                        ty: *target,
                        value: Value::CheckedCast {
                            operand: Box::new(operand),
                            fallback: Box::new(fallback),
                        },
                    }
                } else {
                    match constant_float(&operand)? {
                        Some(value) if float_to_integer(value, to).is_some() => {}
                        Some(_) => {
                            return Err(expr.location.error(
                                ErrorKind::InvalidSource,
                                "constant cast is out of range and requires or [const.cast-check]",
                            ));
                        }
                        None => {
                            return Err(expr.location.error(
                                ErrorKind::InvalidSource,
                                "this cast requires or [expr.cast-handler]",
                            ));
                        }
                    }
                    CheckedExpression {
                        ty: *target,
                        value: Value::Cast(Box::new(operand)),
                    }
                }
            } else if operand.ty == Type::Float(FloatType::F64)
                && *target == Type::Float(FloatType::F32)
            {
                if let Some(fallback) = fallback {
                    CheckedExpression {
                        ty: *target,
                        value: Value::CheckedCast {
                            operand: Box::new(operand),
                            fallback: Box::new(fallback),
                        },
                    }
                } else {
                    match constant_float(&operand)? {
                        Some(value) if !value.is_finite() || (value as f32).is_finite() => {}
                        Some(_) => {
                            return Err(expr.location.error(
                                ErrorKind::InvalidSource,
                                "constant cast is out of range and requires or [const.cast-check]",
                            ));
                        }
                        None => {
                            return Err(expr.location.error(
                                ErrorKind::InvalidSource,
                                "this cast requires or [expr.cast-handler]",
                            ));
                        }
                    }
                    CheckedExpression {
                        ty: *target,
                        value: Value::Cast(Box::new(operand)),
                    }
                }
            } else {
                debug_assert!(safe);
                let value = if let Some(fallback) = fallback {
                    Value::CheckedCast {
                        operand: Box::new(operand),
                        fallback: Box::new(fallback),
                    }
                } else {
                    Value::Cast(Box::new(operand))
                };
                CheckedExpression { ty: *target, value }
            }
        }

        ExpressionKind::Truncate { operand, target } => {
            let operand = expression(operand, None, bindings)?;
            if !matches!(operand.ty, Type::Integer(_)) {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "as% requires an integer operand [expr.as-trunc]",
                ));
            }
            CheckedExpression {
                ty: Type::Integer(*target),
                value: Value::Truncate(Box::new(operand)),
            }
        }
        _ => unreachable!(),
    })
}
