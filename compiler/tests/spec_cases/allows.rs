use crate::test_util::{TemporaryDirectory, compile_and_run_c23};
use suz::{ArithmeticMode, CompilationOptions};

fn options(mode: ArithmeticMode, log_allow: bool) -> CompilationOptions {
    CompilationOptions {
        arithmetic_mode: mode,
        log_allow,
        ..CompilationOptions::default()
    }
}

#[test]
fn allows_track_lexical_sites_in_all_modes_and_keep_runtime_behavior() {
    let source = r#"fix BASE: u8 = 2
fix LIMIT: u8 = BASE + 1 allow [diagnostic.arithmetic: constant bound]
fn next(value: u8) -> u8 { return value + 1 }
fn main() -> u8 {
    fix value = LIMIT +
        1 allow [diagnostic.arithmetic: multiline statement]
    allow [diagnostic.arithmetic: local bound] {
        var sum: u8 = 0
        for i in 0..<value { sum += i }
        if sum != 6 { return 1 }
    }
    if false { fix unused = value + 2 allow [diagnostic.arithmetic: checked even if unreachable] }
    return next(value)
}"#;
    let plain = suz::compile_with_options(
        source.as_bytes(),
        "allows.suz",
        &options(ArithmeticMode::Default, true),
    )
    .unwrap();
    assert!(plain.warnings.is_empty());
    assert!(!plain.c.contains("local bound"));
    let warnings = suz::check_source(source.as_bytes(), ArithmeticMode::Warn).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3); // 呼び出し側の指定では、呼び出される関数の本体の警告は抑制しない。
    let all =
        suz::check_source_with_options(source.as_bytes(), &options(ArithmeticMode::Warn, true))
            .unwrap();
    assert_eq!(all.len(), 5);
    assert_eq!(all.iter().filter(|w| w.allowed_reason.is_some()).count(), 4);
    let denied = suz::check_source(source.as_bytes(), ArithmeticMode::Deny).unwrap_err();
    assert_eq!((denied.line, denied.column), (2, 22));
    assert!(denied.message().contains("[diagnostic.arithmetic]"));
    let output = compile_and_run_c23(source, "allows.suz");
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn invalid_allows_are_rejected_independently_of_warning_options() {
    for (statement, rule) in [
        (
            "fix x = 1 allow [diagnostic.arithmetic: no operation]",
            "allow.unused",
        ),
        (
            "fix x = 1 + 2 or 0 allow [diagnostic.arithmetic: handled]",
            "allow.unused",
        ),
        (
            "fix x = 1 + 2 allow [src.encoding: wrong rule]",
            "allow.rule-check",
        ),
        (
            "fix x = 1 + 2 allow [unknown: wrong rule]",
            "allow.rule-check",
        ),
        (
            "fix x = 1 + 2 allow [diagnostic.arithmetic: a], [diagnostic.arithmetic: b]",
            "allow.rule-check",
        ),
        (
            "fix x = 1 + 2 allow [diagnostic.arithmetic: ]",
            "allow.reason",
        ),
        (
            "fix x = 1 + 2 allow [diagnostic.arithmetic reason]",
            "allow.item",
        ),
        (
            "fix x = 1 + 2 allow [diagnostic.arithmetic : reason]",
            "allow.item",
        ),
        (
            "fix x = 1 + 2 allow [diagnostic.arithmetic: a\tb]",
            "allow.reason",
        ),
        (
            "fix x = 1 + 2 allow [diagnostic.arithmetic: a\nb]",
            "allow.reason",
        ),
        (
            "fix x = 1 + 2 allow [diagnostic.arithmetic: a\0b]",
            "allow.reason",
        ),
        (
            "allow [diagnostic.arithmetic: outer] { fix x = 1 + 2 allow [diagnostic.arithmetic: inner] }",
            "allow.nested",
        ),
        (
            "allow [diagnostic.arithmetic: outer] { allow [diagnostic.arithmetic: inner] { fix x = 1 + 2 } }",
            "allow.nested",
        ),
        (
            "loop { break allow [diagnostic.arithmetic: none] }",
            "allow.unused",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{statement}\nreturn 0\n}}");
        for mode in [
            ArithmeticMode::Default,
            ArithmeticMode::Warn,
            ArithmeticMode::Deny,
        ] {
            let error = suz::check_source(source.as_bytes(), mode).unwrap_err();
            assert!(
                error.message().contains(&format!("[{rule}]")),
                "{statement}: {error}"
            );
        }
    }
}

#[test]
fn trailing_allows_reject_blocks_at_any_expression_depth() {
    for expression in [
        "ifexpr true { 1 + 2 } else { 0 }",
        "(ifexpr true { 1 + 2 } else { 0 })",
        "1 + (expr { 2 + 3 })",
        "identity(expr { 2 + 3 })",
        "matchexpr true { true => 1 + 2\nfalse => 0 }",
        "Point { x: expr { 1 + 2 } }",
        "`${expr { 1 + 2 }}`",
    ] {
        let source = format!(
            "struct Point {{ x: u8 }}\nfn identity(x: u8) -> u8 {{ return x }}\nfn main() -> u8 {{\nfix value = {expression} allow [diagnostic.arithmetic: bound]\nreturn 0\n}}"
        );
        let error = suz::check_source(source.as_bytes(), ArithmeticMode::Default).unwrap_err();
        assert!(
            error.message().contains("[allow.contains-block]"),
            "{expression}: {error}"
        );
    }
    let source = b"struct Point { x: u8 }\nfn main() -> u8 {\nfix point = Point { x: 1 + 2 } allow [diagnostic.arithmetic: small]\nreturn point.x\n}";
    assert!(
        suz::check_source(source, ArithmeticMode::Warn)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn allow_reasons_are_literal_even_inside_template_interpolations() {
    let source = r#"fn main() -> u8 {
    log warn `value: ${expr { 1 + 2 allow [diagnostic.arithmetic: literal ` ${ // { : , \n 日本語] }}`
    return 0
}"#;
    let diagnostics =
        suz::check_source_with_options(source.as_bytes(), &options(ArithmeticMode::Warn, true))
            .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].allowed_reason.as_deref(),
        Some(r"literal ` ${ // { : , \n 日本語")
    );
    assert_eq!(
        diagnostics[0].column,
        source.lines().nth(1).unwrap().find('+').unwrap() + 1
    );
    let output = compile_and_run_c23(source, "allows.suz");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stderr, b"warn allows.suz:2: value: 3\n");
}

#[test]
fn cli_reports_only_enabled_suppressed_warnings_with_reasons() {
    use std::{fs, process::Command};
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 {\n    return 1 + 2 allow [diagnostic.arithmetic: small]\n}\n",
    )
    .unwrap();
    for verb in ["check", "emit-c"] {
        for (flags, expected) in [
            (vec!["--log-allow"], ""),
            (vec!["--warn-unhandled-arithmetic"], ""),
            (
                vec!["--warn-unhandled-arithmetic", "--log-allow"],
                "input.suz:2:14: allowed: integer arithmetic has no 'or' handler [diagnostic.arithmetic] small\n",
            ),
        ] {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_suz"));
            cmd.current_dir(&directory.0)
                .env("PATH", "")
                .args([verb, "input.suz"])
                .args(flags);
            if verb == "emit-c" {
                cmd.args(["-o", "generated"]);
            }
            let output = cmd.output().unwrap();
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stderr, expected.as_bytes());
            if verb == "emit-c" {
                fs::remove_dir_all(directory.0.join("generated")).unwrap();
            }
        }
    }
}
