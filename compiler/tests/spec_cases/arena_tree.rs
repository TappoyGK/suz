//! 表示の集約が実行時の入れ子・回収を変えず、上限後も全体を測れることを検証する。
use crate::{support::batch::Batch, test_util::TemporaryDirectory};
use std::{fs, process::Command};
use suz::{ArenaOptions, ArenaReportFormat, ArenaReportOptions, CompilationOptions};

const RECURSIVE: &str = r#"fn a(n: i64) -> void {
    arena {
        fix ptr p = new n
        if n > 0 { b(n -% 1) }
    }
}
fn b(n: i64) -> void {
    arena {
        fix ptr p = new [n; 2]
        if n > 0 { a(n -% 1) }
    }
}
fn main() -> u8 {
    a(4)
    for i in 0..<3 {
        arena {
            fix ptr p = new (1 as i64)
            a(0)
        }
    }
    return 0
}"#;

fn options(format: ArenaReportFormat, capacity: u16) -> CompilationOptions {
    CompilationOptions {
        log_arena: true,
        arena_report: ArenaReportOptions {
            format,
            capacity: capacity.try_into().unwrap(),
        },
        arena: ArenaOptions {
            initial: 128,
            grow: 0,
            limit: None,
        },
        ..CompilationOptions::default()
    }
}

#[test]
fn arena_tree_folds_mutual_recursion_and_keeps_later_paths() {
    let directory = TemporaryDirectory::new();
    let mut files = Vec::new();
    for (id, format) in [
        ArenaReportFormat::Csv,
        ArenaReportFormat::TreeBytes,
        ArenaReportFormat::Tree,
    ]
    .into_iter()
    .enumerate()
    {
        let path = directory.0.join(format!("{id}.c"));
        fs::write(
            &path,
            suz::compile_with_options(RECURSIVE.as_bytes(), "tree.suz", &options(format, 256))
                .unwrap()
                .c,
        )
        .unwrap();
        files.push((id, path));
    }
    let batch = Batch::build_with_flags(
        &directory.0,
        &files,
        3,
        &["-O1", "-fsanitize=address,undefined"],
    )
    .unwrap();
    let csv = batch.run(0).unwrap();
    assert_eq!(csv.status.code(), Some(0), "{csv:?}");
    assert_eq!(
        String::from_utf8_lossy(&csv.stderr),
        concat!(
            "id,parent,depth,source,line,column,function,peak,recursive,omitted,reserved,blocks,expansions\n",
            "0,,0,\"tree.suz\",13,1,\"main\",56,false,false,128,1,0\n",
            "1,0,1,\"tree.suz\",2,5,\"a\",56,true,false,,,\n",
            "2,1,2,\"tree.suz\",8,5,\"b\",48,true,false,,,\n",
            "3,0,1,\"tree.suz\",16,9,\"main\",16,false,false,,,\n",
            "4,3,2,\"tree.suz\",2,5,\"a\",8,false,false,,,\n"
        )
    );
    for id in [1, 2] {
        let output = batch.run(id).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let text = String::from_utf8(output.stderr).unwrap();
        assert!(
            text.contains("├── tree.suz:2:5") && text.contains("│   └── tree.suz:8:5"),
            "{text}"
        );
        assert!(
            text.contains("└── tree.suz:16:9") && text.contains("    └── tree.suz:2:5"),
            "{text}"
        );
        assert_eq!(text.matches("[recursive]").count(), 2);
        assert!(!text.contains("[omitted]"));
        // 数字の右端が揃うことを、枝を含むUnicode文字数で確認する。
        let ends: Vec<_> = text
            .lines()
            .skip(1)
            .take(5)
            .map(|line| {
                let line = line.split(" [").next().unwrap();
                line.chars().count()
            })
            .collect();
        assert!(ends.iter().all(|end| *end == ends[0]), "{text}");
        assert!(text.ends_with("reserved=128 blocks=1 expansions=0\n"));
        assert_eq!(text.contains("56 B"), id == 2);
    }
}

