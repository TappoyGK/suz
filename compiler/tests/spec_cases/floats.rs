use crate::support::runtime_link;
use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
fn integer_float_literals_require_exact_values_in_every_context() {
    for (declarations, body) in [
        ("", "fix x: f32 = 16777217"),
        ("", "fix x: f32 = -16_777_217"),
        ("", "fix x: f32 = (16777217)"),
        ("", "fix x: f64 = 9007199254740993"),
        ("", "var x: f32 = 0\nx = 16777217"),
        ("fn f(x: f32) -> void {}", "f(16777217)"),
        ("fn f() -> f32 { return 16777217 }", ""),
        (
            "struct Item { value: f32 }",
            "fix x = Item { value: 16777217 }",
        ),
        ("", "fix x: f32 = ifexpr true { 16777217 } else { 0.5 }"),
        ("", "fix x = ifexpr true { 9007199254740993 } else { 0.5 }"),
        ("", "fix x: f32 = 2.0\nfix y = x * 16777217"),
        ("", "fix x: f32 = 2.0\nfix y = 16777217 * x"),
        (
            "fn first[T](a: T, b: T) -> T { return a }",
            "fix x: f32 = 2.0\nfix y = first(16777217, x)",
        ),
        (
            "fn f(x: float) -> float { return x }",
            "fix x: f32 = f(16777217)",
        ),
        (
            "fn f(x: float) -> float { return x + 16777217 }",
            "fix x: f32 = 0\nfix y = f(x)",
        ),
        ("fn f(x: float) -> float { return 9007199254740993 }", ""),
    ] {
        let source = format!("{declarations}\nfn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.rule() == "lit.integer-as-float"
                || error
                    .related
                    .iter()
                    .any(|cause| cause.rule() == "lit.integer-as-float"),
            "{source}\n{error:?}"
        );
    }
    let source = b"fn main() -> u8 {\n    fix x: f32 = -16777217\n    return 0\n}";
    let error = suz::compile(source).unwrap_err();
    assert!(error.message().contains("-16777217 cannot be represented exactly as f32 (nearest is -16777216.0); write -16777217.0 or use as"), "{error:?}");
}

