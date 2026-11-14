use crate::{ArithmeticMode, ErrorKind, compile, compile_with_mode};

#[test]
fn loop_context_scope_and_syntactic_exits_are_checked() {
    for (body, rule) in [
        ("break\nreturn 0", "loop.jump-context"),
        ("if false { continue }\nreturn 0", "loop.jump-context"),
        ("loop { break }\nbreak\nreturn 0", "loop.jump-context"),
        ("loop { break 1 }\nreturn 0", "loop.break"),
        ("loop { continue 1 }\nreturn 0", "loop.continue"),
        ("while 1 {}\nreturn 0", "while.loop"),
        ("loop {}", "fn.value-return-required"),
        ("while true {}", "fn.value-return-required"),
        ("loop { if false { break }\nreturn 1 }", "fn.fallthrough"),
        ("loop { return 1\nbreak }", "fn.fallthrough"),
        (
            "while true { if false { break }\nreturn 1 }",
            "fn.fallthrough",
        ),
        ("fix flag = true\nwhile flag { return 1 }", "fn.fallthrough"),
        ("while (true) { return 1 }", "fn.fallthrough"),
        ("while false { return 1 }", "fn.fallthrough"),
        (
            "loop { fix value: u8 = 1\nbreak }\nreturn value",
            "decl.local-scope",
        ),
        (
            "fix value = 1\nwhile false { fix value = 2 }\nreturn 0",
            "decl.shadowing",
        ),
        (
            "while false { var value = 1 }\nreturn 0",
            "var.never-reassigned",
        ),
        ("while false { return true }\nreturn 0", "type.expected"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource, "{error}");
        assert!(error.message.contains(rule), "expected {rule}: {error}");
    }
    // Accepted infinite paths must not be executed by this test.
    for body in [
        "loop { continue\nreturn 1 }",
        "loop {}\nreturn 0",
        "while true { loop { break }\nreturn 1 }",
    ] {
        compile(format!("fn main() -> u8 {{\n{body}\n}}").as_bytes()).unwrap();
    }
    let source = b"fn main() -> u8 { while false { fix value = -1 }\nreturn 0 }";
    assert_eq!(
        compile_with_mode(source, ArithmeticMode::Warn)
            .unwrap()
            .warnings
            .len(),
        1
    );
    let error = compile_with_mode(source, ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert!(error.message.contains("diagnostic.arithmetic"));
}
