use crate::integer_bounds::INTEGER_BOUNDS;
use crate::support::runtime_link;
use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn safe_casts_preserve_values_and_round_integer_inputs() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (from, min, max) in INTEGER_BOUNDS {
        for (to, to_min, to_max) in INTEGER_BOUNDS {
            if min < to_min || max > to_max {
                continue;
            }
            for value in [min, max] {
                source.push_str(&format!("{{\nfix input: {from} = {value}\nfix converted = input as {to}\nif converted != {value} {{ return 1 }}\n}}\n"));
            }
        }
        for to in ["f32", "f64"] {
            for value in [min, max] {
                source.push_str(&format!("{{\nfix input: {from} = {value}\nfix converted = input as {to}\nif converted != {value}.0 {{ return 2 }}\nfix with_fallback = input as {to} or 7.0\nif with_fallback != converted {{ return 2 }}\n}}\n"));
            }
        }
    }
    source.push_str(
        r#"fix zero: f32 = -0.0
    fix wide_zero = zero as f64
    if 1.0 / wide_zero != -inf { return 3 }
    fix top: f32 = 3.4028235e38
    if top as f64 != 340282346638528859811704183484516925440.0 { return 4 }
    fix infinity: f32 = inf
    fix invalid: f32 = nan
    if infinity as f64 != inf || invalid as f64 == invalid as f64 { return 5 }
    fix half: f32 = 0.5
    if half as f64 * 2.0 != 1.0 { return 6 }
    fix narrow: u8 = 255
    fix inferred = 1 +% (narrow as u16)
    if inferred != 256 { return 7 }
    fix wide_invalid = invalid as f64 or 0.0
    fix same_invalid = invalid as f32 or 0.0
    if wide_invalid == wide_invalid || same_invalid == same_invalid { return 8 }
    if (infinity as f64 or 0.0) != inf || (infinity as f32 or 0.0) != inf { return 9 }
    if 1.0 / (zero as f64 or 1.0) != -inf || 1.0 / (zero as f32 or 1.0) != -inf { return 10 }
    fix double_zero: f64 = -0.0
    if 1.0 / (double_zero as f64 or 1.0) != -inf { return 11 }
    fix rounded = (((9007199791611905 as f32 or 0.0) as u64) -% 9007200328482816) as u8
    if rounded != 0 { return 12 }
    fix retained = (((1.0e300 as f64 or 0.0) / 1.0e300) as u8 -% 1) as i8
    if retained != 0 { return 13 }
    fix widened = ((3.0 as f32) as f64 or 7.0) as u8
    if widened != 3 { return 14 }
    return 0
}
"#,
    );
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("safe-casts.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("safe-casts"));
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn safe_casts_keep_source_types_and_diagnostics() {
    for (body, rule) in [
        ("return true as u8", "expr.as"),
        ("fix x = 1 as f32 or false\nreturn 0", "type.expected"),
        (
            "fix x = 1 as f32 or (1.0e39 as f32)\nreturn 0",
            "const.cast-check",
        ),
        (
            "fix fallback: f64 = 0.0\nreturn (1 as f64 or fallback) as u8",
            "expr.cast-handler",
        ),
        ("fix x = 1 as bool\nreturn 0", "expr.as"),
        ("fix x = 1 as void\nreturn 0", "expr.as"),
        ("fix x = 18446744073709551615 as f64\nreturn 0", "lit.range"),
        (
            "fix x: u8 = 1\nfix y: u16 = x as i64\nreturn 0",
            "type.expected",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "{error:?}");
    }
    let source = b"fn main() -> u8 { fix x = -1 as f32\nreturn 0 }";
    let warned = suz::compile_with_mode(source, suz::ArithmeticMode::Warn).unwrap();
    assert_eq!(warned.warnings.len(), 1);
    assert!(
        suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
            .err()
            .unwrap()
            .message
            .contains("diagnostic.arithmetic")
    );
    let source = b"fn main() -> u8 { fix x = -1.0 as f64\nreturn 0 }";
    assert!(
        suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
            .unwrap()
            .warnings
            .is_empty()
    );
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn checked_integer_casts_cover_type_boundaries() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (from, min, max) in INTEGER_BOUNDS {
        for (to, to_min, to_max) in INTEGER_BOUNDS {
            let mut values = vec![min, max, -1, 0, 1, to_min - 1, to_min, to_max, to_max + 1];
            values.sort_unstable();
            values.dedup();
            for value in values.into_iter().filter(|v| (min..=max).contains(v)) {
                let expected = if (to_min..=to_max).contains(&value) {
                    value
                } else {
                    2
                };
                source.push_str(&format!("{{\nfix input: {from} = {value}\nfix result = input as {to} or 2\nif result != {expected} {{ return 1 }}\n}}\n"));
            }
        }
    }
    source.push_str(
        r#"fix raw: i64 = 300
    fix fallback: i64 = 400
    fix nested = raw as u8 or fallback as u8 or 7
    if nested != 7 { return 2 }
    fix bound = raw as u8 or 1 +% 2
    if bound != 3 { return 3 }
    fix grouped = (raw as u8 or 1) +% 2
    if grouped != 3 { return 4 }
    fix constant = ((255 as u8) +% 1) as i8
    if constant != 0 { return 5 }
    fix constant_fallback = (300 as u8 or 127) as i8
    if constant_fallback != 127 { return 6 }
    fix bits = (~(0 as u8) & 127) as i8
    if bits != 127 { return 7 }
    fix product = ((-1 as u64 or 18446744073709551615) *% 18446744073709551615) as u8
    if product != 1 { return 8 }
    return 0
}
"#,
    );
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("checked-casts.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("checked-casts"));
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn checked_casts_require_handlers_and_check_unselected_expressions() {
    for (body, rule) in [
        ("fix x: i8 = 1\nreturn x as u8", "expr.cast-handler"),
        ("fix x: u16 = 1\nreturn x as u8", "expr.cast-handler"),
        ("return 256 as u8", "const.cast-check"),
        (
            "fix x = true || ((300 as u8) == 0)\nreturn 0",
            "const.cast-check",
        ),
        ("return 1 as u8 or false", "type.expected"),
        ("return 1 as u8 or (300 as u8)", "const.cast-check"),
        (
            "fix fallback: u8 = 0\nreturn (1 as u8 or fallback) as i8",
            "expr.cast-handler",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "{body}: {error:?}");
    }
    suz::compile(b"fn main() -> u8 { fix x = true || ((300 as u8 or 0) == 0)\nreturn 0 }").unwrap();
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn checked_cast_codegen_evaluates_source_once_and_fallback_only_on_failure() {
    let directory = TemporaryDirectory::new();
    for (ty, c_type, input, expected, trace) in [
        ("i64", "int64_t", "7", 7, 1),
        ("i64", "int64_t", "300", 9, 12),
        ("i64", "int64_t", "-1", 9, 12),
        ("f64", "double", "7.75", 7, 1),
        ("f32", "float", "-0.75", 0, 1),
        ("f64", "double", "256.0", 9, 12),
        ("f32", "float", "nan", 9, 12),
    ] {
        // Instrument local reads in generated C while keeping Suz's declaration
        // and type checking intact. A trace records both order and count.
        let source = format!(
            "fn main() -> u8 {{\nfix input: {ty} = {input}\nfix fallback: u8 = 9\nreturn input as u8 or fallback\n}}"
        );
        let generated = suz::compile(source.as_bytes()).unwrap();
        let generated = generated.replace("return (suz_cmp_", "#define suz_local_0 probe(1, suz_local_0)\n#define suz_local_1 probe(2, suz_local_1)\nreturn (suz_cmp_");
        assert!(generated.contains("#define suz_local_0"));
        let c = format!(
            r#"#include <stdint.h>
static int trace;
static {c_type} probe(int id, {c_type} value) {{ trace = trace * 10 + id; return value; }}
#define main generated_main
{generated}
#undef main
int main(void) {{
    int result = generated_main();
    return result != {expected} || trace != {trace};
}}
"#
        );
        let c_file = directory.0.join(format!("probe-{ty}-{input}.c"));
        fs::write(&c_file, c).unwrap();
        let result = build_and_run_c(&c_file, &directory.0.join(format!("probe-{ty}-{input}")));
        assert_eq!(result.status.code(), Some(0));
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn narrowing_float_casts_preserve_rounding_boundaries_and_special_values() {
    let source = br#"fn main() -> u8 {
    fix near_max: f64 = 3.4028235e38
    fix positive = near_max as f32 or 7.0
    fix negative = (-near_max) as f32 or 7.0
    if positive != 3.4028235e38 || negative != -3.4028235e38 { return 1 }
    fix below_midpoint: f64 = 340282356779733623858607532500980858880.0
    fix midpoint: f64 = 340282356779733661637539395458142568448.0
    if (below_midpoint as f32 or 7.0) != 3.4028235e38 { return 2 }
    if (midpoint as f32 or 7.0) != 7.0 { return 3 }
    if ((-below_midpoint) as f32 or 7.0) != -3.4028235e38 { return 4 }
    if ((-midpoint) as f32 or 7.0) != 7.0 { return 5 }
    fix huge: f64 = 1.7976931348623157e308
    if (huge as f32 or 7.0) != 7.0 { return 6 }
    fix infinity: f64 = inf
    fix invalid: f64 = nan
    fix converted = invalid as f32 or 7.0
    if (infinity as f32 or 7.0) != inf || ((-infinity) as f32 or 7.0) != -inf { return 7 }
    if converted == converted { return 8 }
    fix zero: f64 = -0.0
    fix tiny: f64 = -1.0e-300
    if 1.0 / (zero as f32 or 7.0) != -inf { return 9 }
    if 1.0 / (tiny as f32 or 7.0) != -inf { return 10 }
    fix half: f64 = 1.000000059604644775390625
    fix above_half: f64 = 1.000000059604645
    if (half as f32 or 7.0) != 1.0 { return 11 }
    if (above_half as f32 or 7.0) != 1.00000011920928955078125 { return 12 }
    fix subnormal: f64 = 1.401298464324817e-45
    if (subnormal as f32 or 7.0) != 1.401298464324817e-45 { return 13 }
    fix constant = 3.4028235e38 as f32
    if constant != positive { return 14 }
    fix folded = (1.0e39 as f32 or 2.0) as f64 as f32
    if folded != 2.0 { return 15 }
    fix arithmetic = (1.5 * 2.0 / 3.0 + 0.5 - 1.0) as f32
    if arithmetic != 0.5 { return 16 }
    fix integer = ((16777217 as f32) as f64) as f32
    if integer != 16777216.0 { return 17 }
    fix special = (1.0 / 0.0) as f32
    if special != inf { return 18 }
    return 0
}
"#;
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("narrow-floats.c");
    fs::write(&c_file, suz::compile(source).unwrap()).unwrap();
    // Check both normal and optimized generated C, including overflow guards.
    for optimization in ["-O0", "-O2"] {
        let executable = directory.0.join(format!("narrow-floats{optimization}"));
        let built = runtime_link::compiler()
            .args(["-std=c23", "-Wall", "-Wextra", "-Werror", optimization])
            .arg(&c_file)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let result = std::process::Command::new(executable).output().unwrap();
        assert_eq!(result.status.code(), Some(0));
    }
}

#[test]
fn narrowing_float_casts_check_constant_success_and_handler_types() {
    for (body, rule) in [
        ("fix x: f64 = 1.0\nfix y = x as f32", "expr.cast-handler"),
        ("fix x = 1.0e39 as f32", "const.cast-check"),
        ("fix x = -1.0e39 as f32", "const.cast-check"),
        ("fix x = (1.0e38 * 10.0) as f32", "const.cast-check"),
        (
            "fix x = true || ((1.0e39 as f32) == 0.0)",
            "const.cast-check",
        ),
        ("fix x = 1.0 as f32 or false", "type.expected"),
        (
            "fix fallback: f64 = 0.0\nfix x = 1.0 as f32 or fallback",
            "type.expected",
        ),
        ("fix x = 1.0 as f32 or (1.0e39 as f32)", "const.cast-check"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "{body}: {error:?}");
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn float_cast_codegen_evaluates_only_the_selected_path() {
    let directory = TemporaryDirectory::new();
    for (from, to, input, expected, trace) in [
        ("f64", "f32", "1.0", "1.0", 1),
        ("f64", "f32", "1.0e39", "7.0", 12),
        ("i64", "f32", "3", "3.0", 1),
        ("u64", "f64", "3", "3.0", 1),
        ("f32", "f64", "1.0", "1.0", 1),
        ("f32", "f32", "1.0", "1.0", 1),
        ("f64", "f64", "1.0e300", "1.0e300", 1),
    ] {
        let source = format!(
            "fn main() -> u8 {{\nfix input: {from} = {input}\nfix fallback: {to} = 7.0\nfix result = input as {to} or fallback\nreturn ifexpr result == {expected} {{ 0 }} else {{ 1 }}\n}}"
        );
        let generated = suz::compile(source.as_bytes()).unwrap();
        // Instrument only the reads in the cast, after both local declarations.
        let marker = if to == "f32" {
            "const float suz_local_2 ="
        } else {
            "const double suz_local_2 ="
        };
        assert_eq!(generated.matches(marker).count(), 1);
        let generated = generated.replace(marker, &format!("#define suz_local_0 probe(1, suz_local_0)\n#define suz_local_1 probe(2, suz_local_1)\n{marker}"));
        let c = format!(
            r#"static int trace;
static double probe(int id, double value) {{ trace = trace * 10 + id; return value; }}
#define main generated_main
{generated}
#undef main
int main(void) {{
    int result = generated_main();
    return result != 0 || trace != {trace};
}}
"#
        );
        let c_file = directory.0.join(format!("float-probe-{trace}.c"));
        fs::write(&c_file, c).unwrap();
        let result = build_and_run_c(&c_file, &directory.0.join(format!("float-probe-{trace}")));
        assert_eq!(result.status.code(), Some(0));
    }
    let c_file = directory.0.join("constant-only.c");
    fs::write(&c_file, suz::compile(b"fn main() -> u8 { fix x = 3.4028235e38 as f32\nreturn ifexpr x == 3.4028235e38 { 0 } else { 1 } }").unwrap()).unwrap();
    assert_eq!(
        build_and_run_c(&c_file, &directory.0.join("constant-only"))
            .status
            .code(),
        Some(0)
    );
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn float_to_integer_casts_cover_all_integer_boundaries() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (to, min, max) in INTEGER_BOUNDS {
        for from in ["f32", "f64"] {
            let lower = min as f64;
            let upper = (max + 1) as f64;
            let candidates = [
                lower - 1.0,
                lower - 0.75,
                lower,
                lower + 0.75,
                upper - 0.75,
                upper,
                upper + 1.0,
                -1.75,
                -1.0,
                -0.75,
                -0.0,
                0.0,
                0.75,
                1.75,
            ];
            let mut values = Vec::new();
            for value in candidates {
                if from == "f32" {
                    let value = value as f32;
                    values.extend([
                        value.next_down() as f64,
                        value as f64,
                        value.next_up() as f64,
                    ]);
                } else {
                    values.extend([value.next_down(), value, value.next_up()]);
                }
            }
            values.sort_by(f64::total_cmp);
            values.dedup_by(|a, b| a.to_bits() == b.to_bits());
            for value in values {
                // Scientific notation preserves tiny neighbors of zero as well
                // as the representable values next to 64-bit integer bounds.
                let literal = format!("{value:.17e}");
                let truncated = value.trunc() as i128;
                let expected = if (min..=max).contains(&truncated) {
                    truncated
                } else {
                    7
                };
                source.push_str(&format!("{{\nfix input: {from} = {literal}\nfix result = input as {to} or 7\nif result != {expected} {{ return 1 }}\n}}\n"));
            }
            for special in ["nan", "inf", "-inf"] {
                source.push_str(&format!("{{\nfix input: {from} = {special}\nif (input as {to} or 7) != 7 {{ return 2 }}\n}}\n"));
            }
        }
    }
    source.push_str(
        r#"fix constant = (255.75 as u8) as i16
    if constant != 255 { return 3 }
    fix zero = -0.75 as u8
    if zero != 0 { return 4 }
    fix failure = (nan as u16 or 255) as u8
    if failure != 255 { return 5 }
    fix arithmetic = ((2.0 * 128.0) as u16) as i16
    if arithmetic != 256 { return 6 }
    fix signed = (-128.75 as i16) as i8
    if signed != -128 { return 7 }
    return 0
}
"#,
    );
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("float-to-int.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    // The sanitizer also rejects an accidentally executed out-of-range C cast.
    for optimization in ["-O0", "-O2"] {
        let executable = directory.0.join(format!("float-to-int{optimization}"));
        let built = runtime_link::compiler()
            .args([
                "-std=c23",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsanitize=float-cast-overflow",
                "-fno-sanitize-recover=all",
                optimization,
            ])
            .arg(&c_file)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let result = std::process::Command::new(executable).output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
fn float_to_integer_casts_require_handlers_and_check_fallback_types() {
    for (body, rule) in [
        ("fix x: f32 = 1.0\nreturn x as u8", "expr.cast-handler"),
        ("return 256.0 as u8", "const.cast-check"),
        ("return -1.0 as u8", "const.cast-check"),
        ("return nan as u8", "const.cast-check"),
        ("return inf as u8", "const.cast-check"),
        (
            "fix x = 9223372036854775808.0 as i64\nreturn 0",
            "const.cast-check",
        ),
        (
            "fix x = 18446744073709551616.0 as u64\nreturn 0",
            "const.cast-check",
        ),
        (
            "fix x = true || ((nan as u8) == 0)\nreturn 0",
            "const.cast-check",
        ),
        ("return 1.0 as u8 or false", "type.expected"),
        ("return 1.0 as u8 or 0.0", "type.expected"),
        ("return 1.0 as u8 or (nan as u8)", "const.cast-check"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "{body}: {error:?}");
    }
}
