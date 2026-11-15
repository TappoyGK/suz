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
            let mut checked = if super::super::exact::is_operator(operand)
                && super::super::exact::integer(operand, bindings)
            {
                super::super::exact::check(operand, expected, bindings, true)?
            } else {
                expression(operand, expected, bindings)?
            };
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
            *handler =
                Some(match fallback.as_ref().unwrap() {
                    FailureHandler::Value(value) => CheckedHandler::Value(Box::new(
                        super::coerced(value, Some(checked.ty), bindings)?,
                    )),
                    FailureHandler::Return { value, .. } => CheckedHandler::Return(Box::new(
                        crate::check::functions::return_value(value, bindings)?,
                    )),
                });
            checked
        }
        ExpressionKind::BitNot(_) | ExpressionKind::Negate(_) | ExpressionKind::Binary { .. } => {
            let ty = expected
                .or_else(|| operand_type(expr, bindings))
                .unwrap_or_else(|| default_type(expr, bindings));
            let ty = if matches!(ty, Type::Float(_)) {
                match operand_type(expr, bindings) {
                    Some(known @ Type::Float(_)) => super::super::exact::join(ty, known).unwrap(),
                    _ => ty,
                }
            } else {
                ty
            };
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
                    if ty == Type::Text {
                        "text does not support arithmetic or bitwise operations [text.operations]"
                    } else if ty == Type::Symbol {
                        "symbol does not support arithmetic or bitwise operations [symbol.operations]"
                    } else {
                        "invalid operand type for this operator [expr.operator-types]"
                    },
                ));
            }
            let value = match &expr.kind {
                ExpressionKind::Negate(inner) => {
                    let operand = Box::new(numeric_operand(inner, ty, bindings)?);
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
                    if operand_type(inner, bindings).is_some_and(|known| known != ty)
                        && !super::super::exact::is_operator(inner)
                    {
                        return Err(inner.location.error(
                            ErrorKind::InvalidSource,
                            "bit-width operands must have the same integer type [width.same-type]",
                        ));
                    }
                    Value::BitNot(Box::new(expression(inner, Some(ty), bindings)?))
                }
                ExpressionKind::Binary {
                    operator,
                    left,
                    right,
                } => {
                    let typed_left = operand_type(left, bindings);
                    let typed_right = operand_type(right, bindings);
                    if matches!(ty, Type::Integer(_)) {
                        for (operand, known) in [(left, typed_left), (right, typed_right)] {
                            if operator.is_shift() && std::ptr::eq(operand, right) {
                                continue;
                            }
                            if known.is_some_and(|t| t != ty)
                                && !super::super::exact::is_operator(operand)
                            {
                                return Err(operand.location.error(ErrorKind::InvalidSource,"bit-width operands must have the same integer type [width.same-type]"));
                            }
                        }
                    }
                    let left = Box::new(numeric_operand(left, ty, bindings)?);
                    let right = Box::new(if operator.is_shift() {
                        super::super::exact::use_value(right, bindings)?
                    } else {
                        numeric_operand(right, ty, bindings)?
                    });
                    if operator.is_shift() && !matches!(right.ty, Type::Integer(_) | Type::Exact) {
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
            locations,
            operators,
            operands,
        } => {
            let ty = super::joined_type(
                operands
                    .iter()
                    .map(|operand| operand_type(operand, bindings)),
            )
            .unwrap_or_else(|| {
                if operands
                    .iter()
                    .any(|operand| has_float_literal(operand, bindings))
                {
                    Type::Float(FloatType::F64)
                } else {
                    Type::Integer(IntegerType::I64)
                }
            });
            let numeric = matches!(ty, Type::Integer(_) | Type::Float(_));
            let float_type = super::joined_type(operands.iter().map(|operand| {
                operand_type(operand, bindings).filter(|ty| matches!(ty, Type::Float(_)))
            }))
            .unwrap_or(Type::Float(FloatType::F64));
            let source_operands = operands;
            let operands = operands
                .iter()
                .map(|operand| {
                    if numeric {
                        if operand_type(operand, bindings).is_none()
                            && has_float_literal(operand, bindings)
                        {
                            expression(operand, Some(float_type), bindings)
                        } else {
                            super::super::exact::comparison_value(operand, bindings)
                        }
                    } else {
                        expression(operand, Some(ty), bindings)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            if numeric
                && operands
                    .iter()
                    .any(|v| !matches!(v.ty, Type::Integer(_) | Type::Float(_) | Type::Exact))
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "numeric comparison requires numeric operands [expr.operator-types]",
                ));
            }
            if ty == Type::Void
                || (matches!(ty, Type::Bool | Type::Enum(_) | Type::Struct(_))
                    && operators.iter().any(|operator| !operator.is_equality()))
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "ordering comparisons require numeric operands [expr.operator-types]",
                ));
            }
            for (index, ((op, pair), location)) in operators
                .iter()
                .zip(operands.windows(2))
                .zip(locations)
                .enumerate()
            {
                if let Some(result) =
                    super::super::exact::constant_comparison(&pair[0], &pair[1], *op)
                    && super::super::generic::comparison_warning(
                        &source_operands[index],
                        &source_operands[index + 1],
                        *op,
                        bindings,
                        result,
                    )
                {
                    bindings
                        .comparison_warnings
                        .borrow_mut()
                        .insert((location.line, location.column), result);
                }
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

// An integer subtree retains exact integer division even inside float arithmetic.
fn numeric_operand(
    expr: &Expression,
    ty: Type,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    if matches!(ty, Type::Float(_))
        && !super::super::exact::is_operator(expr)
        && let Some(from @ Type::Integer(_)) = operand_type(expr, bindings)
        && !super::super::exact::widening(from, ty)
    {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            "integer operand is not losslessly convertible; use as [arith.mix]",
        ));
    }
    let checked = if matches!(ty, Type::Float(_)) {
        super::coerced(expr, Some(ty), bindings)?
    } else {
        expression(expr, Some(ty), bindings)?
    };
    if matches!(ty, Type::Float(_))
        && let Value::IntegerArithmetic {
            operation: IntegerOperation::Exact(e),
            ..
        } = &checked.value
    {
        fn divisions(e: &super::super::exact::Exact, bindings: &Scope) {
            use super::super::exact::ExactKind;
            match &e.kind {
                ExactKind::Binary(op, a, b) => {
                    if matches!(op, BinaryOperator::Divide) {
                        bindings
                            .division_warnings
                            .borrow_mut()
                            .insert((e.location.line, e.location.column));
                    }
                    divisions(a, bindings);
                    divisions(b, bindings);
                }
                ExactKind::Negate(v) => divisions(v, bindings),
                _ => {}
            }
        }
        divisions(e, bindings);
    }
    Ok(checked)
}
