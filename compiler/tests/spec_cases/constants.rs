use crate::integer_bounds::INTEGER_BOUNDS;
use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn constants_resolve_forward_references_and_evaluate_typed_values() {
    let mut source = String::from(
        r#"fix ANSWER: u8 = LATER
fix WRAPPED: u8 = 255 +% 1
fix BITS = (-1 as i8) as% u8
fix FRACTIONAL = 255.75 as u8
fix NEAR_MAX: f64 = 3.4028235e38
fix ROUNDED = NEAR_MAX as f32
fix ZERO: f64 = -0.0
fix TINY: f32 = 1.401298464324817e-45
fix CONVERTED = nan as u8 or 7
fix EQUAL = 1 < 2 == 2 < 3
fix ORDERED = 3 >= 3 > 1
fix TRUTH = not false && EQUAL && ORDERED && (false == false)
fix INVALID = nan
fix UNEQUAL = INVALID != INVALID
fix UNORDERED = INVALID < 0.0
fix SHORTCUT = true || ((300 as u8 or 0) == 0)
fix SCALED: f32 = (16777216.0 + 1.0) * 0.5
fix ROUNDED_INTEGER = (((9007199791611905 as f32) as u64) -% 9007200328482816) as u8
"#,
    );
    for (ty, min, max) in INTEGER_BOUNDS {
        let suffix = ty.to_ascii_uppercase();
        source.push_str(&format!(
            "fix MIN_{suffix}: {ty} = {min}\nfix MAX_{suffix}: {ty} = {max}\n"
        ));
    }
    source.push_str("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        let suffix = ty.to_ascii_uppercase();
        source.push_str(&format!(
            "if MIN_{suffix} != {min} || MAX_{suffix} != {max} {{ return 1 }}\n"
        ));
    }
    source.push_str(
        r#"    if ANSWER != 7 || WRAPPED != 0 || BITS != 255 || FRACTIONAL != 255 { return 2 }
    if ROUNDED != 3.4028235e38 || TINY != 1.401298464324817e-45 { return 3 }
    if 1.0 / ZERO != -inf || CONVERTED != 7 { return 4 }
    if not TRUTH || not UNEQUAL || UNORDERED || not SHORTCUT { return 5 }
    if SCALED != 8388608.0 || ROUNDED_INTEGER != 0 { return 6 }
    fix proven = SMALL as u8
    if proven != 7 { return 7 }
    return 0
}
fix SMALL: i64 = 7
fix LATER: u8 = SMALL as u8
"#,
    );
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("constants.c");
    fs::write(&path, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&path, &directory.0.join("constants"));
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn constant_declarations_check_cycles_names_scope_and_unevaluated_expressions() {
    for (source, rule) in [
        (
            "fix A = A\nfn main() -> u8 { return 0 }",
            "const.initializer",
        ),
        (
            "fix A = B\nfix B = A\nfn main() -> u8 { return 0 }",
            "const.initializer",
        ),
        (
            "fix A = true || B\nfix B = A\nfn main() -> u8 { return 0 }",
            "const.initializer",
        ),
        (
            "fix A = missing\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "fix A = main\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "fix A = 1\nfix A = true\nfn main() -> u8 { return 0 }",
            "decl.duplicate",
        ),
        (
            "fix main = 1\nfn main() -> u8 { return 0 }",
            "name.top-level-binding",
        ),
        (
            "fn value() -> u8 { return 1 }\nfn main() -> u8 { fix value = 2\nreturn 0 }",
            "decl.shadowing",
        ),
        ("fn main() -> u8 { fix VALUE = 1\nreturn 0 }", "name.value"),
        (
            "fix value = 1\nfn main() -> u8 { return 0 }",
            "name.top-level-binding",
        ),
        (
            "fix VALUE = 1\nfn main() -> u8 { VALUE = 2\nreturn 0 }",
            "const.decl",
        ),
        (
            "fix VALUE = 1\nfn main() -> u8 { VALUE +%= 2\nreturn 0 }",
            "const.decl",
        ),
        ("fix VALUE: u8\nfn main() -> u8 { return 0 }", "const.decl"),
        (
            "fix VALUE = ifexpr true { 1 } else { 2 }\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "fix VALUE = true || expr { false }\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "fix VALUE = true || unknown()\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "fix VALUE = 1 as u8 or return 0\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "fix VALUE = true || ((300 as u8) == 0)\nfn main() -> u8 { return 0 }",
            "const.cast-check",
        ),
        (
            "fix VALUE: u8 = false\nfn main() -> u8 { return 0 }",
            "type.expected",
        ),
        (
            "fix VALUE = true || (false == 1)\nfn main() -> u8 { return 0 }",
            "type.expected",
        ),
        (
            "fix VALUE = 1\nfn main() -> u8 { return VALUE }",
            "type.expected",
        ),
        (
            "fix VALUE = 1 fn main() -> u8 { return 0 }",
            "src.statement-end",
        ),
        ("fix VALUE = 1", "prog.main"),
        (
            "fn main() -> u8 { return 0 }\nfn main() -> u8 { return 1 }",
            "prog.main",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}: {error:?}"
        );
        assert!(error.message().contains(rule), "{source}: {error:?}");
    }
}

#[test]
fn constant_arithmetic_diagnostics_are_reported_once_at_the_declaration() {
    let source = b"fix ALIAS = NEGATIVE\nfix NEGATIVE = -1\nfn main() -> u8 {\nfix first = ALIAS as f32\nfix second = NEGATIVE as f64\nreturn 0\n}";
    let result = suz::compile_with_mode(source, suz::ArithmeticMode::Warn).unwrap();
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        (result.warnings[0].line, result.warnings[0].column),
        (2, 16)
    );
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert_eq!((error.line, error.column), (2, 16));
    assert!(error.message().contains("diagnostic.arithmetic"));
}
