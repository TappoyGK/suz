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
        assert_eq!(error.message(), message, "{source}");
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
            ("fix items = 0", "3"),
            ("fix items = (0", "3)"),
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
            "fix VALUE = 0RANGE3\nfn main() -> u8 { return 0 }",
            "struct Item { value: u8 }\nfn main() -> u8 { fix item = Item { value: 0RANGE3 }\nreturn 0 }",
            "fn main() -> u8 { if 0RANGE3 {}\nreturn 0 }",
            "fn main() -> u8 { while 0RANGE3 {}\nreturn 0 }",
            "fn main() -> u8 { match 0RANGE3 { _ => {} }\nreturn 0 }",
            "fn main() -> u8 { match 0 { 0RANGE3 => {}\n_ => {} }\nreturn 0 }",
            "fn main() -> u8 { return matchexpr 0 { _ => 0RANGE3 } }",
            "fn main() -> u8 { log info \"${0RANGE3}\"\nreturn 0 }",
            "fn main() -> u8 { fix message = \"${0RANGE3}\"\nreturn 0 }",
            "fn main() -> u8 { return expr { 0RANGE3 } }",
            "fn main() -> u8 { return ifexpr true { 0RANGE3 } else { 0 } }",
            "fn main() -> u8 { for i in 0..<3RANGE4 {}\nreturn 0 }",
            "fn main() -> u8 { fix value = 1 / 0 or 0RANGE3\nreturn 0 }",
            "fn main() -> u8 { fix value = 256 as u8 or 0RANGE3\nreturn 0 }",
            "fn f() -> void {\n    fix x: u8 = 1 / 0 or returnRANGE3\n}\nfn main() -> u8 { return 0 }",
            "fn f() -> void {\n    returnRANGE3\n}\nfn main() -> u8 { return 0 }",
            "fn main() -> u8 { fix value = 1 +RANGE3\nreturn 0 }",
            "fn main() -> u8 { log info \"${(RANGE3)}\"\nreturn 0 }",
            "fn main() -> u8 {\n    log info \"${RANGE3}\"\n    return 0\n}",
            "fn main() -> u8 { log info \"${0RANGE}\"\nreturn 0 }",
            "fn main() -> u8 { log info \"${RANGE}\"\nreturn 0 }",
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
                "-std=c23",
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
            assert!(error.message().contains(rule), "{source}: {error:?}");
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
    // コンパイラが生成する範囲の更新処理自体には、整数演算のハンドラーは不要。
    for keyword in ["for", "rev"] {
        let source = format!("fn main() -> u8 {{ {keyword} i in 0..=255 {{}}\nreturn 0 }}");
        suz::compile_with_mode(source.as_bytes(), suz::ArithmeticMode::Deny).unwrap();
    }
}

