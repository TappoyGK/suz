//! 変換・比較・演算に必要なCの補助関数を、使用されたものだけ出力する。
//!
//! 利用元: [`crate::codegen`]。

use super::ExpressionContext;
use std::fmt::Write;

pub(super) fn emit(c: &mut String, context: &ExpressionContext) {
    for (text, id) in &context.text_literals {
        writeln!(
            c,
            "static const char suz_text_literal_{id}[] = {};\nstatic suz_text suz_text_constant_{id};",
            super::expressions::c_string(text)
        )
        .unwrap();
    }
    for id in &context.enum_logs {
        let names = &context.enums[*id].variants;
        let strings = names
            .iter()
            .map(|name| super::expressions::c_string(name))
            .collect::<Vec<_>>()
            .join(", ");
        let lengths = names
            .iter()
            .map(|name| name.len().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            c,
            "static const char *const suz_enum_{id}_names[] = {{ {strings} }};"
        )
        .unwrap();
        writeln!(
            c,
            "static const size_t suz_enum_{id}_lengths[] = {{ {lengths} }};\n"
        )
        .unwrap();
    }
    if !context.float_types.is_empty() {
        c.push_str("#include <string.h>\n\n");
    }
    if context.uses_float_rem || !context.float_euclid.is_empty() {
        c.push_str("#include <math.h>\n\n");
    }
    for ty in &context.float_euclid {
        let (c_type, fmod_fn, fabs_fn, zero) = match ty {
            crate::ast::FloatType::F32 => ("float", "fmodf", "fabsf", "0.0f"),
            crate::ast::FloatType::F64 => ("double", "fmod", "fabs", "0.0"),
        };
        writeln!(
            c,
            "static inline {c_type} suz_euclid_{}({c_type} a, {c_type} b) {{\n    if (!isfinite(b)) return ({c_type})NAN;\n    {c_type} abs_b = {fabs_fn}(b);\n    if (a >= {zero} && a < abs_b) return a == {zero} ? {zero} : a;\n    if (a >= abs_b && a - abs_b < abs_b) return a - abs_b;\n    {c_type} r = {fmod_fn}(a, b);\n    if (r < {zero}) {{\n        {c_type} adjusted = r + abs_b;\n        return adjusted >= abs_b ? {zero} : adjusted;\n    }}\n    return r == {zero} ? {zero} : r;\n}}\n",
            ty.name()
        )
        .unwrap();
    }
    if context.narrow_float_check {
        c.push_str(
            r#"// FLT_MAXの上側の中間値は無限大に丸める（偶数丸め）。
static inline _Bool suz_f32_fits(double value) {
    return !(value >= 0x1.ffffffp127 || value <= -0x1.ffffffp127)
        || value > DBL_MAX || value < -DBL_MAX;
}
"#,
        );
    }
    if context.narrow_float {
        c.push_str(
            r#"static inline float suz_narrow_f32(double value) {
    // FLT_MAXを少し超えた有限値はFLT_MAXへ丸める。
    // 範囲外のCキャストの動作は移植可能でないため、その前に上限へ制限する。
    if (value > FLT_MAX && value <= DBL_MAX) return FLT_MAX;
    if (value < -FLT_MAX && value >= -DBL_MAX) return -FLT_MAX;
    return (float)value;
}

"#,
        );
    }
    for ty in &context.float_helpers {
        let bits = match ty {
            crate::ast::FloatType::F32 => 32,
            crate::ast::FloatType::F64 => 64,
        };
        writeln!(c, "static inline {} suz_{}(uint{bits}_t bits) {{\n    {} value;\n    memcpy(&value, &bits, sizeof(value));\n    return value;\n}}\n", ty.c_name(), ty.name(), ty.c_name()).unwrap();
    }
    // 引数を通じて比較し、有効な自己比較がCの恒真比較の警告を出さないようにする。
    // 使われる補助関数だけを出力する。
    for ty in &context.bit_helpers {
        let (min, max) = ty.bounds();
        let mask = if min < 0 { max * 2 + 1 } else { max };
        writeln!(
            c,
            "static inline {} suz_bits_{}(uint64_t bits) {{",
            ty.c_name(),
            ty.name()
        )
        .unwrap();
        writeln!(c, "    bits &= UINT64_C({mask});").unwrap();
        if min < 0 {
            // 両方の変換はint64_tに収まる。範囲外の符号なしから符号付きへの変換を避ける。
            writeln!(c, "    return bits <= UINT64_C({max}) ? (int64_t)bits : -INT64_C(1) - (int64_t)(UINT64_C({mask}) ^ bits);").unwrap();
        } else {
            writeln!(c, "    return ({})bits;", ty.c_name()).unwrap();
        }
        writeln!(c, "}}\n").unwrap();
    }
    for &unsigned in &context.helpers {
        let (name, ty) = if unsigned {
            ("unsigned", "uint64_t")
        } else {
            ("signed", "int64_t")
        };
        writeln!(c, "static inline int suz_compare_{name}({ty} left, {ty} right) {{\n    return (left > right) - (left < right);\n}}\n").unwrap();
    }
}
