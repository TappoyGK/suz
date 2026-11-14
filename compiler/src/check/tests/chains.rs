use crate::{ArithmeticMode, ErrorKind, compile, compile_with_mode};

#[test]
fn comparison_chains_check_direction_and_every_operand() {
    for expression in [
        "1 < 2 > 3",
        "3 >= 2 <= 1",
        "1 < 2 == 2 > 1",
        "1 != 2 != 3",
        "1 < 2 != 3",
        "1 != 2 < 3",
        "1 == 2 != 3",
    ] {
        let source = format!("fn main() -> u8 {{ fix result = {expression}\nreturn 0 }}");
        let error = compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains("expr.comparison-chain"), "{error}");
    }
    for (body, rule) in [
        (
            "fix first: u8 = 3\nfix second: i32 = 4\nfix result = first < 2 < second",
            "type.expected",
        ),
        ("fix top: u8 = 3\nfix result = 256 < 2 < top", "lit.range"),
        ("fix result = 3 < 2 < missing", "decl.local-scope"),
        ("fix result = true == true < false", "expr.operator-types"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "{error}");
    }
    let source = b"fn main() -> u8 { fix top: u64 = 18446744073709551615\nfix result = 0 < 18446744073709551615 == top\nreturn 0 }";
    compile(source).unwrap();
    let source = b"fn main() -> u8 { fix result = 3 < 2 < -1\nreturn 0 }";
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
}
