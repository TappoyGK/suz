use crate::integer_bounds::INTEGER_BOUNDS;
use crate::test_util::TemporaryDirectory;
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn build(source: &Path, executable: &Path, optimization: &str, standard_env: bool) {
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    let mut command = Command::new(std::env::var("CC").unwrap_or_else(|_| "clang".into()));
    command
        .args([
            "-std=c23",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fsanitize=undefined",
            "-fno-sanitize-recover=all",
            optimization,
        ])
        .arg("-I")
        .arg(&runtime)
        .arg(source)
        .arg(runtime.join("suz_runtime.c"))
        .arg("-lm");
    if standard_env {
        command.arg(runtime.join("suz_env.c"));
    }
    let result = command.arg("-o").arg(executable).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn literal(value: i128, signed: bool) -> String {
    if !signed {
        return format!("UINT64_C({value})");
    }
    if value == i64::MIN as i128 {
        return "(-INT64_C(9223372036854775807) - INT64_C(1))".into();
    }
    if value < 0 {
        format!("(-INT64_C({}))", -value)
    } else {
        format!("INT64_C({value})")
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn checked_integer_runtime_matches_all_type_boundaries_without_undefined_behavior() {
    let mut source = String::from("#include \"suz_runtime.h\"\nint main(void) {\n");
    for (_, min, max) in INTEGER_BOUNDS {
        let signed = min < 0;
        let mut values = vec![min, min + 1, -1, 0, 1, max - 1, max];
        values.retain(|v| (min..=max).contains(v));
        values.sort_unstable();
        values.dedup();
        for &a in &values {
            let width = (max - min + 1).ilog2();
            for count in [
                i64::MIN as i128,
                -1,
                0,
                1,
                i128::from(width - 1),
                i128::from(width),
                64,
                u64::MAX as i128,
            ] {
                for operation in ["shl", "shr"] {
                    let expected = if (0..i128::from(width)).contains(&count) {
                        let value = if operation == "shl" {
                            a * (1_i128 << count)
                        } else {
                            a >> count
                        };
                        (min..=max).contains(&value).then_some(value)
                    } else {
                        None
                    };
                    let ty = if signed { "int64_t" } else { "uint64_t" };
                    let suffix = if signed { "i64" } else { "u64" };
                    source.push_str(&format!("{{ {ty} result = 42; bool ok = suz_try_{operation}_{suffix}({}, {}, {width}, &result);\nif (ok != {}) return 4;\n", literal(a, signed), literal(count as u64 as i128, false), expected.is_some()));
                    source.push_str(&format!(
                        "if (result != {}) return 5;\n}}\n",
                        literal(expected.unwrap_or(42), signed)
                    ));
                }
            }
            for &b in &values {
                for (operation, expected) in [
                    ("add", a.checked_add(b)),
                    ("sub", a.checked_sub(b)),
                    ("mul", a.checked_mul(b)),
                    (
                        "div",
                        a.checked_div(b)
                            .filter(|_| !(signed && a == min && b == -1)),
                    ),
                    (
                        "rem",
                        a.checked_rem(b)
                            .filter(|_| !(signed && a == min && b == -1)),
                    ),
                ] {
                    let expected = expected.filter(|value| (min..=max).contains(value));
                    let ty = if signed { "int64_t" } else { "uint64_t" };
                    let suffix = if signed { "i64" } else { "u64" };
                    let bounds = if signed {
                        format!("{}, {}", literal(min, true), literal(max, true))
                    } else {
                        literal(max, false)
                    };
                    source.push_str(&format!("{{ {ty} result = 42; bool ok = suz_try_{operation}_{suffix}({}, {}, {bounds}, &result);\nif (ok != {}) return 1;\n", literal(a, signed), literal(b, signed), expected.is_some()));
                    if let Some(value) = expected {
                        source.push_str(&format!(
                            "if (result != {}) return 2;\n",
                            literal(value, signed)
                        ));
                    }
                    if expected.is_none() {
                        source.push_str("if (result != 42) return 3;\n");
                    }
                    source.push_str("}\n");
                }
            }
        }
    }
    source.push_str("return 0;\n}\n");
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("checked.c");
    fs::write(&path, source).unwrap();
    for optimization in ["-O0", "-O2"] {
        let executable = directory.0.join(format!("checked{optimization}"));
        build(&path, &executable, optimization, true);
        let result = Command::new(executable).output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
fn panic_runtime_delegates_diagnostic_fragments_and_exit_to_the_environment() {
    let source = r#"#include "suz_runtime.h"
#include "suz_env.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static char text[256];
static size_t used;
void suz_env_log(suz_log_level level, const char *fragment, size_t len) {
    if (level != SUZ_LOG_PANIC) exit(4);
    if (len == 0 || len >= sizeof(text) - used) exit(2);
    memcpy(text + used, fragment, len);
    used += len;
}
_Noreturn void suz_env_panic(void) {
    if (strcmp(text, "panic sample.suz:3:9: integer overflow\n") != 0) exit(3);
    puts("panic callback");
    exit(101);
}
int main(void) {
    suz_panic("sample.suz", 3, 9, "integer overflow");
}
"#;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("custom-env.c");
    fs::write(&path, source).unwrap();
    let executable = directory.0.join("custom-env");
    build(&path, &executable, "-O2", false);
    let result = Command::new(executable).output().unwrap();
    assert_eq!(result.status.code(), Some(101));
    assert_eq!(result.stdout, b"panic callback\n");
    assert!(result.stderr.is_empty());
}

#[test]
fn standard_environment_copies_buffered_output_and_preserves_exit_status_on_failure() {
    let source = r#"#include "suz_runtime.h"
#include "suz_env.h"
#include <stdio.h>
#include <string.h>
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    if (strcmp(argv[1], "closed") == 0 || strcmp(argv[1], "closed-panic") == 0) {
        if (setvbuf(stderr, NULL, _IONBF, 0) != 0) return 3;
        if (getchar() != 'x') return 4;
        suz_env_log(SUZ_LOG_INFO, "discarded\n", 10);
        if (strcmp(argv[1], "closed-panic") == 0) suz_panic("sample.suz", 1, 1, "overflow");
        return 7;
    }
    static char buffer[1024];
    if (setvbuf(stderr, buffer, _IOFBF, sizeof(buffer)) != 0) return 5;
    char text[] = "copy\0bytes\n";
    suz_env_log(SUZ_LOG_INFO, text, sizeof(text) - 1);
    memset(text, 'x', sizeof(text));
    if (strcmp(argv[1], "panic") == 0) suz_panic("sample.suz", 3, 9, "integer overflow");
    return 7;
}
"#;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("standard-env.c");
    fs::write(&path, source).unwrap();
    let executable = directory.0.join("standard-env");
    build(&path, &executable, "-O2", true);
    for (mode, code, expected) in [
        ("normal", 7, b"copy\0bytes\n".as_slice()),
        (
            "panic",
            101,
            b"copy\0bytes\npanic sample.suz:3:9: integer overflow\n".as_slice(),
        ),
    ] {
        let result = Command::new(&executable).arg(mode).output().unwrap();
        assert_eq!(result.status.code(), Some(code));
        assert!(result.stdout.is_empty());
        assert_eq!(result.stderr, expected);
    }
    for (mode, code) in [("closed", 7), ("closed-panic", 101)] {
        let mut child = Command::new(&executable)
            .arg(mode)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // 子プロセスにパイプへ書かせる前に、唯一の読み取り側を閉じる。
        drop(child.stderr.take());
        child.stdin.take().unwrap().write_all(b"x").unwrap();
        let result = child.wait_with_output().unwrap();
        assert_eq!(result.status.code(), Some(code));
        assert!(result.stdout.is_empty());
    }
}

#[test]
fn standard_environment_retries_partial_and_interrupted_writes() {
    let source = r#"#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static char output[64];
static size_t used;
static int mode, calls, failed;
static size_t injected_write(const void *text, size_t size, size_t len, FILE *stream) {
    (void)stream;
    if (size != 1 || len > sizeof(output) - used) exit(2);
    ++calls;
    if (mode == 0 && calls == 2) {
        failed = 1;
        errno = EINTR;
        return 0;
    }
    size_t written = calls == 1 && len > 2 ? 2 : len;
    memcpy(output + used, text, written);
    used += written;
    if (mode == 1) {
        failed = 1;
        errno = EIO;
    }
    return written;
}
static int injected_error(FILE *stream) { (void)stream; return failed; }
static void injected_clear(FILE *stream) { (void)stream; failed = 0; }
#define fwrite injected_write
#define ferror injected_error
#define clearerr injected_clear
#include "suz_env.c"
int main(void) {
    suz_env_log(SUZ_LOG_INFO, "abcdef", 6);
    if (calls != 3 || used != 6 || memcmp(output, "abcdef", 6) != 0) return 3;
    used = 0;
    calls = 0;
    mode = 1;
    suz_env_log(SUZ_LOG_INFO, "abcdef", 6);
    if (calls != 1 || used != 2 || memcmp(output, "ab", 2) != 0) return 4;
    mode = 2;
    suz_env_log(SUZ_LOG_INFO, "XY", 2);
    if (calls != 2 || used != 4 || memcmp(output, "abXY", 4) != 0) return 5;
    return 0;
}
"#;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("write-failures.c");
    fs::write(&path, source).unwrap();
    let executable = directory.0.join("write-failures");
    build(&path, &executable, "-O2", false);
    let result = Command::new(executable).output().unwrap();
    assert_eq!(result.status.code(), Some(0));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}
