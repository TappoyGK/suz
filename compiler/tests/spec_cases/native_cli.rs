use crate::test_util::TemporaryDirectory;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn command(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_suz"));
    command
        .current_dir(directory)
        .env("TMPDIR", directory.join("temporary"));
    for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS"] {
        command.env_remove(name);
    }
    command
}
fn setup() -> TemporaryDirectory {
    let directory = TemporaryDirectory::new();
    fs::create_dir(directory.0.join("temporary")).unwrap();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 { return 7 }",
    )
    .unwrap();
    directory
}
fn code(output: Output, expected: i32) -> Output {
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
    output
}
fn clean(directory: &Path) {
    assert_eq!(
        fs::read_dir(directory.join("temporary")).unwrap().count(),
        0
    );
    assert!(!fs::read_dir(directory).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".suz-")
    }));
}

#[cfg(unix)]
#[test]
fn native_build_rejects_non_clang_and_pre_c23_compilers() {
    use std::os::unix::fs::PermissionsExt;
    let directory = setup();
    let wrapper = directory.0.join("compiler");
    for flag in ["-U__clang__", "-std=c11"] {
        fs::write(
            &wrapper,
            format!("#!/bin/sh\nexec \"${{REAL_CC:-clang}}\" \"$@\" {flag}\n"),
        )
        .unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
        for verb in ["build", "run"] {
            let mut cmd = command(&directory.0);
            cmd.env(
                "REAL_CC",
                std::env::var_os("CC").unwrap_or_else(|| "clang".into()),
            )
            .args([verb, "input.suz", "--cc", "./compiler"]);
            if verb == "build" {
                cmd.args(["-o", "program"]);
            }
            let result = code(cmd.output().unwrap(), 2);
            assert!(result.stdout.is_empty());
            let diagnostic = String::from_utf8_lossy(&result.stderr);
            assert!(diagnostic.contains("[tool.c-compiler]"), "{diagnostic}");
            assert!(diagnostic.contains("C23-capable Clang"), "{diagnostic}");
            assert!(!directory.0.join("program").exists());
            clean(&directory.0);
        }
    }
}

