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
    source += &format!(
        "fix TOP = {}\nvar GLOBAL = {} allow [diagnostic.global-var: nesting boundary]\n",
        wrap("(", ")", 255, "1"),
        wrap("(", ")", 255, "1")
    );
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
    source += "GLOBAL = TOP\nif GLOBAL != 1 { return 3 }\n";
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
