use crate::support::runtime_link;
use crate::{integer_bounds::INTEGER_BOUNDS, test_util::TemporaryDirectory};
use std::{fs, process::Command};

fn assert_range_error(source: &str, line: usize, column: usize) {
    assert_diagnostic(
        source,
        line,
        column,
        "range notation is only allowed in for or rev [for.range]",
    );
}

fn assert_diagnostic(source: &str, line: usize, column: usize, message: &str) {
    for source in [source.to_owned(), source.replace("for ", "rev ")] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource, "{source}");
        assert_eq!((error.line, error.column), (line, column), "{source}");
        assert_eq!(error.message, message, "{source}");
    }
}

#[test]
fn missing_range_end_at_eof_points_to_the_operator() {
    for operator in ["..<", "..="] {
        for (ending, column, message) in [
            (
                "",
                15,
                "expected an end bound after the range operator [for.range]",
            ),
            (
                "\n",
                15,
                "range bounds must touch the range operator [for.range-spacing]",
            ),
            (
                " ",
                15,
                "range bounds must touch the range operator [for.range-spacing]",
            ),
            ("3", 19, "expected '{' [syntax.grammar]"),
            ("(3", 20, "expected ')' [syntax.grammar]"),
        ] {
            let source = format!("fn main() -> u8 {{\n    for i in 0{operator}{ending}");
            assert_diagnostic(&source, 2, column, message);
        }
    }
}

#[test]
fn ranges_used_as_values_report_the_range_rule_at_the_operator() {
    for operator in ["..<", "..="] {
        for (before, after) in [
            ("fix range = 0", "3"),
            ("fix range = (0", "3)"),
            ("for i in (0", "3) {}"),
            ("for i in ((0", "3)) {}"),
            ("for i in 0..<(0", "3) {}"),
            ("fix value = identity(0", "3)"),
            ("for i in identity(0", "3)..<4 {}"),
        ] {
            let source = format!(
                "fn main() -> u8 {{\n    {before}{operator}{after}\n    return 0\n}}\nfn identity(value: u8) -> u8 {{ return value }}"
            );
            assert_range_error(&source, 2, 5 + before.len());
        }
    }
}

#[test]
fn range_misuse_is_diagnosed_in_other_expression_contexts() {
    for operator in ["..<", "..="] {
        for template in [
            "const VALUE = 0RANGE3\nfn main() -> u8 { return 0 }",
            "struct Item { value: u8 }\nfn main() -> u8 { fix item = Item { value: 0RANGE3 }\nreturn 0 }",
            "fn main() -> u8 { if 0RANGE3 {}\nreturn 0 }",
            "fn main() -> u8 { while 0RANGE3 {}\nreturn 0 }",
            "fn main() -> u8 { match 0RANGE3 { _ => {} }\nreturn 0 }",
            "fn main() -> u8 { match 0 { 0RANGE3 => {}\n_ => {} }\nreturn 0 }",
            "fn main() -> u8 { return matchexpr 0 { _ => 0RANGE3 } }",
            "fn main() -> u8 { log info `${0RANGE3}`\nreturn 0 }",
            "fn main() -> u8 { fix message = `${0RANGE3}`\nreturn 0 }",
            "fn main() -> u8 { return expr { 0RANGE3 } }",
            "fn main() -> u8 { return ifexpr true { 0RANGE3 } else { 0 } }",
            "fn main() -> u8 { for i in 0..<3RANGE4 {}\nreturn 0 }",
            "fn main() -> u8 { fix value = 1 / 0 or 0RANGE3\nreturn 0 }",
            "fn main() -> u8 { fix value = 256 as u8 or 0RANGE3\nreturn 0 }",
            "fn f() -> void {\n    fix x: u8 = 1 / 0 or returnRANGE3\n}\nfn main() -> u8 { return 0 }",
            "fn f() -> void {\n    returnRANGE3\n}\nfn main() -> u8 { return 0 }",
            "fn main() -> u8 { fix value = 1 +RANGE3\nreturn 0 }",
            "fn main() -> u8 { log info `${(RANGE3)}`\nreturn 0 }",
            "fn main() -> u8 {\n    log info `${RANGE3}`\n    return 0\n}",
            "fn main() -> u8 { log info `${0RANGE}`\nreturn 0 }",
            "fn main() -> u8 { log info `${RANGE}`\nreturn 0 }",
            "fn main() -> u8 {\n    for i in 0..<3 {}RANGE4\n    return 0\n}",
            "fn main() -> u8 { if true {}RANGE4\nreturn 0 }",
            "fn main() -> u8 { while false {}RANGE4\nreturn 0 }",
            "fn main() -> u8 { match true { true => {}\nfalse => {} }RANGE4\nreturn 0 }",
        ] {
            let (before, _) = template.split_once("RANGE").unwrap();
            let source = template.replace("RANGE", operator);
            assert_range_error(
                &source,
                before.lines().count(),
                before.rsplit('\n').next().unwrap().len() + 1,
            );
        }
    }
}

