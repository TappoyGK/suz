//! 型変換の値の評価、成功判定、変換結果を分けて生成し、値ハンドラーと早期returnで共有する。
//!
//! 利用元: [`crate::codegen::expressions`]、[`crate::codegen::statements`]。

use super::{ExpressionContext, expressions::expression};
use crate::check::CheckedExpression;

// 元の値の評価・成功判定・変換を別々に返し、値の代替処理と早期returnで同じ値域検査を使う。
pub(super) fn cast_parts(
    operand: &CheckedExpression,
    target: crate::ast::Type,
    kind: crate::ast::CastKind,
    location: crate::ast::Location,
    context: &mut ExpressionContext,
) -> (String, String, String) {
    if matches!(operand.ty, crate::ast::Type::CInteger(_))
        || matches!(target, crate::ast::Type::CInteger(_))
    {
        return super::ffi::cast_parts(operand, target, kind, context);
    }
    if operand.ty == crate::ast::Type::Exact {
        let id = context.temporaries.len();
        context.temporaries.push(crate::ast::Type::Exact);
        let name = format!("suz_cmp_{id}");
        let value = super::expressions::expression(operand, context);
        return (
            format!("{name} = {value}"),
            super::exact::fits(target, &name),
            super::exact::converted(target, &name, context),
        );
    }

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
    if operand.ty.is_text() {
        context.uses_text = true;
        let output = context.temporaries.len();
        if let Type::Struct(id) = target {
            context.struct_parsers.insert(id);
            context.uses_arena |= super::text_structure::has_text(Type::Struct(id), context);
            context.temporaries.push(target);
            let position = super::text::position(location, context);
            let input = if operand.ty == Type::Symbol {
                format!("suz_text_as_view({local})")
            } else {
                local.clone()
            };
            return (
                format!("{local} = {source}"),
                format!("suz_parse_struct_{id}({input}, &suz_cmp_{output}, {position})"),
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
                let mode = match kind {
                    crate::ast::CastKind::Checked => 0,
                    crate::ast::CastKind::Wrap => 1,
                    crate::ast::CastKind::Saturating => 2,
                };
                (
                    Type::Integer(IntegerType::U64),
                    "suz_parse_integer",
                    format!(", {}, {}, {mode}", ty.bits(), u8::from(ty.bounds().0 < 0)),
                    format!("suz_bits_{}(suz_cmp_{output})", ty.name()),
                )
            }
            _ => unreachable!(),
        };
        context.temporaries.push(result_type);
        let (bytes, length) = super::text::contents(operand.ty, &local);
        let condition = format!("{function}({bytes}, {length}{arguments}, &suz_cmp_{output})");
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
        (Type::Float(from), Type::Integer(to)) if kind == crate::ast::CastKind::Saturating => {
            context.float_types.insert(from);
            context.float_sat_casts.insert((from, to));
            converted = format!("suz_sat_f2i_{}_{}({local})", from.name(), to.name());
            format!("!isnan({local})")
        }
        (Type::Float(_), Type::Integer(to)) => {
            context.float_types.insert(FloatType::F64);
            let (min, max) = to.bounds();
            // 上限は正確な2のべき乗。i64ではmin - 1を表現できず、
            // 一つ小さいf64の値は既に範囲外になる。
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
                let prefix = if from.integer_storage().unwrap().c_signed() {
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
