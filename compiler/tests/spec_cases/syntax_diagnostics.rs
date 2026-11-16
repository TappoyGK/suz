use crate::test_util::TemporaryDirectory;
use std::{fs, process::Command};

#[test]
fn invalid_syntax_reports_source_errors_through_check() {
    let directory = TemporaryDirectory::new();
    for (index, (source, message)) in [
        (
            "fn helper -> u8 { return 0 }\nfn main() -> u8 { return 0 }",
            "expected '(' [syntax.grammar]",
        ),
        ("fn main() -> u8 return 0", "expected '{' [syntax.grammar]"),
        (
            "fn main() -> u8 { return (1 }",
            "expected ')' [syntax.grammar]",
        ),
        (
            "fn main() -> u8 { for i 0..<3 {}\nreturn 0 }",
            "expected 'in' [syntax.grammar]",
        ),
        (
            "fn main() -> u8 { match true { true {}\nfalse => {} }\nreturn 0 }",
            "expected '=>' [syntax.grammar]",
        ),
        (
            "fn main() -> u8 { fix x: Missing = 1\nreturn 0 }",
            "unknown type 'Missing' [type.name]",
        ),
        (
            "struct Item { x: Missing }\nfn main() -> u8 { return 0 }",
            "unknown type 'Missing' [type.name]",
        ),
        (
            "log info `outside`\nfn main() -> u8 { return 0 }",
            "[decl.top-level]",
        ),
        (
            "fn main() -> u8 { return _ }",
            "expected an expression [syntax.grammar]",
        ),
        (
            "fn main() -> u8 { return match true { true => 0\nfalse => 1 } }",
            "expected an expression [syntax.grammar]",
        ),
        ("fn main() -> u8 { return 0, }", "[src.statement-end]"),
        ("fn main() -> u8 { return 0 ) }", "[src.statement-end]"),
    ]
    .into_iter()
    .enumerate()
    {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}: {error}"
        );
        assert!(error.message().contains(message), "{source}: {error}");
        let input = directory.0.join(format!("invalid-{index}.suz"));
        fs::write(&input, source).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_suz"))
            .arg("check")
            .arg(&input)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1), "{source}");
        assert!(result.stdout.is_empty());
        let diagnostic = String::from_utf8(result.stderr).unwrap();
        assert!(
            diagnostic.starts_with(&format!("{}:", input.display())),
            "{diagnostic}"
        );
        assert!(diagnostic.contains(message), "{diagnostic}");
    }
}

#[test]
fn truncated_programs_are_source_errors_not_unsupported_features() {
    let source = r#"enum Mode: u8 { A, B }
struct Pair { x: u8, y: u8 }
const SEED = Pair { x: 1, y: 2 }
fn add(value: Pair) -> u8 { return value.x + value.y }
fn main() -> u8 {
    // 日本語のコメント
    var count: u8 = 0
    for index in 0..<SEED.y { count += index }
    fix selected = matchexpr Mode.A {
        Mode.A => ifexpr true { add(SEED) } else { 0 }
        Mode.B => 0
    }
    return selected + count
}
"#;
    suz::compile(source.as_bytes()).unwrap();
    let rules = crate::spec_rules();
    for (end, _) in source.char_indices() {
        let prefix = &source[..end];
        if let Err(error) = suz::compile(prefix.as_bytes()) {
            assert_eq!(
                error.kind,
                suz::ErrorKind::InvalidSource,
                "prefix {prefix:?}: {error}"
            );
            assert!(error.line > 0 && error.column > 0);
            let rule = error
                .message()
                .rsplit_once(" [")
                .and_then(|(_, rule)| rule.strip_suffix(']'));
            assert!(
                rule.is_some_and(|rule| rules.contains(rule)),
                "prefix {prefix:?}: missing or unknown specification rule: {error}"
            );
        }
    }
}

#[test]
fn list_commas_follow_elements_before_statement_newlines() {
    for source in [
        "struct Pair { x: u8\n, y: u8,\n}\nfn main() -> u8 { return 0 }",
        "enum Mode: u8 { A\n, B,\n}\nfn main() -> u8 { return 0 }",
        "struct Pair { x: u8, y: u8 }\nfn main() -> u8 { fix p = Pair { x: 1\n, y: 2,\n}\nreturn p.x }",
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().ends_with("[syntax.grammar]"), "{error}");
        assert_eq!(error.column, 1);
    }
    // 括弧内の物理的な改行は、字句解析で取り除かれる。
    suz::compile(
        b"fn sum(a: u8\n, b: u8) -> u8 { return a + b }\nfn main() -> u8 { return sum(1\n, 2) }",
    )
    .unwrap();
}
