use super::super::constants::{constant_float, constant_integer, float_to_integer};
use super::super::ir::{CheckedExpression, Value};
use crate::ast::{FailureHandler, FloatType};
use crate::check::scope::Scope;
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, Type},
};

pub(in crate::check) fn check(
    expr: &Expression,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    Ok(match &expr.kind {
        ExpressionKind::Cast {
            wrap,
            operand,
            target,
            fallback,
        } => {
            let generic_dependent = target.generic() || super::generic_dependent(operand, bindings);
            let concrete_target = bindings.resolve_type(*target);
            let target = &concrete_target;
            let operand = super::super::exact::use_value(operand, bindings)?;
            if operand.ty == Type::Exact {
                return super::super::exact::cast(
                    operand,
                    *target,
                    *wrap,
                    fallback,
                    expr.location,
                    bindings,
                )
                .map_err(|mut error| {
                    error.generic_cast = generic_dependent
                        && matches!(error.rule(), "expr.cast-handler" | "const.cast-check");
                    error
                });
            }
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
                            format!(
                                "{} as% requires an integer target [symbol.cast-wrap]",
                                operand.ty.name()
                            )
                        } else {
                            format!(
                                "unsupported {} conversion target [symbol.cast]",
                                operand.ty.name()
                            )
                        },
                    ));
                }
                if fallback.is_none() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        format!(
                            "{} conversion requires or [symbol.cast-handler]",
                            operand.ty.name()
                        ),
                    ));
                }
            } else if *wrap {
                if fallback.is_some() {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "integer as% does not accept or [expr.cast-wrap-handler-syntax]",
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
                (Type::Char, Type::Integer(_)) | (Type::Integer(_), Type::Char) => {
                    let (min, max) = operand.ty.integer_bounds().unwrap();
                    crate::numeric::Range::bounds(min, max).fits_type(*target)
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
            if fallback.is_none() && !operand.ty.is_text() {
                cast_risk(&operand, concrete_target, safe)?
                    .require_handler(expr.location)
                    .map_err(|mut error| {
                        error.generic_cast = generic_dependent;
                        error
                    })?;
            }
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
                return Ok(checked);
            }
            let fallback = match fallback {
                Some(FailureHandler::Value(value)) => {
                    Some(super::coerced(value, Some(*target), bindings)?)
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
                    CheckedExpression {
                        ty: *target,
                        value: Value::Cast(Box::new(operand)),
                    }
                }
            } else if let (Type::Float(_), Type::Integer(_)) = (operand.ty, *target) {
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

// [expr.cast-risk], [arith.cast], [const.cast-check]. The ordinary checker
// and template analysis use this pure classification; no Scope is mutated.
pub(in crate::check) enum CastRisk {
    Safe,
    Dynamic,
    ConstantOutOfRange,
}
impl CastRisk {
    pub(in crate::check) fn require_handler(
        self,
        location: crate::ast::Location,
    ) -> Result<(), Diagnostic> {
        match self {
            Self::Safe => Ok(()),
            Self::Dynamic => Err(location.error(
                ErrorKind::InvalidSource,
                "this cast requires or [expr.cast-handler]",
            )),
            Self::ConstantOutOfRange => Err(location.error(
                ErrorKind::InvalidSource,
                "constant cast is out of range and requires or [const.cast-check]",
            )),
        }
    }
}
pub(in crate::check) fn cast_risk(
    operand: &CheckedExpression,
    target: Type,
    type_safe: bool,
) -> Result<CastRisk, Diagnostic> {
    if type_safe {
        return Ok(CastRisk::Safe);
    }
    let constant_fits = if operand.ty == Type::Exact {
        super::super::exact::constant(operand)
            .map(|n| crate::numeric::Range::point(n).fits_type(target))
    } else if matches!(operand.ty, Type::Float(_)) {
        constant_float(operand)?.map(|n| match target {
            Type::Integer(to) => float_to_integer(n, to).is_some(),
            Type::Float(FloatType::F32) => !n.is_finite() || (n as f32).is_finite(),
            _ => unreachable!("cast target validated before numeric risk classification"),
        })
    } else {
        constant_integer(operand)?.map(|n| {
            crate::numeric::Range::point(crate::numeric::Int::from_i128(n)).fits_type(target)
        })
    };
    Ok(match constant_fits {
        Some(true) => CastRisk::Safe,
        Some(false) => CastRisk::ConstantOutOfRange,
        None => CastRisk::Dynamic,
    })
}
