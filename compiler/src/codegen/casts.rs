//! 型変換の値の評価、成功判定、変換結果を分けて生成し、値ハンドラーと早期returnで共有する。
//!
//! 利用元: [`crate::codegen::expressions`]、[`crate::codegen::statements`]。

use super::{ExpressionContext, expressions::expression};
use crate::check::CheckedExpression;

fn c_bound_literal(value: i128, signed: bool) -> String {
    if !signed {
        format!("UINT64_C({})", value as u64)
    } else if value == i64::MIN as i128 {
        "INT64_MIN".into()
    } else {
        format!("INT64_C({value})")
    }
}

fn clamp_range_expr(expr: &str, range: crate::ast::RangeType) -> String {
    let (lo, hi) = range.bounds();
    let signed = range.base.bounds().0 < 0;
    let c_base = range.base.c_name();
    let lo_lit = c_bound_literal(lo, signed);
    let hi_lit = c_bound_literal(hi, signed);
    let (base_lo, base_hi) = range.base.bounds();
    let mut body = format!("({c_base})({expr})");
    if hi < base_hi {
        body = format!("(({expr}) > {hi_lit} ? ({c_base}){hi_lit} : {body})");
    }
    if lo > base_lo {
        body = format!("(({expr}) < {lo_lit} ? ({c_base}){lo_lit} : {body})");
    }
    body
}

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
    let native_ty = super::exact::native_type(operand);
    if operand.ty == crate::ast::Type::Exact && native_ty.is_none() {
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
    let operand_ty = native_ty.unwrap_or(operand.ty);
    for ty in [operand_ty, target] {
        if let Type::Float(ty) = ty {
            context.float_types.insert(ty);
        }
    }
    let index = context.temporaries.len();
    context.temporaries.push(operand_ty);
    let source = if let Some(ty) = native_ty {
        super::exact::emit_native(operand, ty, context)
    } else {
        expression(operand, context)
    };
    let local = format!("suz_cmp_{index}");
    if operand_ty.is_text() {
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
        let (result_type, function, arguments, converted, extra_cond) = match target {
            Type::Float(ty) => (
                Type::Float(ty),
                match ty {
                    FloatType::F32 => "suz_parse_f32",
                    FloatType::F64 => "suz_parse_f64",
                },
                String::new(),
                format!("suz_cmp_{output}"),
                None,
            ),
            Type::Char => (
                Type::Char,
                "suz_parse_char",
                String::new(),
                format!("suz_cmp_{output}"),
                None,
            ),
            Type::Bool => (
                Type::Bool,
                "suz_parse_bool",
                String::new(),
                format!("suz_cmp_{output}"),
                None,
            ),
            Type::Integer(ty) => {
                context.bit_helpers.insert(ty);
                if kind == crate::ast::CastKind::Saturating {
                    (
                        Type::Integer(IntegerType::U64),
                        "suz_parse_integer_sat",
                        format!(", {}, {}", ty.bits(), u8::from(ty.bounds().0 < 0)),
                        format!("suz_bits_{}(suz_cmp_{output})", ty.name()),
                        None,
                    )
                } else {
                    (
                        Type::Integer(IntegerType::U64),
                        "suz_parse_integer",
                        format!(
                            ", {}, {}, {}",
                            ty.bits(),
                            u8::from(ty.bounds().0 < 0),
                            u8::from(kind == crate::ast::CastKind::Wrap)
                        ),
                        format!("suz_bits_{}(suz_cmp_{output})", ty.name()),
                        None,
                    )
                }
            }
            Type::Range(r) => {
                let signed = r.base.bounds().0 < 0;
                let (lo, hi) = r.bounds();
                let lo_lit = c_bound_literal(lo, signed);
                let hi_lit = c_bound_literal(hi, signed);
                let val_expr = if signed {
                    format!("((int64_t)suz_cmp_{output})")
                } else {
                    format!("suz_cmp_{output}")
                };
                if kind == crate::ast::CastKind::Saturating {
                    let clamped = format!(
                        "({})({val_expr} < {lo_lit} ? {lo_lit} : ({val_expr} > {hi_lit} ? {hi_lit} : {val_expr}))",
                        r.base.c_name()
                    );
                    (
                        Type::Integer(IntegerType::U64),
                        "suz_parse_integer_sat",
                        format!(", 64, {}", u8::from(signed)),
                        clamped,
                        None,
                    )
                } else {
                    (
                        Type::Integer(IntegerType::U64),
                        "suz_parse_integer",
                        format!(", 64, {}, 0", u8::from(signed)),
                        format!("({}){val_expr}", r.base.c_name()),
                        Some(format!(
                            "({val_expr} >= {lo_lit} && {val_expr} <= {hi_lit})"
                        )),
                    )
                }
            }
            _ => unreachable!(),
        };
        context.temporaries.push(result_type);
        let (bytes, length) = super::text::contents(operand.ty, &local);
        let base_cond = format!("{function}({bytes}, {length}{arguments}, &suz_cmp_{output})");
        let condition = if let Some(extra) = extra_cond {
            format!("({base_cond} && {extra})")
        } else {
            base_cond
        };
        return (format!("{local} = {source}"), condition, converted);
    }
    let mut converted = format!("({}){local}", target.c_name());
    let condition = match (operand_ty, target) {
        (Type::Float(FloatType::F64), Type::Float(FloatType::F32)) => {
            context.narrow_float = true;
            context.narrow_float_check = true;
            converted = format!("suz_narrow_f32({local})");
            format!("suz_f32_fits({local})")
        }
        (Type::Float(from), Type::Integer(to)) if kind == crate::ast::CastKind::Saturating => {
            context.float_types.insert(from);
            context.float_sat_casts.insert((from, to));
            converted = format!(
                "suz_sat_f2i_{}_{}({local}, ({})0)",
                from.name(),
                to.name(),
                to.c_name()
            );
            format!("{local} == {local}")
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
        (Type::Float(from), Type::Range(r)) if kind == crate::ast::CastKind::Saturating => {
            let to = r.base;
            context.float_types.insert(from);
            context.float_sat_casts.insert((from, to));
            let base_sat = format!(
                "suz_sat_f2i_{}_{}({local}, ({})0)",
                from.name(),
                to.name(),
                to.c_name()
            );
            let sat_tmp = context.temporaries.len();
            context.temporaries.push(Type::Integer(to));
            let clamped = clamp_range_expr(&format!("suz_cmp_{sat_tmp}"), r);
            converted = format!("(suz_cmp_{sat_tmp} = {base_sat}, {clamped})");
            format!("{local} == {local}")
        }
        (Type::Float(_), Type::Range(r)) => {
            let to = r.base;
            context.float_types.insert(FloatType::F64);
            let (base_min, base_max) = to.bounds();
            let lower = if to == IntegerType::I64 {
                format!("(double){local} >= {base_min}.0")
            } else {
                format!("(double){local} > {}.0", base_min - 1)
            };
            let base_check = format!("({lower} && (double){local} < {}.0)", base_max + 1);
            let (lo, hi) = r.bounds();
            let signed = to.bounds().0 < 0;
            let lo_lit = c_bound_literal(lo, signed);
            let hi_lit = c_bound_literal(hi, signed);
            let c_base = to.c_name();
            format!(
                "({base_check} && ({c_base}){local} >= {lo_lit} && ({c_base}){local} <= {hi_lit})"
            )
        }
        (Type::Integer(_) | Type::Range(_), Type::Float(_)) | (Type::Float(_), Type::Float(_)) => {
            "1".into()
        }
        _ => unreachable!(),
    };
    (format!("{local} = {source}"), condition, converted)
}
