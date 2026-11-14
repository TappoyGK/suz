use crate::{integer_bounds::INTEGER_BOUNDS, support::batch::Batch, test_util::TemporaryDirectory};
use std::{fs, process::Command};

#[test]
fn logs_preserve_integer_boundaries_boolean_values_and_utf8_bytes() {
    let mut source = String::from("fn main() -> u8 {\n");
    let mut expected = String::new();
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!(
            "{{\nfix low: {ty} = {min}\nfix high: {ty} = {max}\n"
        ));
        let line = source.lines().count() + 1;
        source.push_str("log info `値=${low},${high}; ${true}/${false}; ${ifexpr low < high { 7 } else { 9 }}`\n}\n");
        expected.push_str(&format!(
            "info values.suz:{line}: 値={min},{max}; true/false; 7\n"
        ));
    }
    let line = source.lines().count() + 1;
    source.push_str("log info `\\`\\\\\\$\\t\\r\\n`\nreturn 0\n}\n");
    expected.push_str(&format!("info values.suz:{line}: `\\$\t\r\n\n"));
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("values.c");
    fs::write(
        &path,
        suz::compile_with_source_name(
            source.as_bytes(),
            suz::ArithmeticMode::Default,
            "/some/path/values.suz",
        )
        .unwrap()
        .c,
    )
    .unwrap();
    let batch = Batch::build(&directory.0, &[(0, path)], 1).unwrap();
    let output = batch.run(0).unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, expected.as_bytes());
}

#[test]
fn interpolation_panics_before_output_and_keeps_physical_positions() {
    let source = "fn main() -> u8 {\nfix zero: u8 = 0\nlog info `before`\nlog info `日本語 ${1} pending ${1 / zero}`\nreturn 0\n}\n";
    let column = source
        .lines()
        .nth(3)
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .chars()
        .count()
        + 1;
    let compiled =
        suz::compile_with_source_name(source.as_bytes(), suz::ArithmeticMode::Warn, "unicode.suz")
            .unwrap();
    assert_eq!(compiled.warnings.len(), 1);
    assert_eq!(
        (compiled.warnings[0].line, compiled.warnings[0].column),
        (4, column)
    );
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("panic.c");
    fs::write(&path, compiled.c).unwrap();
    let batch = Batch::build(&directory.0, &[(0, path)], 1).unwrap();
    let output = batch.run(0).unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "info unicode.suz:3: before\npanic unicode.suz:4:{column}: integer division by zero [rt.div-zero]\n"
        )
    );
}

#[test]
fn templates_validate_syntax_escapes_and_embedded_expressions() {
    for (body, rule) in [
        ("log info `${}`", "text.interpolation"),
        ("log info `${1 2}`", "text.interpolation"),
        ("log info `bad\\q`", "text.escape"),
        ("log info `first\nsecond`", "text.template-line"),
        ("log info `${1 // comment}`", "text.template-line"),
        ("log error `text`", "log.syntax"),
        ("log info 1", "log.syntax"),
        (
            "fix n: u16 = 2\nlog info `${n as u8 or return 1}`",
            "expr.or-return-position",
        ),
        ("log info `a` log info `b`", "src.statement-end"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\nreturn 0\n}}\n");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}: {error:?}"
        );
        assert!(error.message.contains(rule), "{source}: {error:?}");
    }
}

#[test]
fn runtime_keeps_log_fragments_atomic_across_threads() {
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("atomic.c");
    fs::write(
        &path,
        r#"#include "suz_runtime.h"
#include "suz_env.h"
#include <assert.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
static _Thread_local int writer;
static atomic_int owner;
static atomic_int lines;
void suz_env_log(const char *text, size_t len) {
    if (len == 5 && memcmp(text, "info ", 5) == 0) {
        assert(atomic_exchange(&owner, writer) == 0);
    }
    assert(atomic_load(&owner) == writer);
    sched_yield();
    if (len == 1 && text[0] == '\n') {
        atomic_fetch_add(&lines, 1);
        atomic_store(&owner, 0);
    }
}
_Noreturn void suz_env_panic(void) { abort(); }
static void *work(void *arg) {
    writer = *(int *)arg;
    for (int i = 0; i < 100; ++i) {
        suz_log_begin("thread.suz", 1);
        suz_log_i64(-i);
        suz_log_bool(true);
        suz_log_end();
    }
    return NULL;
}
int main(void) {
    pthread_t first, second;
    int a = 1, b = 2;
    assert(pthread_create(&first, NULL, work, &a) == 0);
    assert(pthread_create(&second, NULL, work, &b) == 0);
    assert(pthread_join(first, NULL) == 0);
    assert(pthread_join(second, NULL) == 0);
    assert(atomic_load(&lines) == 200);
    return 0;
}
"#,
    )
    .unwrap();
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    let executable = directory.0.join("atomic");
    let built = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread"])
        .arg("-I")
        .arg(&runtime)
        .arg(&path)
        .arg(runtime.join("suz_runtime.c"))
        .arg("-lm")
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let output = Command::new(executable).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}
