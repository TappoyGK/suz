use crate::test_util::TemporaryDirectory as Directory;
use std::{
    fs,
    process::{Command, Output},
};

fn command(directory: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_suz"));
    command
        .current_dir(directory)
        .env("PATH", "")
        .env("SDKROOT", "/nonexistent-sdk");
    command
}

fn failure(result: Output, code: i32, rule: &str) {
    assert_eq!(result.status.code(), Some(code), "{result:?}");
    assert!(result.stdout.is_empty());
    let diagnostic = String::from_utf8(result.stderr).unwrap();
    assert!(diagnostic.starts_with("suz: "), "{diagnostic}");
    assert!(
        diagnostic
            .lines()
            .next()
            .unwrap()
            .ends_with(&format!("[{rule}]")),
        "{diagnostic}"
    );
}

#[test]
fn information_is_embedded_and_does_not_require_source_or_tools() {
    let directory = Directory::new();
    let version = command(&directory.0).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        version.stdout,
        concat!("suz v", env!("CARGO_PKG_VERSION"), "\n").as_bytes()
    );
    assert!(version.stderr.is_empty());
    let licenses = command(&directory.0).arg("--licenses").output().unwrap();
    assert!(licenses.status.success());
    assert!(licenses.stderr.is_empty());
    let text = String::from_utf8(licenses.stdout).unwrap();
    for notice in [
        include_str!("../../../LICENSE-MIT"),
        include_str!("../../../LICENSE-APACHE"),
        include_str!("../../../runtime/third_party/ryu/LICENSE-Boost"),
        "Copyright 2018 Ulf Adams",
    ] {
        assert!(text.contains(notice));
    }
    for name in [
        None,
        Some("build"),
        Some("run"),
        Some("check"),
        Some("emit-c"),
    ] {
        let mut cmd = command(&directory.0);
        if let Some(name) = name {
            cmd.arg(name);
        }
        let help = cmd.arg("--help").output().unwrap();
        assert!(help.status.success());
        assert!(help.stderr.is_empty());
        let text = String::from_utf8(help.stdout).unwrap();
        assert!(text.contains("Usage:") && text.contains("--warn-unhandled-arithmetic"));
        if name == Some("check") {
            assert!(!text.contains("--cc") && !text.contains("--env"));
        }
    }
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}

#[test]
fn check_accepts_option_order_and_dash_prefixed_filenames_without_writing_files() {
    let directory = Directory::new();
    fs::write(
        directory.0.join("-input.suz"),
        "fn main() -> u8 { return 0 }",
    )
    .unwrap();
    for args in [
        vec!["check", "--", "-input.suz"],
        vec!["check", "./-input.suz", "--warn-unhandled-arithmetic"],
        vec!["check", "--warn-unhandled-arithmetic", "./-input.suz"],
    ] {
        let output = command(&directory.0).args(args).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
    }
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    failure(
        command(&directory.0)
            .args(["check", "missing.suz"])
            .output()
            .unwrap(),
        2,
        "tool.check",
    );
}

#[test]
fn usage_errors_are_tagged_and_do_not_read_source() {
    let directory = Directory::new();
    for args in [
        vec![],
        vec!["unknown"],
        vec!["--version", "file.suz"],
        vec!["check"],
        vec!["check", "a", "b"],
        vec!["check", "--help", "--help"],
        vec!["check", "--help", "--unknown"],
        vec!["check", "a", "-o", "b"],
        vec!["check", "a", "--cc", "clang"],
        vec!["check", "a", "--env", "env.c"],
        vec!["check", "a", "--force"],
        vec!["check", "a", "-O2"],
        vec![
            "check",
            "a",
            "--warn-unhandled-arithmetic",
            "--warn-unhandled-arithmetic",
        ],
        vec![
            "check",
            "a",
            "--warn-unhandled-arithmetic",
            "--deny-unhandled-arithmetic",
        ],
        vec!["build", "a"],
        vec!["build", "a", "-o"],
        vec!["build", "a", "-o", "b", "-o", "c"],
        vec!["build", "a", "-o", "b", "-O0", "-O2"],
        vec!["run", "a", "-o", "b"],
        vec!["emit-c", "a", "--cc", "clang"],
    ] {
        let output = command(&directory.0).args(&args).output().unwrap();
        if args.is_empty() {
            assert!(String::from_utf8_lossy(&output.stderr).contains("Usage:"));
        }
        failure(output, 2, "tool.arguments");
    }
}
