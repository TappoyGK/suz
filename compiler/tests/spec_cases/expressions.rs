use crate::integer_bounds::INTEGER_BOUNDS;
use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn logical_operators_follow_truth_tables_and_precedence() {
    let directory = TemporaryDirectory::new();
    for (index, (expression, expected)) in [
        ("false && false", false),
        ("false && true", false),
        ("true && false", false),
        ("true && true", true),
        ("false || false", false),
        ("false || true", true),
        ("true || false", true),
        ("true || true", true),
        ("true || false && false", true),
        ("(true || false) && false", false),
        ("not true && false", false),
        ("not (true && false)", true),
        ("not not false || true && true", true),
        ("true &&\n// continue the expression\nfalse || true", true),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!(
            "fn main() -> u8 {{\nvar result = {expression}\nresult = not result\nif not result {{ return 1 }} else {{ return 0 }}\n}}"
        );
        let c_file = directory.0.join(format!("logical-{index}.c"));
        fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
        let result = build_and_run_c(&c_file, &directory.0.join(format!("logical-{index}")));
        assert_eq!(result.status.code(), Some(i32::from(expected)), "{source}");
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn comparisons_preserve_integer_boundaries_and_boolean_precedence() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        for (left, right) in [(min, max), (max, min), (min, min), (max, max)] {
            source.push_str(&format!(
                "{{\nfix left: {ty} = {left}\nfix right: {ty} = {right}\n"
            ));
            for (operator, expected) in [
                ("==", left == right),
                ("!=", left != right),
                ("<", left < right),
                ("<=", left <= right),
                (">", left > right),
                (">=", left >= right),
            ] {
                source.push_str(&format!(
                    "if (left {operator} right) != {expected} {{ return 1 }}\n"
                ));
            }
            for (operator, expected) in [
                ("==", true),
                ("!=", false),
                ("<", false),
                ("<=", true),
                (">", false),
                (">=", true),
            ] {
                source.push_str(&format!(
                    "if (left {operator} left) != {expected} {{ return 5 }}\n"
                ));
            }
            // Type inference must work regardless of which side contains the literal.
            source.push_str(&format!("if not (left == {left}) {{ return 2 }}\nif not ({right} == right) {{ return 3 }}\n}}\n"));
        }
    }
    for (expression, expected) in [
        ("true == true", true),
        ("true == false", false),
        ("false != true", true),
        ("false != false", false),
        ("not true == false", true),
        ("1 < 2 && 2 >= 2", true),
        ("2 < 1 || 1 != 2 && false", false),
        ("not (2 < 1)", true),
        ("(1 < 2) == true", true),
        ("not false == true && 3 > 2", true),
    ] {
        source.push_str(&format!("if ({expression}) != {expected} {{ return 4 }}\n"));
    }
    source.push_str("return 0\n}\n");
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("comparisons.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("comparisons"));
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn comparison_chains_preserve_values_and_repeat_inside_loops() {
    let mut source = String::from("fn main() -> u8 {\nfix top: u64 = 18446744073709551615\n");
    for (expression, expected) in [
        ("1 < 2 <= 3", true),
        ("3 >= 3 > 1", true),
        ("1 == 1 < 2 == 2 <= 3", true),
        ("3 == 3 >= 2 == 2 > 1", true),
        ("2 == 2 == 2", true),
        ("true == true == true", true),
        ("false == false == false", true),
        ("true == false == true", false),
        ("1 < 3 < 2", false),
        ("3 < 2 < 4", false),
        ("0 < 18446744073709551615 == top", true),
        ("-9223372036854775808 < -1 <= 9223372036854775807", true),
        ("(1 < 2 < 3) == true == (3 > 2 > 1)", true),
        ("(1 != 2) == true == true", true),
        ("false || 1 < 2 == 2 && not false", true),
    ] {
        source.push_str(&format!("if ({expression}) != {expected} {{ return 1 }}\n"));
    }
    source.push_str("var value: u8 = 1\nwhile 0 < value < 3 { value = 3 }\nif value != 3 { return 2 }\nreturn 0\n}\n");
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("chains.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("chains"));
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn bitwise_operations_preserve_width_type_and_precedence() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        let inverted_zero = if min < 0 { -1 } else { max };
        let inverted_max = if min < 0 { min } else { 0 };
        source.push_str(&format!(
            "fix top_{ty}: {ty} = {max}\nfix bottom_{ty}: {ty} = {min}\nfix all_{ty}: {ty} = ~0\nfix mask_{ty}: {ty} = 1 | 2 ^ 3 & 1\n"
        ));
        for condition in [
            format!("all_{ty} == {inverted_zero}"),
            format!("~top_{ty} == {inverted_max}"),
            format!("~~bottom_{ty} == bottom_{ty}"),
            format!("top_{ty} & all_{ty} == top_{ty}"),
            format!("top_{ty} ^ top_{ty} == 0"),
            format!("mask_{ty} == 3"),
            String::from("(1 | 2) ^ 3 & 1 == 2"),
            format!("~0 & top_{ty} == top_{ty}"),
            format!("bottom_{ty} | top_{ty} == {inverted_zero}"),
        ] {
            source.push_str(&format!("if not ({condition}) {{ return 1 }}\n"));
        }
    }
    source.push_str("fix inferred = ~0 & 18446744073709551615 & top_u64\nif inferred != top_u64 { return 2 }\nvar bits: u8 = 0\nbits = ~bits\nreturn bits & 7\n}\n");
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("bitwise.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("bitwise"));
    assert_eq!(result.status.code(), Some(7));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn bitwise_operators_reject_invalid_types_and_keep_arithmetic_diagnostics() {
    for (body, rule) in [
        ("fix x = ~true\nreturn 0", "expr.operator-types"),
        ("fix x = false | true\nreturn 0", "expr.operator-types"),
        (
            "fix x: u8 = 1\nfix y: i8 = 1\nfix z = x & y\nreturn 0",
            "width.same-type",
        ),
        ("fix x: u8 = ~256\nreturn 0", "lit.range"),
        (
            "fix x: u8 = 0\nfix y: u16 = x | 1\nreturn 0",
            "width.same-type",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().contains(rule), "{error:?}");
    }
    let safe = b"fn main() -> u8 { return ~0 & 7 | 2 ^ 1 }";
    for mode in [
        suz::ArithmeticMode::Default,
        suz::ArithmeticMode::Warn,
        suz::ArithmeticMode::Deny,
    ] {
        assert!(
            suz::compile_with_mode(safe, mode)
                .unwrap()
                .warnings
                .is_empty()
        );
    }
    let negative = b"fn main() -> u8 { fix x: i8 = ~(-1 & -2)\nreturn 0 }";
    let warned = suz::compile_with_mode(negative, suz::ArithmeticMode::Warn).unwrap();
    assert_eq!(warned.warnings.len(), 2);
    let error = suz::compile_with_mode(negative, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert!(error.message().contains("diagnostic.arithmetic"));
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn wrapping_arithmetic_preserves_boundaries_and_precedence() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!(
            "fix top_{ty}: {ty} = {max}\nfix bottom_{ty}: {ty} = {min}\n"
        ));
        let doubled_max = if min < 0 { -2 } else { max - 1 };
        for (expression, expected) in [
            (format!("top_{ty} +% 1"), min),
            (format!("1 +% top_{ty}"), min),
            (format!("bottom_{ty} -% 1"), max),
            (format!("top_{ty} *% top_{ty}"), 1),
            (format!("top_{ty} *% 2"), doubled_max),
            (format!("bottom_{ty} *% 2"), 0),
            (format!("top_{ty} +% top_{ty}"), doubled_max),
        ] {
            source.push_str(&format!("if {expression} != {expected} {{ return 1 }}\n"));
        }
    }
    for (expression, expected) in [
        ("2 +% 3 *% 4", 14),
        ("(2 +% 3) *% 4", 20),
        ("10 -% 3 -% 2", 5),
        ("1 -% 2 +% 3", 2),
        ("1 | 2 +% 3", 5),
        ("~0 +% 1", 0),
        ("255 +% 1", 0),
        ("128 *% 2", 0),
    ] {
        source.push_str(&format!(
            "{{\nfix value: u8 = {expression}\nif value != {expected} {{ return 2 }}\n}}\n"
        ));
    }
    source.push_str("fix inferred = 18446744073709551615 +% 1 +% top_u64\nif inferred != top_u64 { return 3 }\nvar count: u8 = 0\nwhile count < 3 { count = count +% 1 }\nreturn count\n}\n");
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("wrapping.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("wrapping"));
    assert_eq!(result.status.code(), Some(3));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn wrapping_arithmetic_checks_types_but_is_not_an_arithmetic_diagnostic_target() {
    for (body, rule) in [
        ("fix x = true +% false\nreturn 0", "expr.operator-types"),
        ("fix x: u8 = 256 +% 0\nreturn 0", "lit.range"),
        (
            "fix x: u8 = 1\nfix y: i8 = 1\nfix z = x *% y\nreturn 0",
            "width.same-type",
        ),
        (
            "fix x: u8 = 1\nfix y: u16 = x -% 1\nreturn 0",
            "width.same-type",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().contains(rule), "{error:?}");
    }
    let source = b"fn main() -> u8 { return 255 +% 1 -% 2 *% 3 }";
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
    let source = b"fn main() -> u8 { fix value: i8 = -1 +% -2\nreturn 0 }";
    let warned = suz::compile_with_mode(source, suz::ArithmeticMode::Warn).unwrap();
    assert_eq!(warned.warnings.len(), 2);
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert!(error.message().contains("diagnostic.arithmetic"));
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn truncating_casts_preserve_integer_bits_and_precedence() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (from, min, max) in INTEGER_BOUNDS {
        let mut values = vec![min, max];
        if min < 0 {
            values.push(-1);
        }
        for (to, target_min, target_max) in INTEGER_BOUNDS {
            let modulus = target_max - target_min + 1;
            for value in &values {
                let residue = value.rem_euclid(modulus);
                let expected = if residue > target_max {
                    residue - modulus
                } else {
                    residue
                };
                source.push_str(&format!(
                    "{{\nfix input: {from} = {value}\nfix converted = input as% {to}\nif converted != {expected} {{ return 1 }}\n}}\n"
                ));
            }
        }
    }
    for (expression, expected) in [
        ("255 as% i8 as% u16", 65535),
        ("~0 as% u16", 65535),
        ("(~0 as% u8) as% u16", 255),
        ("257 as% u8 +% 1", 2),
        ("128 as% u8 *% 2", 0),
        ("(257 +% 1) as% u8", 2),
        ("(255 +% 1) as% u16", 256),
    ] {
        source.push_str(&format!("if {expression} != {expected} {{ return 2 }}\n"));
    }
    source.push_str("fix inferred = 1 +% (257 as% u8)\nreturn inferred -% 2\n}\n");
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("truncating.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("truncating"));
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn truncating_casts_check_source_types_without_propagating_target_types() {
    for (body, rule) in [
        ("return true as% u8", "expr.as-trunc"),
        ("fix x = 1 as% bool\nreturn 0", "expr.as-trunc"),
        ("fix x = 1 as% f32\nreturn 0", "expr.as-trunc"),
        ("fix x = 1 as% void\nreturn 0", "expr.as-trunc"),
        ("return 18446744073709551615 as% u8", "lit.range"),
        ("return 1 as% u16", "type.expected"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().contains(rule), "{error:?}");
    }
    let source = b"fn main() -> u8 { return 257 as% u8 }";
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
    let source = b"fn main() -> u8 { return -1 as% u8 }";
    let warned = suz::compile_with_mode(source, suz::ArithmeticMode::Warn).unwrap();
    assert_eq!(warned.warnings.len(), 1);
    assert_eq!(
        (warned.warnings[0].line, warned.warnings[0].column),
        (1, 26)
    );
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert!(error.message().contains("diagnostic.arithmetic"));
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn ifexpr_selects_values_and_infers_branch_types() {
    let source = r#"fn main() -> u8 {
    fix narrow: u8 = 7
    fix inferred = ifexpr false { 256 as% u8 } elif false { 2 } else { narrow }
    if inferred != 7 { return 1 }
    fix selected: u8 = ifexpr false { 1 } elif true { 2 } elif true { 3 } else { 4 }
    if selected != 2 { return 2 }
    fix nested = ifexpr true { ifexpr false { false } else { true } } else { false }
    if not nested { return 3 }
    fix top: u64 = 18446744073709551615
    fix wide = ifexpr true { 18446744073709551615 } else { top }
    if wide != top { return 4 }
    fix combined: u8 = (ifexpr false { 255 } else { 7 }) +% 1
    if combined != 8 { return 5 }
    var count: u8 = 0
    while ifexpr count < 3 { true } else { false } {
        count +%= 1
    }
    return ifexpr count == 3 {
        0
    } else {
        6
    }
}
"#;
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("ifexpr.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("ifexpr"));
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn ifexpr_checks_all_branches_and_requires_values() {
    for (body, rule) in [
        ("return ifexpr true { 0 }", "expr.if-value"),
        ("return ifexpr 1 { 0 } else { 1 }", "if.condition"),
        (
            "return ifexpr true { 0 } elif 1 { 1 } else { 2 }",
            "if.condition",
        ),
        ("return ifexpr true { 0 } else { false }", "type.expected"),
        ("return ifexpr true { 0 } else { 256 }", "lit.range"),
        ("return ifexpr true {} else { 0 }", "block.value"),
        ("return ifexpr true { 0 }\nelse { 1 }", "expr.else-line"),
        (
            "return ifexpr false { return 1 } else { 0 }",
            "expr.jump-prohibited",
        ),
        (
            "return ifexpr false { break } else { 0 }",
            "expr.jump-prohibited",
        ),
        (
            "return ifexpr false { continue } else { 0 }",
            "expr.jump-prohibited",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().contains(rule), "{error:?}");
    }
    let source = b"fn main() -> u8 { fix x = ifexpr true { 0 } else { -1 }\nreturn 0 }";
    let warned = suz::compile_with_mode(source, suz::ArithmeticMode::Warn).unwrap();
    assert_eq!(warned.warnings.len(), 1);
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert!(error.message().contains("diagnostic.arithmetic"));
    let source = b"fn main() -> u8 { return ifexpr true { fix x: u8 = 0\nx } else { 1 } }";
    assert!(suz::compile(source).is_ok());
}
