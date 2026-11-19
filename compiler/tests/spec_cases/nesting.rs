use crate::test_util::TemporaryDirectory;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn suz(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_suz"))
        .current_dir(dir)
        .args(args)
        .env_remove("SUZ_CFLAGS")
        .env_remove("CFLAGS")
        .env_remove("CPPFLAGS")
        .env_remove("LDFLAGS")
        .output()
        .unwrap()
}

fn success(output: Output) {
    assert!(output.status.success(), "{output:?}");
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn wrap(open: &str, close: &str, count: usize, value: &str) -> String {
    format!("{}{value}{}", open.repeat(count), close.repeat(count))
}

fn program(expr: &str) -> String {
    format!(
        "struct Point {{ x: i32, y: i32 }}\nfix P = Point {{ x: 0, y: 0 }}\nfn id(n: int) -> int {{ return n }}\nfn main() -> u8 {{\nfix value = {expr}\nreturn 0\n}}"
    )
}

#[test]
fn nesting_limit_rejects_deep_inputs_without_aborting() {
    // スタック枯渇はcatch_unwindで捕捉できないため、実際のCLIを別プロセスで検査する。
    let directory = TemporaryDirectory::new();
    for count in [255, 10_000] {
        for (family, expression) in [
            wrap("(", ")", count, "1"),
            wrap("id(", ")", count, "1"),
            format!("{}true", "not ".repeat(count)),
            format!("1{}", " + 1".repeat(count)),
            format!("P{}", " with { x: 1 }".repeat(count)),
            wrap("expr { ", " }", count, "1"),
            wrap("`${", "}`", count, "1"),
        ]
        .into_iter()
        .enumerate()
        {
            let offset = match family {
                0 => 255, // 次の丸括弧、または境界直後のリテラル。
                1 => 255 * 3,
                2 => 255 * 4,
                3 => expression.match_indices('+').nth(254).unwrap().0,
                4 => expression.match_indices("with").nth(254).unwrap().0,
                5 => expression.match_indices('{').nth(127).unwrap().0,
                6 => expression.match_indices("${").nth(254).unwrap().0,
                _ => unreachable!(),
            };
            fs::write(directory.0.join("deep.suz"), program(&expression)).unwrap();
            let output = suz(&directory.0, &["check", "deep.suz"]);
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("[src.nesting-limit]") && stderr.contains("256"),
                "{stderr}"
            );
            assert!(
                stderr.contains(&format!(
                    "deep.suz:5:{}:",
                    "fix value = ".len() + offset + 1
                )),
                "{stderr}"
            );
        }
        let source = format!(
            "fn main() -> u8 {{ {}return 0{}\nreturn 0 }}",
            "if true { ".repeat(count),
            " }".repeat(count)
        );
        fs::write(directory.0.join("deep.suz"), source).unwrap();
        let output = suz(&directory.0, &["check", "deep.suz"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("[src.nesting-limit]"));
    }
    // 他のコマンドも生成・ビルドに進む前に同じ診断で拒否する。
    for args in [
        vec!["build", "deep.suz", "-o", "bad"],
        vec!["run", "deep.suz"],
        vec!["emit-c", "deep.suz", "-o", "bad-c"],
    ] {
        let output = suz(&directory.0, &args);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("[src.nesting-limit]"));
    }
    assert!(!directory.0.join("bad").exists() && !directory.0.join("bad-c").exists());
}

#[test]
fn nesting_limit_reports_the_operator_that_deepens_an_existing_subtree() {
    let directory = TemporaryDirectory::new();
    for height in [254, 255] {
        let e = wrap("(", ")", height - 1, "1");
        for (expression, marker) in [
            (format!("1 + {e} + 1"), "+"),
            (format!("P with {{ x: {e} }} with {{ y: 1 }}"), "with"),
        ] {
            let source = program(&expression);
            let line = source.lines().nth(4).unwrap();
            let column = if height == 254 {
                line.rfind(marker).unwrap()
            } else {
                line.find(marker).unwrap()
            } + 1;
            fs::write(directory.0.join("deep.suz"), source).unwrap();
            let output = suz(&directory.0, &["check", "deep.suz"]);
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(&format!("deep.suz:5:{column}:")),
                "{output:?}"
            );
        }
    }
}

