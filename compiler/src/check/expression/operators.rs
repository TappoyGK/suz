//! 単項・二項演算のオペランド型と整数幅を検査し、整数の正確な演算と浮動小数点演算を区別する。
//!
//! 利用元: [`crate::check::expression`]。
//! 主な対象規則: `[arith.mix]`、`[expr.arithmetic-handler]`、`[expr.not]`、`[expr.operator-types]`、`[symbol.operations]`、`[text.operations]`。

use super::super::ir::{CheckedExpression, Value};
use super::super::ir::{CheckedHandler, IntegerOperation};
use super::expression;
use super::{default_type, has_float_source, operand_type};
use crate::ast::{BinaryOperator, FailureHandler, FloatType, IntegerType};
use crate::check::generic::domain;
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
            let mut call = operand.as_ref();
            while let ExpressionKind::Group(inner) = &call.kind {
                call = inner;
            }
            match &call.kind {
                ExpressionKind::MethodCall { .. } => {
                    return super::super::methods::call(
                        call,
                        expected,
                        bindings,
                        fallback.as_ref(),
                    );
                }
                ExpressionKind::Call {
                    name, arguments, ..
                } => {
                    return super::super::functions::call(
                        call,
                        name,
                        arguments,
                        expected,
                        bindings,
                        fallback.as_ref(),
                    );
                }
                ExpressionKind::Invoke { target, arguments } => {
                    return super::super::function_values::invoke(
                        call,
                        target,
                        arguments,
                        bindings,
                        fallback.as_ref(),
                    );
                }
                _ => {}
            }
            if bindings.has_operator_methods
                && super::super::operator_methods::is_call(call, bindings)
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "operator.arith-boundary",
                        "operator method calls cannot have an or handler"
                    ),
                ));
            }
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
                    crate::diagnostic!(
                        "expr.arithmetic-handler",
                        "or requires a fallible integer operation"
                    ),
                ));
            };
            if handler.is_some() {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.arithmetic-handler",
                        "this operation already has an or handler"
                    ),
                ));
            }
            *handler = Some(match fallback.as_ref().unwrap() {
                FailureHandler::Ignore(ignore) => {
                    return Err(crate::check::errors::invalid_handler(ignore.location));
                }
                FailureHandler::Errors(handler) => {
                    return Err(crate::check::errors::invalid_handler(handler.location));
                }
                FailureHandler::Fail { location, .. } => {
                    return Err(super::super::errors::invalid_handler(*location));
                }
                FailureHandler::Value(value) => {
                    let exits = matches!(value.kind, ExpressionKind::Panic(_));
                    CheckedHandler::Value(Box::new(super::super::arrays::bounds::with_fallback(
                        bindings,
                        exits,
                        || super::coerced(value, Some(checked.ty), bindings),
                    )?))
                }
                FailureHandler::Return { value, location } => CheckedHandler::Return {
                    value: Box::new(super::super::arrays::bounds::with_fallback(
                        bindings,
                        true,
                        || crate::check::functions::return_value(value, bindings),
                    )?),
                    location: *location,
                },
            });
            checked
        }
        ExpressionKind::BitNot(_)
        | ExpressionKind::WrapNegate(_)
        | ExpressionKind::SaturatingNegate(_)
        | ExpressionKind::Negate(_)
        | ExpressionKind::Binary { .. } => {
            let ty = expected
                .or_else(|| operand_type(expr, bindings))
                .unwrap_or_else(|| default_type(expr, bindings));
            let ty = if ty.is_float() {
                match operand_type(expr, bindings) {
                    Some(known) if known.is_float() => {
                        super::super::exact::join(ty, known).unwrap()
                    }
                    _ => ty,
                }
            } else {
                ty
            };
            let arithmetic = matches!(&expr.kind, ExpressionKind::Negate(_))
                || matches!(&expr.kind, ExpressionKind::Binary { operator, .. } if operator.is_arithmetic());
            let integer_arithmetic = arithmetic && ty.is_integer();
            let integer_only = matches!(
                &expr.kind,
                ExpressionKind::Binary {
                    operator: BinaryOperator::ShiftLeft
                        | BinaryOperator::ShiftRight
                        | BinaryOperator::LogicalShiftRight
                        | BinaryOperator::SaturatingShiftLeft,
                    ..
                }
            );
            let operator_symbol = match &expr.kind {
                ExpressionKind::Binary { operator, .. }
                    if super::super::operator_methods::supported(*operator) =>
                {
                    Some(operator.symbol())
                }
                _ => None,
            };
            let type_error = |ty: Type| {
                if ty == Type::Integer(IntegerType::Len)
                    && (!arithmetic
                        || matches!(&expr.kind, ExpressionKind::Binary { operator, .. } if operator.is_shift()))
                {
                    return Some(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "expr.operator-types",
                            "len_t does not support bit-width operations"
                        ),
                    ));
                }
                if matches!(
                    &expr.kind,
                    ExpressionKind::Negate(_) | ExpressionKind::SaturatingNegate(_)
                ) && matches!(ty, Type::Integer(integer) if integer.bounds().0 == 0)
                {
                    return Some(expr.location.error(
                        ErrorKind::InvalidSource,
                        crate::diagnostic!(
                            "expr.operator-types",
                            "integer negation requires a signed type"
                        ),
                    ));
                }
                ((integer_only && !ty.is_integer())
                    || !(ty.is_integer() || (arithmetic && ty.is_float())))
                .then(|| {
                    expr.location.error(
                        ErrorKind::InvalidSource,
                        match ty {
                            Type::Struct(_) | Type::Enum(_)
                                if matches!(
                                    &expr.kind,
                                    ExpressionKind::Negate(_)
                                        | ExpressionKind::WrapNegate(_)
                                        | ExpressionKind::SaturatingNegate(_)
                                        | ExpressionKind::BitNot(_)
                                ) =>
                            {
                                crate::diagnostic!(
                                    "operator.unary",
                                    "no unary operator {} defined for this type",
                                    super::super::operator_methods::unary(expr).unwrap().0
                                )
                            }
                            Type::Text => crate::diagnostic!(
                                "text.operations",
                                "text does not support arithmetic or bitwise operations"
                            ),
                            Type::Symbol => crate::diagnostic!(
                                "symbol.operations",
                                "symbol does not support arithmetic or bitwise operations"
                            ),
                            Type::Struct(_) | Type::Enum(_) if operator_symbol.is_some() => {
                                crate::diagnostic!(
                                    "operator.overload",
                                    "no matching operator {}; accepted right-hand types: none",
                                    operator_symbol.unwrap()
                                )
                            }
                            _ => crate::diagnostic!(
                                "expr.operator-types",
                                "invalid operand type for this operator"
                            ),
                        },
                    )
                })
            };
            let judgment = domain::judge_one(ty, |ty| type_error(ty).is_none());
            if !judgment.possible() {
                return Err(domain::indices()
                    .find_map(|i| type_error(domain::at(ty, i)?))
                    .expect("invalid operator type has a diagnostic"));
            }
            if judgment == domain::Judgment::Deferred {
                bindings.defer_rule(|i| type_error(domain::at(ty, i)?));
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
                    if !super::super::exact::is_operator(inner) {
                        same_width(operand_type(inner, bindings), ty, inner.location, bindings)?;
                    }
                    Value::BitNot(Box::new(expression(inner, Some(ty), bindings)?))
                }
                ExpressionKind::WrapNegate(inner) => {
                    if !super::super::exact::is_operator(inner) {
                        same_width(operand_type(inner, bindings), ty, inner.location, bindings)?;
                    }
                    Value::WrapNegate(Box::new(expression(inner, Some(ty), bindings)?))
                }
                ExpressionKind::SaturatingNegate(inner) => {
                    if !super::super::exact::is_operator(inner) {
                        same_width(operand_type(inner, bindings), ty, inner.location, bindings)?;
                    }
                    Value::SaturatingNegate(Box::new(expression(inner, Some(ty), bindings)?))
                }
                ExpressionKind::Binary {
                    operator,
                    left,
                    right,
                } => {
                    let typed_left = operand_type(left, bindings);
                    let typed_right = operand_type(right, bindings);
                    if domain::judge_one(ty, Type::is_integer).possible() {
                        for (operand, known) in [(left, typed_left), (right, typed_right)] {
                            if operator.is_shift() && std::ptr::eq(operand, right) {
                                continue;
                            }
                            if !super::super::exact::is_operator(operand) {
                                same_width(known, ty, operand.location, bindings)?;
                            }
                        }
                    }
                    let left = Box::new(numeric_operand(left, ty, bindings)?);
                    let right = Box::new(if operator.is_shift() {
                        super::super::exact::use_value(right, bindings)?
                    } else {
                        numeric_operand(right, ty, bindings)?
                    });
                    if operator.is_shift() && !(right.ty.is_integer() || right.ty == Type::Exact) {
                        return Err(expr.location.error(
                            ErrorKind::InvalidSource,
                            crate::diagnostic!(
                                "expr.operator-types",
                                "shift count requires an integer type"
                            ),
                        ));
                    }
                    let saturating_shift_infallible =
                        matches!(*operator, BinaryOperator::SaturatingShiftLeft)
                            && right
                                .ty
                                .integer_storage()
                                .is_some_and(|r| r.bounds().0 == 0);
                    if integer_arithmetic && !saturating_shift_infallible {
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
                    if matches!(operand.ty, Type::Struct(_) | Type::Enum(_)) {
                        crate::diagnostic!(
                            "operator.unary",
                            "no unary operator not defined for this type"
                        )
                    } else {
                        crate::diagnostic!("expr.not", "not requires a bool operand")
                    },
                ));
            }
            CheckedExpression {
                ty: Type::Bool,
                value: Value::Not(Box::new(operand)),
            }
        }
        ExpressionKind::Logical { .. } => {
            let (checked, true_facts, false_facts) =
                super::super::arrays::bounds::eval_condition(expr, bindings)?;
            *bindings.array_bounds.facts.borrow_mut() =
                super::super::arrays::facts::Facts::join_all(&[true_facts, false_facts]);
            checked
        }
        ExpressionKind::Comparison {
            locations,
            operators,
            operands,
        } => {
            if let Some(value) = super::super::operator_methods::comparison(expr, bindings)? {
                return Ok(value);
            }
            let ty = super::joined_type(
                operands
                    .iter()
                    .map(|operand| operand_type(operand, bindings)),
            )
            .unwrap_or_else(|| {
                if operands
                    .iter()
                    .any(|operand| has_float_source(operand, bindings))
                {
                    Type::Float(FloatType::F64)
                } else {
                    Type::Integer(IntegerType::I64)
                }
            });
            let numeric = ty.is_numeric();
            let float_type = super::joined_type(
                operands
                    .iter()
                    .map(|operand| operand_type(operand, bindings).filter(|ty| ty.is_float())),
            )
            .unwrap_or(Type::Float(FloatType::F64));
            let operands = operands
                .iter()
                .map(|operand| {
                    if numeric {
                        if operand_type(operand, bindings).is_none()
                            && has_float_source(operand, bindings)
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
                    .any(|v| !(v.ty.is_numeric() || v.ty == Type::Exact))
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "expr.operator-types",
                        "numeric comparison requires numeric operands"
                    ),
                ));
            }
            if crate::check::arrays::contains_reference(ty, bindings) {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "array.value",
                        "array references and structs containing them cannot be compared"
                    ),
                ));
            }
            if crate::check::ffi::contains_pointer(ty, bindings) {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    if crate::check::arena::contains(ty, bindings) || ty.array_id().is_some() {
                        crate::diagnostic!(
                            "arena.ptr-operations",
                            "pointer values cannot be compared"
                        )
                    } else {
                        crate::diagnostic!(
                            "ffi.pointer-operations",
                            "pointer values cannot be compared"
                        )
                    },
                ));
            }
            if bindings.contains_function(ty) {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    crate::diagnostic!(
                        "fn.value-compare",
                        "function values and structs containing them cannot be compared"
                    ),
                ));
            }
            if matches!(ty, Type::Struct(_)) && operators.iter().all(|op| op.is_equality()) {
                crate::check::type_parameters::struct_equality(ty, bindings, expr.location)?;
            }
            if ty == Type::Void
                || (matches!(ty, Type::Bool | Type::Enum(_) | Type::Struct(_))
                    && operators.iter().any(|operator| !operator.is_equality()))
            {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    if matches!(ty, Type::Struct(_) | Type::Enum(_)) {
                        crate::diagnostic!(
                            "operator.overload",
                            "no matching operator {}; accepted right-hand types: none",
                            operators
                                .iter()
                                .find(|op| !op.is_equality())
                                .unwrap()
                                .symbol()
                        )
                    } else {
                        crate::diagnostic!(
                            "expr.operator-types",
                            "ordering comparisons require numeric operands"
                        )
                    },
                ));
            }
            for ((op, pair), location) in operators.iter().zip(operands.windows(2)).zip(locations) {
                if let Some(result) =
                    super::super::exact::constant_comparison(&pair[0], &pair[1], *op)
                {
                    bindings
                        .outputs
                        .comparisons
                        .borrow_mut()
                        .insert((location.line, location.column), result);
                }
            }
            CheckedExpression {
                ty: Type::Bool,
                value: Value::Comparison {
                    locations: locations.clone(),
                    methods: Vec::new(),
                    operators: operators.clone(),
                    operands,
                },
            }
        }
        _ => unreachable!(),
    })
}

