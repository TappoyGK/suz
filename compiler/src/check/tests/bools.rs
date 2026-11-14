use crate::{ArithmeticMode, ErrorKind, compile_with_mode};

#[test]
fn bool_bindings_and_not_are_checked_without_arithmetic_diagnostics() {
    let source = b"fn main() -> u8 {\nfix ready = true\nvar flag: bool = not ready\nflag = not not (false)\nreturn 0\n}";
    for mode in [
        ArithmeticMode::Default,
        ArithmeticMode::Warn,
        ArithmeticMode::Deny,
    ] {
        let compiled = compile_with_mode(source, mode).unwrap();
        assert!(compiled.warnings.is_empty());
    }
    for (body, rule, line, column) in [
        ("fix flag: bool = 1\nreturn 0", "type.expected", 2, 18),
        ("fix value: u8 = true\nreturn value", "type.expected", 2, 17),
        ("return false", "type.expected", 2, 8),
        ("fix flag = not 1\nreturn 0", "expr.not", 2, 12),
        (
            "fix value = 1\nfix flag = not value\nreturn 0",
            "expr.not",
            3,
            12,
        ),
        ("var flag = true\nflag = 0\nreturn 0", "type.expected", 3, 8),
        (
            "var count = 0\ncount = false\nreturn 0",
            "type.expected",
            3,
            9,
        ),
        (
            "fix flag = true\nflag = false\nreturn 0",
            "var.fix-assign",
            3,
            1,
        ),
        ("var flag = false\nreturn 0", "var.never-reassigned", 2, 5),
        ("return 0\nfix flag = not 1", "expr.not", 3, 12),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = compile_with_mode(source.as_bytes(), ArithmeticMode::Default)
            .err()
            .unwrap();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "{error}");
        assert_eq!((error.line, error.column), (line, column), "{error}");
    }
}
