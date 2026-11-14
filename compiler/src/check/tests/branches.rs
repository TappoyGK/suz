use crate::{ArithmeticMode, ErrorKind, compile, compile_with_mode};

#[test]
fn checks_all_branches_scopes_and_return_paths() {
    for (body, rule) in [
        ("if true { return 1 }", "fn.fallthrough"),
        ("if true { return 1 } else {}", "fn.fallthrough"),
        (
            "if true { return 1 } elif false {} else { return 2 }",
            "fn.fallthrough",
        ),
        (
            "if true { if false { return 1 } } else { return 2 }",
            "fn.fallthrough",
        ),
        ("if true {} else {}", "fn.value-return-required"),
        ("if 1 { return 1 }\nreturn 0", "if.condition"),
        (
            "if true { return 0 } elif 1 { return 1 } else { return 2 }",
            "if.condition",
        ),
        ("if true { return 0 } else { return true }", "type.expected"),
        (
            "if true { fix value = 0 } else { fix copy = value }\nreturn 0",
            "decl.local-scope",
        ),
        (
            "if true { fix value = 0 }\nreturn value",
            "decl.local-scope",
        ),
        (
            "fix value = 0\nif false { fix value = 1 }\nreturn 0",
            "decl.shadowing",
        ),
        (
            "if false { var value = 1 }\nreturn 0",
            "var.never-reassigned",
        ),
        ("if true {}\nelse {}\nreturn 0", "expr.else-line"),
        (
            "if true {} // comment\nelif false {}\nreturn 0",
            "expr.else-line",
        ),
        ("if false {} else if true {}\nreturn 0", "if.elif"),
        ("if false {} else {} elif true {}\nreturn 0", "if.elif"),
        ("if false {} else {} else {}\nreturn 0", "if.elif"),
        ("else {}\nreturn 0", "if.elif"),
        ("if true {} if false {}\nreturn 0", "src.statement-end"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource, "{error}");
        assert!(error.message.contains(rule), "expected {rule}: {error}");
    }
    let source = b"fn main() -> u8 {\nif true { return 0 } else { fix value = -1\nreturn 1 }\n}";
    let warned = compile_with_mode(source, ArithmeticMode::Warn).unwrap();
    assert_eq!(warned.warnings.len(), 1);
    let denied = compile_with_mode(source, ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert!(denied.message.contains("diagnostic.arithmetic"));
}