#[test]
fn nesting_boundaries_cover_expression_and_statement_children() {
    let directory = TemporaryDirectory::new();
    // $を深い式に置き換える。@は257段を拒否する位置で、入力からは取り除く。
    // 演算子が既存の式を深くする場合以外は、257段目の葉を指す。
    let cases = [
        (253, "1", "fix value = - $"),
        (253, "1", "fix value: u8 = ~ $"),
        (253, "`a`", "fix value = len $"),
        (253, "1", "fix value: u8 = 1 + 1 @or $"),
        (253, "1", "fix value = Point { x: $, y: 0 }"),
        (253, "1", "fix value = matchexpr $ { _ => 0 }"),
        (252, "1", "fix value = matchexpr 0 { _ => $ }"),
        (252, "1", "fix value = matchexpr 0 { _ => { $ } }"),
        (253, "P", "fix value = $@.x"),
        (253, "`a`", "fix value = $@[:]"),
        (252, "`a`", "fix value = $[0] @or 'a'"),
        (252, "0", "fix value = `a`[$] @or 'a'"),
        (252, "0", "fix value = `a`[$:] @or ``"),
        (252, "1", "fix value = `a`[:$] @or ``"),
        (253, "1", "fix value = 0 @< $ < 2"),
        (253, "1", "while false { fix value = $ }"),
        (253, "1", "loop { fix value = $\nbreak }"),
        (253, "1", "for i in 0..<1 { fix value = $ }"),
        (253, "1", "rev i in 0..<1 { fix value = $ }"),
        (254, "0", "for i in $..<1 {}"),
        (254, "1", "for i in 0..<$ {}"),
        (254, "0", "rev i in $..<1 {}"),
        (254, "1", "rev i in 0..<$ {}"),
        (254, "false", "if false {} elif $ {}"),
        (253, "1", "if false {} elif true { fix value = $ }"),
        (253, "1", "if false {} else { fix value = $ }"),
        (254, "1", "match $ { _ => {} }"),
        (253, "1", "match 0 { _ => { fix value = $ } }"),
        (253, "1", "match 0 { _ => $ }"),
        (253, "1", "{ fix value = $ }"),
        (
            253,
            "1",
            "allow [diagnostic.arithmetic: boundary test] { fix n: u8 = id(1) as u8 or 0\nfix sum: u8 = n + n\nfix value = $ }",
        ),
    ];
    for (parens, leaf, template) in cases {
        for extra in [0, 1] {
            let leaf = if template.contains('@') {
                leaf.to_owned()
            } else {
                format!("@{leaf}")
            };
            let expression = wrap("(", ")", parens + extra, &leaf);
            let statement = template.replace('$', &expression);
            let source = program("0").replace("fix value = 0", &statement);
            let offset = source.find('@').unwrap();
            let line_number = source[..offset].bytes().filter(|b| *b == b'\n').count() + 1;
            let column = source[..offset].rsplit('\n').next().unwrap().len() + 1;
            fs::write(directory.0.join("boundary.suz"), source.replace('@', "")).unwrap();
            let output = suz(&directory.0, &["check", "boundary.suz"]);
            if extra == 0 {
                assert!(output.status.success(), "{template}: {output:?}");
                success(output);
            } else {
                assert_eq!(output.status.code(), Some(1), "{template}: {output:?}");
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(
                    stderr.contains("[src.nesting-limit]")
                        && stderr.contains(&format!("boundary.suz:{line_number}:{column}:")),
                    "{template}: {stderr}"
                );
            }
        }
    }
}

