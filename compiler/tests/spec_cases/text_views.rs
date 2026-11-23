use super::symbol_runtime::build_with_environment_optimized;
use std::process::Command;

#[test]
fn views_reject_missing_handlers_wrong_types_and_hidden_returns() {
    for (body, rule) in [
        ("fix x = `abc`.[0]", "text.slice-handler"),
        ("fix x = `abc`.[:2]", "text.slice-handler"),
        ("fix x = `abc`.[:] or ``", "text.slice-handler"),
        ("fix x = `abc`.[true] or '?'", "text.slice-bounds"),
        ("fix x = `abc`.[0.0:] or ``", "text.slice-bounds"),
        ("fix x = `abc`.[0] or ``", "type.expected"),
        ("fix x = `abc`.[:1] or 'a'", "type.expected"),
        ("fix x: symbol = `abc`.[:]", "type.expected"),
        ("fix x = `abc`.[:] as symbol or ``", "text.to-symbol"),
        ("fix x = `abc`.[:] as% symbol or ``", "symbol.cast-wrap"),
        (
            "fix x = (`abc`.[:1] or return 1).[0] or '?'",
            "expr.or-return-position",
        ),
        (
            "fix x = expr { fix v = `abc`.[0] or return 1\nv }",
            "expr.jump-prohibited",
        ),
        (
            "fix x = `abc`.[(`a`.[0] or return 1) as u32] or '?'",
            "expr.or-return-position",
        ),
        (
            "var v: text = `abc`\nfor ch in v.[:] {}",
            "for.bound-reference",
        ),
        ("fix x = `abc`.[0..<1] or ``", "text.slice-omit"),
        ("fix x = `abc`.[..=1] or ``", "text.slice-omit"),
        ("fix x = `abc`.[:1..=2] or ``", "text.slice-omit"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message().contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
}

#[test]
fn views_preserve_ranges_without_allocations_and_reintern_only_explicitly() {
    let source = r#"struct Pair { first: text, second: text }
fn scan(input: text) -> u32 {
    fix body = input.[1:5] or return 100
    if body != `あ😀\0z` { return 101 }
    if (body.[0] or '?') != 'あ' { return 102 }
    if (body.[1] or '?') != '😀' { return 103 }
    if (body.[2] or '?') != '\0' { return 104 }
    if (body.[3] or '?') != 'z' { return 105 }
    if (body.[4] or '?') != '?' { return 106 }
    if (body.[-1] or '?') != '?' { return 107 }
    fix maximum: u64 = 18446744073709551615
    if (body.[maximum] or '?') != '?' { return 108 }
    if (body.[4:] or `bad`) != `` { return 109 }
    if (body.[3:2] or `bad`) != `bad` { return 110 }
    if (body.[-1:] or `bad`) != `bad` { return 111 }
    if (body.[:5] or `bad`) != `bad` { return 112 }
    if ((input.[1:] or ``).[0] or '?') != 'あ' { return 113 }
    fix copy = Pair { first: body, second: body.[:] }
    if copy != (Pair { first: `あ😀\0z`, second: body }) { return 114 }
    var count: u32 = 0
    for ch in copy.first { count += 1 }
    rev ch in copy.second { count += 1 }
    fix joined = ifexpr true { `default` } else { body }
    fix reverse_join = ifexpr false { body } else { `default` }
    if joined != reverse_join { return 115 }
    match body { `あ😀\0z` => { count += 1 }
        _ => { return 116 } }
    log info `${body}`
    return count
}
fn parse(input: text) -> u8 {
    fix number = input.[1:3] or return 100
    return number as u8 or 101
}
fn intern(input: text) -> symbol { return input as symbol }
fn main() -> u8 { return 0 }
"#;
    let driver = r#"
 #include <string.h>
int main(void) {
    assert(suz_program() == 0);
    suz_text_location location = {"test", 1, 1};
    suz_text source = suz_text_intern("x\xe3\x81\x82\xf0\x9f\x98\x80\0zy", 11, false, location);
    suz_text digits = suz_text_intern("x42y", 4, false, location);
    size_t calls = text_calls;
    uint64_t used = suz_symbol_pool_used();
    text_fail_after = calls;
    assert(suz_fn_0(suz_text_as_view(source)) == 9);
    assert(suz_fn_1(suz_text_as_view(digits)) == 42);
    assert(text_calls == calls && suz_symbol_pool_used() == used);
    text_fail_after = SIZE_MAX;
    suz_text_view view;
    assert(suz_text_slice(suz_text_as_view(digits), 1, 3, &view));
    suz_text first = suz_fn_2(view);
    assert(text_calls > calls);
    calls = text_calls;
    assert(suz_fn_2(view) == first);
    assert(suz_text_intern("42", 2, true, location) == first);
    assert(text_calls == calls);
    return 0;
}
"#;
    for optimized in [false, true]
        .into_iter()
        .filter(|v| !v || !cfg!(debug_assertions))
    {
        let (_dir, executable) = build_with_environment_optimized(source, driver, optimized);
        let output = Command::new(executable).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn view_slicing_matches_unicode_boundaries_for_every_pair_of_positions() {
    let source = r#"fn slice(value: text, start: i64, end: i64) -> text {
    return value.[start:end] or `invalid`
}
fn at(value: text, pos: i64) -> char { return value.[pos] or '?' }
fn equal(left: text, right: text) -> bool { return left == right && not (left != right) }
fn main() -> u8 { return 0 }
"#;
    let mut driver = String::from(
        r#"
 #include <string.h>
int main(void) {
    assert(suz_program() == 0);
    suz_text_location location = {"test", 1, 1};
"#,
    );
    for sample in ["", "abc\0z", "aéあ😀\0z"] {
        let scalars: Vec<char> = sample.chars().collect();
        let bytes: String = sample.bytes().map(|byte| format!("\\{byte:03o}")).collect();
        driver.push_str(&format!(
            "{{ suz_text value = suz_text_intern(\"{bytes}\", {}, true, location);\n",
            sample.len()
        ));
        driver.push_str("suz_text_view view = suz_text_as_view(value); size_t calls = text_calls; text_fail_after = calls;\n");
        for start in -1..=scalars.len() as i64 + 1 {
            for end in -1..=scalars.len() as i64 + 1 {
                let expected: String = if 0 <= start && start <= end && end <= scalars.len() as i64
                {
                    scalars[start as usize..end as usize].iter().collect()
                } else {
                    "invalid".into()
                };
                let bytes: String = expected
                    .bytes()
                    .map(|byte| format!("\\{byte:03o}"))
                    .collect();
                let length = expected.len();
                let count = expected.chars().count();
                let same = u8::from(expected == sample);
                driver.push_str(&format!(
                    r#"{{
    suz_text_view result = suz_fn_0(view, {start}, {end});
    suz_text_view expected = {{"{bytes}", {length}, {count}, NULL}};
    assert(result.length == {length});
    assert(result.scalars == {count});
    assert(memcmp(result.bytes, "{bytes}", {length}) == 0);
    assert(suz_fn_2(result, expected));
    assert(suz_fn_2(result, view) == {same});
}}
"#
                ));
            }
            let expected = usize::try_from(start)
                .ok()
                .and_then(|i| scalars.get(i))
                .copied()
                .unwrap_or('?');
            driver.push_str(&format!(
                "assert(suz_fn_1(view, {start}) == {});\n",
                expected as u32
            ));
        }
        driver.push_str("assert(text_calls == calls); text_fail_after = SIZE_MAX; }\n");
    }
    driver.push_str("return 0; }\n");
    for optimized in [false, true]
        .into_iter()
        .filter(|v| !v || !cfg!(debug_assertions))
    {
        let (_dir, executable) = build_with_environment_optimized(source, &driver, optimized);
        let output = Command::new(executable).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn constant_views_preserve_diagnostics_and_lazy_failure_evaluation() {
    let source = "fn main() -> u8 { fix value = (`abc`.[:1 + 1] or ``) as symbol\nreturn 0 }";
    let compiled =
        suz::compile_with_source_name(source.as_bytes(), suz::ArithmeticMode::Warn, "view.suz")
            .unwrap();
    assert_eq!(compiled.warnings.len(), 1);
    assert_eq!(compiled.warnings[0].column, source.find('+').unwrap() + 1);
    let function = compiled.c.split("int main(void)").next().unwrap();
    assert!(!function.contains("suz_text_intern("));
    for source in [
        "fix PART = `abc`.[:1] or (`x`.[1 / (0 as u8):] or ``)\nfn main() -> u8 { return 0 }",
        "fix PART = `abc`.[-1:] or `fallback`\nfn main() -> u8 { return 0 }",
    ] {
        suz::compile(source.as_bytes()).unwrap();
    }
    let error = suz::compile(
        b"fix PART = `abc`.[-1:] or (`x`.[1 / (0 as u8):] or ``)\nfn main() -> u8 { return 0 }",
    )
    .unwrap_err();
    assert!(error.message().contains("[const.evaluation]"));
}

#[test]
fn conversion_diagnostics_identify_the_source_type() {
    for (value, ty) in [("S", "symbol"), ("`abc`.[:]", "text")] {
        for (conversion, message) in [
            (
                "as u8",
                format!("{ty} conversion requires or [symbol.cast-handler]"),
            ),
            (
                "as% bool or false",
                format!("{ty} as% requires an integer target [symbol.cast-wrap]"),
            ),
            (
                "as symbol",
                format!("unsupported {ty} conversion target [symbol.cast]"),
            ),
        ] {
            if ty == "text" && conversion == "as symbol" {
                continue;
            }
            let source = format!(
                "fix S: symbol = `abc`\nfn main() -> u8 {{ fix value = ({value}) {conversion}\nreturn 0 }}"
            );
            assert_eq!(
                suz::compile(source.as_bytes()).unwrap_err().message(),
                message
            );
        }
    }
}
