use super::ExpressionContext;
use std::fmt::Write;

pub(super) fn emit(c: &mut String, context: &ExpressionContext) {
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
    if context.narrow_float_check {
        c.push_str(
            r#"// The midpoint above FLT_MAX rounds to infinity (ties to even).
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
    // Finite inputs just above FLT_MAX round to FLT_MAX. Clamp them
    // before the C cast, whose out-of-range behavior is not portable.
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
    // Compare through parameters so valid self-comparisons do not trigger C's
    // tautological-comparison warnings. Emit only helpers that are used.
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
            // Both conversions fit int64_t; avoid out-of-range unsigned-to-signed casts.
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
