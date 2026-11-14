use crate::{ArithmeticMode, ErrorKind, compile, compile_with_mode};

#[test]
fn comparison_operands_keep_their_types_and_constrain_literals() {
    for (body, rule) in [
        (
            "fix value: u8 = 1\nfix result = value < 256\nreturn 0",
            "lit.range",
        ),
        (
            "fix value: u8 = 1\nfix result = -1 < value\nreturn 0",
            "lit.range",
        ),
        (
            "fix first: u8 = 1\nfix second: u16 = 1\nfix result = first == second\nreturn 0",
            "type.expected",
        ),
        ("fix result = 1 == true\nreturn 0", "type.expected"),
        ("fix result = true < false\nreturn 0", "expr.operator-types"),
        ("fix result: u8 = 1 < 2\nreturn 0", "type.expected"),
        (
            "fix result = 18446744073709551615 == 18446744073709551615\nreturn 0",
            "lit.range",
        ),
        (
            "fix value = 1\nfix result = not value == 0\nreturn 0",
            "expr.not",
        ),
        (
            "fix result = false && (true < false)\nreturn 0",
            "expr.operator-types",
        ),
        ("fix result = missing == 0\nreturn 0", "decl.local-scope"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "expected {rule}: {error}");
    }
    for expression in [
        "top == 18446744073709551615",
        "18446744073709551615 == (top)",
    ] {
        let source = format!(
            "fn main() -> u8 {{\nfix top: u64 = 18446744073709551615\nfix result: bool = {expression}\nreturn 0\n}}"
        );
        compile(source.as_bytes()).unwrap();
    }
    let source = b"fn main() -> u8 { fix result = -1 < 0\nreturn 0 }";
    assert_eq!(
        compile_with_mode(source, ArithmeticMode::Warn)
            .unwrap()
            .warnings
            .len(),
        1
    );
    assert!(
        compile_with_mode(source, ArithmeticMode::Deny)
            .err()
            .unwrap()
            .message
            .contains("diagnostic.arithmetic")
    );
    let source = b"fn main() -> u8 { fix result = 1 < 2\nreturn 0 }";
    assert!(
        compile_with_mode(source, ArithmeticMode::Deny)
            .unwrap()
            .warnings
            .is_empty()
    );
}
