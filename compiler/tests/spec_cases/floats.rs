use crate::support::runtime_link;
use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

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
    fix integer: f32 = 16777217
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
