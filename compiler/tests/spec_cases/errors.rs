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
    log info `${run(fn []() -> void { send() or ignore: ` { ' \ })}`
    return 0
}
"#;
    let output = compile_and_run_c23(source, "ignore.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stderr, b"info ignore.suz:7: 7\n");
}