#[test]
fn generic_integer_float_diagnostics_show_each_candidate_once() {
    for sign in ["", "-"] {
        // intとの組み合わせが増えても、同じfloat型の診断を繰り返さない。
        let source = format!(
            "fn h(n: int, x: float) -> float {{ return x * {sign}9007199254740993 }}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "lit.integer-as-float");
        assert!(error.message().contains(&format!("{sign}9007199254740993 cannot be represented exactly as float; write {sign}9007199254740993.0 or use as")), "{error:?}");
        assert_eq!(error.related.len(), 2, "{error:?}");
        for ty in ["f32", "f64"] {
            assert!(error.related.iter().any(|cause| cause.message().contains(&format!("{sign}9007199254740993 cannot be represented exactly as {ty} (nearest is {sign}9007199254740992.0); write {sign}9007199254740993.0 or use as"))), "{error:?}");
        }
    }
    // f32では範囲外、f64では精度不足となる場合も、両方の理由を示す。
    let source = b"fn h(x: float) -> float { return x * 340282366920938463463374607431768211457 }\nfn main() -> u8 { return 0 }";
    let error = suz::compile(source).unwrap_err();
    assert_eq!(error.rule(), "lit.integer-as-float");
    assert_eq!(error.related.len(), 2, "{error:?}");
    assert!(
        error
            .related
            .iter()
            .any(|cause| cause.rule() == "lit.float-range"
                && cause.message().contains("out of range for f32")),
        "{error:?}"
    );
    assert!(
        error
            .related
            .iter()
            .any(|cause| cause.rule() == "lit.integer-as-float"
                && cause
                    .message()
                    .contains("as f64 (nearest is 340282366920938463463374607431768211456.0)")),
        "{error:?}"
    );
}

#[test]
fn integer_float_literals_accept_exact_values_beyond_integer_type_ranges() {
    for (ty, exact, inexact) in [
        ("f32", "16777218", "16777219"),
        ("f64", "9007199254740994", "9007199254740995"),
        // 2^128と2^200。u128に戻す判定や、一律の大きさ制限では扱えない。
        (
            "f64",
            "340282366920938463463374607431768211456",
            "340282366920938463463374607431768211457",
        ),
        (
            "f64",
            "1606938044258990275541962092341162602522202993782792835301376",
            "1606938044258990275541962092341162602522202993782792835301377",
        ),
    ] {
        for sign in ["", "-"] {
            let source = format!("fn main() -> u8 {{ fix x: {ty} = {sign}{exact}\nreturn 0 }}");
            suz::compile(source.as_bytes()).unwrap();
            let source = source.replace(exact, inexact);
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert!(
                error.message().contains("[lit.integer-as-float]"),
                "{error:?}"
            );
        }
    }
    // 同じ汎用本体をf64で具体化すれば、16777217を正確に表せる。
    suz::compile(b"fn f(x: float) -> float { return x + 16777217 }\nfn main() -> u8 { fix x: f64 = 0\nfix y = f(x)\nreturn 0 }").unwrap();
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn float_literals_and_comparisons_preserve_types_and_special_values() {
    let source = r#"fn main() -> u8 {
    fix separated: f32 = 16_777_216.0
    fix fraction = 0.123_456
    fix exponent = -1_2.5_0e-0_1
    if separated != 16777216.0 || fraction != 0.123456 || exponent != -1.25 { return 10 }
    fix narrow: f32 = 0.5
    fix wide = 0.5
    if narrow != 0.5 || 0.5 != narrow || wide != 0.5 { return 1 }
    fix integer: f32 = 16777217.0
    fix tie: f32 = 1.000000059604644775390625
    fix above_tie: f32 = 1.0000000596046447753906250000000000000001
    if integer != 16777216.0 || tie != 1.0 || above_tie != 1.00000011920928955078125 { return 2 }
    fix maximum: f32 = 3.4028235e38
    fix tiny: f32 = 1.0e-50
    fix subnormal: f32 = 1.401298464324817070923729583289916131280e-45
    if not (0.0 < subnormal < maximum < inf) || tiny != 0.0 { return 3 }
    fix invalid: f32 = nan
    if invalid == invalid || invalid < 0.0 || invalid <= 0.0 || invalid > 0.0 || invalid >= 0.0 { return 4 }
    if not (invalid != invalid) || 0.0 < invalid < inf { return 5 }
    if not (-inf < -1.0 < 0.0 < inf) || inf != inf || -0.0 != 0.0 { return 6 }
    if not (0 < 0.5 < 1) || not (1 > 0.5 > 0) { return 7 }
    fix chosen = ifexpr true { 0.25 } else { narrow }
    fix mixed = ifexpr false { 1 } else { 0.5 }
    if chosen != 0.25 || mixed != 0.5 { return 8 }
    var current: f32 = -1
    current = 1.25e-2
    return ifexpr current == 0.0125 { 0 } else { 9 }
}
"#;
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("floats.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("floats"));
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn float_types_reject_overflow_and_implicit_conversions() {
    for (body, rule) in [
        ("return 1.0", "type.expected"),
        ("fix x: f32 = 1.0e40\nreturn 0", "lit.float-range"),
        ("fix x = 1.0e400\nreturn 0", "lit.float-range"),
        ("fix x: u8 = 1.0\nreturn 0", "type.expected"),
        ("fix x = 1\nfix y: f32 = x\nreturn 0", "type.expected"),
        ("fix x = 1.0 & 1.0\nreturn 0", "expr.operator-types"),
        ("fix x = 1.0 +% 1.0\nreturn 0", "expr.operator-types"),
        ("return 1.0 as% u8", "expr.as-trunc"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().contains(rule), "{error:?}");
    }
    let source = b"fn main() -> u8 { fix x: f32 = -1\nfix y: f64 = -0\nfix z = -inf\nreturn 0 }";
    for mode in [
        suz::ArithmeticMode::Default,
        suz::ArithmeticMode::Warn,
        suz::ArithmeticMode::Deny,
    ] {
        assert!(
            suz::compile_with_mode(source, mode)
                .unwrap()
                .warnings
                .is_empty()
        );
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn float_arithmetic_preserves_rounding_and_ieee_results_when_optimized() {
    let source = r#"fn main() -> u8 {
    fix positive: f32 = 3.5
    fix negative = -positive
    if negative != -3.5 || -negative != positive { return 1 }
    fix mixed = 1 + 0.5
    if mixed != 1.5 || 2.0 + 3.0 * 4.0 != 14.0 || 10.0 - 3.0 - 2.0 != 5.0 { return 2 }
    fix base: f32 = 16777216.0
    fix next: f32 = 16777218.0
    if base + 1.0 != base || next + 1.0 != 16777220.0 { return 3 }
    fix wide: f64 = 9007199254740992.0
    if wide + 1.0 != wide { return 4 }
    fix maximum: f32 = 3.4028235e38
    if maximum * 2.0 != inf || -maximum * 2.0 != -inf { return 5 }
    fix zero: f32 = 0.0
    fix negative_zero = -zero
    if 1.0 / negative_zero != -inf || 1.0 / -negative_zero != inf { return 6 }
    fix invalid = zero / zero
    fix infinity_difference = inf - inf
    if invalid == invalid || infinity_difference == infinity_difference { return 7 }
    fix tiny: f32 = 1.401298464324817070923729583289916131280e-45
    if tiny / 2.0 != 0.0 { return 8 }
    fix left: f32 = 1.00000011920928955078125
    fix right: f32 = 0.99999988079071044921875
    if left * right - 1.0 != 0.0 { return 9 }
    var value: f32 = 1
    value += 2
    value *= value
    value -= 1
    value /= 4
    return ifexpr value == 2.0 && 1.0 < value < 3.0 { 0 } else { 10 }
}
"#;
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("float-arithmetic.c");
    let executable = directory.0.join("float-arithmetic");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let built = runtime_link::compiler()
        .args(["-std=c23", "-O2", "-Wall", "-Wextra", "-Werror"])
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
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn float_arithmetic_checks_types_and_stays_out_of_integer_diagnostics() {
    for (body, rule) in [
        ("fix x = -true\nreturn 0", "expr.operator-types"),
        ("fix x = true + false\nreturn 0", "expr.operator-types"),
        ("fix x: f32 = 1.0\nx += 1.0\nreturn 0", "var.fix-assign"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().contains(rule), "{error:?}");
    }
    let source = b"fn main() -> u8 { var x: f32 = -(1.0)\nx += -x * (2.0 - 1.0) / 2.0\nreturn 0 }";
    for mode in [
        suz::ArithmeticMode::Default,
        suz::ArithmeticMode::Warn,
        suz::ArithmeticMode::Deny,
    ] {
        assert!(
            suz::compile_with_mode(source, mode)
                .unwrap()
                .warnings
                .is_empty()
        );
    }
    let source = b"fn main() -> u8 { fix x: i8 = 1\nfix y = -x\nreturn 0 }";
    assert_eq!(
        suz::compile_with_mode(source, suz::ArithmeticMode::Warn)
            .unwrap()
            .warnings
            .len(),
        1
    );
}

#[test]
fn entry_restores_floating_point_environment_before_suz_runs() {
    let directory = TemporaryDirectory::new();
    // 浮動小数点式のないプログラムも環境を初期化する必要がある。
    // テスト用コードが演算する前にフラグを確認する。
    let generated = suz::compile(b"fn main() -> u8 { return 7 }").unwrap();
    fs::write(directory.0.join("entry.c"), generated).unwrap();
    let driver = directory.0.join("driver.c");
    fs::write(
        &driver,
        r#"
#include <fenv.h>
#include <math.h>
#pragma STDC FENV_ACCESS ON
#define main suz_entry
#include "entry.c"
#undef main

int main(void) {
    const int modes[] = { FE_UPWARD, FE_DOWNWARD, FE_TOWARDZERO };
    for (unsigned i = 0; i < sizeof(modes) / sizeof(modes[0]); ++i) {
        if (fesetround(modes[i]) != 0) return 1;
        if (feraiseexcept(FE_INVALID | FE_DIVBYZERO | FE_INEXACT) != 0) return 2;
#if defined(__APPLE__) && defined(__aarch64__)
        // Cには移植可能なトラップ・ゼロへのフラッシュ制御がない。
        // ホストが提供する設定も試し、丸めモードだけを戻す実装では通らないようにする。
        fenv_t dirty;
        if (fegetenv(&dirty) != 0) return 11;
        dirty.__fpcr |= __fpcr_flush_to_zero | __fpcr_trap_invalid | __fpcr_trap_divbyzero;
        if (fesetenv(&dirty) != 0) return 12;
#endif
        if (suz_entry() != 7) return 3;
        if (fegetround() != FE_TONEAREST) return 4;
        if (fetestexcept(FE_ALL_EXCEPT) != 0) return 5;
        volatile float base = 16777216.0f, next = 16777218.0f, step = 1.0f;
        if (base + step != base || next + step != 16777220.0f) return 6;
        volatile double wide = 9007199254740992.0, one = 1.0;
        if (wide + one != wide) return 7;
        volatile float zero = 0.0f, tiny = 0x1p-149f;
        if (!isinf(step / zero) || !isnan(zero / zero)) return 8;
        if (tiny * step != tiny || tiny / 2.0f != 0.0f) return 9;
        if (!signbit(-zero)) return 10;
    }
    return 0;
}
"#,
    )
    .unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = directory.0.join(format!("entry{optimization}"));
        let built = runtime_link::compiler()
            .args(["-std=c23", "-Wall", "-Wextra", "-Werror", optimization])
            .arg(&driver)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let result = std::process::Command::new(&executable).output().unwrap();
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
fn float_remainder_and_euclidean_remainder_work_at_runtime_and_compile_time() {
    let source = r#"fix C_F64_REM: f64 = -7.5 % 2.0
fix C_F32_REM: f32 = -7.5 % 2.0
fix C_F64_EUC: f64 = -0.25 %% 1.0
fix C_F32_EUC: f32 = -0.25 %% 1.0
fix C_F64_TINY: f64 = -1.0e-20 %% 1.0
fix C_F32_TINY: f32 = -1.0e-10 %% 1.0
fix C_F64_NEG_DIV: f64 = -0.25 %% -1.0
fix C_F64_INF: f64 = 1.0 %% inf
fix C_F64_NEG_INF: f64 = -1.0 %% -inf
fix C_F32_INF: f32 = 1.0 %% inf
fix C_F32_NEG_INF: f32 = -1.0 %% -inf
fix C_I8_EUC: i8 = -7 %% 3
fix C_I8_EUC_NEG_DIV: i8 = -7 %% -3
fix C_I8_MIN_EUC: i8 = (-128 as i8) %% -1
fix C_LEN_EUC: len_t = (10 as len_t) %% 3

fn main() -> u8 {
    if C_F64_REM != -1.5 || C_F32_REM != -1.5 { return 1 }
    if C_F64_EUC != 0.75 || C_F32_EUC != 0.75 { return 2 }
    if C_F64_TINY != 0.0 || C_F32_TINY != 0.0 { return 3 }
    if C_F64_NEG_DIV != 0.75 { return 4 }
    if C_F64_INF == C_F64_INF || C_F64_NEG_INF == C_F64_NEG_INF { return 15 }
    if C_F32_INF == C_F32_INF || C_F32_NEG_INF == C_F32_NEG_INF { return 16 }
    if C_I8_EUC != 2 || C_I8_EUC_NEG_DIV != 2 || C_I8_MIN_EUC != 0 || C_LEN_EUC != 1 { return 5 }

    var phase: f64 = -0.25
    phase %%= 1.0
    if phase != 0.75 { return 6 }

    var f32_val: f32 = -7.5
    f32_val %= 2.0
    if f32_val != -1.5 { return 7 }

    var tiny64: f64 = -1.0e-20
    tiny64 %%= 1.0
    if tiny64 != 0.0 { return 8 }

    var tiny32: f32 = -1.0e-10
    tiny32 %%= 1.0
    if tiny32 != 0.0 { return 9 }

    // 浮動小数点の 0 除算および右辺 inf/-inf はパニックせず NaN になる
    fix zero64: f64 = 0.0
    fix pinf64: f64 = 1.0 / zero64
    fix ninf64: f64 = 0.0 - pinf64
    fix one64: f64 = 1.0
    fix neg64: f64 = -1.0
    fix nan_rem = 1.0 % zero64
    fix nan_euc = 1.0 %% zero64
    fix nan_inf1 = one64 %% pinf64
    fix nan_inf2 = neg64 %% pinf64
    fix nan_inf3 = one64 %% ninf64
    fix nan_inf4 = neg64 %% ninf64
    if nan_rem == nan_rem || nan_euc == nan_euc { return 10 }
    if nan_inf1 == nan_inf1 || nan_inf2 == nan_inf2 || nan_inf3 == nan_inf3 || nan_inf4 == nan_inf4 { return 17 }
    fix zero32: f32 = 0.0
    fix pinf32: f32 = 1.0 / zero32
    fix ninf32: f32 = 0.0 - pinf32
    fix one32: f32 = 1.0
    fix neg32: f32 = -1.0
    fix nan32_1 = one32 %% pinf32
    fix nan32_2 = neg32 %% pinf32
    fix nan32_3 = one32 %% ninf32
    fix nan32_4 = neg32 %% ninf32
    if nan32_1 == nan32_1 || nan32_2 == nan32_2 || nan32_3 == nan32_3 || nan32_4 == nan32_4 { return 18 }

    // 整数の %% と %%= および 0 除算の or ハンドラ（2のべき乗最適化・i64最小値境界を含む）
    var idx: i32 = -1
    idx %%= 10
    if idx != 9 { return 11 }
    fix i64_min: i64 = -9223372036854775808
    fix ring_wrap: i64 = (i64_min - 1) %% 1024
    if ring_wrap != 1023 { return 19 }
    fix zero_i32: i32 = 0
    fix fallback: i32 = (7 %% zero_i32) or 42
    if fallback != 42 { return 12 }

    // 整数と浮動小数点の混在（[arith.mix]）
    fix n: i16 = -7
    fix mixed_rem: f64 = n % 2.0
    fix mixed_euc: f64 = n %% 2.0
    if mixed_rem != -1.0 || mixed_euc != 1.0 { return 13 }

    // -0.0 %% 1.0 は +0.0（文字列表示が "0.0"）
    fix neg_zero: f64 = -0.0
    fix pos_zero = neg_zero %% 1.0
    if textbuf `${pos_zero}` != `0.0` { return 14 }

    return 0
}
"#;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("float-rem.c");
    fs::write(&path, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let output = build_and_run_c(&path, &directory.0.join("float-rem"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());

    // 整数の定数ゼロ除算（[arith.div-zero-always]）
    let bad = b"fn main() -> u8 {\n    fix x: i32 = 7\n    fix y: i32 = x %% 0\n    return 0\n}\n";
    let error = suz::compile(bad).unwrap_err();
    assert!(
        error.message().contains("arith.div-zero-always"),
        "{error:?}"
    );
}
