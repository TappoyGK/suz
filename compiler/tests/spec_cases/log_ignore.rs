use crate::{support::batch::Batch, test_util::TemporaryDirectory};
use std::{fs, process::Command};
use suz::{CompilationOptions, LogLevel};

const SOURCE: &str = r#"error Err { Bad, Other }
fn send(ok: bool) -> void or Err { if ok { return }
fail Err.Bad }
fn main() -> u8 {
    send(true) or ignore: success-only
    send(false) or ignore: literal ` ${ // 日本語
    send(false) or error {
        Err.Bad => ignore: selected
        Err.Other => ignore: unselected
    }
    send(true) or error { _ => ignore: success-arm }
    send(false) or error { _ => { fix unused = 1 } }
    send(false) or error { _ => {
        ignore: first
        ignore: second
        return 0
        ignore: unreachable
    } }
    return 1
}
"#;
const EXPECTED: &str = "ignore input.suz:6: literal ` ${ // 日本語\nignore input.suz:8: selected\nignore input.suz:14: first\nignore input.suz:15: second\n";

#[test]
fn ignore_records_only_reached_sites_independently_of_log_level() {
    let directory = TemporaryDirectory::new();
    let mut files = Vec::new();
    for (i, (enabled, level)) in [
        (false, LogLevel::Debug),
        (true, LogLevel::Info),
        (true, LogLevel::Off),
    ]
    .into_iter()
    .enumerate()
    {
        let c = suz::compile_with_options(
            SOURCE.as_bytes(),
            "some/path/input.suz",
            &CompilationOptions {
                log_ignore: enabled,
                log_level: level,
                ..Default::default()
            },
        )
        .unwrap()
        .c;
        assert_eq!(c.contains("SUZ_LOG_IGNORE"), enabled);
        assert_eq!(c.contains("literal"), enabled);
        assert_eq!(c.contains("input.suz"), enabled);
        assert!(!c.contains("suz_text_intern("));
        let path = directory.0.join(format!("ignore-{i}.c"));
        fs::write(&path, c).unwrap();
        files.push((i, path));
    }
    let batch = Batch::build(&directory.0, &files, 3).unwrap();
    for i in 0..3 {
        let output = batch.run(i).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stderr, if i == 0 { "" } else { EXPECTED }.as_bytes());
    }
    // 無効時にも理由の空文字・制御文字を検査する。
    for reason in ["", "bad\treason"] {
        let source = format!(
            "error Err {{ Bad }}\nfn send() -> void or Err {{ fail Err.Bad }}\nfn main() -> u8 {{ send() or ignore: {reason}\nreturn 0 }}"
        );
        for enabled in [false, true] {
            let options = CompilationOptions {
                log_ignore: enabled,
                ..Default::default()
            };
            assert!(suz::compile_with_options(source.as_bytes(), "input.suz", &options).is_err());
            assert!(suz::check_source_with_options(source.as_bytes(), &options).is_err());
        }
    }
}

#[test]
fn cli_ignore_is_shared_and_exported_without_enabling_ordinary_logs() {
    let directory = TemporaryDirectory::new();
    fs::write(directory.0.join("input.suz"), SOURCE).unwrap();
    let binary = env!("CARGO_BIN_EXE_suz");
    for command in ["check", "run", "build", "emit-c"] {
        let duplicate = Command::new(binary)
            .args([command, "input.suz", "--log-ignore", "--log-ignore"])
            .output()
            .unwrap();
        assert_eq!(duplicate.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&duplicate.stderr).contains("[tool.log-ignore]"));
        let mut process = Command::new(binary);
        process.current_dir(&directory.0).args([
            command,
            "input.suz",
            "--log-ignore",
            "--log-level",
            "off",
        ]);
        match command {
            "build" => {
                process.args(["-o", "built"]);
            }
            "emit-c" => {
                process.args(["-o", "exported"]);
            }
            "check" => {
                process.env("PATH", "");
            }
            _ => {}
        }
        let output = process.output().unwrap();
        assert!(output.status.success(), "{command}: {output:?}");
        assert_eq!(
            output.stderr,
            if command == "run" { EXPECTED } else { "" }.as_bytes()
        );
    }
    let built = Command::new(directory.0.join("built")).output().unwrap();
    assert_eq!(built.stderr, EXPECTED.as_bytes());
    let exported = directory.0.join("exported");
    let build = Command::new("bash")
        .current_dir(&exported)
        .args(["scripts/build-c.sh", "program.c", "-o", "program", "-O2"])
        .output()
        .unwrap();
    assert!(build.status.success(), "{build:?}");
    let run = Command::new(exported.join("program")).output().unwrap();
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(run.stderr, EXPECTED.as_bytes());
}