fn same_width(
    known: Option<Type>,
    ty: Type,
    location: crate::ast::Location,
    bindings: &Scope,
) -> Result<(), Diagnostic> {
    let Some(from) = known.filter(|from| *from != ty) else {
        return Ok(());
    };
    let error = || {
        location.error(
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "width.same-type",
                "bit-width operands must have the same integer type"
            ),
        )
    };
    let judgment = domain::judge(from, ty, |from, to| !to.is_integer() || from == to);
    if !judgment.possible() {
        return Err(error());
    }
    if judgment == domain::Judgment::Deferred {
        bindings.defer_rule(|i| {
            let (from, to) = (domain::at(from, i)?, domain::at(ty, i)?);
            (to.is_integer() && from != to).then(error)
        });
    }
    Ok(())
}

// 浮動小数点演算の中でも、整数の部分木では正確な整数除算を維持する。
fn numeric_operand(
    expr: &Expression,
    ty: Type,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    if !super::super::exact::is_operator(expr)
        && let Some(from) = operand_type(expr, bindings).filter(|from| *from != ty)
    {
        let compatible = |from: Type, to: Type| {
            !(from.is_integer() && to.is_float()) || super::super::exact::widening(from, to)
        };
        let error = || {
            expr.location.error(
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "arith.mix",
                    "integer operand is not losslessly convertible; use as"
                ),
            )
        };
        let judgment = domain::judge(from, ty, compatible);
        if !judgment.possible() {
            return Err(error());
        }
        if judgment == domain::Judgment::Deferred {
            bindings.defer_rule(|i| {
                (!compatible(domain::at(from, i)?, domain::at(ty, i)?)).then(error)
            });
        }
    }
    let checked = super::floating_operand(expr, ty, bindings)?;
    if ty.is_float()
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
                            .outputs
                            .divisions
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
