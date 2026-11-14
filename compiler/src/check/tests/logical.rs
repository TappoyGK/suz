use crate::{ArithmeticMode, ErrorKind, compile, compile_with_mode};

#[test]
fn both_operands_are_checked_even_when_short_circuited() {
    for (expression, rule) in [
        ("false && 1", "expr.operator-types"),
        ("true || 1", "expr.operator-types"),
        ("1 && false", "expr.operator-types"),
        ("0 || true", "expr.operator-types"),
        ("false && missing", "decl.local-scope"),
        ("true || not 1", "expr.not"),
        ("false && (true = false)", "assign.statement"),
    ] {
        let source = format!("fn main() -> u8 {{\nfix result = {expression}\nreturn 0\n}}");
        let error = compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "{error}");
    }
    let source = b"fn main() -> u8 { fix result: u8 = true && false\nreturn 0 }";
    assert!(
        compile(source)
            .unwrap_err()
            .message
            .contains("type.expected")
    );
    for mode in [
        ArithmeticMode::Default,
        ArithmeticMode::Warn,
        ArithmeticMode::Deny,
    ] {
        let source = b"fn main() -> u8 { fix result = true || false && not false\nreturn 0 }";
        assert!(compile_with_mode(source, mode).unwrap().warnings.is_empty());
    }
}
