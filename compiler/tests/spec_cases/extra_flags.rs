use crate::test_util::TemporaryDirectory;
use std::{fs, path::Path, process::Command};

fn cli(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_suz"));
    command.current_dir(directory);
    for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS"] {
        command.env_remove(name);
    }
    command
}

#[cfg(unix)]
#[test]
fn extra_flags_match_between_cli_and_emitted_script() {
    use std::os::unix::fs::PermissionsExt;
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 { return 0 }",
    )
    .unwrap();
    let emitted = cli(&directory.0)
        .args(["emit-c", "input.suz", "-o", "generated"])
        .output()
        .unwrap();
    assert!(emitted.status.success(), "{emitted:?}");
    let compiler = directory.0.join("compiler");
    // 全許可フラグの転送を検査する。CPU名やPGOファイルの実在には依存させない。
    fs::write(
        &compiler,
        r#"#!/bin/bash
printf '%s\n' BEGIN "$@" >> "$FLAG_LOG"
while [[ $# -gt 0 ]]; do
    if [[ "$1" == '-o' ]]; then
        printf '#!/bin/sh\nexit 0\n' > "$2"
        chmod +x "$2"
        break
    fi
    shift
done
"#,
    )
    .unwrap();
    fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
    let accepted = [
        "",
        " \t\n\r\u{b}\u{c}",
        "-march=native",
        "-mcpu=native",
        "-mtune=native",
        "-g",
        "-fno-omit-frame-pointer",
        "-flto",
        "-flto=thin",
        "-flto=full",
        "-fprofile-instr-generate",
        "-fprofile-instr-generate=profile",
        "-fprofile-instr-use=profile",
        "-fsanitize=address",
        "-fsanitize=undefined",
        "-fsanitize=address,undefined",
        "-fsanitize=undefined,address,address",
        "-g\n-flto\t-fno-omit-frame-pointer\r-g\u{b}-g\u{c}-g",
        "-fprofile-instr-generate=$(touch_injected)",
    ];
    let rejected = [
        "-march=",
        "-mcpu=",
        "-mtune=",
        "-flto=other",
        "-fprofile-instr-use=",
        "-fprofile-instr-generate=",
        "-fsanitize=",
        "-fsanitize=address,",
        "-fsanitize=,address",
        "-fsanitize=address,,undefined",
        "-fsanitize=float-divide-by-zero",
        "-fsanitize=thread",
        "-O3",
        "-Ofast",
        "-ffast-math",
        "-fno-signed-zeros",
        "-ffp-contract=fast",
        "-DTEST",
        "-Iinclude",
        "-include",
        "@response",
        "-g;touch",
        "'-g'",
        "-g *",
        "-g -Wno-error",
    ];
    for (allowed, flags) in accepted
        .iter()
        .map(|f| (true, f))
        .chain(rejected.iter().map(|f| (false, f)))
    {
        for route in ["build", "run", "script"] {
            let output = directory.0.join("output");
            let log = directory.0.join("flags.log");
            let _ = fs::remove_file(&output);
            let _ = fs::remove_file(&log);
            let mut command = if route == "script" {
                let mut c = Command::new(directory.0.join("generated/scripts/build-c.sh"));
                c.arg(directory.0.join("generated/program.c"))
                    .arg("-o")
                    .arg(&output)
                    .env("CC", &compiler);
                for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS"] {
                    c.env_remove(name);
                }
                c
            } else {
                let mut c = cli(&directory.0);
                c.args([route, "input.suz", "--cc"]).arg(&compiler);
                if route == "build" {
                    c.arg("-o").arg(&output);
                }
                c
            };
            let result = command
                .env("SUZ_CFLAGS", flags)
                .env("FLAG_LOG", &log)
                .output()
                .unwrap();
            assert_eq!(
                result.status.code(),
                Some(if allowed { 0 } else { 2 }),
                "{route} {flags}: {result:?}"
            );
            if allowed {
                let recorded = fs::read_to_string(&log).unwrap();
                let expected = flags
                    .split([' ', '\t', '\n', '\r', '\u{b}', '\u{c}'])
                    .filter(|f| !f.is_empty())
                    .collect::<Vec<_>>();
                for invocation in recorded.split("BEGIN\n").skip(1) {
                    let args = invocation.lines().collect::<Vec<_>>();
                    assert_eq!(&args[..expected.len()], &expected, "{route} {flags}");
                    assert!(args[expected.len()..].contains(&"-ffp-contract=off"));
                    assert!(!args.contains(&"-ffp-model=strict"));
                }
            } else {
                assert!(!log.exists(), "compiler was invoked: {route} {flags}");
                assert!(
                    String::from_utf8_lossy(&result.stderr).contains("[tool.extra-flags]"),
                    "{result:?}"
                );
                assert!(!output.exists());
            }
        }
    }
}

#[test]
fn extra_flags_build_real_programs_through_both_routes() {
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("input.suz"),
        "fn main() -> u8 { return 7 }",
    )
    .unwrap();
    let result = cli(&directory.0)
        .args(["emit-c", "input.suz", "-o", "generated"])
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    for route in ["run", "script"] {
        let executable = directory.0.join("program");
        let mut command = if route == "run" {
            let mut c = cli(&directory.0);
            c.args(["run", "input.suz", "-O2"]);
            c
        } else {
            let mut c = Command::new(directory.0.join("generated/scripts/build-c.sh"));
            c.arg(directory.0.join("generated/program.c"))
                .arg("-o")
                .arg(&executable)
                .arg("-O2");
            for name in ["CFLAGS", "CPPFLAGS", "LDFLAGS"] {
                c.env_remove(name);
            }
            c
        };
        let result = command
            .env(
                "SUZ_CFLAGS",
                "-g -fno-omit-frame-pointer -fsanitize=address,undefined",
            )
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(if route == "run" { 7 } else { 0 }),
            "{result:?}"
        );
        if route == "script" {
            assert_eq!(Command::new(&executable).status().unwrap().code(), Some(7));
        }
    }
}
