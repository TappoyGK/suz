use crate::support::runtime_link;
use crate::{integer_bounds::INTEGER_BOUNDS, test_util::TemporaryDirectory};
use std::{fs, process::Command};

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
}}
"#,
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
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "{source}: {error:?}");
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
    suz::compile_with_mode(
        b"fn main() -> u8 { for i in 0..=255 {}\nreturn 0 }",
        suz::ArithmeticMode::Deny,
    )
    .unwrap();
}
