use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn cast_returns_handle_initializers_assignments_and_compound_assignments() {
    let directory = TemporaryDirectory::new();
    for (index, (body, expected)) in [
        ("fix raw: i64 = 300\nfix count = ((raw as u8 or return 7))\nreturn count", 7),
        ("fix raw: i64 = 3\nvar count = raw as u8 or return 7\ncount = 4\nreturn count", 4),
        ("var count: u8 = 5\nfix raw: f64 = 256.0\ncount = raw as u8 or return count\nreturn 99", 5),
        ("var count: u8 = 5\nfix raw: i64 = 300\ncount +%= (raw as u8 or return count)\nreturn 99", 5),
        ("var count: u8 = 5\nfix raw: f64 = 3.75\ncount +%= ((raw as u8 or return 7))\nreturn count", 8),
        ("var count: f32 = 1.0\nfix raw: f64 = 1.0e39\ncount += raw as f32 or return 7\nreturn 99", 7),
        ("var count: f32 = 1.0\nfix raw: f64 = 2.0\ncount += raw as f32 or return 7\nreturn ifexpr count == 3.0 { 0 } else { 99 }", 0),
        ("fix raw: f64 = nan\nfix count = raw as i64 or return 1 +% 6\nreturn 99", 7),
        ("fix raw: f64 = inf\nfix count = raw as u64 or return ifexpr true { 7 } else { 8 }\nreturn 99", 7),
        ("fix raw: u8 = 3\nfix value = raw as f32 or return 99\nreturn ifexpr value == 3.0 { 0 } else { 99 }", 0),
        ("loop {\nfix raw: i64 = 300\nfix count = raw as u8 or return 7\n}", 7),
        ("fix first = 3.0 as u8 or return 99\nfix second = 4.0 as u8 or return 99\nreturn first +% second", 7),
    ].into_iter().enumerate() {
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
fn cast_return_positions_and_return_types_are_checked_before_lowering() {
    for (body, rule) in [
        (
            "fix raw: i64 = 300\nfix x = (raw as u8 or return 7) + 2\nreturn 0",
            "expr.or-return-position",
        ),
        ("return 300 as u8 or return 7", "expr.or-return-position"),
        (
            "fix x = 1 as u8 or (300 as u8 or return 7)\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "fix x = 300 as u8 or return (300 as u8 or return 7)\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "fix x = (300 as u8 or return 7) as u16\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "if (300 as u8 or return 7) == 0 { return 0 }\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "while (300 as u8 or return 7) == 0 { break }\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "var count: u8 = 0\ncount +%= (300 as u8 or return 7) +% 1\nreturn count",
            "expr.or-return-position",
        ),
        (
            "fix x = ifexpr true { 300 as u8 or return 7 } else { 0 }\nreturn x",
            "expr.jump-prohibited",
        ),
        (
            "fix x = ifexpr (300 as u8 or return 7) == 0 { 0 } else { 1 }\nreturn x",
            "expr.jump-prohibited",
        ),
        (
            "fix x = 300 as u8 or return false\nreturn 0",
            "type.expected",
        ),
        ("fix x = 1 as u8 or return false\nreturn 0", "type.expected"),
        ("fix x = 300 as u8 or return\nreturn 0", "fn.return"),
        (
            "fix x = 300 as u8 or return x\nreturn 0",
            "decl.local-scope",
        ),
        ("fix x = 300 as u8 or return 7", "fn.fallthrough"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{body}: {error:?}"
        );
        assert!(error.message().contains(rule), "{body}: {error:?}");
    }
    let source = b"fn main() -> u8 { fix x = -1 as u8 or return (-2 as u8 or 7)\nreturn 0 }";
    let warned = suz::compile_with_mode(source, suz::ArithmeticMode::Warn).unwrap();
    assert_eq!(warned.warnings.len(), 2);
    assert!(
        suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
            .err()
            .unwrap()
            .message()
            .contains("diagnostic.arithmetic")
    );
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn cast_return_codegen_evaluates_source_once_and_return_value_only_on_failure() {
    let directory = TemporaryDirectory::new();
    for (input, expected, trace) in [("3.75", 3, 1), ("256.0", 7, 12), ("nan", 7, 12)] {
        let source = format!(
            "fn main() -> u8 {{\nfix input: f64 = {input}\nfix failure: u8 = 7\nfix count = input as u8 or return failure\nreturn count\n}}"
        );
        let generated = suz::compile(source.as_bytes()).unwrap();
        let marker = "suz_cmp_0 = suz_local_0;";
        assert_eq!(generated.matches(marker).count(), 1);
        let generated = generated.replace(marker, &format!("#define suz_local_0 probe(1, suz_local_0)\n#define suz_local_1 probe(2, suz_local_1)\n{marker}"));
        let c = format!(
            r#"static int trace;
static double probe(int id, double value) {{ trace = trace * 10 + id; return value; }}
#define main generated_main
{generated}
#undef main
int main(void) {{
    int result = generated_main();
    return result != {expected} || trace != {trace};
}}
"#
        );
        let path = directory.0.join(format!("trace-{input}.c"));
        fs::write(&path, c).unwrap();
        assert_eq!(
            build_and_run_c(&path, &directory.0.join(format!("trace-{input}")))
                .status
                .code(),
            Some(0)
        );
    }
}
