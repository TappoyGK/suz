use crate::test_util::TemporaryDirectory;
use std::{fs, path::Path, process::Command};

fn build_command() -> Command {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut command = Command::new(root.join("scripts/build-c.sh"));
    for variable in ["CFLAGS", "CPPFLAGS", "LDFLAGS"] {
        command.env_remove(variable);
    }
    command
}

#[test]
fn c_headers_require_clang_c23_and_wide_integers() {
    let directory = TemporaryDirectory::new();
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    let source = directory.0.join("requirements.c");
    for header in ["suz_runtime.h", "suz_env.h"] {
        fs::write(&source, format!("#include \"{header}\"\n")).unwrap();
        for flags in [
            vec!["-std=c11"],
            vec!["-std=c23", "-U__clang__"],
            vec!["-std=c23", "-U__BITINT_MAXWIDTH__"],
        ] {
            let result = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
                .args(flags)
                .args(["-fsyntax-only", "-I"])
                .arg(&runtime)
                .arg(&source)
                .output()
                .unwrap();
            assert!(!result.status.success());
            assert!(
                String::from_utf8_lossy(&result.stderr).contains("[runtime.c-check]"),
                "{result:?}"
            );
        }
    }
}

#[test]
fn generated_c_and_runtime_reject_unsafe_math_modes() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("program.c");
    fs::write(
        &source,
        suz::compile(b"fn main() -> u8 { return 0 }").unwrap(),
    )
    .unwrap();
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    for file in [
        &source,
        &runtime.join("suz_runtime.c"),
        &runtime.join("suz_float.c"),
        &runtime.join("suz_format_float.c"),
    ] {
        for flag in [
            "-ffast-math",
            "-ffinite-math-only",
            "-fno-signed-zeros",
            "-freciprocal-math",
            "-funsafe-math-optimizations",
        ] {
            let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
                .args(["-std=c23", "-fsyntax-only", flag, "-I"])
                .arg(&runtime)
                .arg("-I")
                .arg(runtime.join("third_party/ryu"))
                .arg(file)
                .output()
                .unwrap();
            assert!(
                !output.status.success(),
                "{} accepted {flag}",
                file.display()
            );
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(
                diagnostic.contains("[runtime.float-environment]")
                    || diagnostic.contains("FENV_ACCESS"),
                "{diagnostic}"
            );
        }
    }
}

#[test]
fn build_command_rejects_uncontrolled_flags_before_compiling() {
    let directory = TemporaryDirectory::new();
    let output = directory.0.join("program");
    for flag in [
        "-ffast-math",
        "-ffinite-math-only",
        "-fno-signed-zeros",
        "-ffp-contract=fast",
        "-fdenormal-fp-math=positive-zero",
        "-fassociative-math",
        "-Ofast",
    ] {
        let result = build_command()
            .arg("missing.c")
            .arg("-o")
            .arg(&output)
            .arg(flag)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stderr).contains("[runtime.float-environment]"));
        assert!(!output.exists());
    }
    for variable in ["CFLAGS", "CPPFLAGS", "LDFLAGS"] {
        let result = build_command()
            .env(variable, "-ffast-math")
            .arg("missing.c")
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stderr).contains(variable));
        assert!(!output.exists());
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn build_command_preserves_ieee_results_and_protects_existing_output() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("program.c");
    fs::write(
        &source,
        suz::compile(
            br#"fn main() -> u8 {
    fix zero: f64 = 0.0
    fix invalid = zero / zero
    if invalid == invalid || 1.0 / -zero != -inf { return 1 }
    fix tiny: f32 = 1.40129846e-45
    if tiny + tiny != 2.80259693e-45 { return 2 }
    log info `${invalid} ${-zero}`
    return 7
}"#,
        )
        .unwrap(),
    )
    .unwrap();
    for optimization in ["-O0", "-O2"] {
        let output = directory.0.join(format!("program{optimization}"));
        let built = build_command()
            .arg(&source)
            .arg("-o")
            .arg(&output)
            .arg(optimization)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let result = Command::new(&output).output().unwrap();
        assert_eq!(result.status.code(), Some(7));
        assert_eq!(result.stderr, b"info <input>:7: nan -0.0\n");
        assert!(result.stdout.is_empty());
        let bytes = fs::read(&output).unwrap();
        let duplicate = build_command()
            .arg(&source)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(duplicate.status.code(), Some(2));
        assert_eq!(fs::read(&output).unwrap(), bytes);
    }
}

#[cfg(unix)]
#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn stack_exhaustion_terminates_abnormally() {
    use std::os::unix::process::ExitStatusExt;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let directory = TemporaryDirectory::new();
    let source = directory.0.join("recursive.c");
    fs::write(
        &source,
        suz::compile(
            br#"fn recurse(n: u64) -> void {
    if n == 0 { return }
    recurse(n - 1)
    log info `${n}`
}
fn main() -> u8 {
    recurse(18446744073709551615)
    return 0
}"#,
        )
        .unwrap(),
    )
    .unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = directory.0.join(format!("recursive{optimization}"));
        let built = build_command()
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .arg(optimization)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        // Limit only the child; suppress core dumps and prevent tail-call elimination
        // with the observable log after each recursive call.
        let mut child = Command::new("/bin/sh")
            .args([
                "-c",
                "ulimit -c 0 && ulimit -s 1024 && exec \"$1\"",
                "suz-stack-test",
            ])
            .arg(&executable)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("stack exhaustion did not terminate within 10 seconds ({optimization})");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(
            status.signal().is_some(),
            "expected abnormal termination: {status}"
        );
    }
}
