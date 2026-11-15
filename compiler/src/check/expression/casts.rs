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
            wrap,
            operand,
            target,
            fallback,
        } => {
            let operand = expression(operand, None, bindings)?;
            if !*wrap && operand.ty == Type::Text && *target == Type::Symbol {
                if fallback.is_some() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "text interning does not accept or [text.to-symbol]",
                    ));
                }
                let constant = super::super::constants::evaluate(&operand)
                    .ok()
                    .flatten()
                    .and_then(|value| {
                        if let Value::Text { text } = value.value {
                            Some(text)
                        } else {
                            None
                        }
                    });
                return Ok(CheckedExpression {
                    ty: Type::Symbol,
                    value: Value::Intern {
                        constant,
                        operand: Box::new(operand),
                        location: expr.location,
                    },
                });
            }
            if operand.ty.is_text() {
                let valid = if *wrap {
                    matches!(target, Type::Integer(_))
                } else {
                    matches!(
                        target,
                        Type::Integer(_)
                            | Type::Float(_)
                            | Type::Bool
                            | Type::Char
                            | Type::Struct(_)
                    )
                };
                if !valid {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        if *wrap {
                            "text as% requires an integer target [symbol.cast-wrap]"
                        } else {
                            "unsupported text conversion target [symbol.cast]"
                        },
                    ));
                }
                if fallback.is_none() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "text conversion requires or [symbol.cast-handler]",
                    ));
                }
            } else if *wrap {
                if fallback.is_some() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "integer as% does not accept or [symbol.cast-wrap-handler-syntax]",
                    ));
                }
                if !matches!((operand.ty, target), (Type::Integer(_), Type::Integer(_))) {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "as% requires integer source and target types [expr.as-trunc]",
                    ));
                }
                return Ok(CheckedExpression {
                    ty: *target,
                    value: Value::Truncate(Box::new(operand)),
                });
            }
            let safe = match (operand.ty, *target) {
                (Type::Symbol | Type::Text, _) => false,
                (Type::Char, Type::Integer(to)) => to.bounds().1 >= 0x10ffff,
                (Type::Integer(from), Type::Char) => {
                    let (min, max) = from.bounds();
                    min >= 0 && max < 0xd800
                }
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
                        location: expr.location,
                        wrap: *wrap,
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
            if operand.ty.is_text() {
                let constant_struct = if let Type::Struct(id) = target
                    && super::super::constants::is_constant(fallback.as_ref().unwrap())
                {
                    // Defer errors in unselected constant operands to constant evaluation.
                    super::super::constants::evaluate(&operand)
                        .ok()
                        .flatten()
                        .and_then(|value| {
                            let Value::Text { text, .. } = value.value else {
                                return None;
                            };
                            super::super::text_structure::parse(&text, *id, bindings).map(Box::new)
                        })
                } else {
                    None
                };
                return Ok(CheckedExpression {
                    ty: *target,
                    value: Value::CheckedCast {
                        location: expr.location,
                        constant_struct,
                        wrap: *wrap,
                        operand: Box::new(operand),
                        fallback: Box::new(fallback.unwrap()),
                    },
                });
            }
            if operand.ty.integer_bounds().is_some() && target.integer_bounds().is_some() {
                if let Some(fallback) = fallback {
                    CheckedExpression {
                        ty: *target,
                        value: Value::CheckedCast {
                            location: expr.location,
                            constant_struct: None,
                            wrap: false,
                            operand: Box::new(operand),
                            fallback: Box::new(fallback),
                        },
                    }
                } else {
                    if !safe {
                        let (min, max) = target.integer_bounds().unwrap();
                        match constant_integer(&operand)? {
                            Some(value)
                                if (min..=max).contains(&value)
                                    && (*target != Type::Char
                                        || !(0xd800..=0xdfff).contains(&value)) => {}
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
                            location: expr.location,
                            constant_struct: None,
                            wrap: false,
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
                            location: expr.location,
                            constant_struct: None,
                            wrap: false,
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
                        location: expr.location,
                        constant_struct: None,
                        wrap: false,
                        operand: Box::new(operand),
                        fallback: Box::new(fallback),
                    }
                } else {
                    Value::Cast(Box::new(operand))
                };
                CheckedExpression { ty: *target, value }
            }
        }

        _ => unreachable!(),
    })
}
