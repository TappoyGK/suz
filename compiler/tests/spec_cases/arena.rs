use crate::test_util::TemporaryDirectory;
use std::{fs, path::Path, process::Command};

#[test]
fn arena_runtime_aligns_rewinds_and_checks_before_mutation() {
    let directory = TemporaryDirectory::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let executable = directory.0.join("arena");
    let built = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
        .args([
            "-std=c23",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fsanitize=undefined",
            "-fno-sanitize-recover=all",
            "-iquote",
        ])
        .arg(root.join("runtime"))
        .arg(root.join("compiler/tests/spec_cases/arena_runtime.c"))
        .arg(root.join("runtime/suz_arena.c"))
        .arg(root.join("runtime/suz_runtime.c"))
        .args(["-lm", "-o"])
        .arg(&executable)
        .output()
        .unwrap();
    assert!(built.status.success(), "{built:?}");
    for (mode, code, message) in [
        ("success", 0, ""),
        (
            "capacity",
            101,
            "arena.suz:8:5: arena capacity exceeded: limit=9, used=1, additional=9 [rt.arena-allocation]",
        ),
        (
            "padding-overflow",
            101,
            "size calculation overflow [rt.arena-allocation]",
        ),
        (
            "end-overflow",
            101,
            "size calculation overflow [rt.arena-allocation]",
        ),
        (
            "init-failure",
            101,
            "arena.suz:4:2: arena allocation failed: requested=128 [rt.arena-storage]",
        ),
    ] {
        let output = Command::new(&executable).arg(mode).output().unwrap();
        assert_eq!(output.status.code(), Some(code), "{mode}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{mode}: {output:?}"
        );
    }
}

#[test]
fn arena_capacity_cli_and_export_preserve_setting_without_allocating() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("main.suz");
    fs::write(&source, "fn main() -> u8 { return 0 }").unwrap();
    let binary = env!("CARGO_BIN_EXE_suz");
    for (index, value, bytes) in [
        (0, None, 1_073_741_824_u64),
        (1, Some("1"), 1),
        (2, Some("2K"), 2048),
        (3, Some("3M"), 3_145_728),
        (4, Some("2G"), 2_147_483_648),
        (5, Some("18446744073709551615"), u64::MAX),
    ] {
        let output_dir = directory.0.join(format!("export-{index}"));
        let mut command = Command::new(binary);
        command
            .arg("emit-c")
            .arg(&source)
            .arg("-o")
            .arg(&output_dir);
        if let Some(value) = value {
            command.args(["--arena-limit", value]);
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let program = output_dir.join("program.c");
        let generated = fs::read_to_string(&program).unwrap();
        assert!(generated.contains(&format!("#define SUZ_ARENA_LIMIT UINT64_C({bytes})")));
        assert!(!generated.contains("suz_arena_init("));
        if index == 2 {
            // 構文が未公開の段階でも、出力物だけでランタイムと設定を使えることを確かめる。
            let harness = format!(
                r#"#define main unused_suz_main
{generated}
#undef main
#include "suz_arena.h"
int main(void) {{ suz_arena_init(SUZ_ARENA_LIMIT, "test", 1, 1); return suz_arena.capacity == 2048 ? 0 : 1; }}
"#
            );
            fs::write(&program, harness).unwrap();
            let executable = directory.0.join("exported");
            let built = Command::new("bash")
                .arg(output_dir.join("scripts/build-c.sh"))
                .arg(&program)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(built.status.success(), "{built:?}");
            assert!(Command::new(executable).status().unwrap().success());
        }
    }
    // 未使用のarenaが大容量を確保しないことを、確保を一切許さない環境層で検査する。
    let environment = directory.0.join("no_alloc.c");
    fs::write(
        &environment,
        r#"#include "suz_env.h"
#include <stdlib.h>
void *suz_env_alloc(size_t n) { (void)n; _Exit(90); }
void suz_env_free(void *p) { (void)p; _Exit(91); }
void suz_env_log(suz_log_level l, const char *s, size_t n) { (void)l; (void)s; (void)n; }
_Noreturn void suz_env_panic(void) { _Exit(101); }
"#,
    )
    .unwrap();
    for verb in ["build", "run"] {
        let executable = directory.0.join("app");
        let mut command = Command::new(binary);
        command
            .arg(verb)
            .arg(&source)
            .args(["--arena-limit", "18446744073709551615"])
            .arg("--env")
            .arg(&environment);
        if verb == "build" {
            command.arg("-o").arg(&executable);
        }
        let result = command.output().unwrap();
        assert!(result.status.success(), "{verb}: {result:?}");
        if verb == "build" {
            assert!(Command::new(executable).status().unwrap().success());
        }
    }
}

#[test]
fn arena_capacity_rejects_invalid_and_duplicate_values() {
    let binary = env!("CARGO_BIN_EXE_suz");
    for verb in ["build", "run", "emit-c"] {
        for value in [
            "",
            "0",
            "0K",
            "-1",
            "+1",
            "1k",
            "1.5M",
            "unlimited",
            "1Gx",
            "18446744073709551616",
            "17179869184G",
        ] {
            let result = Command::new(binary)
                .args([verb, "missing.suz", "--arena-limit", value])
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(2), "{verb} {value}: {result:?}");
            assert!(String::from_utf8_lossy(&result.stderr).contains("[tool.arena-capacity]"));
        }
        for tail in [
            vec!["--arena-limit"],
            vec!["--arena-limit", "1", "--arena-limit", "2"],
        ] {
            let result = Command::new(binary)
                .args([verb, "missing.suz"])
                .args(tail)
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(2));
            assert!(String::from_utf8_lossy(&result.stderr).contains("[tool.arena-capacity]"));
        }
    }
    let result = Command::new(binary)
        .args(["check", "missing.suz", "--arena-limit", "1"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("[tool.arguments]"));
}
