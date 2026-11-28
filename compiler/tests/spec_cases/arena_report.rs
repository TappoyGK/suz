//! 容量調整に使える値かを、測定値ちょうどの容量と1バイト不足の実行で確かめる。
use crate::{support::batch::Batch, test_util::TemporaryDirectory};
use std::{fs, process::Command};
use suz::{ArenaOptions, CompilationOptions, LogLevel};

pub(super) const SOURCE: &str = r#"
error Err { Bad }
fnshape Read() -> i64
fn call(f: fn Read) -> i64 { return f() }
fn failed() -> i64 or Err {
    arena { fix p = new (5 as i64)
        fail Err.Bad
    }
}
fn returned() -> i64 {
    arena { fix p = new (6 as i64)
        arena { fix q = new (7 as i64)
            return .p + .q
        }
    }
}
fn recursive(n: i64) -> void {
    arena { fix p = new n
        fix small = new (1 as u8)
        if n != 0 { recursive(n -% 1) }
    }
}
fn main() -> u8 {
    fix byte = new (1 as u8)
    for i in 0..<4 {
        arena {
            fix a = new #[2 as i64; 3]
            if a.[0] != 2 { return 1 }
            if returned() != 13 { return 2 }
            fix result = failed() or error {
                _ => { arena { fix r = new (8 as i64)
                    recover .r
                } }
            }
            if result != 8 { return 3 }
            if call(fn []() -> i64 {
                arena { fix c = new (9 as i64)
                    return .c
                }
            }) != 9 { return 4 }
            recursive(2)
            loop { arena { fix b = new (10 as i64)
                break
            } }
            if i < 3 { continue }
        }
    }
    fix last = new (11 as i64)
    if .byte != 1 { return 5 }
    if .last != 11 { return 6 }
    return 0
}
"#;

fn options(initial: u64, grow: u64) -> CompilationOptions {
    CompilationOptions {
        log_arena: true,
        arena_report: suz::ArenaReportOptions {
            format: suz::ArenaReportFormat::Summary,
            ..suz::ArenaReportOptions::default()
        },
        log_level: LogLevel::Off,
        arena: ArenaOptions {
            initial,
            grow,
            limit: None,
        },
        ..CompilationOptions::default()
    }
}

