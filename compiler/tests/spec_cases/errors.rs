use crate::test_util::compile_and_run_c23;

#[test]
fn fallible_calls_preserve_success_failure_and_lazy_handlers() {
    let source = r#"
error ReadError { Missing, Denied }
error DecodeError { Broken }
struct Record { problem: ReadError }
fnshape Reader(bool) -> i32 or ReadError, DecodeError.Broken
fnshape Same(bool) -> i32 or DecodeError.Broken, ReadError.Denied, ReadError.Missing
fn read(ok: bool) -> i32 or ReadError, DecodeError.Broken {
    if ok { return 7 }
    fail ReadError.Missing
}
fn forward(ok: bool) -> i32 or ReadError, DecodeError {
    fix n = read(ok) or fail
    return n
}
fn mapped() -> i32 or DecodeError.Broken {
    fix n = read(false) or fail DecodeError.Broken
    return n
}
fn fallback() -> i32 { log info `fallback`
    return 9 }
fn send() -> void or ReadError { fail ReadError.Denied }
fn success() -> void or ReadError { return }
fn falling() -> void or ReadError { log info `fallthrough` }
fn returned() -> i32 { fix n = read(false) or return 12
    return n }
fn invoke(f: fn Reader) -> i32 { return f(false) or 13 }
fn main() -> u8 {
    fix f: static fn Reader = fn read
    fix g: static fn Same = f
    log info `${read(true) or fallback()} ${forward(false) or 8} ${mapped() or 10}`
    log info `${g(false) or fallback()} ${returned()} ${invoke(fn read)}`
    log info `${invoke(fn [](ok: bool) -> i32 or ReadError, DecodeError.Broken { fail ReadError.Missing })}`
    send() or ignore: 送信失敗は無視
    success() or panic `unexpected`
    falling() or ignore: 失敗は無視
    fix record = Record { problem: ReadError.Denied }
    log info `${record.problem} ${record.problem == ReadError.Denied}`
    match record.problem {
        ReadError.Missing => { return 1 }
        ReadError.Denied => { return 0 }
    }
}
"#;
    let output = compile_and_run_c23(source, "errors.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(": 7 8 10\n"), "{stderr}");
    assert_eq!(stderr.matches(": fallback\n").count(), 1, "{stderr}");
    assert!(stderr.contains(": 9 12 13\n"), "{stderr}");
    assert!(stderr.contains(": Denied true\n"), "{stderr}");
}

#[test]
fn fallible_signatures_reject_unhandled_or_undeclared_errors() {
    let prefix = "error Err { First, Second }\nfn risky() -> i32 or Err { fail Err.First }\n";
    for source in [
        "fn main() -> u8 { risky()\nreturn 0 }",
        "fn main() -> u8 { fix n = risky()\nreturn 0 }",
        "fn main() -> u8 { risky() or ignore: 理由\nreturn 0 }",
        "fn main() -> u8 { fail Err.First }",
        "fn bad() -> i32 or Err.Second { fail Err.First }\nfn main() -> u8 { return 0 }",
        "fn bad(e: Err) -> i32 or Err.First { fail e }\nfn main() -> u8 { return 0 }",
        "fn bad() -> i32 or Err.First { fix n = risky() or fail\nreturn n }\nfn main() -> u8 { return 0 }",
        "fn bad() -> i32 or Err { return risky() or fail }\nfn main() -> u8 { return 0 }",
        "fn bad() -> i32 or Err, Err.First { fail Err.First }\nfn main() -> u8 { return 0 }",
        "fn main() -> u8 or Err { return 0 }",
        "fnshape Action() -> i32 or Err.First\nfn main() -> u8 { fix f: static fn Action = fn risky\nreturn 0 }",
        "fn main() -> u8 { fix n = Err.First as u8 or 0\nreturn n }",
        "fn main() -> u8 { fix e = 0 as Err or Err.First\nreturn 0 }",
        "fn main() -> u8 { fix e = Err.First + Err.Second\nreturn 0 }",
        "fn main() -> u8 { fix n = risky() or panic ``\nreturn 0 }",
        "fn main() -> u8 { fix n = risky() or panic(`bad`)\nreturn 0 }",
        "fn send() -> void or Err { fail Err.First }\nfn main() -> u8 { send() or ignore:   }",
        "fn send() -> void or Err { fail Err.First }\nfn main() -> u8 { send() or ignore: x\ty\nreturn 0 }",
        "fn main() -> u8 { fix n = 1 as u8 or ignore: 理由\nreturn 0 }",
    ] {
        let source = format!("{prefix}{source}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn panic_handlers_work_for_existing_operations_and_calls() {
    for operation in [
        "risky()",
        "`bad` as i32",
        "`bad` as% i32",
        "(127 as i8 + 1 as i8)",
        "`a`[2]",
        "`a`[0:2]",
        "300 as u8",
    ] {
        let source = format!(
            "error Err {{ Bad }}\nfn risky() -> i32 or Err {{ fail Err.Bad }}\nfn main() -> u8 {{ fix n = {operation} or panic `bad ${{7}}`\nreturn 0 }}"
        );
        let output = compile_and_run_c23(&source, "panic-errors.suz");
        assert_eq!(output.status.code(), Some(101), "{operation}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).ends_with(": bad 7\n"),
            "{output:?}"
        );
    }
}

#[test]
fn generic_calls_and_closures_keep_error_sets_and_evaluation_order() {
    let source = r#"
error Err { Bad, Other }
fnshape Generic(i16) -> i16 or Err.Bad
fn number(x: int) -> int or Err.Bad {
    if x == 0 { fail Err.Bad }
    return x
}
fn relay(x: int) -> int or Err.Bad {
    fix n = number(x) or fail
    return n
}
fnshape Action() -> i32 or Err
fn run(f: fn Action) -> i32 { return f() or 99 }
fn failed_value(e: Err) -> Err or Err { fail e }
fn send() -> void or Err { fail Err.Bad }
fn empty() -> void { send() or ignore: 理由 }
fn main() -> u8 {
    fix f: static fn Generic = fn number
    log info `${f(0) or 8} ${relay(2 as i16) or 9}`
    var n: i32 = 0
    fix result = run(fn [var n]() -> i32 or Err {
        n = 5
        fail Err.Other
    })
    log info `${result} ${n}`
    fix e = failed_value(Err.Other) or Err.Bad
    fix kind = matchexpr e {
        Err.Bad => 1
        Err.Other => 2
    }
    empty()
    log info `${kind}`
    return 0
}
"#;
    let output = compile_and_run_c23(source, "generic-errors.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(": 8 2\n"), "{stderr}");
    assert!(stderr.contains(": 99 5\n"), "{stderr}");
    assert!(stderr.contains(": 1\n"), "{stderr}");
}

#[test]
fn ignore_reason_is_literal_inside_template_interpolations() {
    let source = r#"error Err { Bad }
fn send() -> void or Err { fail Err.Bad }
fnshape Once() -> void
fn run(f: fn Once) -> i32 { f()
    return 7 }
fn main() -> u8 {
    log info `${run(fn []() -> void { send() or ignore: // ` { ' \ })}`
    return 0
}
"#;
    let output = compile_and_run_c23(source, "ignore.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stderr, b"info ignore.suz:7: 7\n");
}

#[test]
fn void_call_statements_propagate_or_return_only_on_failure() {
    let source = r#"
error Err { Bad }
error Mapped { Other }
fnshape Action(bool) -> void or Err
fn send(ok: bool) -> void or Err { if not ok { fail Err.Bad } }
fn relay(ok: bool) -> void or Err {
    (send(ok) or fail)
    log info `sent`
}
fn mapped() -> void or Mapped {
    send(false) or fail Mapped.Other
    log info `unexpected`
}
fn direct() -> void {
    send(false) or return
    log info `unexpected`
}
fn indirect(f: fn Action, ok: bool) -> u8 {
    f(ok) or return 3
    return 4
}
fn main() -> u8 {
    relay(true) or return 1
    relay(false) or ignore: 外への伝播を確認 // ここも理由
    mapped() or ignore: 変換後の伝播を確認
    direct()
    fix f: static fn Action = fn send
    f(false) or ignore: 関数値からの失敗
    log info `${indirect(fn send, false)} ${indirect(fn send, true)}`
    log info `${indirect(fn [](ok: bool) -> void or Err { send(ok) or fail }, false)}`
    send(false) or return 7
    return 8
}
"#;
    let output = compile_and_run_c23(source, "void-errors.suz");
    assert_eq!(output.status.code(), Some(7), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches(": sent\n").count(), 1, "{stderr}");
    assert!(stderr.contains(": 3 4\n"), "{stderr}");
    assert!(stderr.contains(": 3\n"), "{stderr}");
    assert!(!stderr.contains("unexpected"), "{stderr}");
}

#[test]
fn error_diagnostics_distinguish_type_context_and_declared_set() {
    let prefix = "error Err { Bad, Other }\nenum Choice: u8 { Bad }\n";
    for (declaration, rule, message) in [
        (
            "var X: u8 or Err = 1",
            "err.no-first-class",
            "only allowed on",
        ),
        (
            "fn bad() -> void { var x: u8 or Err = 1 }",
            "err.no-first-class",
            "only allowed on",
        ),
        (
            "fn bad(a: u8 or Err) -> void {}",
            "err.no-first-class",
            "only allowed on",
        ),
        (
            "struct Record { a: u8 or Err }",
            "err.no-first-class",
            "only allowed on",
        ),
        (
            "fnshape Shape(u8 or Err) -> void",
            "err.no-first-class",
            "only allowed on",
        ),
        (
            "fnshape Shape() -> void\nfn bad(a: fn Shape or Err) -> void {}",
            "err.no-first-class",
            "only allowed on",
        ),
        (
            "fn bad() -> u8 or Err { fail 3 }",
            "err.fail",
            "requires an error value, found",
        ),
        (
            "fn bad() -> u8 or Err { fail true }",
            "err.fail",
            "requires an error value, found bool",
        ),
        (
            "fn bad() -> u8 or Err { fail Choice.Bad }",
            "err.fail",
            "requires an error value, found Choice",
        ),
        (
            "fn bad() -> u8 { fail Err.Bad }",
            "err.fail",
            "requires an enclosing function with declared errors",
        ),
        (
            "fn bad() -> u8 or Err.Bad { fail Err.Other }",
            "err.fail",
            "not included",
        ),
        (
            "fn bad() -> u8 or Err { fix n = 1 }",
            "fn.value-return-required",
            "or fail statement",
        ),
    ] {
        let source = format!("{prefix}{declaration}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err().to_string();
        assert!(
            error.contains(&format!("[{rule}]")) && error.contains(message),
            "{source}\n{error}"
        );
    }
    let error = suz::compile(b"fn main() -> u8 { fix n = 1 }")
        .unwrap_err()
        .to_string();
    assert!(!error.contains("fail statement"), "{error}");
}

#[test]
fn void_statement_exception_does_not_allow_nested_or_value_call_jumps() {
    let prefix = "error Err { Bad }\nfn send() -> void or Err { fail Err.Bad }\nfn read() -> u8 or Err { fail Err.Bad }\n";
    for (body, rule) in [
        ("read() or return 1\nreturn 0", "expr.or-return-position"),
        ("read() or fail\nreturn 0", "expr.or-return-position"),
        (
            "fix n = expr { send() or return 1\n0 }\nreturn n",
            "expr.jump-prohibited",
        ),
        (
            "fix n = expr { send() or fail\n0 }\nreturn n",
            "expr.jump-prohibited",
        ),
        (
            "fix n = ifexpr true { send() or return 1 } else { 0 }\nreturn 0",
            "expr.jump-prohibited",
        ),
        ("return read() or fail", "expr.or-return-position"),
        (
            "fix n = 1 + (read() or return 1)\nreturn 0",
            "expr.or-return-position",
        ),
    ] {
        let source =
            format!("{prefix}fn bad() -> u8 or Err {{ {body} }}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err().to_string();
        assert!(error.contains(&format!("[{rule}]")), "{source}\n{error}");
    }
}

#[test]
fn error_arms_select_variants_and_recover_with_nested_scopes() {
    let source = r#"
error ReadError { Missing, Denied }
error DecodeError { Broken }
fn read(which: u8) -> i32 or ReadError, DecodeError {
    if which == 0 { return 7 }
    if which == 1 { fail ReadError.Missing }
    if which == 2 { fail ReadError.Denied }
    fail DecodeError.Broken
}
fn fallback(n: i32) -> i32 { log info `fallback ${n}`
    return n }
fn pick(which: u8) -> i32 {
    return read(which) or error {
        ReadError.Missing => 11
        ReadError.Denied => {
            fix n = read(3) or error { _ => 4 }
            recover n + 8
        }
        DecodeError.Broken => fallback(13)
    }
}
fn main() -> u8 {
    log info `${pick(0)} ${pick(1)} ${pick(2)} ${pick(3)}`
    var counter: i32 = 0
    fix value = read(1) or error {
        ReadError.Missing => { counter = 5
            if true { recover counter } else { recover 0 }
        }
        _ => 0
    }
    log info `${value} ${counter}`
    return 0
}
"#;
    let output = compile_and_run_c23(source, "arms.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(": 7 11 12 13\n"), "{stderr}");
    assert_eq!(stderr.matches(": fallback 13\n").count(), 1, "{stderr}");
    assert!(stderr.contains(": 5 5\n"), "{stderr}");
}

#[test]
fn error_arm_jumps_reach_the_enclosing_function_and_loop() {
    let source = r#"
error Err { Bad, Other }
fn read(n: u8) -> i32 or Err {
    if n == 0 { return 7 }
    if n == 1 { fail Err.Bad }
    fail Err.Other
}
fn returned() -> i32 {
    fix value = read(1) or error { Err.Bad => return 8
        Err.Other => 9 }
    return value
}
fn failed() -> i32 or Err {
    fix value = read(1) or error {
        Err.Bad => fail Err.Other
        Err.Other => 9
    }
    return value
}
fn main() -> u8 {
    log info `${returned()} ${failed() or 10}`
    for i in 0..<4 {
        fix value = read(i as u8 or 0) or error {
            Err.Bad => continue
            Err.Other => break
        }
        log info `${value}`
    }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "jumps.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(": 8 10\n"), "{stderr}");
    assert_eq!(stderr.matches(": 7\n").count(), 1, "{stderr}");
}

#[test]
fn void_error_arms_allow_ignore_and_nonempty_fallthrough() {
    let source = r#"
error Err { Bad, Other }
fn send(n: u8) -> void or Err {
    if n == 1 { fail Err.Bad }
    if n == 2 { fail Err.Other }
}
fn main() -> u8 {
    send(0) or error { _ => panic `unexpected` }
    send(1) or error {
        Err.Bad => ignore: 通知は不要 // 理由
        Err.Other => { log info `unexpected` }
    }
    send(2) or error {
        Err.Bad => ignore: 通知は不要
        Err.Other => { log info `handled` }
    }
    send(1) or error { _ => return 7 }
    return 8
}
"#;
    let output = compile_and_run_c23(source, "void-arms.suz");
    assert_eq!(output.status.code(), Some(7), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(": handled\n"), "{stderr}");
    assert!(!stderr.contains("unexpected"), "{stderr}");
}

#[test]
fn error_arms_reject_invalid_coverage_values_and_control_positions() {
    let prefix = "error Err { Bad, Other }\nerror Extra { Bad }\nfn read() -> u8 or Err { fail Err.Bad }\nfn send() -> void or Err { fail Err.Bad }\nfnshape Once() -> void\nfn invoke(f: fn Once) -> void { f() }\nfn read_text() -> text or Err { fail Err.Bad }\n";
    for (body, rule) in [
        (
            "fix n = read() or error { Err.Bad => 0 }\nreturn 0",
            "err.or-error",
        ),
        (
            "fix n = read() or error { Err.Bad => 0\nErr.Bad => 1\n_ => 2 }\nreturn 0",
            "err.or-error",
        ),
        (
            "fix n = read() or error { Extra.Bad => 0\n_ => 2 }\nreturn 0",
            "err.or-error",
        ),
        (
            "fix n = read() or error { _ => 0\nErr.Bad => 1 }\nreturn 0",
            "err.or-error",
        ),
        (
            "fix n = read() or error { _ => {} }\nreturn 0",
            "err.or-error",
        ),
        (
            "fix n = read() or error { _ => { if true { recover 0 } } }\nreturn 0",
            "err.or-error",
        ),
        (
            "fix n = read() or error { _ => true }\nreturn 0",
            "type.expected",
        ),
        ("send() or error { _ => {} }\nreturn 0", "err.or-error"),
        (
            "send() or error { _ => { recover 0 } }\nreturn 0",
            "err.recover",
        ),
        ("send() or error { _ => 0 }\nreturn 0", "err.or-error"),
        (
            "fix n = read() or error { _ => { ignore: 理由\nrecover 0 } }\nreturn 0",
            "err.or-ignore",
        ),
        (
            "if false { return 0 }\nloop { read() or error { _ => break } }",
            "fn.fallthrough",
        ),
        (
            "fix n = read() or error { _ => { loop { read() or error { _ => break } } } }\nreturn n",
            "err.or-error",
        ),
        (
            "fix n = read() or error { _ => 0 } allow [diagnostic.arithmetic: 理由]\nreturn n",
            "allow.contains-block",
        ),
        (
            "fix n = read() or error { _ => { invoke(fn []() -> void { recover 0 })\nrecover 0 } }\nreturn n",
            "err.recover",
        ),
        (
            "loop { fix n = read() or error { _ => { invoke(fn []() -> void { break })\nrecover 0 } } }\nreturn 0",
            "loop.jump-context",
        ),
        (
            "fix n = len (read_text() or error { _ => return 0 })\nreturn 0",
            "expr.or-return-position",
        ),
        ("recover 0", "err.recover"),
        ("ignore: 理由\nreturn 0", "err.or-ignore"),
        (
            "return read() or error { _ => return 0 }",
            "expr.or-return-position",
        ),
        (
            "fix n = expr { fix v = read() or error { _ => return 0 }\nv }\nreturn n",
            "expr.jump-prohibited",
        ),
        (
            "fix n = read() or error { _ => break }\nreturn n",
            "loop.jump-context",
        ),
    ] {
        let source = format!("{prefix}fn main() -> u8 {{ {body} }}");
        let error = suz::compile(source.as_bytes()).unwrap_err().to_string();
        assert!(error.contains(&format!("[{rule}]")), "{source}\n{error}");
    }
}

#[test]
fn error_arms_keep_generic_types_and_closure_recovery_scopes() {
    let source = r#"
error Err { Bad }
fnshape Reader(i16) -> i16 or Err
fn read(n: int) -> int or Err {
    if n == 0 { fail Err.Bad }
    return n
}
fn relay(n: int) -> int {
    return read(n) or error { _ => { recover n } }
}
fnshape Once() -> i16
fn invoke(f: fn Once) -> i16 { return f() }
fn main() -> u8 {
    fix reader: static fn Reader = fn read
    var count: i16 = 0
    fix n = reader(0) or error { _ => {
        fix inner = reader(0) or error { _ => {
            count = 3
            recover invoke(fn []() -> i16 {
                fix n = read(0 as i16) or error { _ => return 4 }
                return n
            })
        } }
        count += inner
        recover count
    } }
    log info `${n} ${count} ${relay(2 as i16)}`
    return 0
}
"#;
    let output = compile_and_run_c23(source, "generic-arms.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(": 7 7 2\n"), "{stderr}");
}
