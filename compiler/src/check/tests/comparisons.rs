use crate::{ArithmeticMode, ErrorKind, compile, compile_with_mode};

#[test]
fn comparison_operands_keep_their_types_and_constrain_literals() {
    for (body, rule) in [
        ("fix result = 1 == true\nreturn 0", "type.expected"),
        ("fix result = true < false\nreturn 0", "expr.operator-types"),
        ("fix result: u8 = 1 < 2\nreturn 0", "type.expected"),
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

#[test]
fn array_reference_comparisons_report_array_value_at_operator() {
    for (source, line, column) in [
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix b = new #[1, 2]\n    fix m = a < b\n    return 0\n}",
            4,
            15,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix m = a < 1.5\n    return 0\n}",
            3,
            15,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix x: i64 = 1\n    fix m = a < x\n    return 0\n}",
            4,
            15,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix x: i64 = 1\n    fix m = x < a\n    return 0\n}",
            4,
            15,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix k = 1 < a\n    return 0\n}",
            3,
            15,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix m = a == 1\n    return 0\n}",
            3,
            15,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix out = new #[0; 2]\n    out#[not (a > 1)] = 1\n    return 0\n}",
            4,
            17,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix m = 1 < a < 3\n    return 0\n}",
            3,
            15,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix m = 1 < 2 < a\n    return 0\n}",
            3,
            19,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix t: text = \"hi\"\n    fix m = t == a\n    return 0\n}",
            4,
            15,
        ),
        (
            "struct Holder { values: .[i32] }\nfn main() -> u8 {\n    fix h = Holder { values: .[1] }\n    fix m = h == 1\n    return 0\n}",
            4,
            15,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix m = a == .[]\n    return 0\n}",
            3,
            15,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix m = .[] == a\n    return 0\n}",
            3,
            17,
        ),
        (
            "fn main() -> u8 {\n    fix a = new .[1, 2]\n    fix m = a == new #[]\n    return 0\n}",
            3,
            15,
        ),
        (
            "struct Point { x: i32, y: i32 }\nfn main() -> u8 {\n    fix a = new soa #[Point { x: 1, y: 2 }]\n    fix m = a == 1\n    return 0\n}",
            4,
            15,
        ),
        (
            "struct Point { x: i32, y: i32 }\nfn main() -> u8 {\n    fix a = new soa #[Point { x: 1, y: 2 }]\n    fix m = a == soa .[]\n    return 0\n}",
            4,
            15,
        ),
    ] {
        let error = compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource, "{source}: {error}");
        assert_eq!(error.rule(), "array.value", "{source}: {error}");
        assert_eq!(
            (error.line, error.column),
            (line, column),
            "{source}: {error}"
        );
    }
    // オペランド内の未定義名や部分式の誤りは、配列比較の [array.value] より先に報告する。
    for (source, rule, line, column) in [
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix m = a < undefined_name\n    return 0\n}",
            "decl.local-scope",
            3,
            17,
        ),
        (
            "fn main() -> u8 {\n    fix a = new #[1, 2]\n    fix m = (a.[5] + true) < a\n    return 0\n}",
            "array.access-handler",
            3,
            15,
        ),
    ] {
        let error = compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidSource, "{source}: {error}");
        assert_eq!(error.rule(), rule, "{source}: {error}");
        assert_eq!(
            (error.line, error.column),
            (line, column),
            "{source}: {error}"
        );
    }
}
