use super::symbol_runtime::{build_with_environment, build_with_environment_optimized};
use std::process::Command;

#[test]
fn startup_registers_unused_folded_and_pattern_symbols_before_direct_access() {
    let source = r#"fix UNUSED = `unused`
fix FOLDED = `${`fold`}${`ed`}`
fn unused() -> symbol { return `not called` }
fn compare(t: symbol) -> u8 {
    var count: u8 = 0
    for index in 0..<3 {
        if t == `v1` { count += 1 }
        if t != `` { count += 1 }
        match t { `v1` => { count += 1 }
            _ => {} }
        fix selected: u8 = matchexpr t { `v1` => 1
            _ => 0 }
        count += selected
    }
    return count
}
fn main() -> u8 {
    if false { fix message = `not selected` }
    return 0
}
"#;
    let generated = suz::compile(source.as_bytes()).unwrap();
    let (functions, entry) = generated.split_once("int main(void)").unwrap();
    assert!(!functions.contains("suz_text_intern("));
    assert_eq!(entry.matches("suz_text_intern(").count(), 8);
    let driver = r#"
#include <string.h>
int main(void) {
    assert(suz_program() == 0);
    size_t calls = text_calls;
    uint64_t used = suz_text_pool_used();
    assert(calls > 0 && used == text_live);
    text_fail_after = calls;
    const char *values[] = { "unused", "fold", "ed", "folded", "not called", "not selected", "v1", "" };
    suz_text_location location = {"driver", 1, 1};
    for (size_t i = 0; i < sizeof(values) / sizeof(values[0]); ++i) {
        suz_text value = suz_text_intern(values[i], strlen(values[i]), true, location);
        assert(value != NULL);
    }
    suz_text target = suz_text_intern("v1", 2, false, location);
    for (unsigned repeat = 0; repeat < 1000; ++repeat) assert(suz_fn_1(target) == 12);
    assert(text_calls == calls && suz_text_pool_used() == used);
    return 0;
}
"#;
    for optimized in [false, true]
        .into_iter()
        .filter(|optimized| !optimized || !cfg!(debug_assertions))
    {
        let (_directory, executable) = build_with_environment_optimized(source, driver, optimized);
        let result = Command::new(executable).output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn startup_failure_points_to_main_keyword_before_main_body_runs() {
    let source =
        "fix UNUSED = `not used`\n  fn    main() -> u8 { log info `main entered`\nreturn 0 }";
    let driver = "int main(void) { text_fail_after = 0; text_expected_panic_calls = 1; return suz_program(); }";
    let (_directory, executable) = build_with_environment(source, driver);
    let result = Command::new(executable).output().unwrap();
    assert_eq!(result.status.code(), Some(101));
    assert_eq!(
        result.stderr,
        b"panic text.suz:2:3: symbol memory allocation failed [rt.symbol-allocation]\n"
    );
}

#[test]
fn logs_reference_generated_and_long_symbol_without_allocating() {
    let source = r#"fn make_text(count: u8) -> symbol {
    log info `inner: ${count}`
    return `count: ${count}`
}
fn write(message: symbol, top: u64) -> void {
    fix fraction = 1.5
    fix yes = true
    fix finite = top * top < inf
    log info `outer: ${message} / ${fraction} / ${yes} / ${top} / ${finite}`
}
fn main() -> u8 { return 0 }
"#;
    let driver = r#"
#include <string.h>
int main(void) {
    assert(suz_program() == 0);
    size_t before = text_calls;
    suz_text value = suz_fn_0(3);
    assert(text_calls > before);
    size_t after = text_calls;
    text_fail_after = after;
    suz_fn_1(value, UINT64_MAX);
    assert(text_calls == after);
    text_fail_after = SIZE_MAX;
    char bytes[8192];
    memset(bytes, 'x', sizeof(bytes));
    bytes[4096] = 0;
    suz_text long_value = suz_text_intern(bytes, sizeof(bytes), false, (suz_text_location){"driver", 1, 1});
    after = text_calls;
    size_t live = text_live;
    text_fail_after = after;
    for (unsigned i = 0; i < 3; ++i) suz_fn_1(long_value, UINT64_MAX);
    assert(text_calls == after && text_live == live);
    return 0;
}
"#;
    let (_directory, executable) = build_with_environment(source, driver);
    let result = Command::new(executable).output().unwrap();
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut expected =
        b"info text.suz:2: inner: 3\ninfo text.suz:9: outer: count: 3 / 1.5 / true / 18446744073709551615 / true\n".to_vec();
    let mut bytes = vec![b'x'; 8192];
    bytes[4096] = 0;
    for _ in 0..3 {
        expected.extend_from_slice(b"info text.suz:9: outer: ");
        expected.extend_from_slice(&bytes);
        expected.extend_from_slice(b" / 1.5 / true / 18446744073709551615 / true\n");
    }
    assert_eq!(result.stderr, expected);
}