#[test]
fn arena_tree_limit_omits_whole_subtrees_without_losing_peak() {
    let directory = TemporaryDirectory::new();
    let mut files = Vec::new();
    for capacity in [1, 2, 3] {
        let path = directory.0.join(format!("{capacity}.c"));
        fs::write(
            &path,
            suz::compile_with_options(
                RECURSIVE.as_bytes(),
                "tree.suz",
                &options(ArenaReportFormat::Csv, capacity),
            )
            .unwrap()
            .c,
        )
        .unwrap();
        files.push((capacity as usize, path));
    }
    let batch = Batch::build(&directory.0, &files, 3).unwrap();
    for capacity in [1, 2, 3] {
        let result = batch.run(capacity).unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        let text = String::from_utf8(result.stderr).unwrap();
        let rows: Vec<Vec<&str>> = text
            .lines()
            .skip(1)
            .map(|line| line.split(',').collect())
            .collect();
        assert_eq!(rows.len(), capacity, "{text}");
        assert_eq!(rows[0][7], "56", "{text}");
        assert_eq!(rows[0][9], "true", "{text}"); // 後続のmainのarenaも省略される。
        if capacity > 1 {
            assert_eq!(rows[1][7], "56", "{text}");
        }
        if capacity == 2 {
            assert_eq!(rows[1][9], "true");
            assert_eq!(rows[1][8], "false"); // 省略したBの中のAを別経路に付け替えない。
        }
        if capacity == 3 {
            assert_eq!(rows[1][8], "true");
            assert_eq!(rows[2][8], "true");
            assert_eq!(rows[2][7], "48");
        }
    }
}

#[test]
fn arena_tree_preserves_all_exit_paths_and_closure_names() {
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("exits.c");
    let source = super::arena_report::SOURCE;
    fs::write(
        &path,
        suz::compile_with_options(
            source.as_bytes(),
            "exits.suz",
            &options(ArenaReportFormat::Csv, 256),
        )
        .unwrap()
        .c,
    )
    .unwrap();
    let batch = Batch::build(&directory.0, &[(0, path)], 1).unwrap();
    let output = batch.run(0).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(
        text.lines()
            .nth(1)
            .unwrap()
            .contains(",73,false,false,128,1,0"),
        "{text}"
    );
    assert!(text.contains("\"main::<closure>\""), "{text}");
    assert!(
        text.contains("\"failed\"") && text.contains("\"returned\""),
        "{text}"
    );
}

