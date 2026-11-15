use crate::integer_bounds::INTEGER_BOUNDS;
use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn shifts_keep_left_types_and_accept_every_integer_count_type() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (left, min, max) in INTEGER_BOUNDS {
        let width = (max - min + 1).ilog2();
        for (right, _, count_max) in INTEGER_BOUNDS {
            source.push_str(&format!("{{\nvar value: {left} = 3\nfix count: {right} = 1\nfix huge: {right} = {count_max}\nif value << count != 6 || value >> count != 1 {{ return 1 }}\nvalue <<= count\nvalue >>= count\nif value != 3 || (value << huge or 7) != 7 || (value >> huge or 7) != 7 {{ return 2 }}\n}}\n"));
        }
        source.push_str(&format!("{{\nfix lo: {left} = {min}\nfix hi: {left} = {max}\nif lo >> 0 != lo || hi << 0 != hi {{ return 3 }}\nif (hi << 1 or 7) != 7 || (hi >> {width} or 7) != 7 {{ return 4 }}\n"));
        if min < 0 {
            source.push_str(&format!("fix negative: {left} = -7\nfix minus_two: {left} = -2\nif negative >> 1 != -4 || lo >> {} != -1 {{ return 5 }}\nif (-1 << {}) != lo || (minus_two << {} or 7) != 7 {{ return 6 }}\n", width - 1, width - 1, width - 1));
        } else {
            source.push_str(&format!(
                "fix one: {left} = 1\nif hi >> {} != 1 || (one << {}) != {} {{ return 7 }}\n",
                width - 1,
                width - 1,
                1_i128 << (width - 1)
            ));
        }
        source.push_str("}\n");
    }
    source.push_str(
        r#"fix count: u8 = 8
    fix inferred = 1 << count
    if inferred != 256 { return 8 }
    fix independent: u8 = 1 << 256 or 7
    if independent != 7 { return 9 }
    if 1 << 2 + 1 != 8 || 1 | 2 << 2 != 9 || 1 << 3 >> 2 != 2 { return 10 }
    if 1 + 1 << 2 != 8 || 8 >> 1 + 1 != 2 || 2 * 3 << 1 != 12 { return 11 }
    return 0
}
"#,
    );
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("shifts.c");
    fs::write(&path, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let output = build_and_run_c(&path, &directory.0.join("shifts"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn shifts_check_types_diagnostics_and_constant_evaluation() {
    for body in [
        "fix value = 1 << 1.0\nreturn 0",
        "fix value = 1.0 >> 1\nreturn 0",
        "fix value = true << 1\nreturn 0",
        "fix value = 1 >> false\nreturn 0",
        "fix count: f32 = 1.0\nfix value = 1 << count\nreturn 0",
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message.contains("expr.operator-types"), "{error:?}");
    }
    for declaration in [
        "const VALUE: u8 = 1 << 8",
        "const VALUE: i64 = 1 << 63",
        "const VALUE: u64 = 1 >> 64",
        "const VALUE: i8 = 1 >> -1",
        "const VALUE: i8 = -2 << 7",
        "const VALUE: u8 = ((1 << 8) >> 1) or 7",
    ] {
        let source = format!("{declaration}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message.contains("const.evaluation"),
            "{source}: {error:?}"
        );
    }
    let source = b"fn main() -> u8 {\nvar value: u8 = 1\nvalue <<= 1\nfix next = value >> 1 or 7\nreturn next\n}";
    let warnings = suz::compile_with_mode(source, suz::ArithmeticMode::Warn)
        .unwrap()
        .warnings;
    assert_eq!(
        warnings
            .iter()
            .map(|w| (w.line, w.column))
            .collect::<Vec<_>>(),
        [(3, 7)]
    );
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert_eq!((error.line, error.column), (3, 7));
    let source = br#"const COUNT: u8 = 8
const INFERRED = 1 << COUNT
const MINIMUM: i64 = -1 << 63
const SIGN_FILL: i64 = MINIMUM >> 63
const LARGEST_BIT: u64 = 1 << 63
const FALLBACK: u8 = 1 << 256 or 7
const LAZY: u8 = 1 << 1 or (1 / (0 as u8))
const SHORT = true || (1 << 64 == 0)
fn main() -> u8 {
    return ifexpr INFERRED == 256 && SIGN_FILL == -1 && LARGEST_BIT == 9223372036854775808 && FALLBACK == 7 && LAZY == 2 && SHORT { 0 } else { 1 }
}
"#;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("constant-shifts.c");
    fs::write(&path, suz::compile(source).unwrap()).unwrap();
    let output = build_and_run_c(&path, &directory.0.join("constant-shifts"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}
