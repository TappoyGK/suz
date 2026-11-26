use crate::test_util::TemporaryDirectory;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn emit(binary: &Path, directory: &Path, arguments: &[&str]) -> Output {
    Command::new(binary)
        .current_dir(directory)
        .env("PATH", "")
        .env("SDKROOT", "/nonexistent-sdk")
        .arg("emit-c")
        .args(arguments)
        .output()
        .unwrap()
}

fn success(result: &Output) {
    assert!(result.status.success(), "{result:?}");
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

fn names(directory: &Path) -> Vec<std::ffi::OsString> {
    let mut result: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    result.sort();
    result
}

#[test]
fn moved_binary_exports_bundled_sources_and_custom_environment_without_tools() {
    let directory = TemporaryDirectory::new();
    let binary = directory.0.join("suz");
    fs::copy(env!("CARGO_BIN_EXE_suz"), &binary).unwrap();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 { log info \"hello\"\nreturn 7 }",
    )
    .unwrap();
    success(&emit(
        &binary,
        &directory.0,
        &["input.suz", "-o", "generated"],
    ));
    let generated = directory.0.join("generated");
    let c = fs::read_to_string(generated.join("program.c")).unwrap();
    assert!(c.contains("input.suz"));
    assert!(!c.contains(&directory.0.display().to_string()));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for name in [
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "scripts/build-c.sh",
        "runtime/suz_env.c",
        "runtime/third_party/ryu/LICENSE-Boost",
        "runtime/third_party/ryu/LICENSE-Apache2",
    ] {
        assert_eq!(
            fs::read(generated.join(name)).unwrap(),
            fs::read(root.join(name)).unwrap(),
            "{name}"
        );
    }
    let custom = b"/* custom environment */\n";
    fs::write(directory.0.join("custom.c"), custom).unwrap();
    success(&emit(
        &binary,
        &directory.0,
        &["--env", "custom.c", "-o", "custom", "input.suz"],
    ));
    assert_eq!(
        fs::read(directory.0.join("custom/runtime/suz_env.c")).unwrap(),
        custom
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(generated.join("scripts/build-c.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
}

#[test]
fn export_failures_preserve_existing_paths_and_leave_no_staging_directories() {
    let directory = TemporaryDirectory::new();
    let binary = Path::new(env!("CARGO_BIN_EXE_suz"));
    let source = "fn main() -> u8 { return 0 }";
    fs::write(directory.0.join("input.suz"), source).unwrap();
    fs::write(
        directory.0.join("invalid.suz"),
        "fn main() -> u8 { return 256 }",
    )
    .unwrap();
    fs::create_dir(directory.0.join("empty")).unwrap();
    fs::write(directory.0.join("file"), b"keep").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("missing", directory.0.join("dangling")).unwrap();
        std::os::unix::fs::symlink("empty", directory.0.join("link")).unwrap();
    }
    let before = names(&directory.0);
    for (args, code, rule) in [
        (vec!["input.suz", "-o", "empty"], 2, "tool.output"),
        (vec!["input.suz", "-o", "file"], 2, "tool.output"),
        (vec!["input.suz", "-o", "input.suz"], 2, "tool.output"),
        (
            vec!["input.suz", "-o", "new", "--env", "missing.c"],
            2,
            "tool.environment",
        ),
        (
            vec!["input.suz", "-o", "missing/new"],
            2,
            "tool.publish-output",
        ),
        (vec!["invalid.suz", "-o", "new"], 1, "lit.range"),
        (vec!["invalid.suz", "-o", "file"], 1, "lit.range"),
    ] {
        let output = emit(binary, &directory.0, &args);
        assert_eq!(output.status.code(), Some(code), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!("[{rule}]")),
            "{output:?}"
        );
        assert_eq!(names(&directory.0), before);
    }
    #[cfg(unix)]
    for name in ["dangling", "link"] {
        assert_eq!(
            emit(binary, &directory.0, &["input.suz", "-o", name])
                .status
                .code(),
            Some(2)
        );
        assert!(
            fs::symlink_metadata(directory.0.join(name))
                .unwrap()
                .is_symlink()
        );
    }
    assert_eq!(
        fs::read(directory.0.join("input.suz")).unwrap(),
        source.as_bytes()
    );
    assert_eq!(fs::read(directory.0.join("file")).unwrap(), b"keep");
    assert!(names(&directory.0.join("empty")).is_empty());
}

#[test]
fn concurrent_exports_publish_only_one_complete_directory() {
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 { return 0 }",
    )
    .unwrap();
    let start = || {
        Command::new(env!("CARGO_BIN_EXE_suz"))
            .current_dir(&directory.0)
            .args(["emit-c", "input.suz", "-o", "generated"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let first = start();
    let second = start();
    let mut codes = [
        first.wait_with_output().unwrap().status.code(),
        second.wait_with_output().unwrap().status.code(),
    ];
    codes.sort();
    assert_eq!(codes, [Some(0), Some(2)]);
    assert_eq!(names(&directory.0), ["generated", "input.suz"]);
    assert!(directory.0.join("generated/program.c").is_file());
    assert!(
        directory
            .0
            .join("generated/runtime/third_party/ryu/ryu/d2s.c")
            .is_file()
    );
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn exported_sources_build_independently_with_standard_and_custom_environments() {
    let directory = TemporaryDirectory::new();
    let binary = Path::new(env!("CARGO_BIN_EXE_suz"));
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 {\nfix fraction = 1.5\nfix yes = true\nlog info \"${fraction} ${yes}\"\nreturn 7\n}",
    )
    .unwrap();
    fs::write(
        directory.0.join("custom.c"),
        r#"#include "suz_env.h"
#include <stdio.h>
#include <stdlib.h>
void *suz_env_alloc(size_t size) { return malloc(size); }
void suz_env_free(void *ptr) { free(ptr); }
void suz_env_log(suz_log_level level, const char *bytes, size_t length) {
    (void)level;
    fwrite(bytes, 1, length, stdout);
}
_Noreturn void suz_env_panic(void) { exit(101); }
"#,
    )
    .unwrap();
    for custom in [false, true] {
        let name = if custom { "custom" } else { "standard" };
        let mut args = vec!["input.suz", "-o", name];
        if custom {
            args.extend(["--env", "custom.c"]);
        }
        success(&emit(binary, &directory.0, &args));
        let exported = directory.0.join(name);
        let moved = directory.0.join(format!("moved-{name}"));
        fs::rename(exported, &moved).unwrap();
        for optimization in ["-O0", "-O2"] {
            let output_name = format!("program{optimization}");
            let mut command = Command::new(moved.join("scripts/build-c.sh"));
            command
                .current_dir(&moved)
                .args(["program.c", "-o", &output_name, optimization]);
            for variable in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS"] {
                command.env_remove(variable);
            }
            let built = command.output().unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            let result = Command::new(moved.join(output_name)).output().unwrap();
            assert_eq!(result.status.code(), Some(7));
            let expected = b"info input.suz:4: 1.5 true\n";
            if custom {
                assert_eq!(result.stdout, expected);
                assert!(result.stderr.is_empty());
            } else {
                assert_eq!(result.stderr, expected);
                assert!(result.stdout.is_empty());
            }
        }
    }
}

#[test]
fn exported_symbol_pool_limits_preserve_units_defaults_and_full_u64_range() {
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 { fix value: symbol = \"x\"\nreturn 0 }",
    )
    .unwrap();
    let binary = Path::new(env!("CARGO_BIN_EXE_suz"));
    for (index, (value, expected)) in [
        (None, 1_073_741_824_u64),
        (Some("1"), 1),
        (Some("0001"), 1),
        (Some("2K"), 2048),
        (Some("256M"), 268_435_456),
        (Some("2G"), 2_147_483_648),
        (Some("unlimited"), 0),
        (Some("18446744073709551615"), u64::MAX),
        (Some("17179869183G"), 18_446_744_072_635_809_792),
    ]
    .into_iter()
    .enumerate()
    {
        let output = format!("output-{index}");
        let mut args = vec!["input.suz", "-o", output.as_str()];
        if let Some(value) = value {
            args.extend(["--symbol-pool-limit", value]);
        }
        success(&emit(binary, &directory.0, &args));
        let generated = fs::read_to_string(directory.0.join(&output).join("program.c")).unwrap();
        assert!(
            generated.contains(&format!("suz_symbol_pool_init(UINT64_C({expected}));")),
            "{value:?}: {generated}"
        );
    }
    for command_name in ["build", "run", "emit-c", "check"] {
        let output = Command::new(binary)
            .args([command_name, "--help"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let help = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            help.contains("--symbol-pool-limit"),
            command_name != "check"
        );
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn emitted_c_preserves_symbol_pool_limits() {
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 { fix value: symbol = \"x\"\nreturn 0 }",
    )
    .unwrap();
    let binary = Path::new(env!("CARGO_BIN_EXE_suz"));
    for (index, limit) in [None, Some("256M"), Some("2G"), Some("unlimited"), Some("1")]
        .into_iter()
        .enumerate()
    {
        let name = format!("generated-{index}");
        let mut args = vec!["input.suz", "-o", name.as_str()];
        if let Some(limit) = limit {
            args.extend(["--symbol-pool-limit", limit]);
        }
        success(&emit(binary, &directory.0, &args));
        let generated = directory.0.join(name);
        let mut build = Command::new(generated.join("scripts/build-c.sh"));
        build
            .current_dir(&generated)
            .args(["program.c", "-o", "program"]);
        for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS"] {
            build.env_remove(name);
        }
        let built = build.output().unwrap();
        assert!(built.status.success(), "{built:?}");
        let output = Command::new(generated.join("program")).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(if limit == Some("1") { 101 } else { 0 }),
            "{output:?}"
        );
        if limit == Some("1") {
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("symbol pool limit exceeded: limit=1, used=0")
            );
        } else {
            assert!(output.stderr.is_empty());
        }
    }
}
