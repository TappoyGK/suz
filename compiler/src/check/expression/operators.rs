use super::super::ir::{CheckedExpression, Value};
use super::super::ir::{CheckedHandler, IntegerOperation};
use super::expression;
use super::{default_type, has_float_literal, operand_type};
use crate::ast::{BinaryOperator, FailureHandler, FloatType, IntegerType};
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
        ExpressionKind::ArithmeticHandler { operand, fallback } => {
            let mut checked = expression(operand, expected, bindings)?;
            if let Value::Integer {
                value,
                minus: Some(location),
            } = checked.value
            {
                checked.value = Value::IntegerArithmetic {
                    operation: IntegerOperation::NegatedLiteral(value),
                    location,
                    handler: None,
                };
            }
            let Value::IntegerArithmetic { handler, .. } = &mut checked.value else {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "or requires a fallible integer operation [expr.arithmetic-handler]",
                ));
            };
            if handler.is_some() {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "this operation already has an or handler [expr.arithmetic-handler]",
                ));
            }
            *handler = Some(match fallback.as_ref().unwrap() {
                FailureHandler::Value(value) => {
                    CheckedHandler::Value(Box::new(expression(value, Some(checked.ty), bindings)?))
                }
                FailureHandler::Return { value, .. } => CheckedHandler::Return(Box::new(
                    crate::check::functions::return_value(value, bindings)?,
                )),
            });
            checked
        }
        ExpressionKind::BitNot(_) | ExpressionKind::Negate(_) | ExpressionKind::Binary { .. } => {
            let ty = expected
                .or_else(|| operand_type(expr, bindings))
                .unwrap_or_else(|| default_type(expr));
            let arithmetic = matches!(&expr.kind, ExpressionKind::Negate(_))
                || matches!(&expr.kind, ExpressionKind::Binary { operator, .. } if operator.is_arithmetic());
            let integer_arithmetic = arithmetic && matches!(ty, Type::Integer(_));
            if matches!(&expr.kind, ExpressionKind::Negate(_))
                && matches!(ty, Type::Integer(integer) if integer.bounds().0 == 0)
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "integer negation requires a signed type [expr.operator-types]",
                ));
            }
            let integer_only = matches!(
                &expr.kind,
                ExpressionKind::Binary {
                    operator: BinaryOperator::Remainder
                        | BinaryOperator::ShiftLeft
                        | BinaryOperator::ShiftRight,
                    ..
                }
            );
            if (integer_only && !matches!(ty, Type::Integer(_)))
                || !matches!(
                    (arithmetic, ty),
                    (true, Type::Float(_) | Type::Integer(_)) | (false, Type::Integer(_))
                )
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "invalid operand type for this operator [expr.operator-types]",
                ));
            }
            let value = match &expr.kind {
                ExpressionKind::Negate(inner) => {
                    let operand = Box::new(expression(inner, Some(ty), bindings)?);
                    if integer_arithmetic {
                        Value::IntegerArithmetic {
                            operation: IntegerOperation::Negate(operand),
                            location: expr.location,
                            handler: None,
                        }
                    } else {
                        Value::Negate(operand)
                    }
                }
                ExpressionKind::BitNot(inner) => {
                    Value::BitNot(Box::new(expression(inner, Some(ty), bindings)?))
                }
                ExpressionKind::Binary {
                    operator,
                    left,
                    right,
                } => {
                    let left = Box::new(expression(left, Some(ty), bindings)?);
                    let right = Box::new(expression(
                        right,
                        if operator.is_shift() { None } else { Some(ty) },
                        bindings,
                    )?);
                    if operator.is_shift() && !matches!(right.ty, Type::Integer(_)) {
                        return Err(expr.location.error(
                            ErrorKind::InvalidSource,
                            "shift count requires an integer type [expr.operator-types]",
                        ));
                    }
                    if integer_arithmetic {
                        Value::IntegerArithmetic {
                            operation: IntegerOperation::Binary {
                                operator: *operator,
                                left,
                                right,
                            },
                            location: expr.location,
                            handler: None,
                        }
                    } else {
                        Value::Binary {
                            operator: *operator,
                            left,
                            right,
                        }
                    }
                }
                _ => unreachable!(),
            };
            CheckedExpression { ty, value }
        }
        ExpressionKind::Not(inner) => {
            let operand = expression(inner, None, bindings)?;
            if operand.ty != Type::Bool {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "not requires a bool operand [expr.not]",
                ));
            }
            CheckedExpression {
                ty: Type::Bool,
                value: Value::Not(Box::new(operand)),
            }
        }
        ExpressionKind::Logical {
            operator,
            left,
            right,
        } => {
            let left = expression(left, None, bindings)?;
            let right = expression(right, None, bindings)?;
            if left.ty != Type::Bool || right.ty != Type::Bool {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    format!(
                        "{} requires bool operands [expr.operator-types]",
                        operator.symbol()
                    ),
                ));
            }
            CheckedExpression {
                ty: Type::Bool,
                value: Value::Logical {
                    operator: *operator,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            }
        }
        ExpressionKind::Comparison {
            operators,
            operands,
        } => {
            let ty = operands
                .iter()
                .find_map(|operand| operand_type(operand, bindings))
                .unwrap_or_else(|| {
                    if operands.iter().any(has_float_literal) {
                        Type::Float(FloatType::F64)
                    } else {
                        Type::Integer(IntegerType::I64)
                    }
                });
            let operands = operands
                .iter()
                .map(|operand| expression(operand, Some(ty), bindings))
                .collect::<Result<Vec<_>, _>>()?;
            if ty == Type::Void
                || (matches!(ty, Type::Bool | Type::Enum(_) | Type::Struct(_))
                    && operators.iter().any(|operator| !operator.is_equality()))
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "ordering comparisons require numeric operands [expr.operator-types]",
                ));
            }
            CheckedExpression {
                ty: Type::Bool,
                value: Value::Comparison {
                    operators: operators.clone(),
                    operands,
                },
            }
        }
        _ => unreachable!(),
    })
}