#[test]
fn native_settings_and_input_aliases_are_rejected_before_compilation() {
    let directory = setup();
    fs::write(directory.0.join("env.c"), "environment").unwrap();
    fs::hard_link(directory.0.join("input.suz"), directory.0.join("alias")).unwrap();
    fs::create_dir(directory.0.join("folder")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("input.suz", directory.0.join("link")).unwrap();
    for target in ["input.suz", "alias", "env.c", "folder"] {
        let result = code(
            command(&directory.0)
                .args([
                    "build",
                    "input.suz",
                    "-o",
                    target,
                    "--force",
                    "--env",
                    "env.c",
                ])
                .output()
                .unwrap(),
            2,
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("[tool.output]"));
    }
    #[cfg(unix)]
    code(
        command(&directory.0)
            .args(["build", "input.suz", "-o", "link", "--force"])
            .output()
            .unwrap(),
        2,
    );
    for variable in ["CFLAGS", "CPPFLAGS", "LDFLAGS"] {
        let result = code(
            command(&directory.0)
                .env(variable, "-ffast-math")
                .args(["run", "input.suz"])
                .output()
                .unwrap(),
            2,
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("[tool.build-settings]"));
    }
    let result = code(
        command(&directory.0)
            .args(["run", "input.suz", "--cc", "./missing"])
            .output()
            .unwrap(),
        2,
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("xcode-select --install"));
    assert_eq!(
        fs::read_to_string(directory.0.join("input.suz")).unwrap(),
        "fn main() -> u8 { return 7 }"
    );
    clean(&directory.0);
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn native_build_protects_outputs_on_failures_and_replaces_only_with_force() {
    let directory = setup();
    fs::write(directory.0.join("program"), b"original").unwrap();
    code(
        command(&directory.0)
            .args(["build", "input.suz", "-o", "program"])
            .output()
            .unwrap(),
        2,
    );
    fs::write(
        directory.0.join("invalid.suz"),
        "fn main() -> u8 { return 256 }",
    )
    .unwrap();
    code(
        command(&directory.0)
            .args(["build", "invalid.suz", "-o", "program", "--force"])
            .output()
            .unwrap(),
        1,
    );
    fs::write(directory.0.join("bad.c"), "#error broken environment\n").unwrap();
    let failed = code(
        command(&directory.0)
            .args([
                "build",
                "input.suz",
                "-o",
                "program",
                "--force",
                "--env",
                "bad.c",
            ])
            .output()
            .unwrap(),
        4,
    );
    assert!(String::from_utf8_lossy(&failed.stderr).contains("broken environment"));
    assert_eq!(fs::read(directory.0.join("program")).unwrap(), b"original");
    clean(&directory.0);
    code(
        command(&directory.0)
            .args(["build", "input.suz", "-o", "program", "--force"])
            .output()
            .unwrap(),
        0,
    );
    code(
        Command::new(directory.0.join("program")).output().unwrap(),
        7,
    );
    clean(&directory.0);
}

#[cfg(unix)]
#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn output_created_during_compilation_is_not_overwritten() {
    use std::os::unix::fs::PermissionsExt;
    let directory = setup();
    let wrapper = directory.0.join("compiler");
    fs::write(&wrapper, "#!/bin/sh\n\"${REAL_CC:-clang}\" \"$@\" || exit $?\ncase \"$*\" in *program.c*) printf preserved > \"$RACE_OUTPUT\" ;; esac\n").unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    code(
        command(&directory.0)
            .env("RACE_OUTPUT", directory.0.join("program"))
            .env(
                "REAL_CC",
                std::env::var_os("CC").unwrap_or_else(|| "clang".into()),
            )
            .args(["build", "input.suz", "-o", "program", "--cc", "./compiler"])
            .output()
            .unwrap(),
        2,
    );
    assert_eq!(fs::read(directory.0.join("program")).unwrap(), b"preserved");
    clean(&directory.0);
}

#[cfg(unix)]
#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn run_inherits_streams_and_cwd_and_cleans_up_after_signals() {
    use std::{
        io::Write,
        os::unix::process::{CommandExt, ExitStatusExt},
        process::Stdio,
        time::{Duration, Instant},
    };
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let directory = setup();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 { log info `ready`\nreturn 7 }",
    )
    .unwrap();
    fs::write(
        directory.0.join("env.c"),
        r#"#include "suz_env.h"
#include <stdio.h>
#include <stdlib.h>
void *suz_env_alloc(size_t size) { return malloc(size); }
void suz_env_free(void *ptr) { free(ptr); }
void suz_env_log(suz_log_level level, const char *s, size_t n) {
    (void)level;
    static int written; (void)s; (void)n;
    if (!written) { putchar(getchar()); written = 1; }
}
_Noreturn void suz_env_panic(void) { exit(101); }
"#,
    )
    .unwrap();
    let mut child = command(&directory.0)
        .args(["run", "input.suz", "--env", "env.c"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"stream").unwrap();
    let result = code(child.wait_with_output().unwrap(), 7);
    assert_eq!(result.stdout, b"s");
    assert!(result.stderr.is_empty());
    clean(&directory.0);
    fs::write(
        directory.0.join("env.c"),
        r#"#include "suz_env.h"
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
void *suz_env_alloc(size_t size) { return malloc(size); }
void suz_env_free(void *ptr) { free(ptr); }
void suz_env_log(suz_log_level level, const char *s, size_t n) {
    (void)level;
    (void)s; (void)n;
    FILE *f = fopen("ready", "w"); if (!f) exit(91);
    fputs("ready", f); fclose(f);
    for (;;) pause();
}
_Noreturn void suz_env_panic(void) { exit(101); }
"#,
    )
    .unwrap();
    for (signal, group) in [(2, false), (15, true)] {
        let mut child = command(&directory.0)
            .args(["run", "input.suz", "--env", "env.c"])
            .process_group(0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut sent = false;
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if !sent && directory.0.join("ready").exists() {
                let pid = child.id() as i32;
                assert_eq!(unsafe { kill(if group { -pid } else { pid }, signal) }, 0);
                sent = true;
            }
            if Instant::now() > deadline {
                unsafe {
                    kill(-(child.id() as i32), 9);
                }
                child.wait().unwrap();
                panic!("run did not propagate signal {signal}");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(sent);
        assert_eq!(status.signal(), Some(signal));
        fs::remove_file(directory.0.join("ready")).unwrap();
        clean(&directory.0);
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn build_and_run_apply_text_pool_limits() {
    let directory = setup();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 { fix value = `x`\nreturn 0 }",
    )
    .unwrap();
    for (index, limit) in [None, Some("256M"), Some("2G"), Some("unlimited"), Some("1")]
        .into_iter()
        .enumerate()
    {
        let expected = if limit == Some("1") { 101 } else { 0 };
        let name = format!("program-{index}");
        for command_name in ["build", "run"] {
            let mut args = vec![command_name, "input.suz"];
            if command_name == "build" {
                args.extend(["-o", name.as_str()]);
            }
            if let Some(limit) = limit {
                args.extend(["--text-pool-limit", limit]);
            }
            let output = code(
                command(&directory.0).args(args).output().unwrap(),
                if command_name == "build" { 0 } else { expected },
            );
            let output = if command_name == "build" {
                code(
                    Command::new(directory.0.join(&name)).output().unwrap(),
                    expected,
                )
            } else {
                output
            };
            if expected == 101 {
                assert!(
                    String::from_utf8_lossy(&output.stderr).starts_with(
                        "panic input.suz:1:1: text pool limit exceeded: limit=1, used=0"
                    ),
                    "{output:?}"
                );
            } else {
                assert!(output.stderr.is_empty());
            }
            assert!(output.stdout.is_empty());
            clean(&directory.0);
        }
    }
}
