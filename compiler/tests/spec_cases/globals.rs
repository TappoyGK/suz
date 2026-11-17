use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
fn global_values_initialize_and_preserve_copies_and_evaluation_order() {
    let source = r#"
struct Inner { label: symbol, view: text }
struct State { inner: Inner, count: len_t }
var STATE = State { inner: Inner { label: TITLE, view: TITLE[1:] or `` }, count: 2 }
var COUNT: i32 = INITIAL
var FLAG = false
var LETTER = 'a'
var RATIO: f64 = 1.5
var MODE = Mode.First
fix INITIAL: i32 = 1
fix TITLE = `あ😀x`
enum Mode: u8 { First, Last }
fn next() -> i32 {
    fix before = COUNT
    COUNT += 1
    return before
}
fn pair(a: i32, b: i32) -> bool { return a == 1 && b == 2 }
fn change() -> i32 { COUNT = 10
return 2 }
fn generic(x: int) -> int { COUNT += 1
return x }
fn unused(x: float) -> float { FLAG = true
return x }
fn main() -> u8 {
    fix old = STATE
    STATE.inner.label = `changed`
    STATE.inner.view = `other`
    STATE.count += 1
    if old.inner.label != TITLE || old.inner.view != `😀x` || old.count != 2 { return 1 }
    if not pair(next(), next()) || COUNT != 3 { return 2 }
    COUNT = 1
    fix sum = COUNT + change()
    if sum != 3 || COUNT != 10 { return 3 }
    COUNT = 1
    COUNT += change()
    if COUNT != 3 { return 4 }
    fix block = expr { COUNT += 1
        expr { COUNT += 1
            COUNT } }
    if block != 5 || COUNT != 5 { return 5 }
    if generic(7) != 7 || COUNT != 6 { return 6 }
    if FLAG { return 7 }
    LETTER = '😀'
    RATIO *= 2.0
    MODE = Mode.Last
    if LETTER != '😀' || RATIO != 3.0 || MODE != Mode.Last { return 8 }
    fix end = STATE.count
    for i in 0..<end { COUNT += 1 }
    if COUNT != 9 { return 9 }
    return 0
}
"#;
    let result = suz::compile_with_mode(source.as_bytes(), suz::ArithmeticMode::Default).unwrap();
    assert_eq!(
        result
            .warnings
            .iter()
            .filter(|w| w.rule() == "diagnostic.global-var")
            .count(),
        6
    );
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("globals.c");
    fs::write(&path, result.c).unwrap();
    let output = build_and_run_c(&path, &directory.0.join("globals"));
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // 最適化によって、大域変数の読み取りが副作用のある呼び出しをまたがないことも確認する。
    let source_path = directory.0.join("globals.suz");
    fs::write(&source_path, source).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_suz"))
        .arg("run")
        .arg(&source_path)
        .arg("-O2")
        .env_remove("CFLAGS")
        .env_remove("CPPFLAGS")
        .env_remove("LDFLAGS")
        .env_remove("SUZ_CFLAGS")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn global_declarations_reject_invalid_initializers_names_and_uses() {
    for (source, rule) in [
        (
            "const N = 1\nfn main() -> u8 { return 0 }",
            "decl.top-level",
        ),
        (
            "var bad = 1\nfn main() -> u8 { return 0 }",
            "name.top-level-binding",
        ),
        (
            "fix bad = 1\nfn main() -> u8 { return 0 }",
            "name.top-level-binding",
        ),
        (
            "fix N = 1\nvar N = 2\nfn main() -> u8 { return 0 }",
            "decl.duplicate",
        ),
        (
            "var N = 1\nfn main() -> u8 { return 0 }",
            "var.never-reassigned",
        ),
        (
            "var N = N\nfn main() -> u8 { N = 2\nreturn 0 }",
            "const.expression",
        ),
        (
            "fix C = N\nvar N = 1\nfn main() -> u8 { N = 2\nreturn 0 }",
            "const.expression",
        ),
        (
            "var N = M\nvar M = 1\nfn main() -> u8 { N = 2\nM = 3\nreturn 0 }",
            "const.expression",
        ),
        (
            "var N: int = 1\nfn main() -> u8 { N = 2\nreturn 0 }",
            "num.places",
        ),
        (
            "var N = get()\nfn get() -> u8 { return 1 }\nfn main() -> u8 { N = 2\nreturn 0 }",
            "const.expression",
        ),
        (
            "fix N = 1\nfn main() -> u8 { N = 2\nreturn 0 }",
            "const.decl",
        ),
        (
            "var N = 1\nfn main() -> u8 { for i in 0..<N { N += 1 }\nreturn 0 }",
            "for.bound-reference",
        ),
        (
            "var S = `abc`\nfn main() -> u8 { rev c in S { S = `` }\nreturn 0 }",
            "for.bound-reference",
        ),
        (
            "var N = 1\nfn main() -> u8 { N = 2\nmatch N { N => { return 0 }\n_ => { return 1 } } }",
            "const.expression",
        ),
        (
            "var N = 1\nfn main() -> u8 { N = expr { N = 2\n3 }\nreturn 0 }",
            "assign.rhs-write",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).expect_err(source);
        assert_eq!(error.rule(), rule, "{source}: {error:?}");
    }
    // constは識別子として利用でき、未使用関数の代入も構文上の代入として数える。
    suz::compile(b"var N = 1\nfn unused() -> void { N = 2 }\nfn main() -> u8 { fix const: u8 = 0\nreturn const }").unwrap();
}

#[test]
fn global_warnings_allow_logging_and_deny_are_independent_of_arithmetic_mode() {
    let source = b"var N = 1 allow [diagnostic.global-var: process state]\nfn main() -> u8 { N = 2\nreturn 0 }";
    for arithmetic_mode in [
        suz::ArithmeticMode::Default,
        suz::ArithmeticMode::Warn,
        suz::ArithmeticMode::Deny,
    ] {
        let mut options = suz::CompilationOptions {
            arithmetic_mode,
            ..Default::default()
        };
        assert!(
            suz::check_source_with_options(source, &options)
                .unwrap()
                .is_empty()
        );
        options.log_allow = true;
        let warnings = suz::check_source_with_options(source, &options).unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!((warnings[0].line, warnings[0].column), (1, 5));
        assert_eq!(warnings[0].allowed_reason.as_deref(), Some("process state"));
        options.deny_global_var = true;
        let error = suz::check_source_with_options(source, &options).unwrap_err();
        assert_eq!(error.rule(), "diagnostic.global-var-deny");
    }
    let error = suz::compile(
        b"fix N = 1 allow [diagnostic.global-var: constant]\nfn main() -> u8 { return 0 }",
    )
    .err()
    .unwrap();
    assert_eq!(error.rule(), "allow.unused");
}

#[test]
fn cli_global_policy_applies_to_all_source_commands() {
    use std::process::Command;
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("global.suz");
    fs::write(&source, "var N = 1 allow [diagnostic.global-var: shared state]\nfn main() -> u8 { N = 2\nreturn 0 }").unwrap();
    for command in ["check", "build", "run", "emit-c"] {
        let mut args = vec![command.to_owned(), source.display().to_string()];
        if matches!(command, "build" | "emit-c") {
            args.extend(["-o".into(), directory.0.join(command).display().to_string()]);
        }
        args.push("--deny-global-var".into());
        let output = Command::new(env!("CARGO_BIN_EXE_suz"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{command}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("[diagnostic.global-var-deny]"));
        assert!(!directory.0.join(command).exists());
        let help = Command::new(env!("CARGO_BIN_EXE_suz"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&help.stdout).contains("--deny-global-var"));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .args(["check", source.to_str().unwrap(), "--log-allow"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("allowed:"));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[diagnostic.global-var] shared state")
    );
}
