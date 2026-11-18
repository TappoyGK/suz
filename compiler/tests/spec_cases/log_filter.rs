use crate::{support::batch::Batch, test_util::TemporaryDirectory};
use std::{fs, process::Command};
use suz::{CompilationOptions, LogLevel};

fn options(level: LogLevel) -> CompilationOptions {
    CompilationOptions {
        log_level: level,
        ..CompilationOptions::default()
    }
}

const LEVELS: [LogLevel; 5] = [
    LogLevel::Debug,
    LogLevel::Info,
    LogLevel::Warn,
    LogLevel::Error,
    LogLevel::Off,
];
const SOURCE: &str = "fn main() -> u8 {\nfix value = 7\nlog debug `debug-only ${value}`\nlog info `info-only ${value}`\nlog warn `warn-only ${value}`\nlog error `error-only ${value}`\nreturn 0 }";

#[test]
fn log_threshold_removes_reads_output_and_log_only_data() {
    let directory = TemporaryDirectory::new();
    let mut files = Vec::new();
    for (index, level) in LEVELS.into_iter().enumerate() {
        let c = suz::compile_with_options(SOURCE.as_bytes(), "levels.suz", &options(level))
            .unwrap()
            .c;
        for (at, name) in ["debug", "info", "warn", "error"].into_iter().enumerate() {
            assert_eq!(
                c.contains(&format!("{name}-only")),
                at >= index,
                "{level:?}: {c}"
            );
        }
        assert_eq!(
            c.matches("const int64_t suz_log_").count(),
            4_usize.saturating_sub(index)
        );
        let path = directory.0.join(format!("level-{index}.c"));
        fs::write(&path, c).unwrap();
        files.push((index, path));
    }
    let batch = Batch::build(&directory.0, &files, 5).unwrap();
    for index in 0..5 {
        let result = batch.run(index).unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        let expected: String = ["debug", "info", "warn", "error"]
            .iter()
            .enumerate()
            .skip(index)
            .map(|(at, name)| format!("{name} levels.suz:{}: {name}-only 7\n", at + 3))
            .collect();
        assert_eq!(result.stderr, expected.as_bytes());
    }
    assert_eq!(
        suz::compile(SOURCE.as_bytes()).unwrap(),
        suz::compile_with_options(SOURCE.as_bytes(), "<input>", &options(LogLevel::Info))
            .unwrap()
            .c
    );

    // log固有の表示名は消す。独立した定数宣言や、残るログの表示名は消さない。
    let source = b"enum Hidden: u8 { HiddenVariant }\nenum Visible: u8 { VisibleVariant }\nfix DECLARED = `declared-symbol`\nfn main() -> u8 { log debug `${Hidden.HiddenVariant} ${Visible.VisibleVariant} ${DECLARED}`\nlog warn `${Visible.VisibleVariant}`\nreturn 0 }";
    let c = suz::compile_with_options(source, "data.suz", &options(LogLevel::Warn))
        .unwrap()
        .c;
    assert!(!c.contains("HiddenVariant"));
    assert!(c.contains("VisibleVariant"));
    assert!(c.contains("declared-symbol") && c.contains("suz_text_intern("));
}

#[test]
fn filtering_keeps_diagnostics_and_all_panic_paths() {
    for source in [
        "fn main() -> u8 { log debug `${missing}`\nreturn 0 }",
        "struct Point { x: u8 }\nfn main() -> u8 { fix point = Point { x: 1 }\nlog debug `${point}`\nreturn 0 }",
        "fn main() -> u8 { log debug `${1 + 2}`\nreturn 0 }",
        "fix BAD: u8 = 1 / 0\nfn main() -> u8 { log debug `${BAD}`\nreturn 0 }",
    ] {
        let expected =
            suz::compile_with_options(source.as_bytes(), "check.suz", &options(LogLevel::Debug))
                .err()
                .unwrap()
                .to_string();
        for level in LEVELS {
            assert_eq!(
                suz::compile_with_options(source.as_bytes(), "check.suz", &options(level))
                    .err()
                    .unwrap()
                    .to_string(),
                expected
            );
            assert_eq!(
                suz::check_source_with_options(source.as_bytes(), &options(level))
                    .err()
                    .unwrap()
                    .to_string(),
                expected
            );
        }
    }
    let source = b"fn main() -> u8 { fix n: u8 = 1 + 2 allow [diagnostic.arithmetic: small]\nlog debug `${n}`\nreturn 0 }";
    for level in LEVELS {
        let config = CompilationOptions {
            arithmetic_mode: suz::ArithmeticMode::Warn,
            log_allow: true,
            ..options(level)
        };
        let warnings = suz::compile_with_options(source, "warnings.suz", &config)
            .unwrap()
            .warnings;
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].allowed_reason.as_deref(), Some("small"));
    }
    let directory = TemporaryDirectory::new();
    let mut files = Vec::new();
    for (i, body) in [
        "panic `explicit`",
        "fix n = risky() or panic `handler`\nreturn 0",
        "fix zero: u8 = 0\nreturn 1 / zero",
    ]
    .iter()
    .enumerate()
    {
        let source = format!(
            "error Err {{ Bad }}\nfn risky() -> u8 or Err {{ fail Err.Bad }}\nfn main() -> u8 {{ {body} }}"
        );
        let c = suz::compile_with_options(source.as_bytes(), "panic.suz", &options(LogLevel::Off))
            .unwrap()
            .c;
        let path = directory.0.join(format!("panic-{i}.c"));
        fs::write(&path, c).unwrap();
        files.push((i, path));
    }
    let batch = Batch::build(&directory.0, &files, 3).unwrap();
    for (i, expected) in ["explicit", "handler", "[rt.div-zero]"].iter().enumerate() {
        let output = batch.run(i).unwrap();
        assert_eq!(output.status.code(), Some(101), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
    }
}

#[test]
fn cli_log_level_is_shared_by_all_commands_and_exports_filtered_c() {
    let directory = TemporaryDirectory::new();
    fs::write(directory.0.join("input.suz"), SOURCE).unwrap();
    let binary = env!("CARGO_BIN_EXE_suz");
    for command in ["check", "run", "build", "emit-c"] {
        for arguments in [
            vec!["--log-level"],
            vec!["--log-level", ""],
            vec!["--log-level", "INFO"],
            vec!["--log-level", "panic"],
            vec!["--log-level", "info", "--log-level", "warn"],
        ] {
            let output = Command::new(binary)
                .current_dir(&directory.0)
                .args([command, "input.suz"])
                .args(arguments)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(2), "{output:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("[tool.log-level]"),
                "{output:?}"
            );
        }
        let mut process = Command::new(binary);
        process
            .current_dir(&directory.0)
            .args([command, "input.suz", "--log-level", "warn"]);
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
        if command == "run" {
            assert_eq!(
                output.stderr,
                b"warn input.suz:5: warn-only 7\nerror input.suz:6: error-only 7\n"
            );
        } else {
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
    let built = Command::new(directory.0.join("built")).output().unwrap();
    assert_eq!(
        built.stderr,
        b"warn input.suz:5: warn-only 7\nerror input.suz:6: error-only 7\n"
    );
    let exported = directory.0.join("exported");
    let build = Command::new("bash")
        .current_dir(&exported)
        .args(["scripts/build-c.sh", "program.c", "-o", "program", "-O2"])
        .output()
        .unwrap();
    assert!(build.status.success(), "{build:?}");
    let run = Command::new(exported.join("program")).output().unwrap();
    assert_eq!(run.stderr, built.stderr);
}
