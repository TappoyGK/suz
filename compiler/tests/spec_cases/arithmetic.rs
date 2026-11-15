use crate::integer_bounds::INTEGER_BOUNDS;
use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::{fs, process::Command};

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn integer_arithmetic_and_compound_assignments_preserve_type_boundaries() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!("{{\nfix lo: {ty} = {min}\nfix hi: {ty} = {max}\nif lo + 1 != {} || hi - 1 != {} {{ return 1 }}\nif lo * 1 != lo || hi * 1 != hi {{ return 2 }}\nvar value: {ty} = 2\nvalue += 3\nvalue *= 4\nvalue -= 1\nif value != 19 {{ return 3 }}\n}}\n", min+1, max-1));
    }
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!("{{\nfix lo: {ty} = {min}\nfix hi: {ty} = {max}\nif (hi + 1 or 7) != 7 || (lo - 1 or 7) != 7 || (hi * 2 or 7) != 7 {{ return 8 }}\nif (hi + 0 or (hi + 1)) != hi {{ return 9 }}\n}}\n"));
    }
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!("{{\nfix lo: {ty} = {min}\nfix hi: {ty} = {max}\nif hi / 3 != {} || hi % 3 != {} {{ return 10 }}\nif lo / 3 != {} || lo % 3 != {} {{ return 11 }}\nif (hi / 0 or 7) != 7 || (hi % 0 or 7) != 7 {{ return 12 }}\nvar value: {ty} = 23\nvalue /= 3\nvalue %= 4\nif value != 3 {{ return 13 }}\n", max / 3, max % 3, min / 3, min % 3));
        if min < 0 {
            source.push_str("if (lo / -1 or 7) != 7 || (lo % -1 or 7) != 7 { return 14 }\nif 7 / -3 != -2 || 7 % -3 != 1 || -7 / -3 != 2 || -7 % -3 != -1 { return 15 }\n");
        }
        source.push_str("}\n");
    }
    for (ty, min, max) in INTEGER_BOUNDS.into_iter().filter(|(_, min, _)| *min < 0) {
        source.push_str(&format!("{{\nfix lo: {ty} = {min}\nfix hi: {ty} = {max}\nfix zero: {ty} = 0\nif -hi != {} || -(lo + 1) != hi || --hi != hi || -zero != 0 {{ return 16 }}\nif (-lo or 7) != 7 {{ return 17 }}\nfix literal: {ty} = {min} or (-lo)\nif literal != lo || -(1) != -1 {{ return 18 }}\n}}\n", -max));
    }
    source.push_str(
        r#"var count: u8 = 0
    while count < 3 { count += 1 }
    fix condition = true || ((255 as u8) + 1 == 0)
    if not condition { return 4 }
    fix selected: u8 = ifexpr true { 7 } else { (255 as u8) + 1 }
    if selected != 7 { return 5 }
    fix raw: i64 = 4
    count += raw as u8 or return 9
    return ifexpr count == 7 { 0 } else { 6 }
}
"#,
    );
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("arithmetic.c");
    fs::write(&path, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&path, &directory.0.join("arithmetic"));
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn integer_arithmetic_failures_panic_at_the_suz_operator_even_when_optimized() {
    let directory = TemporaryDirectory::new();
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    for (index, (body, line, column, reason)) in [
        (
            "fix a: u8 = 1\nfix value = a << 8",
            3,
            15,
            "shift count out of range [rt.shift-count]",
        ),
        (
            "fix a: i64 = 1\nfix value = a >> -1",
            3,
            15,
            "shift count out of range [rt.shift-count]",
        ),
        (
            "fix a: i64 = 1\nfix value = a << 63",
            3,
            15,
            "integer left shift overflow [rt.shift-overflow]",
        ),
        (
            "var count: u8 = 128\ncount <<= 1",
            3,
            7,
            "integer left shift overflow [rt.shift-overflow]",
        ),
        (
            "var count: u8 = 1\ncount >>= 8",
            3,
            7,
            "shift count out of range [rt.shift-count]",
        ),
        (
            "fix a: i8 = -128\nfix value = -a",
            3,
            13,
            "integer negation overflow [rt.int-overflow]",
        ),
        (
            "fix a: i64 = -9223372036854775808\nfix value = -a",
            3,
            13,
            "integer negation overflow [rt.int-overflow]",
        ),
        (
            "fix a: i8 = -128\nfix value = -(-a) or 7",
            3,
            15,
            "integer negation overflow [rt.int-overflow]",
        ),
        (
            "fix a: u8 = 255\nfix value = a + 1",
            3,
            15,
            "integer addition overflow [rt.int-overflow]",
        ),
        (
            "fix a: i64 = -9223372036854775808\nfix value = a - 1",
            3,
            15,
            "integer subtraction overflow [rt.int-overflow]",
        ),
        (
            "fix a: u64 = 18446744073709551615\nfix value = a * 2",
            3,
            15,
            "integer multiplication overflow [rt.int-overflow]",
        ),
        (
            "var count: u8 = 255\ncount += 1",
            3,
            7,
            "integer addition overflow [rt.int-overflow]",
        ),
        (
            "fix a: u8 = 0\nfix value = 7 / a",
            3,
            15,
            "integer division by zero [rt.div-zero]",
        ),
        (
            "fix a: u64 = 0\nfix value = 7 % a",
            3,
            15,
            "integer remainder by zero [rt.div-zero]",
        ),
        (
            "fix a: i64 = -9223372036854775808\nfix value = a / -1",
            3,
            15,
            "integer division overflow [rt.div-overflow]",
        ),
        (
            "fix a: i8 = -128\nfix value = a % -1",
            3,
            15,
            "integer remainder overflow [rt.div-overflow]",
        ),
        (
            "var count: u8 = 7\ncount /= 0",
            3,
            7,
            "integer division by zero [rt.div-zero]",
        ),
        (
            "var count: i8 = -128\ncount %= -1",
            3,
            7,
            "integer remainder overflow [rt.div-overflow]",
        ),
        (
            "var count: u8 = 255\ncount += (1 / 1) or 0",
            3,
            7,
            "integer addition overflow [rt.int-overflow]",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!("fn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let name = "dir/日本語\"\\name.suz";
        let compiled =
            suz::compile_with_source_name(source.as_bytes(), suz::ArithmeticMode::Default, name)
                .unwrap();
        let path = directory.0.join(format!("panic-{index}.c"));
        fs::write(&path, compiled.c).unwrap();
        for optimization in ["-O0", "-O2"] {
            let executable = directory.0.join(format!("panic-{index}{optimization}"));
            let built = Command::new(std::env::var("CC").unwrap_or_else(|_| "clang".into()))
                .args([
                    "-std=c23",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-fsanitize=undefined",
                    "-fno-sanitize-recover=all",
                    optimization,
                ])
                .arg("-I")
                .arg(&runtime)
                .arg(&path)
                .arg(runtime.join("suz_runtime.c"))
                .arg("-lm")
                .arg(runtime.join("suz_env.c"))
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            let result = Command::new(executable).output().unwrap();
            assert_eq!(result.status.code(), Some(101));
            assert!(result.stdout.is_empty());
            assert_eq!(
                String::from_utf8(result.stderr).unwrap(),
                format!("panic {name}:{line}:{column}: {reason}\n")
            );
        }
    }
}

#[test]
fn constant_arithmetic_checks_only_evaluated_operations() {
    let source = br#"const MAX: u8 = 255
const SUM: u8 = BASE + 2
const BASE: u8 = 3
const PRODUCT = SUM * 3
const DIFFERENCE = PRODUCT - 1
const SHORT = true || (MAX + 1 == 0)
const CHAIN = 2 < 1 < (MAX + 1)
const FALLBACK: u8 = 1 as u8 or (MAX + 1)
const CONVERTED = (1 + 2) as u8
const HANDLED: u8 = 255 + 1 or 255 * 2 or 7
const LAZY: u8 = 1 + 2 or (255 + 1)
const MINIMUM: i8 = -128 or 0
const NEGATED = -MINIMUM or 7
const DIRECT: i8 = -(1)
const CONSTANT_LAZY: i8 = -128 or (1 / 0)
const QUOTIENT: i8 = -7 / 3
const REMAINDER: i8 = -7 % 3
const DIVISION_FALLBACK: u8 = 7 / 0 or 8
const REMAINDER_FALLBACK: i8 = -128 % -1 or 9
const LAZY_DIVISION: u8 = 1 + 2 or (1 / 0)
const NARROWED = (9223372036854775807 + 1 or 7) as u8
fn main() -> u8 {
    return ifexpr DIFFERENCE == 14 && SHORT && not CHAIN && FALLBACK == 1 && CONVERTED == 3 && HANDLED == 7 && LAZY == 3 && NARROWED == 7 && QUOTIENT == -2 && REMAINDER == -1 && DIVISION_FALLBACK == 8 && REMAINDER_FALLBACK == 9 && LAZY_DIVISION == 3 && NEGATED == 7 && DIRECT == -1 && CONSTANT_LAZY == MINIMUM { 0 } else { 1 }
}
"#;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("constants.c");
    fs::write(&path, suz::compile(source).unwrap()).unwrap();
    assert_eq!(
        build_and_run_c(&path, &directory.0.join("constants"))
            .status
            .code(),
        Some(0)
    );
    for source in [
        "const LO: i8 = -128\nconst BAD = -LO\nfn main() -> u8 { return 0 }",
        "const LO: i8 = -128\nconst BAD = -(-LO) or 7\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = 1 / 0\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = 1 % 0\nfn main() -> u8 { return 0 }",
        "const BAD: i8 = -128 / -1\nfn main() -> u8 { return 0 }",
        "const BAD: i8 = -128 % -1\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = (1 / 0) + 1 or 7\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = ((255 + 1) * 0) or 7\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = 255 + 1 or (255 + 1)\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = 255 + 1\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = 0 - 1\nfn main() -> u8 { return 0 }",
        "const BAD: u64 = 18446744073709551615 * 2\nfn main() -> u8 { return 0 }",
        "const MAX: u8 = 255\nconst BAD = false || (MAX + 1 == 0)\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = 300 as u8 or (255 + 1)\nfn main() -> u8 { return 0 }",
        "const MAX: u8 = 255\nconst BAD = (MAX + 1) as u16\nfn main() -> u8 { return 0 }",
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(
            error.message.contains("const.evaluation"),
            "{source}: {error:?}"
        );
    }
    let source = b"fn main() -> u8 {\nvar x: u8 = 1\nx += 1\nfix y = x * 2 - 1\nreturn y\n}";
    let warnings = suz::compile_with_mode(source, suz::ArithmeticMode::Warn)
        .unwrap()
        .warnings;
    assert_eq!(
        warnings
            .iter()
            .map(|w| (w.line, w.column))
            .collect::<Vec<_>>(),
        [(3, 3), (4, 11), (4, 15)]
    );
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert_eq!((error.line, error.column), (3, 3));
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn integer_operands_are_evaluated_once_left_to_right_and_stop_at_a_panic() {
    let directory = TemporaryDirectory::new();
    for (index, (value, expression, expected, trace)) in [
        (2, "a * b", 6, "12\n"),
        (255, "(a + 1) + b", 101, "1\n"),
        (255, "((a + 1) * b) or 7", 101, "1\n"),
        (2, "a * b or (a + b)", 6, "12\n"),
        (255, "a + b or (a - b)", 252, "1212\n"),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!(
            "fn main() -> u8 {{\nfix a: u8 = {value}\nfix b: u8 = 3\nreturn {expression}\n}}"
        );
        let generated = suz::compile(source.as_bytes()).unwrap();
        let marker = "return (suz_cmp_";
        assert_eq!(generated.matches(marker).count(), 1);
        let generated = generated.replace(marker, "#define suz_local_0 probe(1, suz_local_0)\n#define suz_local_1 probe(2, suz_local_1)\nreturn (suz_cmp_");
        let c = format!(
            r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
static int trace;
static uint8_t probe(int id, uint8_t value) {{ trace = trace * 10 + id; return value; }}
static void report(void) {{ printf("%d\n", trace); }}
#define main generated_main
{generated}
#undef main
int main(void) {{ atexit(report); return generated_main(); }}
"#
        );
        let path = directory.0.join(format!("order-{index}.c"));
        fs::write(&path, c).unwrap();
        let result = build_and_run_c(&path, &directory.0.join(format!("order-{index}")));
        assert_eq!(result.status.code(), Some(expected));
        assert_eq!(String::from_utf8(result.stdout).unwrap(), trace);
        if expected != 101 {
            assert!(result.stderr.is_empty());
        }
    }
}
