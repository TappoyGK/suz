use crate::{integer_bounds::INTEGER_BOUNDS, test_util::compile_and_run_c11};

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn function_calls_preserve_scalar_types_and_function_return_handlers() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!(
            "if identity_{ty}({min}) != {min} || identity_{ty}({max}) != {max} {{ return 1 }}\n"
        ));
    }
    source.push_str(
        r#"fix original: u8 = 2
    if copied(original) != 3 || original != 2 { return 2 }
    if narrow(300) != -7 || narrow(2) != 2 { return 3 }
    if checked(255) != 2.5 || checked(1) != 2.0 { return 4 }
    if identity_f32(1.5) != 1.5 || identity_f64(-inf) != -inf { return 5 }
    if identity_f32(nan) == identity_f32(nan) { return 6 }
    if identity_f64(-0.0) != 0.0 { return 7 }
    if not identity_bool(true) || identity_bool(false) { return 8 }
    return 0
}
fn copied(input: u8) -> u8 {
    var copy = input
    copy += 1
    return expr {
        fix result = copy
        result
    }
}
fn narrow(input: u16) -> i16 {
    fix small = input as u8 or return -7
    return small as i16
}
fn checked(input: u8) -> f64 {
    fix result = (input + 1) or return 2.5
    return result as f64
}
fn identity_f32(input: f32) -> f32 { return input }
fn identity_f64(input: f64) -> f64 { return input }
fn identity_bool(input: bool) -> bool { return input }
"#,
    );
    for (ty, _, _) in INTEGER_BOUNDS {
        source.push_str(&format!(
            "fn identity_{ty}(input: {ty}) -> {ty} {{ return input }}\n"
        ));
    }
    let output = compile_and_run_c11(&source, "functions.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn calls_sequence_arguments_and_keep_recursive_temporaries_separate() {
    let source = r#"fn main() -> u8 {
    var counter: u8 = 0
    fix result = pair(expr {
        counter += 1
        counter
    }, expr {
        counter *= 2
        pair(counter, 3)
    })
    if result != 33 || counter != 2 { return 1 }
    if fib(8) != 21 { return 2 }
    fix selected = expr {
        fix local: u8 = 5
        pair(local, pair(local, 1))
    }
    if selected != 101 { return 3 }
    return 0
}
fn pair(left: u8, right: u8) -> u8 { return left * 10 + right }
fn fib(n: u8) -> u8 {
    if n < 2 { return n }
    return fib(n - 1) + fib(n - 2)
}
"#;
    let output = compile_and_run_c11(source, "functions.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn void_failure_returns_exit_only_the_called_function() {
    let source = r#"fn narrow(raw: u16) -> void {
    fix count = raw as u8 or return
    log info `cast=${count}`
}
fn increment(raw: u8) -> void {
    fix count = (raw + 1) or return
    log info `sum=${count}`
    return
}
fn main() -> u8 {
    narrow(300)
    narrow(3)
    increment(255)
    increment(3)
    return 0
}
"#;
    let output = compile_and_run_c11(source, "functions.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"info functions.suz:3: cast=3\ninfo functions.suz:7: sum=4\n"
    );
}

#[test]
fn functions_reject_invalid_parameters_void_values_and_returns() {
    for (declarations, body, rule) in [
        ("fn f(x: u8, x: u8) -> void {}", "", "fn.parameter-scope"),
        ("fn f(x: u8) -> void { x = 2 }", "", "fn.parameter"),
        (
            "fn f(x: u8) -> void { { fix x = 2 } }",
            "",
            "decl.shadowing",
        ),
        ("fn f(f: u8) -> void {}", "", "decl.shadowing"),
        (
            "fn name() -> u8 { return 0 }\nfn f(name: u8) -> void {}",
            "",
            "decl.shadowing",
        ),
        ("fn f() -> void {}\nfn f() -> void {}", "", "decl.duplicate"),
        ("fn f(x) -> void {}", "", "fn.parameter"),
        ("fn f(x: void) -> void {}", "", "type.void"),
        ("fn f() -> void {}", "fix x = f()", "type.void"),
        ("fn f() -> void {}", "f() == f()", "expr.operator-types"),
        ("fn f() -> void {}", "f() + f()", "expr.operator-types"),
        (
            "fn f() -> void {}",
            "fix x = matchexpr true {\ntrue => f()\nfalse => f()\n}",
            "branch.expression",
        ),
        ("fn f() -> void { return 0 }", "", "fn.return"),
        (
            "fn f() -> void {}\nfn g() -> void { return f() }",
            "",
            "fn.return",
        ),
        ("fn f() -> u8 { return }", "", "fn.return"),
        ("fn f() -> u8 { loop {} }", "", "fn.value-return-required"),
        (
            "fn f() -> void {\nfix x = 300 as u8 or return 0\n}",
            "",
            "fn.return",
        ),
        (
            "fn f() -> u8 {\nfix x = 300 as u8 or return\nreturn x\n}",
            "",
            "fn.return",
        ),
        ("fn f() -> void {}", "fix f = 1", "decl.shadowing"),
        ("", "unknown()", "fn.call"),
        ("", "fn nested() -> void {}", "decl.location"),
        ("return 1", "", "fn.return"),
        (
            "fn f(x: u8) -> u8 { return x }",
            "fix x = f(300 as u8 or return 0)",
            "expr.or-return-position",
        ),
    ] {
        let source = format!("{declarations}\nfn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}: {error}"
        );
        assert!(
            error.message.contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
}
