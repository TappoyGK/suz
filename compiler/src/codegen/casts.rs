use super::{ExpressionContext, expressions::expression};
use crate::check::CheckedExpression;

// Return source evaluation, success test, and conversion separately so value
// fallbacks and early returns use exactly the same range checks.
pub(super) fn cast_parts(
    operand: &CheckedExpression,
    target: crate::ast::Type,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    use crate::ast::{FloatType, IntegerType, Type};
    for ty in [operand.ty, target] {
        if let Type::Float(ty) = ty {
            context.float_types.insert(ty);
        }
    }
    let index = context.temporaries.len();
    context.temporaries.push(operand.ty);
    let source = expression(operand, context);
    let local = format!("suz_cmp_{index}");
    let mut converted = format!("({}){local}", target.c_name());
    let condition = match (operand.ty, target) {
        (Type::Float(FloatType::F64), Type::Float(FloatType::F32)) => {
            context.narrow_float = true;
            context.narrow_float_check = true;
            converted = format!("suz_narrow_f32({local})");
            format!("suz_f32_fits({local})")
        }
        (Type::Float(_), Type::Integer(to)) => {
            context.float_types.insert(FloatType::F64);
            let (min, max) = to.bounds();
            // The upper bound is an exact power of two. For i64, min - 1
            // is not representable: the next lower f64 is already out of range.
            let lower = if to == IntegerType::I64 {
                format!("(double){local} >= {min}.0")
            } else {
                format!("(double){local} > {}.0", min - 1)
            };
            format!("{lower} && (double){local} < {}.0", max + 1)
        }
        (from, to) if from.integer_bounds().is_some() && to.integer_bounds().is_some() => {
            let (from_min, from_max) = from.integer_bounds().unwrap();
            let (to_min, to_max) = to.integer_bounds().unwrap();
            let mut conditions = Vec::new();
            if from_min < to_min {
                conditions.push(format!("{local} >= INT64_C({to_min})"));
            }
            if from_max > to_max {
                let prefix = if from.integer_storage().unwrap().bounds().0 < 0 {
                    "INT64_C"
                } else {
                    "UINT64_C"
                };
                conditions.push(format!("{local} <= {prefix}({to_max})"));
            }
            if conditions.is_empty() {
                "1".into()
            } else {
                conditions.join(" && ")
            }
        }
        (Type::Integer(_), Type::Float(_)) | (Type::Float(_), Type::Float(_)) => "1".into(),
        _ => unreachable!(),
    };
    (format!("{local} = {source}"), condition, converted)
}
