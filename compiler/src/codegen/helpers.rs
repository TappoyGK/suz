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
    if context.saturating_helpers.iter().any(|(op, ty)| {
        matches!(
            (*op, *ty),
            ("add" | "sub" | "mul", crate::ast::IntegerType::I64)
                | ("mul", crate::ast::IntegerType::U64)
        )
    }) {
        c.push_str("#include <stdckdint.h>\n\n");
    }
    if context.uses_float_rem || !context.float_euclid.is_empty() {
        c.push_str("#include <math.h>\n\n");
    }
    for &(from, to) in &context.float_sat_casts {
        let c_from = from.c_name();
        let c_to = to.c_name();
        let (min, max) = to.bounds();
        let bits = to.bits();
        let limit = to
            .name()
            .to_uppercase()
            .replace('I', "INT")
            .replace('U', "UINT");
        let min_expr = if min == 0 {
            format!("({c_to})0")
        } else {
            format!("{limit}_MIN")
        };
        if from == crate::ast::FloatType::F32 && bits <= 16 {
            writeln!(
                c,
                "static inline {c_to} suz_sat_f2i_{}_{}({c_from} x, {c_to} nan_val) {{\n    float c = x < {min}.0f ? {min}.0f : (x > {max}.0f ? {max}.0f : x);\n    return x != x ? nan_val : ({c_to})c;\n}}\n",
                from.name(),
                to.name(),
            )
            .unwrap();
        } else if bits <= 32 {
            writeln!(
                c,
                "static inline {c_to} suz_sat_f2i_{}_{}({c_from} x, {c_to} nan_val) {{\n    double d = (double)x;\n    double c = d < {min}.0 ? {min}.0 : (d > {max}.0 ? {max}.0 : d);\n    return d != d ? nan_val : ({c_to})c;\n}}\n",
                from.name(),
                to.name(),
            )
            .unwrap();
        } else {
            let max_plus_1 = max + 1;
            writeln!(
                c,
                "static inline {c_to} suz_sat_f2i_{}_{}({c_from} x, {c_to} nan_val) {{\n    double d = (double)x;\n    return d != d ? nan_val : (d <= {min}.0 ? {min_expr} : (d >= {max_plus_1}.0 ? {limit}_MAX : ({c_to})d));\n}}\n",
                from.name(),
                to.name(),
            )
            .unwrap();
        }
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
    for &(op, ty) in &context.saturating_helpers {
        let c_type = ty.c_name();
        let name = ty.name();
        let (min, _max) = ty.bounds();
        let signed = min < 0;
        let bits = ty.bits();
        let limit = name.to_uppercase().replace('I', "INT").replace('U', "UINT");
        match (op, ty) {
            ("add", crate::ast::IntegerType::I64) => {
                writeln!(
                    c,
                    "static inline int64_t suz_sat_add_i64(int64_t a, int64_t b) {{\n    int64_t r;\n    return ckd_add(&r, a, b) ? (b > 0 ? INT64_MAX : INT64_MIN) : r;\n}}\n"
                )
                .unwrap();
            }
            ("sub", crate::ast::IntegerType::I64) => {
                writeln!(
                    c,
                    "static inline int64_t suz_sat_sub_i64(int64_t a, int64_t b) {{\n    int64_t r;\n    return ckd_sub(&r, a, b) ? (b < 0 ? INT64_MAX : INT64_MIN) : r;\n}}\n"
                )
                .unwrap();
            }
            ("mul", crate::ast::IntegerType::I64) => {
                writeln!(
                    c,
                    "static inline int64_t suz_sat_mul_i64(int64_t a, int64_t b) {{\n    int64_t r;\n    return ckd_mul(&r, a, b) ? ((a ^ b) < 0 ? INT64_MIN : INT64_MAX) : r;\n}}\n"
                )
                .unwrap();
            }
            ("mul", crate::ast::IntegerType::U64) => {
                writeln!(
                    c,
                    "static inline uint64_t suz_sat_mul_u64(uint64_t a, uint64_t b) {{\n    uint64_t r;\n    return ckd_mul(&r, a, b) ? UINT64_MAX : r;\n}}\n"
                )
                .unwrap();
            }
            ("add", _) if signed => {
                writeln!(
                    c,
                    "static inline {c_type} suz_sat_add_{name}({c_type} a, {c_type} b) {{\n    int64_t r = (int64_t)a + (int64_t)b;\n    return r > {limit}_MAX ? {limit}_MAX : (r < {limit}_MIN ? {limit}_MIN : ({c_type})r);\n}}\n"
                )
                .unwrap();
            }
            ("sub", _) if signed => {
                writeln!(
                    c,
                    "static inline {c_type} suz_sat_sub_{name}({c_type} a, {c_type} b) {{\n    int64_t r = (int64_t)a - (int64_t)b;\n    return r > {limit}_MAX ? {limit}_MAX : (r < {limit}_MIN ? {limit}_MIN : ({c_type})r);\n}}\n"
                )
                .unwrap();
            }
            ("mul", _) if signed => {
                let wide = if bits < 32 { "int32_t" } else { "int64_t" };
                writeln!(
                    c,
                    "static inline {c_type} suz_sat_mul_{name}({c_type} a, {c_type} b) {{\n    {wide} r = ({wide})a * ({wide})b;\n    return r > {limit}_MAX ? {limit}_MAX : (r < {limit}_MIN ? {limit}_MIN : ({c_type})r);\n}}\n"
                )
                .unwrap();
            }
            ("add", _) => {
                writeln!(
                    c,
                    "static inline {c_type} suz_sat_add_{name}({c_type} a, {c_type} b) {{\n    return a > ({c_type})({limit}_MAX - b) ? {limit}_MAX : ({c_type})(a + b);\n}}\n"
                )
                .unwrap();
            }
            ("sub", _) => {
                writeln!(
                    c,
                    "static inline {c_type} suz_sat_sub_{name}({c_type} a, {c_type} b) {{\n    return a < b ? ({c_type})0 : ({c_type})(a - b);\n}}\n"
                )
                .unwrap();
            }
            ("mul", _) => {
                let wide = if bits < 32 { "uint32_t" } else { "uint64_t" };
                writeln!(
                    c,
                    "static inline {c_type} suz_sat_mul_{name}({c_type} a, {c_type} b) {{\n    {wide} r = ({wide})a * ({wide})b;\n    {wide} m = r < {limit}_MAX ? r : {limit}_MAX;\n    return ({c_type})m;\n}}\n"
                )
                .unwrap();
            }
            ("neg", _) => {
                writeln!(
                    c,
                    "static inline {c_type} suz_sat_neg_{name}({c_type} a) {{\n    return a == {limit}_MIN ? {limit}_MAX : ({c_type})(-(int64_t)a);\n}}\n"
                )
                .unwrap();
            }
            ("shl", _) if signed => {
                writeln!(
                    c,
                    "static inline {c_type} suz_sat_shl_{name}({c_type} a, uint64_t shift) {{\n    if (a == 0 || shift == 0) return a;\n    if (shift >= {bits}) return a > 0 ? {limit}_MAX : {limit}_MIN;\n    if (a > 0) return (uint64_t)a > ((uint64_t){limit}_MAX >> shift) ? {limit}_MAX : ({c_type})((uint64_t)a << shift);\n    int64_t limit = -(INT64_C(1) << ({bits} - 1 - shift));\n    if (a < limit) return {limit}_MIN;\n    return ({c_type})(int64_t)((uint64_t)(int64_t)a << shift);\n}}\n"
                )
                .unwrap();
            }
            ("shl", _) => {
                writeln!(
                    c,
                    "static inline {c_type} suz_sat_shl_{name}({c_type} a, uint64_t shift) {{\n    if (a == 0 || shift == 0) return a;\n    if (shift >= {bits}) return {limit}_MAX;\n    return (uint64_t)a > ((uint64_t){limit}_MAX >> shift) ? {limit}_MAX : ({c_type})((uint64_t)a << shift);\n}}\n"
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
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
