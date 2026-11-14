use crate::check::scope::Scope;
mod blocks;
mod branches;
mod casts;
mod operators;
pub(crate) mod structs;

use super::ir::{CheckedExpression, Value};
use crate::{
    Diagnostic, ErrorKind,
    ast::{Expression, ExpressionKind, FloatType, IntegerType, Location, Type},
};

// Find a known type through expressions that preserve their operand type.
// Numeric literals receive a type from the other operand before defaulting.
// The comparison's bool result never supplies the operand type.
pub(super) fn operand_type(expr: &Expression, bindings: &Scope) -> Option<Type> {
    match &expr.kind {
        ExpressionKind::Template(_) => Some(Type::Text),
        ExpressionKind::Struct { id, .. } => Some(Type::Struct(*id)),
        ExpressionKind::Field { target, name } => {
            let Type::Struct(id) = operand_type(target, bindings)? else {
                return None;
            };
            bindings.structs[id]
                .fields
                .iter()
                .find(|field| &field.name == name)
                .map(|field| field.ty)
        }
        ExpressionKind::Variant { enum_name, .. } => bindings
            .enums
            .iter()
            .find(|e| &e.name == enum_name)
            .map(|e| Type::Enum(e.ty)),
        ExpressionKind::Void => Some(Type::Void),
        ExpressionKind::Call { name, .. } => bindings.functions.get(name).map(|f| f.result),
        ExpressionKind::Block { statements, value } => {
            blocks::operand_type(statements, value, bindings)
        }
        ExpressionKind::Match { arms, .. } => arms
            .iter()
            .find_map(|arm| operand_type(&arm.body, bindings)),
        ExpressionKind::Cast { target, .. } => Some(*target),
        ExpressionKind::If {
            branches,
            otherwise,
        } => branches
            .iter()
            .find_map(|(_, value)| operand_type(value, bindings))
            .or_else(|| operand_type(otherwise, bindings)),
        ExpressionKind::Integer { .. } | ExpressionKind::Float(_) => None,
        ExpressionKind::Name(name) => bindings.get(name).map(|binding| binding.ty),
        ExpressionKind::ArithmeticHandler { operand: inner, .. }
        | ExpressionKind::Group(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::Negate(inner) => operand_type(inner, bindings),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => operand_type(left, bindings).or_else(|| {
            if operator.is_shift() {
                None
            } else {
                operand_type(right, bindings)
            }
        }),
        ExpressionKind::Bool(_)
        | ExpressionKind::Not(_)
        | ExpressionKind::Logical { .. }
        | ExpressionKind::Comparison { .. } => Some(Type::Bool),
    }
}

// Only result-producing operands participate; conditions and cast sources do not.
fn has_float_literal(expr: &Expression) -> bool {
    match &expr.kind {
        ExpressionKind::Match { arms, .. } => arms.iter().any(|arm| has_float_literal(&arm.body)),
        ExpressionKind::Block { value, .. } => has_float_literal(value),
        ExpressionKind::Float(_) => true,
        ExpressionKind::ArithmeticHandler { operand: inner, .. }
        | ExpressionKind::Group(inner)
        | ExpressionKind::BitNot(inner)
        | ExpressionKind::Negate(inner) => has_float_literal(inner),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => has_float_literal(left) || (!operator.is_shift() && has_float_literal(right)),
        ExpressionKind::If {
            branches,
            otherwise,
        } => {
            branches.iter().any(|(_, value)| has_float_literal(value))
                || has_float_literal(otherwise)
        }
        _ => false,
    }
}
fn default_type(expr: &Expression) -> Type {
    if has_float_literal(expr) {
        Type::Float(FloatType::F64)
    } else {
        Type::Integer(IntegerType::I64)
    }
}
fn float_literal(
    text: &str,
    ty: FloatType,
    location: Location,
) -> Result<CheckedExpression, Diagnostic> {
    let invalid = || {
        location.error(
            ErrorKind::InvalidSource,
            "floating-point literal is out of range [lit.float-range]",
        )
    };
    let special = matches!(text, "nan" | "-nan" | "inf" | "-inf");
    let text = text.replace('_', "");
    let bits = match ty {
        FloatType::F32 => {
            let value = text.parse::<f32>().map_err(|_| invalid())?;
            if value.is_infinite() && !special {
                return Err(invalid());
            }
            u64::from(value.to_bits())
        }
        FloatType::F64 => {
            let value = text.parse::<f64>().map_err(|_| invalid())?;
            if value.is_infinite() && !special {
                return Err(invalid());
            }
            value.to_bits()
        }
    };
    Ok(CheckedExpression {
        ty: Type::Float(ty),
        value: Value::Float(bits),
    })
}

pub(super) fn expression(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let checked = expression_inner(expr, expected, bindings)?;
    super::text::register_constants(&checked, bindings);
    Ok(checked)
}

fn expression_inner(
    expr: &Expression,
    expected: Option<Type>,
    bindings: &Scope,
) -> Result<CheckedExpression, Diagnostic> {
    let checked = match &expr.kind {
        ExpressionKind::Template(parts) => super::text::template(parts, expr.location, bindings)?,
        ExpressionKind::Struct { .. } | ExpressionKind::Field { .. } => {
            structs::check(expr, bindings)?
        }
        ExpressionKind::Variant { enum_name, variant } => {
            let decl = bindings
                .enums
                .iter()
                .find(|e| &e.name == enum_name)
                .ok_or_else(|| {
                    expr.location
                        .error(ErrorKind::InvalidSource, "unknown enum type [enum.access]")
                })?;
            let value = decl
                .variants
                .iter()
                .position(|name| name == variant)
                .ok_or_else(|| {
                    expr.location.error(
                        ErrorKind::InvalidSource,
                        "unknown enum variant [enum.access]",
                    )
                })?;
            CheckedExpression {
                ty: Type::Enum(decl.ty),
                value: Value::Integer {
                    value: value as i128,
                    minus: None,
                },
            }
        }
        ExpressionKind::Void => CheckedExpression {
            ty: Type::Void,
            value: Value::Void,
        },
        ExpressionKind::Call { name, arguments } => {
            super::functions::call(expr, name, arguments, bindings)?
        }
        ExpressionKind::Block { statements, value } => {
            blocks::check(statements, value, expected, bindings)?
        }
        ExpressionKind::ArithmeticHandler { .. }
        | ExpressionKind::BitNot(_)
        | ExpressionKind::Negate(_)
        | ExpressionKind::Binary { .. }
        | ExpressionKind::Not(_)
        | ExpressionKind::Logical { .. }
        | ExpressionKind::Comparison { .. } => operators::check(expr, expected, bindings)?,
        ExpressionKind::Cast { .. } => casts::check(expr, expected, bindings)?,
        ExpressionKind::Float(text) => {
            let ty =
                match expected {
                    None => FloatType::F64,
                    Some(Type::Float(ty)) => ty,
                    _ => return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "a floating-point literal requires a floating-point type [type.expected]",
                    )),
                };
            float_literal(text, ty, expr.location)?
        }
        ExpressionKind::Match { .. } | ExpressionKind::If { .. } => {
            branches::check(expr, expected, bindings)?
        }
        ExpressionKind::Integer { digits, negative } => {
            if let Some(Type::Float(ty)) = expected {
                let text = if *negative {
                    format!("-{digits}")
                } else {
                    digits.clone()
                };
                return float_literal(&text, ty, expr.location);
            }
            let ty = match expected {
                Some(Type::Float(_)) => unreachable!(),
                Some(Type::Bool | Type::Void | Type::Enum(_) | Type::Struct(_) | Type::Text) => {
                    return Err(expr.location.error(
                        ErrorKind::InvalidSource,
                        "an integer literal requires a numeric type [type.expected]",
                    ));
                }
                Some(Type::Integer(ty)) => ty,
                None => IntegerType::I64,
            };
            let fail = || {
                expr.location.error(
                    ErrorKind::InvalidSource,
                    format!(
                        "integer literal is out of range for {} [lit.range]",
                        ty.name()
                    ),
                )
            };
            let magnitude = digits.parse::<i128>().map_err(|_| fail())?;
            let value = if *negative { -magnitude } else { magnitude };
            let (min, max) = ty.bounds();
            if value < min || value > max {
                return Err(fail());
            }
            if *negative && min == 0 {
                return Err(expr.location.error(
                    ErrorKind::InvalidSource,
                    "integer negation requires a signed type [expr.operator-types]",
                ));
            }
            CheckedExpression {
                ty: Type::Integer(ty),
                value: Value::Integer {
                    value,
                    minus: negative.then_some(expr.location),
                },
            }
        }
        ExpressionKind::Bool(value) => CheckedExpression {
            ty: Type::Bool,
            value: Value::Bool(*value),
        },
        ExpressionKind::Name(name) => {
            let binding = bindings.get(name).ok_or_else(|| {
                expr.location.error(
                    ErrorKind::InvalidSource,
                    format!("binding '{name}' is not in scope [decl.local-scope]"),
                )
            })?;
            binding.constant.clone().unwrap_or(CheckedExpression {
                ty: binding.ty,
                value: Value::Local(binding.id),
            })
        }
        ExpressionKind::Group(inner) => expression(inner, expected, bindings)?,
    };
    if expected.is_some_and(|ty| ty != checked.ty) {
        return Err(expr.location.error(
            ErrorKind::InvalidSource,
            format!(
                "expected {}, found {} [type.expected]",
                bindings.type_name(expected.unwrap()),
                bindings.type_name(checked.ty)
            ),
        ));
    }
    Ok(checked)
}