#[test]
fn nesting_limit_checks_nested_blocks_and_top_level_initializers() {
    let directory = TemporaryDirectory::new();
    for (opening, closing, token) in [
        ("while false {", "}", "false"),
        ("loop {", "}", "{"),
        ("for i in 0..<0 {", "}", "0"),
        ("rev i in 0..<0 {", "}", "0"),
        ("match 0 { _ => {", "}}", "0"),
        ("{", "}", "{"),
    ] {
        for count in [255, 256] {
            // 1行に1つの構文を置き、257段目へ入る条件・範囲・ブロックの位置を検査する。
            let source = format!(
                "fn main() -> u8 {{\n{}{}\nreturn 0\n}}",
                (0..count)
                    .map(|i| format!("{}\n", opening.replace(" i ", &format!(" i{i:03} "))))
                    .collect::<String>(),
                closing.repeat(count)
            );
            fs::write(directory.0.join("blocks.suz"), source).unwrap();
            let output = suz(&directory.0, &["check", "blocks.suz"]);
            if count == 255 {
                success(output);
            } else {
                assert_eq!(output.status.code(), Some(1), "{opening}: {output:?}");
                let last_opening = opening.replace(" i ", " i255 ");
                let column = last_opening.find(token).unwrap() + 1;
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(
                    stderr.contains("[src.nesting-limit]")
                        && stderr.contains(&format!("blocks.suz:257:{column}:")),
                    "{opening}: {stderr}"
                );
            }
        }
    }
    let source = format!(
        "fix VALUE = {}\nfn main() -> u8 {{ return 0 }}",
        wrap("(", ")", 256, "1")
    );
    fs::write(directory.0.join("global.suz"), source).unwrap();
    let output = suz(&directory.0, &["check", "global.suz"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let column = "fix VALUE = ".len() + 257;
    assert!(
        stderr.contains("[src.nesting-limit]")
            && stderr.contains(&format!("global.suz:1:{column}:")),
        "{stderr}"
    );
}

#[test]
fn deeply_nested_unclosed_inputs_exit_with_a_diagnostic() {
    let directory = TemporaryDirectory::new();
    for (opening, rule) in [
        ("(", "src.nesting-limit"),
        ("id(", "src.nesting-limit"),
        ("`${", "symbol.template-line"),
    ] {
        // 閉じ記号に到達しない入力でも、再帰でプロセスが異常終了しないことを確認する。
        // 未完のテンプレートは構文解析より先に字句解析が拒否する。
        let source = format!(
            "fn main() -> u8 {{\nfix value = {}1\n",
            opening.repeat(100_000)
        );
        fs::write(directory.0.join("unclosed.suz"), source).unwrap();
        let output = suz(&directory.0, &["check", "unclosed.suz"]);
        assert_eq!(output.status.code(), Some(1), "{opening}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("[{rule}]")),
            "{output:?}"
        );
    }
}

#[test]
fn nesting_boundary_compiles_and_runs_through_all_commands() {
    let directory = TemporaryDirectory::new();
    // 本体のブロックが1段、式の根が2段。高さ255の式の最深部は256段になる。
    let mut mixed = "1".to_owned();
    for i in 0..127 {
        mixed = if i % 2 == 0 {
            format!("expr {{ {mixed} }}")
        } else {
            format!("ifexpr true {{ {mixed} }} else {{ 0 }}")
        };
    }
    let e = wrap("(", ")", 253, "1");
    let cases = [
        (wrap("(", ")", 254, "1"), "1"),
        (wrap("id(", ")", 254, "1"), "1"),
        (format!("{}true", "not ".repeat(254)), "true"),
        (format!("1{}", " + 1".repeat(254)), "255"),
        (format!("1{}", " as u8".repeat(254)), "1"),
        (
            format!("P{}", " with { x: 1 }".repeat(254)),
            "(Point { x: 1, y: 0 })",
        ),
        (mixed, "1"),
        (wrap("`${", "}`", 254, "1"), "`1`"),
        (format!("1 + {e}"), "2"),
        (format!("P with {{ x: {e} }}"), "(Point { x: 1, y: 0 })"),
    ];
    let mut source = "struct Point { x: i32, y: i32 }\nfix P = Point { x: 0, y: 0 }\nfn id(n: int) -> int { return n }\n".to_owned();
    source += &format!("fix TOP = {}\n", wrap("(", ")", 255, "1"));
    for (i, (expression, expected)) in cases.iter().enumerate() {
        source += &format!(
            "fn case_{i}() -> bool {{\nfix value = {expression}\nreturn value == {expected}\n}}\n"
        );
    }
    source += &format!(
        "fn blocks() -> bool {{ {}return true{}\nreturn false }}\n",
        "if true { ".repeat(254),
        " }".repeat(254)
    );
    source += "fn main() -> u8 {\n";
    source += "if TOP != 1 { return 3 }\n";
    for i in 0..cases.len() {
        source += &format!("if not case_{i}() {{ return 1 }}\n");
    }
    source += "return ifexpr blocks() { 0 } else { 2 }\n}";
    fs::write(directory.0.join("boundary.suz"), source).unwrap();
    success(suz(&directory.0, &["check", "boundary.suz"]));
    for opt in ["-O0", "-O2"] {
        let executable = format!("boundary{opt}");
        success(suz(
            &directory.0,
            &["build", "boundary.suz", opt, "-o", &executable],
        ));
        success(Command::new(directory.0.join(executable)).output().unwrap());
        success(suz(&directory.0, &["run", "boundary.suz", opt]));
    }
    success(suz(
        &directory.0,
        &["emit-c", "boundary.suz", "-o", "generated"],
    ));
    success(
        Command::new("sh")
            .current_dir(directory.0.join("generated"))
            .args(["scripts/build-c.sh", "program.c", "-O2", "-o", "boundary"])
            .output()
            .unwrap(),
    );
    success(
        Command::new(directory.0.join("generated/boundary"))
            .output()
            .unwrap(),
    );
}