#[test]
fn arena_tree_cli_defaults_units_limits_and_csv_escaping() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("main.suz");
    fs::write(
        &source,
        "fn main() -> u8 { arena { fix ptr p = new [0 as u8; 1536] }\nreturn 0 }",
    )
    .unwrap();
    let binary = env!("CARGO_BIN_EXE_suz");
    let output = Command::new(binary)
        .arg("run")
        .arg(&source)
        .args(["--log-arena", "--log-arena-limit", "1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("1.5 KiB [omitted]"), "{text}");
    for value in ["0", "65536", "-1", "+2", "1K", ""] {
        let output = Command::new(binary)
            .args([
                "run",
                "missing.suz",
                "--log-arena",
                "--log-arena-limit",
                value,
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{value}: {output:?}");
    }
    for args in [
        vec!["run", "missing.suz", "--log-arena-limit", "1"],
        vec!["run", "missing.suz", "--log-arena", "--log-arena-limit"],
        vec![
            "run",
            "missing.suz",
            "--log-arena",
            "--log-arena-limit",
            "1",
            "--log-arena-limit",
            "2",
        ],
        vec!["check", "missing.suz", "--log-arena-limit", "1"],
    ] {
        let output = Command::new(binary).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{output:?}");
    }
    let path = directory.0.join("quoted.c");
    let generated = suz::compile_with_options(
        b"fn main() -> u8 { arena { fix ptr p = new (1 as u8) }\nreturn 0 }",
        "日本語,\"quoted\"\n.suz",
        &options(ArenaReportFormat::Csv, 256),
    )
    .unwrap()
    .c;
    fs::write(&path, generated).unwrap();
    let batch = Batch::build(&directory.0, &[(0, path)], 1).unwrap();
    let output = batch.run(0).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("\"日本語,\"\"quoted\"\"\n.suz\""),
        "{output:?}"
    );
}

#[test]
fn arena_tree_deep_recursion_does_not_consume_the_row_budget() {
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("deep.c");
    let mut config = options(ArenaReportFormat::Csv, 256);
    config.arena.initial = 8192;
    let source = RECURSIVE.replace("a(4)", "a(600)");
    fs::write(
        &path,
        suz::compile_with_options(source.as_bytes(), "deep.suz", &config)
            .unwrap()
            .c,
    )
    .unwrap();
    let batch = Batch::build(&directory.0, &[(0, path)], 1).unwrap();
    let output = batch.run(0).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stderr).unwrap();
    let rows: Vec<Vec<&str>> = text
        .lines()
        .skip(1)
        .map(|line| line.split(',').collect())
        .collect();
    assert_eq!(rows.len(), 5, "{text}");
    assert_eq!(rows[0][7], "7208", "{text}");
    assert!(rows.iter().all(|row| row[9] == "false"), "{text}");
    assert_eq!(rows[3][6], "\"main\""); // 深い再帰の後に開く領域も記録される。
    assert_eq!(rows[4][7], "8");
}

#[test]
fn arena_tree_exports_separate_storage_and_does_not_allocate_during_output() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("main.suz");
    fs::write(
        &source,
        "fn main() -> u8 { arena { fix ptr p = new (1 as u8) }\nreturn 0 }",
    )
    .unwrap();
    let environment = directory.0.join("environment.c");
    // 初期ブロックと計測用の2個だけを許し、記録容量がreservedに混ざらないことも確かめる。
    let env = r#"
#include "suz_env.h"
#include <stdlib.h>
#include <stdio.h>
static unsigned allocations;
void *suz_env_alloc(size_t size) {
    if (++allocations > 2) _Exit(90);
    return malloc(size);
}
void suz_env_free(void *p) { free(p); }
void suz_env_log(suz_log_level level, const char *text, size_t length) {
    if (level != SUZ_LOG_INFO || allocations != 2) _Exit(91);
    fwrite(text, 1, length, stderr);
}
_Noreturn void suz_env_panic(void) { _Exit(101); }
"#;
    fs::write(&environment, env).unwrap();
    let export = directory.0.join("export");
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .arg("emit-c")
        .arg(&source)
        .arg("-o")
        .arg(&export)
        .arg("--env")
        .arg(&environment)
        .args([
            "--log-arena",
            "--log-arena-format",
            "csv",
            "--log-arena-limit",
            "2",
            "--arena-initial",
            "1",
            "--arena-grow",
            "0",
            "--log-level",
            "off",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let executable = directory.0.join("app");
    let output = Command::new("bash")
        .arg(export.join("scripts/build-c.sh"))
        .arg(export.join("program.c"))
        .arg("-o")
        .arg(&executable)
        .arg("--env")
        .arg(&environment)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let output = Command::new(&executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(
        text.lines()
            .nth(1)
            .unwrap()
            .ends_with(",1,false,false,1,1,0"),
        "{text}"
    );
    assert_eq!(text.lines().count(), 3);

    // 計測領域の確保に失敗したら利用者のmainへ入らず、位置付きのパニックにする。
    fs::write(
        &environment,
        env.replace(
            "if (++allocations > 2) _Exit(90);",
            "if (++allocations == 2) return NULL;",
        )
        .replace("level != SUZ_LOG_INFO", "level != SUZ_LOG_PANIC"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .arg("run")
        .arg(&source)
        .arg("--env")
        .arg(&environment)
        .args(["--log-arena", "--arena-initial", "1", "--arena-grow", "0"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101), "{output:?}");
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(
        text.contains(":1:1: arena report allocation failed [rt.arena-report-storage]"),
        "{text}"
    );
    assert!(!text.contains("reserved="), "{text}");
}

#[test]
fn arena_tree_option_diagnostics_name_their_own_rules() {
    let binary = env!("CARGO_BIN_EXE_suz");
    for (option, valid, rule) in [
        ("--log-arena-format", "tree", "[tool.arena-report-format]"),
        ("--log-arena-limit", "256", "[tool.arena-report-limit]"),
    ] {
        for tail in [
            vec![option],
            vec![option, "invalid"],
            vec![option, valid, option, valid],
        ] {
            let output = Command::new(binary)
                .args(["run", "missing.suz", "--log-arena"])
                .args(tail)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(2), "{output:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(rule),
                "{output:?}"
            );
        }
    }
}

#[test]
fn arena_tree_indexes_alternating_parents_and_names_generic_methods() {
    let directory = TemporaryDirectory::new();
    // 同じ子の直前の親が毎回変わる。キャッシュが外れても索引から既存行を取得する。
    let mut many = String::from(
        "fn child() -> void { arena { fix ptr p = new [0 as u8; 3] } }\nfn main() -> u8 { for i in 0..<5 {\n",
    );
    for _ in 0..64 {
        many.push_str("arena { child() }\n");
    }
    many.push_str("}\nreturn 0 }");
    let methods = r#"
struct Pt { x: i64 }
fn Pt#bump() -> void {
    arena { fix ptr p = new [0 as u8; 4] }
    self#x = self.x +% 1
}
struct Box[T] { v: T }
fn Box[T].get() -> T {
    arena { fix ptr p = new [0 as u8; 2] }
    return self.v
}
fn main() -> u8 {
    var p = Pt { x: 1 }
    p#bump()
    fix a = Box[i64] { v: 1 }
    fix b = Box[u8] { v: 2 }
    fix x = a.get()
    fix y = b.get()
    return 0
}
"#;
    let mut files = Vec::new();
    for (id, source) in [many.as_str(), methods].iter().enumerate() {
        let path = directory.0.join(format!("indexed-{id}.c"));
        fs::write(
            &path,
            suz::compile_with_options(
                source.as_bytes(),
                "indexed.suz",
                &options(ArenaReportFormat::Csv, 256),
            )
            .unwrap()
            .c,
        )
        .unwrap();
        files.push((id, path));
    }
    let batch = Batch::build_with_flags(
        &directory.0,
        &files,
        2,
        &["-O1", "-fsanitize=address,undefined"],
    )
    .unwrap();
    let output = batch.run(0).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stderr).unwrap();
    let rows: Vec<Vec<&str>> = text
        .lines()
        .skip(1)
        .map(|line| line.split(',').collect())
        .collect();
    assert_eq!(rows.len(), 129, "{text}");
    for row in rows {
        assert_eq!(row[7], "8"); // ASanの丸め後の3バイト配列。
        assert_eq!(row[8], "false");
        assert_eq!(row[9], "false");
    }
    let output = batch.run(1).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8_lossy(&output.stderr);
    assert_eq!(text.matches("\"Pt.bump\"").count(), 1, "{text}");
    assert_eq!(text.matches("\"Box.get\"").count(), 1, "{text}");
}

#[test]
fn disabled_arena_reporting_does_not_link_reporting_functions() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("main.suz");
    fs::write(
        &source,
        "fn main() -> u8 { arena { fix ptr p = new (1 as u8) }\nreturn 0 }",
    )
    .unwrap();
    let native = directory.0.join("native");
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&native)
        .arg("-O2")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let export = directory.0.join("export");
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .arg("emit-c")
        .arg(&source)
        .arg("-o")
        .arg(&export)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let exported = directory.0.join("exported");
    let output = Command::new("bash")
        .arg(export.join("scripts/build-c.sh"))
        .arg(export.join("program.c"))
        .arg("-o")
        .arg(&exported)
        .arg("-O2")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    for executable in [native, exported] {
        let output = Command::new("nm").arg(&executable).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let symbols = String::from_utf8_lossy(&output.stdout);
        assert!(
            !symbols.contains("suz_arena_report")
                && !symbols.contains("suz_arena_tree")
                && !symbols.contains("suz_log_report"),
            "{symbols}"
        );
        assert!(Command::new(executable).status().unwrap().success());
    }
}