fn peak(stderr: &[u8]) -> u64 {
    let text = std::str::from_utf8(stderr).unwrap();
    text.strip_prefix("arena peak=")
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn capacity_roundtrip(flags: &[&str], expected: u64) {
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("measure.c");
    let generated = suz::compile_with_options(SOURCE.as_bytes(), "report.suz", &options(1, 8))
        .unwrap()
        .c;
    fs::write(&path, generated).unwrap();
    let batch = Batch::build_with_flags(&directory.0, &[(0, path)], 1, flags).unwrap();
    let measured = batch.run(0).unwrap();
    assert_eq!(measured.status.code(), Some(0), "{measured:?}");
    assert!(measured.stdout.is_empty());
    let bytes = peak(&measured.stderr);
    // 各再帰でi64の後にu8を置く。ASanの8バイト丸めがピークにも反映される。
    assert_eq!(bytes, expected);
    let mut files = Vec::new();
    for (id, capacity) in [(1, bytes), (2, bytes - 1)] {
        let path = directory.0.join(format!("fixed-{id}.c"));
        fs::write(
            &path,
            suz::compile_with_options(SOURCE.as_bytes(), "report.suz", &options(capacity, 0))
                .unwrap()
                .c,
        )
        .unwrap();
        files.push((id, path));
    }
    let batch = Batch::build_with_flags(&directory.0, &files, 2, flags).unwrap();
    let exact = batch.run(1).unwrap();
    assert_eq!(exact.status.code(), Some(0), "{exact:?}");
    assert_eq!(
        String::from_utf8_lossy(&exact.stderr),
        format!("arena peak={bytes} reserved={bytes} blocks=1 expansions=0\n")
    );
    let short = batch.run(2).unwrap();
    assert_eq!(short.status.code(), Some(101), "{short:?}");
    let stderr = String::from_utf8_lossy(&short.stderr);
    assert!(stderr.contains("arena capacity exceeded"), "{stderr}");
    assert!(!stderr.contains("arena peak="), "{stderr}");
}

#[test]
fn arena_report_peak_survives_exits_recursion_arrays_and_closures() {
    capacity_roundtrip(&["-O2"], 73);
}

#[test]
fn arena_report_uses_sanitizer_layout() {
    capacity_roundtrip(&["-O1", "-fsanitize=address,undefined"], 80);
}

#[test]
fn arena_report_counts_growth_reuse_and_suppresses_empty_runs() {
    let directory = TemporaryDirectory::new();
    let sources = [
        "fn main() -> u8 { return 0 }",
        "fn main() -> u8 { arena {}\nreturn 0 }",
        "fn main() -> u8 { if false { fix p = new 1 }\nreturn 0 }",
        "fn main() -> u8 { for i in 0..<3 { arena { fix p = new (1 as u8)\nfix q = new (2 as i64) } }\nreturn 0 }",
        "fn main() -> u8 { fix p = new 1\npanic \"stop\" }",
    ];
    let mut files = Vec::new();
    for (id, source) in sources.iter().enumerate() {
        let path = directory.0.join(format!("{id}.c"));
        fs::write(
            &path,
            suz::compile_with_options(source.as_bytes(), "report.suz", &options(0, 8))
                .unwrap()
                .c,
        )
        .unwrap();
        files.push((id, path));
    }
    let batch = Batch::build(&directory.0, &files, 4).unwrap();
    for id in 0..3 {
        let output = batch.run(id).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
    let output = batch.run(3).unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "arena peak=16 reserved=16 blocks=2 expansions=2\n"
    );
    let output = batch.run(4).unwrap();
    assert_eq!(output.status.code(), Some(101), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("arena peak="));

    let generated = suz::compile(SOURCE.as_bytes()).unwrap();
    assert!(!generated.contains("suz_arena_report"));
    assert!(!generated.contains("SUZ_LOG_ARENA"));
}

#[test]
fn arena_report_cli_validates_options_and_exports_instrumentation() {
    let binary = env!("CARGO_BIN_EXE_suz");
    for args in [
        vec!["run", "missing.suz", "--log-arena", "--log-arena"],
        vec!["run", "missing.suz", "--log-arena-format"],
        vec!["run", "missing.suz", "--log-arena-format", "invalid"],
        vec!["run", "missing.suz", "--log-arena-format", "summary"],
        vec![
            "run",
            "missing.suz",
            "--log-arena",
            "--log-arena-format",
            "summary",
            "--log-arena-format",
            "summary",
        ],
        vec!["check", "missing.suz", "--log-arena"],
        vec!["check", "missing.suz", "--log-arena-format", "summary"],
    ] {
        let output = Command::new(binary).args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
    }
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("main.suz");
    fs::write(
        &source,
        "fn main() -> u8 { fix p = new (1 as u8)\nreturn 0 }",
    )
    .unwrap();
    let export = directory.0.join("export");
    let output = Command::new(binary)
        .arg("emit-c")
        .arg(&source)
        .arg("-o")
        .arg(&export)
        .args([
            "--log-arena",
            "--log-arena-format",
            "summary",
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
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let output = Command::new(executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "arena peak=1 reserved=1 blocks=1 expansions=0\n"
    );
}

#[test]
fn arena_report_output_uses_info_without_allocating_or_interning() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("main.suz");
    fs::write(
        &source,
        "fn main() -> u8 { fix p = new (1 as u8)\nreturn 0 }",
    )
    .unwrap();
    let environment = directory.0.join("environment.c");
    // 実体用の初期ブロック1個だけを許す。出力時の追加確保や文字列プール生成を検出する。
    fs::write(
        &environment,
        r#"
#include "suz_env.h"
#include <stdlib.h>
#include <stdio.h>
static unsigned allocations;
void *suz_env_alloc(size_t size) {
    if (++allocations != 1) _Exit(90);
    return malloc(size);
}
void suz_env_free(void *p) { free(p); }
void suz_env_log(suz_log_level level, const char *text, size_t length) {
    if (level != SUZ_LOG_INFO || allocations != 1) _Exit(91);
    fwrite(text, 1, length, stderr);
}
_Noreturn void suz_env_panic(void) { _Exit(101); }
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .arg("run")
        .arg(&source)
        .arg("--env")
        .arg(&environment)
        .args([
            "--log-arena",
            "--log-arena-format",
            "summary",
            "--log-level",
            "off",
            "--arena-initial",
            "1",
            "--arena-grow",
            "0",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "arena peak=1 reserved=1 blocks=1 expansions=0\n"
    );
}
