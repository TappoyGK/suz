use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn arithmetic_returns_preserve_assignment_state_and_only_evaluate_failure_values() {
    let directory = TemporaryDirectory::new();
    for (index, (body, expected)) in [
        ("fix value: u8 = 1 << 8 or return 7\nreturn 99", 7),
        ("var value: u8 = 9\nvalue >>= 1 << 64 or return value\nreturn 99", 9),
        ("var value: u8 = 8\nfix count: u64 = 1\nvalue >>= count << 0 or return 99\nreturn value", 4),
        (
            "fix lo: i8 = -128\nfix value = -lo or return 7\nreturn 99",
            7,
        ),
        (
            "var value: i8 = -128\nvalue = -value or return (value as% u8)\nreturn 99",
            128,
        ),
        (
            "var value: i8 = 1\nfix lo: i8 = -128\nvalue += -lo or return 7\nreturn 99",
            7,
        ),
        (
            "fix value: i8 = -128 or return (255 + 1)\nreturn value as% u8",
            128,
        ),
        (
            "fix value: i8 = -(1) or return 99\nreturn value as% u8",
            255,
        ),
        ("fix value: u8 = 7 / 0 or return 7\nreturn value", 7),
        (
            "var value: u8 = 9\nvalue = 7 % 0 or return value\nreturn 99",
            9,
        ),
        (
            "var value: u8 = 9\nvalue /= 1 / 0 or return value\nreturn 99",
            9,
        ),
        ("var value: u8 = 9\nvalue %= 1 / 0 or 4\nreturn value", 1),
        (
            "fix top: u8 = 255\nfix value = ((top + 1 or return 7))\nreturn value",
            7,
        ),
        (
            "var value: u8 = 9\nvalue = 255 + 1 or return value\nreturn 99",
            9,
        ),
        (
            "var value: u8 = 9\nvalue += (255 as u8) + 1 or return value\nreturn 99",
            9,
        ),
        (
            "var value: u8 = 9\nvalue += 1 + 2 or return (255 + 1)\nreturn value",
            12,
        ),
        (
            "var value: u8 = 9\nvalue = 1 + 2 or return (255 + 1)\nreturn value",
            3,
        ),
        (
            "fix value: u8 = 255 + 1 or return (255 + 1 or 7)\nreturn 99",
            7,
        ),
        ("loop {\nfix value: u8 = 255 + 1 or return 7\n}", 7),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let c = suz::compile(source.as_bytes()).unwrap();
        let path = directory.0.join(format!("return-{index}.c"));
        fs::write(&path, c).unwrap();
        let result = build_and_run_c(&path, &directory.0.join(format!("return-{index}")));
        assert_eq!(result.status.code(), Some(expected), "{body}");
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
fn arithmetic_handlers_check_types_and_control_transfer_positions() {
    for (body, rule) in [
        ("fix value: i8 = -129 or 0\nreturn 0", "lit.range"),
        ("fix value: u8 = -0\nreturn 0", "expr.operator-types"),
        ("fix value = -true\nreturn 0", "expr.operator-types"),
        ("fix value = -1 or false\nreturn 0", "type.expected"),
        ("fix value = -1 or return false\nreturn 0", "type.expected"),
        (
            "fix value = -(-1 or return 7)\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "fix value = ifexpr true { -1 or return 7 } else { 0 }\nreturn 0",
            "expr.jump-prohibited",
        ),
        (
            "fix value = 1.0 % 2.0 or 0.0\nreturn 0",
            "expr.arithmetic-handler",
        ),
        (
            "fix value = 1.0 %% 2.0 or 0.0\nreturn 0",
            "expr.arithmetic-handler",
        ),
        ("fix value = true % false\nreturn 0", "expr.operator-types"),
        ("fix value = 1 / 2 or false\nreturn 0", "type.expected"),
        (
            "fix value = (1 % 0 or return 7) + 3\nreturn 0",
            "expr.or-return-position",
        ),
        ("fix value = 1 + 2 or false\nreturn 0", "type.expected"),
        (
            "fix value = 1 + 2 or return false\nreturn 0",
            "type.expected",
        ),
        ("fix value = 1 + 2 or return\nreturn 0", "fn.return"),
        ("fix value = 1 + 2 or return 7", "fn.fallthrough"),
        (
            "fix value = 1 + 2 or return value\nreturn 0",
            "decl.local-scope",
        ),
        ("return 1 + 2 or return 7", "expr.or-return-position"),
        (
            "fix value = (1 + 2 or return 7) + 3\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "fix value = 1 + 2 or (3 + 4 or return 7)\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "fix value = 1 + 2 or return (3 + 4 or return 7)\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "fix value = (1 + 2 or return 7) as u8\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "if (1 + 2 or return 7) == 3 { return 0 }\nreturn 1",
            "expr.or-return-position",
        ),
        (
            "while (1 + 2 or return 7) == 3 { break }\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "var value: u8 = 1\nvalue += (1 + 2 or return 7) + 3\nreturn value",
            "expr.or-return-position",
        ),
        (
            "fix value = ifexpr true { 1 + 2 or return 7 } else { 0 }\nreturn 0",
            "expr.jump-prohibited",
        ),
        (
            "fix value = ifexpr (1 + 2 or return 7) == 3 { 0 } else { 1 }\nreturn 0",
            "expr.jump-prohibited",
        ),
        (
            "fix value = 1 +% 2 or 0\nreturn 0",
            "expr.arithmetic-handler",
        ),
        (
            "fix value = 1.0 + 2.0 or 0.0\nreturn 0",
            "expr.arithmetic-handler",
        ),
        (
            "fix value = (1 + 2 or 0) or 0\nreturn 0",
            "expr.arithmetic-handler",
        ),
        (
            "fix value = 1 + 2 == 3 or false\nreturn 0",
            "expr.arithmetic-handler",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}: {error:?}"
        );
        assert!(error.message().contains(rule), "{source}: {error:?}");
    }
    let error =
        suz::compile(b"fix VALUE = 1 + 2 or return 7\nfn main() -> u8 { return 0 }").unwrap_err();
    assert!(error.message().contains("const.expression"));
}

#[test]
fn arithmetic_warning_modes_track_each_operation_after_return_lowering() {
    let source = b"fn main() -> u8 {\nvar value: u8 = 1\nvalue += 1 + 2 or return (3 + 4)\nfix next = ((value + 1) * 2) or 0\nreturn next\n}";
    let warnings = suz::compile_with_mode(source, suz::ArithmeticMode::Warn)
        .unwrap()
        .warnings;
    assert_eq!(
        warnings
            .iter()
            .map(|w| (w.line, w.column))
            .collect::<Vec<_>>(),
        [(3, 7), (3, 29)]
    );
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert_eq!((error.line, error.column), (3, 7));
    let handled = b"fn main() -> u8 {\nfix value: u8 = ((1 + 2 or 0) * 3) or return (255 + 1 or 7)\nreturn value\n}";
    assert!(suz::compile_with_mode(handled, suz::ArithmeticMode::Deny).is_ok());
}

#[test]
fn negation_handlers_cover_the_operator_but_preserve_operand_diagnostics() {
    let source = b"fn main() -> u8 {\nfix first: i8 = -128 or 0\nfix second = -first or 7\nfix third = -(-1) or 0\nreturn 0\n}";
    let warnings = suz::compile_with_mode(source, suz::ArithmeticMode::Warn)
        .unwrap()
        .warnings;
    assert!(warnings.is_empty());
    assert!(suz::compile_with_mode(source, suz::ArithmeticMode::Deny).is_ok());
    let handled =
        b"fix LO: i8 = -128 or 0\nfn main() -> u8 {\nfix value = -LO or return 7\nreturn 0\n}";
    assert!(suz::compile_with_mode(handled, suz::ArithmeticMode::Deny).is_ok());
}