#[test]
fn arena_tree_shares_debug_helpers_and_optimizes_small_loops() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("main.suz");
    fs::write(&source, "fn main() -> u8 {\narena { fix ptr p = new (1 as u8) }\narena { fix ptr p = new (2 as i64) }\nreturn 0 }").unwrap();
    let mut reports = Vec::new();
    for optimization in ["-O0", "-O2"] {
        let executable = directory.0.join(format!("app{optimization}"));
        let output = Command::new(env!("CARGO_BIN_EXE_suz"))
            .arg("build")
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .args([optimization, "--log-arena", "--log-arena-format", "csv"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let output = Command::new("nm").arg(&executable).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let symbols = String::from_utf8_lossy(&output.stdout);
        // 小さい例での展開を確認する。多数の位置がある場合は共有してもよい。
        for name in [
            "suz_arena_tree_enter",
            "suz_arena_tree_leave",
            "suz_arena_report_leave",
        ] {
            let count = symbols
                .lines()
                .filter(|line| {
                    line.split_whitespace()
                        .last()
                        .is_some_and(|symbol| symbol.trim_start_matches('_') == name)
                })
                .count();
            assert_eq!(
                count,
                usize::from(optimization == "-O0"),
                "{optimization}: {symbols}"
            );
        }
        let output = Command::new(executable).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        reports.push(output.stderr);
    }
    assert_eq!(reports[0], reports[1]);
}
