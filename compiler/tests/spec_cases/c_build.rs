use crate::test_util::TemporaryDirectory;
use std::{fs, path::Path, process::Command};

fn build_command() -> Command {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut command = Command::new(root.join("scripts/build-c.sh"));
    for variable in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS"] {
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
fn generated_c_and_runtime_reject_active_unsafe_math_macros() {
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
        // 固定フラグで打ち消さず、危険な設定のマクロが残る場合の拒否を確認する。
        for flag in ["-ffast-math", "-ffinite-math-only"] {
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
            assert!(diagnostic.contains("[runtime.unsafe-math]"), "{diagnostic}");
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
        assert!(String::from_utf8_lossy(&result.stderr).contains("[tool.build-settings]"));
        assert!(!output.exists());
    }
    for variable in ["CFLAGS", "CPPFLAGS", "LDFLAGS", "SUZ_CFLAGS"] {
        let result = build_command()
            .env(variable, "-ffast-math")
            .arg("missing.c")
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(diagnostic.contains(variable), "{diagnostic}");
        let rule = if variable == "SUZ_CFLAGS" {
            "[tool.extra-flags]"
        } else {
            "[tool.build-settings]"
        };
        assert!(diagnostic.contains(rule), "{diagnostic}");
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
    fix negative_zero = -zero
    log info "${invalid} ${negative_zero}"
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
        assert_eq!(result.stderr, b"info <input>:8: nan -0.0\n");
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
    log info "${n}"
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
        // 制限は子プロセスだけに適用する。コアダンプを抑え、各再帰呼び出しの後にログを出して末尾呼び出しの除去を防ぐ。
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

fn precise_math_with_flag(flag: &str) {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("precise.c");
    let generated = suz::compile(b"fn main() -> u8 { return 0 }")
        .unwrap()
        .replace("int main(void)", "int suz_entry(void)");
    // volatile入力で定数畳み込みだけの検査になることを避ける。
    // 通常なら危険な最適化が現れる式を、生成Cと同じヘッダーの下で評価する。
    fs::write(
        &source,
        format!(
            "{generated}\n{}",
            r#"
#include <math.h>
__attribute__((noinline)) int unordered(double x) { return x != x; }
__attribute__((noinline)) int infinite(double x) { return x == INFINITY || x == -INFINITY; }
__attribute__((noinline)) float add_zero(float x) { return x + 0.0f; }
__attribute__((noinline)) float divide(float x) { return x / 3.0f; }
__attribute__((noinline)) float separated(float a, float b, float c) { return a * b + c; }
__attribute__((noinline)) float ordered(float a, float b, float c) { return (a + b) + c; }
int main(void) {
    if (suz_entry() != 0) return 1;
    volatile double invalid = NAN, positive = INFINITY, negative = -INFINITY;
    if (!unordered(invalid) || !infinite(positive) || !infinite(negative)) return 7;
    volatile float negative_zero = -0.0f, five = 5.0f;
    if (signbit(add_zero(negative_zero))) return 2;
    if (divide(five) != 0x1.aaaaaap0f) return 3;
    volatile float a = 0x1.000002p0f, b = 0x1.fffffcp-1f, c = -1.0f;
    if (separated(a, b, c) != 0.0f) return 4;
    volatile float large = 0x1p100f, opposite = -0x1p100f, one = 1.0f;
    if (ordered(large, opposite, one) != 1.0f) return 5;
    volatile float tiny = 0x1p-149f, zero = 0.0f;
    if (tiny * one != tiny || !isinf(one / zero) || !isnan(zero / zero)) return 6;
    return 0;
}
"#
        ),
    )
    .unwrap();
    let executable = directory.0.join("precise");
    // 意図的に競合するフラグを渡すため、上書き通知だけはエラーにしない。
    // 実際のCLIでは危険なフラグを許可リストで拒否する。
    let built = crate::support::runtime_link::compiler()
        .args(["-std=c23", "-O2", "-Wall", "-Wextra", "-Werror", flag])
        .args([
            "-fhonor-nans",
            "-fhonor-infinities",
            "-ffp-contract=off",
            "-fdenormal-fp-math=ieee",
            "-Wno-error=overriding-option",
        ])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{flag}: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let result = Command::new(&executable).output().unwrap();
    assert_eq!(result.status.code(), Some(0), "{flag}: {result:?}");
}

#[test]
fn precise_math_with_no_signed_zeros() {
    precise_math_with_flag("-fno-signed-zeros");
}

#[test]
fn precise_math_with_reciprocal_math() {
    precise_math_with_flag("-freciprocal-math");
}

#[test]
fn precise_math_with_unsafe_math_optimizations() {
    precise_math_with_flag("-funsafe-math-optimizations");
}

#[test]
fn precise_math_with_fp_contract_fast() {
    precise_math_with_flag("-ffp-contract=fast");
}

#[test]
fn precise_math_with_no_honor_nans() {
    precise_math_with_flag("-fno-honor-nans");
}

#[test]
fn precise_math_with_no_honor_infinities() {
    precise_math_with_flag("-fno-honor-infinities");
}

#[test]
fn precise_math_with_fp_model_fast() {
    precise_math_with_flag("-ffp-model=fast");
}

#[test]
fn precise_math_with_fast_math() {
    precise_math_with_flag("-ffast-math");
}

#[test]
fn precise_math_with_finite_math_only() {
    precise_math_with_flag("-ffinite-math-only");
}

#[test]
fn precise_math_allows_vectorizing_independent_elements() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("vector.c");
    fs::write(&source, "#include \"suz_config.h\"\nvoid scale(float *restrict dst, const float *restrict src) { for (int i = 0; i < 4096; ++i) dst[i] = src[i] * 0.5f; }\n").unwrap();
    let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
        .args([
            "-std=c23",
            "-O2",
            "-fhonor-nans",
            "-fhonor-infinities",
            "-ffp-contract=off",
            "-fdenormal-fp-math=ieee",
            "-Rpass=loop-vectorize",
            "-S",
            "-emit-llvm",
            "-I",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime"))
        .arg(&source)
        .args(["-o", "-"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("vectorized loop"),
        "{output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("fmul <"),
        "{output:?}"
    );
}