#[test]
fn indexed_array_and_type_iteration_runtime_and_diagnostics() {
    let source = r#"
enum Wave: u8 { Sine, Saw, Square }
enum Single: u8 { Only }
range Step: u8 = 0..<4
range One: i32 = 3..=3
range SignedByte: i8 = -128..=127
range BigLow: i64 = -9223372036854775808..=-9223372036854775806
range BigHigh: u64 = 18446744073709551613..=18446744073709551615

struct Voice {
    freq: i32,
}

struct State {
    voices: #[Voice],
}

fn run_ptr(state: ref State) -> i32 {
    var acc: i32 = 0
    for i, v in (.state).voices {
        acc = acc + (i as i32) * 10 + v.freq or 0
    }
    return acc
}

fn run_row(grid: .[i32; _, _], y: len_t) -> i32 {
    var acc: i32 = 0
    if y < len grid {
        for x, v in grid.[y] {
            acc = acc + (x as i32) * 10 + v or 0
        }
    }
    return acc
}

fn main() -> u8 {
    // SoA 列挙型添字配列の 2 変数反復
    fix soa_enum: soa #[Voice; Wave] = new soa #[Voice { freq: 5 }; Wave]
    for w, v in soa_enum {
        soa_enum.[w]#freq = v.freq + (w as i32) or return 1
    }
    if soa_enum.[Wave.Sine].freq != 5 || soa_enum.[Wave.Saw].freq != 6 || soa_enum.[Wave.Square].freq != 7 {
        return 2
    }

    // 切り出し・ポインタ経由・2 次元配列の 1 行の 2 変数反復
    fix base: #[i32] = new #[10, 20, 30, 40]
    var slice_acc: i32 = 0
    for i, x in base.[1:] {
        slice_acc = slice_acc + (i as i32) * 100 + x or return 3
    }
    if slice_acc != 390 { return 4 }

    fix st = new State { voices: new #[Voice { freq: 1 }, Voice { freq: 2 }] }
    if run_ptr(st) != 13 { return 5 }

    fix grid: .[i32; _, _] = .[[1, 2], [3, 4]]
    if run_row(grid, 1) != 17 { return 6 }

    // 1 要素列挙型・1 値 range・端点（i8/i64/u64）反復
    var single_count: i32 = 0
    for s in Single { single_count = single_count + 1 or return 7 }
    rev s in Single { single_count = single_count + 1 or return 8 }
    if single_count != 2 { return 9 }

    var one_sum: i32 = 0
    for o in One { one_sum = one_sum + (o as i32) or return 10 }
    rev o in One { one_sum = one_sum + (o as i32) or return 11 }
    if one_sum != 6 { return 12 }

    var sb: i32 = 0
    for v in SignedByte {
        sb = sb + 1 or return 13
        if v == 127 { continue }
    }
    rev v in SignedByte {
        sb = sb + 1 or return 14
        if v == -128 { continue }
    }
    if sb != 512 { return 15 }

    var bl: i32 = 0
    for v in BigLow { bl = bl + 1 or return 16 }
    rev v in BigLow { bl = bl + 1 or return 17 }
    if bl != 6 { return 18 }

    var bh: i32 = 0
    for v in BigHigh {
        bh = bh + 1 or return 19
        if v == 18446744073709551615 { continue }
    }
    rev v in BigHigh { bh = bh + 1 or return 20 }
    if bh != 6 { return 21 }

    // 値域制限型の型名反復で範囲保証が効くこと
    fix steps: .[i32] = new .[10, 20, 30, 40]
    var step_sum: i32 = 0
    for s in Step {
        fix v = steps.[s]
        step_sum = step_sum + v or return 22
    }
    if step_sum != 100 { return 23 }
    return 0
}
"#;
    let dir = TemporaryDirectory::new();
    let c_path = dir.0.join("main.c");
    fs::write(&c_path, suz::compile(source.as_bytes()).unwrap()).unwrap();
    for optimization in ["-O0", "-O2"] {
        let bin_path = dir.0.join(format!("main{optimization}"));
        let out = runtime_link::compiler()
            .args([
                "-std=c23",
                "-Wall",
                "-Wextra",
                "-Werror",
                optimization,
                "-fsanitize=undefined",
                "-fno-sanitize-recover=all",
            ])
            .arg(&c_path)
            .arg("-o")
            .arg(&bin_path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "C compile failed ({optimization}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let run = Command::new(&bin_path).output().unwrap();
        assert_eq!(
            run.status.code(),
            Some(0),
            "runtime failed ({optimization}): {}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    // 拒否ケース
    for (source, rule) in [
        (
            "fn main() -> u8 { for i, x in 0..<3 {}\nreturn 0 }",
            "for.pair-target",
        ),
        (
            "fn main() -> u8 { for i, c in \"abc\" {}\nreturn 0 }",
            "for.pair-target",
        ),
        (
            "enum Wave: u8 { A }\nfn main() -> u8 { for w, x in Wave {}\nreturn 0 }",
            "for.pair-target",
        ),
        (
            "range Step: u8 = 0..<4\nfn main() -> u8 { for s, x in Step {}\nreturn 0 }",
            "for.pair-target",
        ),
        (
            "fn main() -> u8 { for w in UnknownEnum {}\nreturn 0 }",
            "for.type-name",
        ),
        (
            "struct Vec2 { x: f32 }\nfn main() -> u8 { for p in Vec2 {}\nreturn 0 }",
            "for.type-name",
        ),
        (
            "enum Wave: u8 { A }\nfn main() -> u8 { fix g = new #[1; Wave]\nfor w, _ in g {}\nreturn 0 }",
            "src.ident",
        ),
        (
            "fn main() -> u8 { fix a = new .[1, 2]\nfor x, x in a {}\nreturn 0 }",
            "decl.duplicate",
        ),
        (
            "fn main() -> u8 { fix a = new .[1, 2]\nfor i, x in a { i = 0 }\nreturn 0 }",
            "for.variable",
        ),
        (
            "fn main() -> u8 { fix a = new .[1, 2]\nfor i, x in a { x = 0 }\nreturn 0 }",
            "for.variable",
        ),
        (
            "fn main() -> u8 { var a = new .[1, 2]\nfor i, x in a {}\nreturn 0 }",
            "for.bound-reference",
        ),
        // ポインタ経由の配列では範囲保証を覚えないため、別アクセスに or が必要
        (
            "struct State { a: .[i32] }\nfn f(s: ref State) -> i32 { for i, x in (.s).a { fix y = (.s).a.[i]\nreturn y }\nreturn 0 }\nfn main() -> u8 { return 0 }",
            "array.access-handler",
        ),
        // 切り出しでは範囲保証を覚えないため、元配列へのアクセスに or が必要
        (
            "fn f(a: .[i32]) -> i32 { if 1 <= len a { for i, x in a.[1:] { fix y = a.[i]\nreturn y } }\nreturn 0 }\nfn main() -> u8 { return 0 }",
            "array.access-handler",
        ),
    ] {
        for source in [source.to_owned(), source.replace("for ", "rev ")] {
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.kind, suz::ErrorKind::InvalidSource, "{source}");
            assert!(error.message().contains(rule), "{source}: {error:?}");
        }
    }
}
