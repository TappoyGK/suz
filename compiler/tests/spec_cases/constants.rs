use crate::integer_bounds::INTEGER_BOUNDS;
use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn constants_resolve_forward_references_and_evaluate_typed_values() {
    let mut source = String::from(
        r#"const answer: u8 = later
const wrapped: u8 = 255 +% 1
const bits = (-1 as i8) as% u8
const fractional = 255.75 as u8
const near_max: f64 = 3.4028235e38
const rounded = near_max as f32
const zero: f64 = -0.0
const tiny: f32 = 1.401298464324817e-45
const converted = nan as u8 or 7
const equal = 1 < 2 == 2 < 3
const ordered = 3 >= 3 > 1
const truth = not false && equal && ordered && (false == false)
const invalid = nan
const unequal = invalid != invalid
const unordered = invalid < 0.0
const shortcut = true || ((300 as u8 or 0) == 0)
const scaled: f32 = (16777216.0 + 1.0) * 0.5
const rounded_integer = (((9007199791611905 as f32) as u64) -% 9007200328482816) as u8
"#,
    );
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!(
            "const min_{ty}: {ty} = {min}\nconst max_{ty}: {ty} = {max}\n"
        ));
    }
    source.push_str("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!(
            "if min_{ty} != {min} || max_{ty} != {max} {{ return 1 }}\n"
        ));
    }
    source.push_str(
        r#"    if answer != 7 || wrapped != 0 || bits != 255 || fractional != 255 { return 2 }
    if rounded != 3.4028235e38 || tiny != 1.401298464324817e-45 { return 3 }
    if 1.0 / zero != -inf || converted != 7 { return 4 }
    if not truth || not unequal || unordered || not shortcut { return 5 }
    if scaled != 8388608.0 || rounded_integer != 0 { return 6 }
    fix proven = small as u8
    if proven != 7 { return 7 }
    return 0
}
const small: i64 = 7
const later: u8 = small as u8
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
            "const a = a\nfn main() -> u8 { return 0 }",
            "const.initializer",
        ),
        (
            "const a = b\nconst b = a\nfn main() -> u8 { return 0 }",
            "const.initializer",
        ),
        (
            "const a = true || b\nconst b = a\nfn main() -> u8 { return 0 }",
            "const.initializer",
        ),
        (
            "const a = missing\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "const a = main\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "const a = 1\nconst a = true\nfn main() -> u8 { return 0 }",
            "decl.duplicate",
        ),
        (
            "const main = 1\nfn main() -> u8 { return 0 }",
            "decl.duplicate",
        ),
        (
            "const value = 1\nfn main() -> u8 { fix value = 2\nreturn 0 }",
            "decl.shadowing",
        ),
        (
            "fn main() -> u8 { const value = 1\nreturn 0 }",
            "decl.location",
        ),
        (
            "fix value = 1\nfn main() -> u8 { return 0 }",
            "decl.location",
        ),
        (
            "const value = 1\nfn main() -> u8 { value = 2\nreturn 0 }",
            "const.decl",
        ),
        (
            "const value = 1\nfn main() -> u8 { value +%= 2\nreturn 0 }",
            "const.decl",
        ),
        (
            "const value: u8\nfn main() -> u8 { return 0 }",
            "const.decl",
        ),
        (
            "const value = ifexpr true { 1 } else { 2 }\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "const value = true || expr { false }\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "const value = true || unknown()\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "const value = 1 as u8 or return 0\nfn main() -> u8 { return 0 }",
            "const.expression",
        ),
        (
            "const value = true || ((300 as u8) == 0)\nfn main() -> u8 { return 0 }",
            "const.cast-check",
        ),
        (
            "const value: u8 = false\nfn main() -> u8 { return 0 }",
            "type.expected",
        ),
        (
            "const value = true || (false == 1)\nfn main() -> u8 { return 0 }",
            "type.expected",
        ),
        (
            "const value = 1\nfn main() -> u8 { return value }",
            "type.expected",
        ),
        (
            "const value = 1 fn main() -> u8 { return 0 }",
            "src.statement-end",
        ),
        ("const value = 1", "prog.main"),
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
        assert!(error.message.contains(rule), "{source}: {error:?}");
    }
}

#[test]
fn constant_arithmetic_diagnostics_are_reported_once_at_the_declaration() {
    let source = b"const alias = negative\nconst negative = -1\nfn main() -> u8 {\nfix first = alias as f32\nfix second = negative as f64\nreturn 0\n}";
    let result = suz::compile_with_mode(source, suz::ArithmeticMode::Warn).unwrap();
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        (result.warnings[0].line, result.warnings[0].column),
        (2, 18)
    );
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert_eq!((error.line, error.column), (2, 18));
    assert!(error.message.contains("diagnostic.arithmetic"));
}
