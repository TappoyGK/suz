use super::symbol_runtime::build_with_environment;
use std::process::Command;

const SOURCE: &str = r#"struct Inner { name: symbol, count: u8 }
struct Item { inner: Inner, valid: bool }
fn convert(input: symbol) -> symbol {
    fix item = input as Item or return `fallback`
    return item.inner.name
}
fn main() -> u8 { return 0 }
"#;

const DRIVER: &str = r#"
#include <string.h>
static const suz_text_location input_location = {"driver", 1, 1};
static suz_text input(const char *bytes) {
    return suz_text_intern(bytes, strlen(bytes), true, input_location);
}
int main(int argc, char **argv) {
    assert(argc >= 2);
    uint64_t limit = strcmp(argv[1], "limit") == 0 ? strtoull(argv[2], NULL, 10) : 0;
    assert(suz_program() == 0);
    suz_text_pool_init(limit);
    suz_text fallback = input("fallback");
    if (strcmp(argv[1], "validation") == 0) {
        const char *invalid[] = {
            "Item { inner: Inner { name: `unseen\\0value`, count: 256 }, valid: true }",
            "Item { inner: Inner { name: `unseen\\0value`, count: 1 }, valid: true }tail",
            "Item { inner: Inner { name: `unseen\\0value`, count: 1 }, valid: true, valid: false }",
            "Item { inner: Inner { name: `unseen\\0value`, count: 1 } }",
            "Item { inner: Inner { name: `unseen\\0value`, count: 1 }, valid: wrong }"
        };
        for (size_t i = 0; i < sizeof(invalid) / sizeof(invalid[0]); ++i) {
            suz_text source = input(invalid[i]);
            uint64_t before = suz_text_pool_used();
            size_t calls_before = text_calls;
            for (unsigned repeat = 0; repeat < 100; ++repeat) assert(suz_fn_0(source) == fallback);
            assert(suz_text_pool_used() == before && text_calls == calls_before);
        }
        suz_text source = input("Item { inner: Inner { name: `a\\0b`, count: 1 }, valid: true }");
        suz_text parsed = suz_fn_0(source);
        assert(suz_text_length(parsed) == 3 && memcmp(suz_text_bytes(parsed), "a\0b", 3) == 0);
        uint64_t before = suz_text_pool_used();
        for (unsigned repeat = 0; repeat < 100; ++repeat) {
            assert(suz_fn_0(source) == parsed);
            assert(suz_text_pool_used() == before && text_live == before);
        }
        assert(suz_text_intern("a\0b", 3, true, input_location) == parsed);
        return 0;
    }
    suz_text source = input("Item { inner: Inner { name: `a\\0b`, count: 1 }, valid: true }");
    if (strcmp(argv[1], "probe") == 0) {
        printf("%llu\n", (unsigned long long)suz_text_pool_used());
        return 0;
    }
    if (strcmp(argv[1], "oom") == 0) {
        text_fail_after = text_calls + strtoull(argv[2], NULL, 10);
        text_expected_panic_calls = text_fail_after + 1;
    } else {
        text_expected_panic_calls = text_calls + 1;
    }
    suz_fn_0(source);
    return 99;
}
"#;

#[test]
fn struct_symbol_parsing_does_not_intern_failed_inputs_and_frees_temporary_storage() {
    let (_directory, path) = build_with_environment(SOURCE, DRIVER);
    let output = Command::new(&path).arg("validation").output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let probe = Command::new(&path).arg("probe").output().unwrap();
    assert!(probe.status.success());
    let limit = String::from_utf8(probe.stdout).unwrap();
    for (mode, argument) in [("oom", "0"), ("oom", "1"), ("limit", limit.trim())] {
        let output = Command::new(&path).args([mode, argument]).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(101),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let diagnostic = String::from_utf8(output.stderr).unwrap();
        assert!(
            diagnostic.starts_with("panic text.suz:4:22: "),
            "{diagnostic}"
        );
        assert!(
            diagnostic.ends_with(" [rt.symbol-allocation]\n"),
            "{diagnostic}"
        );
        if mode == "limit" {
            assert!(
                diagnostic.contains("text pool limit exceeded:"),
                "{diagnostic}"
            );
        } else {
            assert!(
                diagnostic.contains("symbol memory allocation failed"),
                "{diagnostic}"
            );
        }
    }
}

#[test]
fn constant_struct_symbol_results_keep_static_payloads_in_bindings_and_constants() {
    let content = "x".repeat(1000);
    let source = format!(
        r#"struct Label {{ name: symbol }}
fix ITEM = `Label {{ name: \`{content}\` }}` as Label or Label {{ name: `` }}
fn from_const() -> symbol {{ return ITEM.name }}
fn from_expression() -> symbol {{
    fix item = `Label {{ name: \`{content}\` }}` as Label or Label {{ name: `` }}
    return item.name
}}
fn main() -> u8 {{ return 0 }}
"#
    );
    let driver = r#"
int main(int argc, char **argv) {
    assert(suz_program() == 0);
    uint64_t used = suz_text_pool_used();
    assert(used < 1000); /* Static payloads are excluded from the pool limit. */
    if (argc == 1) {
        printf("%llu\n", (unsigned long long)used);
        return 0;
    }
    suz_text_pool_init(strtoull(argv[1], NULL, 10));
    text_fail_after = text_calls;
    suz_text first = suz_fn_0();
    assert(suz_text_length(first) == 1000);
    assert(suz_fn_1() == first);
    assert(suz_text_pool_used() == strtoull(argv[1], NULL, 10));
    return 0;
}
"#;
    let (_directory, path) = build_with_environment(&source, driver);
    let probe = Command::new(&path).output().unwrap();
    assert!(probe.status.success());
    let limit = String::from_utf8(probe.stdout).unwrap();
    let output = Command::new(&path).arg(limit.trim()).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
}
