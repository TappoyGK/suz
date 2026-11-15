use super::{ExpressionContext, expressions::expression};
use crate::check::CheckedExpression;

// Return source evaluation, success test, and conversion separately so value
// fallbacks and early returns use exactly the same range checks.
pub(super) fn cast_parts(
    operand: &CheckedExpression,
    target: crate::ast::Type,
    wrap: bool,
    location: crate::ast::Location,
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
    if operand.ty == Type::Text {
        context.uses_text = true;
        let output = context.temporaries.len();
        if let Type::Struct(id) = target {
            context.struct_parsers.insert(id);
            context.temporaries.push(target);
            let position = super::text::position(location, context);
            return (
                format!("{local} = {source}"),
                format!("suz_parse_struct_{id}({local}, &suz_cmp_{output}, {position})"),
                format!("suz_cmp_{output}"),
            );
        }
        let (result_type, function, arguments, converted) = match target {
            Type::Float(ty) => (
                Type::Float(ty),
                match ty {
                    FloatType::F32 => "suz_parse_f32",
                    FloatType::F64 => "suz_parse_f64",
                },
                String::new(),
                format!("suz_cmp_{output}"),
            ),
            Type::Char => (
                Type::Char,
                "suz_parse_char",
                String::new(),
                format!("suz_cmp_{output}"),
            ),
            Type::Bool => (
                Type::Bool,
                "suz_parse_bool",
                String::new(),
                format!("suz_cmp_{output}"),
            ),
            Type::Integer(ty) => {
                context.bit_helpers.insert(ty);
                (
                    Type::Integer(IntegerType::U64),
                    "suz_parse_integer",
                    format!(
                        ", {}, {}, {}",
                        ty.bits(),
                        u8::from(ty.bounds().0 < 0),
                        u8::from(wrap)
                    ),
                    format!("suz_bits_{}(suz_cmp_{output})", ty.name()),
                )
            }
            _ => unreachable!(),
        };
        context.temporaries.push(result_type);
        let condition = format!(
            "{function}(suz_text_bytes({local}), suz_text_length({local}){arguments}, &suz_cmp_{output})"
        );
        return (format!("{local} = {source}"), condition, converted);
    }
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
            if to == Type::Char {
                conditions.push(format!("({local} < 0xd800 || {local} > 0xdfff)"));
            }
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