#[test]
fn range_delimiters_follow_complete_start_bounds() {
    for operator in ["..<", "..="] {
        for start in [
            "(identity(0))",
            "expr { fix value: u8 = 0\nvalue }",
            "ifexpr true { 0 } else { 1 }",
            "1 / 0 or 0",
            "256 as u8 or 0",
            "256 as u8 or 1 / 0 or 0",
        ] {
            let source = format!(
                "fn identity(value: u8) -> u8 {{ return value }}\nfn main() -> u8 {{ for i in {start}{operator}(identity(3)) {{}}\nreturn 0 }}"
            );
            for source in [source.clone(), source.replace("for ", "rev ")] {
                suz::compile(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error}"));
            }
        }
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn ranges_handle_integer_extremes_and_nested_control_without_overflow() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!(
            r#"{{
    fix low: {ty} = {min}
    fix high: {ty} = {max}
    fix before: {ty} = {before}
    var count: u8 = 0
    for i in before..=high {{
        if i < before {{ return 1 }}
        count +%= 1
        if count > 2 {{ return 2 }}
        continue
    }}
    if count != 2 {{ return 3 }}
    for i in low..=low {{
        if i != low {{ return 4 }}
        count +%= 1
        break
    }}
    for i in high..<high {{ return 5 }}
    for i in high..=low {{ return 6 }}
    for i in before..<high {{
        if i != before {{ return 7 }}
        count +%= 1
    }}
    if count != 4 {{ return 8 }}
    fix after: {ty} = {after}
    count = 0
    rev i in low..=after {{
        if i > after {{ return 10 }}
        count +%= 1
        if count > 2 {{ return 11 }}
        continue
    }}
    if count != 2 {{ return 12 }}
    rev i in low..<low {{ return 13 }}
    rev i in after..=low {{ return 14 }}
    rev i in low..<after {{
        if i != low {{ return 15 }}
        count +%= 1
        continue
    }}
    if count != 3 {{ return 16 }}

}}
"#,
            after = min + 1,
            before = max - 1
        ));
    }
    source.push_str(
        r#"var count: u8 = 0
    for outer in 0..<3 {
        for inner in 0..<3 {
            if inner == 1 { break }
            count +%= 1
        }
        var n: u8 = 0
        while n < 2 {
            n +%= 1
            continue
        }
        count +%= n
        if outer == 1 { continue }
        count +%= 1
    }
    return ifexpr count == 11 { 0 } else { 9 }
}
"#,
    );
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("ranges.c");
    fs::write(&path, suz::compile(source.as_bytes()).unwrap()).unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = directory.0.join(format!("ranges{optimization}"));
        let built = runtime_link::compiler()
            .args([
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                optimization,
                "-fsanitize=undefined",
                "-fno-sanitize-recover=all",
            ])
            .arg(&path)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let output = Command::new(executable).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
    }
}

#[test]
fn range_bounds_check_nested_references_scope_and_control_transfers() {
    for (body, rule) in [
        ("fix rev: u8 = 1\nreturn 0", "src.ident"),
        ("for i in 0..<1 { i = 0 }\nreturn 0", "for.variable"),
        ("for i in 0..<1 {}\nreturn i", "decl.local-scope"),
        ("for Bad in 0..<1 {}\nreturn 0", "name.value"),
        ("for i in true..<false {}\nreturn 0", "for.bounds"),
        ("for i in 0.0..<2.0 {}\nreturn 0", "for.bounds"),
        ("fix i = 0\nfor i in 0..<1 {}\nreturn 0", "decl.shadowing"),
        ("for i in 0..<1 { fix i = 2 }\nreturn 0", "decl.duplicate"),
        ("for main in 0..<1 {}\nreturn 0", "decl.shadowing"),
        ("for i in i..<1 {}\nreturn 0", "decl.local-scope"),
        (
            "var n: u8 = 2\nfor i in 0..<(n + 1) {}\nn = 0\nreturn 0",
            "for.bound-reference",
        ),
        (
            "var n: u8 = 2\nfor i in 0..<(ifexpr false { n } else { 2 }) {}\nn = 0\nreturn 0",
            "for.bound-reference",
        ),
        (
            "fix n: u16 = 2\nfor i in (n as u8 or return 1)..<3 {}\nreturn 0",
            "expr.or-return-position",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        for source in [source.clone(), source.replace("for ", "rev ")] {
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
            assert!(error.message.contains(rule), "{source}: {error:?}");
        }
    }
    let source = b"fn main() -> u8 {\nfix n: u8 = 2\nfor i in (n - 1)..<(n + 1) { fix x = i * 2 }\nreturn 0\n}";
    let warnings = suz::compile_with_mode(source, suz::ArithmeticMode::Warn)
        .unwrap()
        .warnings;
    assert_eq!(
        warnings
            .iter()
            .map(|w| (w.line, w.column))
            .collect::<Vec<_>>(),
        [(3, 13), (3, 23), (3, 40)]
    );
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert_eq!((error.line, error.column), (3, 13));
    // The compiler-generated range increment itself needs no arithmetic handler.
    for keyword in ["for", "rev"] {
        let source = format!("fn main() -> u8 {{ {keyword} i in 0..=255 {{}}\nreturn 0 }}");
        suz::compile_with_mode(source.as_bytes(), suz::ArithmeticMode::Deny).unwrap();
    }
}
