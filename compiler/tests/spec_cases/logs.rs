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
        source.push_str(
            "fix yes = true\nfix no = false\nfix selected = ifexpr low < high { 7 } else { 9 }\n",
        );
        let line = source.lines().count() + 1;
        source.push_str("log info `値=${low},${high}; ${yes}/${no}; ${selected}`\n}\n");
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
fn templates_validate_syntax_escapes_and_embedded_expressions() {
    for (body, rule) in [
        ("log info `${}`", "symbol.interpolation"),
        ("log info `${1 2}`", "symbol.interpolation"),
        ("log info `bad\\q`", "symbol.escape"),
        ("log info `first\nsecond`", "symbol.template-line"),
        ("log info `${1 // comment}`", "symbol.template-line"),
        ("log off `text`", "log.level"),
        ("log info 1", "log.syntax"),
        (
            "fix n: u16 = 2\nlog info `${n as u8 or return 1}`",
            "output.embed-value",
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
        assert!(error.message().contains(rule), "{source}: {error:?}");
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
static const suz_log_level levels[] = { SUZ_LOG_DEBUG, SUZ_LOG_INFO, SUZ_LOG_WARN, SUZ_LOG_ERROR, SUZ_LOG_IGNORE };
void suz_env_log(suz_log_level level, const char *text, size_t len) {
    assert(level == levels[writer - 1]);
    if ((len == 4 && (memcmp(text, "info", 4) == 0 || memcmp(text, "warn", 4) == 0)) ||
        (len == 5 && (memcmp(text, "debug", 5) == 0 || memcmp(text, "error", 5) == 0)) ||
        (len == 6 && memcmp(text, "ignore", 6) == 0)) {
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
        suz_log_begin(levels[writer - 1], "thread.suz", 1);
        suz_log_i64(-i);
        suz_log_bool(true);
        suz_log_f32(1.25f);
        suz_log_f64(-2.5);
        suz_log_end();
    }
    return NULL;
}
int main(void) {
    pthread_t threads[5];
    int writers[] = {1, 2, 3, 4, 5};
    for (int i = 0; i < 5; ++i) assert(pthread_create(&threads[i], NULL, work, &writers[i]) == 0);
    for (int i = 0; i < 5; ++i) assert(pthread_join(threads[i], NULL) == 0);
    assert(atomic_load(&lines) == 500);
    return 0;
}
"#,
    )
    .unwrap();
    let runtime = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    let executable = directory.0.join("atomic");
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()));
    super::support::float_runtime::link(&mut command, &runtime);
    let built = command
        .args(["-std=c23", "-Wall", "-Wextra", "-Werror", "-pthread"])
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

#[test]
fn output_interpolations_allow_names_fields_and_captures() {
    let source = r#"
struct Position { x: u8 }
struct Player { pos: Position }
enum State: u8 { Ready }
error Problem { Missing }
fix CONSTANT = 6
var GLOBAL: u8 = 7
fnshape Show() -> void
fn invoke(f: fn Show) -> void { f() }
fn show(parameter: u8) -> void {
    fix debug = 1
    var info: u8 = 2
    info = 3
    fix warn = Player { pos: Position { x: 4 } }
    log debug `${debug} ${info} ${warn.pos.x} ${parameter}`
    log info `${CONSTANT} ${GLOBAL} ${State.Ready} ${Problem.Missing}`
    for i in 0..<1 { log warn `${i}` }
    invoke(fn [debug, var info]() -> void { log error `${debug} ${info}` })
    log info `continued`
}
fn main() -> u8 {
    GLOBAL = 7
    show(5)
    return 0
}
"#;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("levels.c");
    let options = suz::CompilationOptions {
        log_level: suz::LogLevel::Debug,
        ..suz::CompilationOptions::default()
    };
    fs::write(
        &path,
        suz::compile_with_options(source.as_bytes(), "levels.suz", &options)
            .unwrap()
            .c,
    )
    .unwrap();
    let output = Batch::build(&directory.0, &[(0, path)], 1)
        .unwrap()
        .run(0)
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("debug levels.suz:15: 1 3 4 5\n"),
        "{stderr}"
    );
    assert!(stderr.contains(": 6 7 Ready Missing\n"), "{stderr}");
    assert!(stderr.contains("warn levels.suz:17: 0\n"), "{stderr}");
    assert!(stderr.contains("error levels.suz:18: 1 3\n"), "{stderr}");
    assert!(stderr.ends_with(": continued\n"), "{stderr}");
}

#[test]
fn output_interpolations_reject_computation_for_logs_and_both_panic_forms() {
    let declarations = "error Err { Bad }\nstruct Point { x: u8 }\nfnshape Action() -> u8\nfn call(f: fn Action) -> u8 { return f() }\nfn read() -> u8 { return 1 }\nfn risky() -> u8 or Err { fail Err.Bad }\n";
    for expression in [
        "read()",
        "f()",
        "call(fn []() -> u8 { return 1 })",
        "n + 1",
        "n == 1",
        "true && false",
        "not true",
        "n as u16",
        "n as% u8",
        "message[0:1]",
        "message[0]",
        "len message",
        "point with { x: 2 }",
        "Point { x: 2 }",
        "`nested`",
        "ifexpr true { 1 } else { 2 }",
        "matchexpr n { _ => 1 }",
        "expr { 1 }",
        "1",
        "true",
        "(n)",
    ] {
        for output in [
            "log debug",
            "log info",
            "log warn",
            "log error",
            "panic",
            "risky() or panic",
        ] {
            let source = format!(
                "{declarations}fn main() -> u8 {{ fix n = 1\nfix message = `x`\nfix point = Point {{ x: 1 }}\nfix f: static fn Action = fn read\n{output} `${{{expression}}}`\nreturn 0 }}"
            );
            let error = suz::compile(source.as_bytes()).unwrap_err().to_string();
            assert!(error.contains("[output.embed-value]"), "{source}\n{error}");
        }
    }
    // 通常のsymbolテンプレートは、計算した値を整形する用途を引き続き持つ。
    let source = format!(
        "{declarations}fn main() -> u8 {{ fix message = `${{read() + 2}}`\nlog info `${{message}}`\nreturn 0 }}"
    );
    let output = crate::test_util::compile_and_run_c23(&source, "symbol.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).ends_with(": 3\n"));
}

#[test]
fn c_string_trigraph_sequences_preserve_output_bytes() {
    // Cの三文字表記9種類を、同じ変換を使う各出力経路で確認する。
    let text = "日本語 ??= ??( ??) ??/ ??' ??< ??> ??! ??-";
    let source = format!(
        "error Err {{ Bad }}\nfn send() -> void or Err {{ fail Err.Bad }}\nfn main() -> u8 {{\nsend() or ignore: {text}\nfix value = `{text}`\nlog info `${{value}}`\nlog info `{text}`\npanic `{text}`\n}}"
    );
    let c = suz::compile_with_options(
        source.as_bytes(),
        "trigraph.suz",
        &suz::CompilationOptions {
            log_ignore: true,
            ..Default::default()
        },
    )
    .unwrap()
    .c;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("trigraph.c");
    fs::write(&path, c).unwrap();
    let batch = Batch::build(&directory.0, &[(0, path)], 1).unwrap();
    let output = batch.run(0).unwrap();
    assert_eq!(output.status.code(), Some(101), "{output:?}");
    assert_eq!(output.stderr, format!(
        "ignore trigraph.suz:4: {text}\ninfo trigraph.suz:6: {text}\ninfo trigraph.suz:7: {text}\npanic trigraph.suz:8:1: {text}\n"
    ).as_bytes());
}
