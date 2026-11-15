use super::text_runtime::build_with_environment_optimized;
use std::process::Command;

#[test]
fn character_literals_and_conversions_reject_invalid_values_and_operations() {
    for literal in [
        "''",
        "'ab'",
        "'é'",
        "'\n'",
        "'\\q'",
        "'\\u{}'",
        "'\\u{d800}'",
        "'\\u{DFFF}'",
        "'\\u{110000}'",
        "'\\u{0000000}'",
        "'\\u{xyz}'",
        "'\\u{1234'",
        "'\\",
        "'",
        "'a",
    ] {
        let source = format!("fn main() -> u8 {{ fix ch = {literal}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message.contains("[char.literal]"),
            "{source}: {error}"
        );
    }
    for (body, rule) in [
        ("fix x = 'a' + 'b'", "expr.operator-types"),
        ("fix x = 'a' << 1", "expr.operator-types"),
        ("fix x = ~'a'", "expr.operator-types"),
        ("fix x = 'a' as f32", "expr.as"),
        ("fix x = true as char", "expr.as"),
        ("fix x = 'a' as% u32", "expr.as-trunc"),
        ("fix x = 55296 as char", "const.cast-check"),
        ("fix x = -1 as char", "const.cast-check"),
        ("fix n: u32 = 65\nfix x = n as char", "expr.cast-handler"),
        ("fix x = `a` as char", "text.cast-handler"),
        (
            "var message = `a`\nfor ch in message {}",
            "for.bound-reference",
        ),
        (
            "var message = `a`\nrev ch in `${message}` {}",
            "for.bound-reference",
        ),
        ("for ch in 12 {}", "char.for-type"),
        ("for ch in 'a'..<100 {}", "type.expected"),
        ("for ch in `a` { ch = 'b' }", "for.variable"),
        ("fix x = matchexpr 'a' { 'a' => 1 }", "match.exhaustive"),
        (
            "fix x = matchexpr 'a' { 'a' => 1\n'\\u{61}' => 2\n_ => 0 }",
            "match.pattern",
        ),
        ("fix x: char = 65", "type.expected"),
        (
            "fix x = expr { rev ch in `a` { continue }\n0 }",
            "expr.jump-prohibited",
        ),
        ("fix char = 1", "src.ident"),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message.contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
}

#[test]
fn character_constants_match_runtime_casts_and_struct_parsing() {
    let mut constants = String::new();
    let mut source = String::from(
        r#"struct Item { ch: char }
fn integer(n: i64) -> char { return n as char or '?' }
fn parse_text(t: text) -> char { return t as char or '?' }
fn structure(t: text) -> Item { return t as Item or Item { ch: '?' } }
fn main() -> u8 {
    fix byte: u8 = 65
    if (byte as char) != 'A' { return 1 }
"#,
    );
    for value in [
        0, 0x7f, 0x80, 0x7ff, 0x800, 0xd7ff, 0xe000, 0xffff, 0x10000, 0x10ffff,
    ] {
        constants.push_str(&format!(
            r#"const CH{value} = '\u{{{value:X}}}'
const PARSED{value} = `${{CH{value}}}` as char or '?'
const ITEM{value} = `Item {{ ch: '\\u{{{value:x}}}' }}` as Item or Item {{ ch: '?' }}
"#
        ));
        source.push_str(&format!(
            r#"{{
    if (CH{value} as u32) != {value} {{ return 2 }}
    if integer({value}) != CH{value} {{ return 3 }}
    if parse_text(`${{CH{value}}}`) != CH{value} {{ return 4 }}
    if PARSED{value} != CH{value} {{ return 5 }}
    fix input = `Item {{ ch: '\\u{{{value:x}}}' }}`
    if structure(input) != ITEM{value} || ITEM{value}.ch != CH{value} {{ return 6 }}
}}
"#
        ));
    }
    for value in [-1, 0xd800, 0xdfff, 0x110000, i64::MAX] {
        source.push_str(&format!(
            r#"{{
    fix ch = {value} as char or '?'
    if ch != '?' || integer({value}) != ch {{ return 7 }}
}}
"#
        ));
    }
    source.push_str(
        r#"
    if parse_text(``) != '?' || parse_text(`ab`) != '?' || parse_text(`é`) != '?' { return 8 }
    if structure(`Item { ch: '\\u{D800}' }`).ch != '?' { return 9 }
    if ('あ' as u8 or 3) != 3 { return 10 }
    if ('0' <= '5' <= '9') == false { return 11 }
    return 0
}
"#,
    );
    source = constants + &source;
    let driver = "int main(void) { return suz_program(); }";
    for optimized in [false, true]
        .into_iter()
        .filter(|v| !v || !cfg!(debug_assertions))
    {
        let (_dir, executable) = build_with_environment_optimized(&source, driver, optimized);
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
fn text_iteration_and_character_logs_allocate_nothing_after_startup() {
    let source = r#"fn scan(t: text) -> u32 {
    var count: u32 = 0
    for ch in t {
        log info `${ch}`
        count += 1
        continue
    }
    rev ch in t {
        log info `${ch}`
        count += 1
        continue
    }
    return count
}
fn ranges() -> u32 {
    var count: u32 = 0
    for ch in '\u{D7FF}'..='\u{E000}' { count += 1 }
    rev ch in '\u{D7FF}'..='\u{E000}' { count += 1 }
    for ch in '\u{10FFFF}'..='\u{10FFFF}' { count += 1
continue }
    rev ch in '\0'..='\0' { count += 1
continue }
    rev ch in '\u{D7FF}'..<'\u{E000}' { count += 1
continue }
    rev ch in '\0'..<'\0' { return 100 }
    for ch in 'z'..<'a' { return 101 }
    return count
}
const SAMPLE = `aあ😀\0`
const EMPTY = ``
fn main() -> u8 { return 0 }
"#;
    let driver = r#"
int main(void) {
    assert(suz_program() == 0);
    suz_text_location location = {"test", 1, 1};
    suz_text text = suz_text_intern("a\xe3\x81\x82\xf0\x9f\x98\x80\0", 9, true, location);
    suz_text empty = suz_text_intern("", 0, true, location);
    size_t calls = text_calls;
    uint64_t used = suz_text_pool_used();
    text_fail_after = calls;
    assert(suz_fn_0(text) == 8);
    assert(suz_fn_0(empty) == 0);
    assert(suz_fn_1() == 7);
    assert(text_calls == calls && suz_text_pool_used() == used);
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
        assert!(output.stderr.windows(4).any(|w| w == "😀".as_bytes()));
        assert!(output.stderr.contains(&0));
    }
}

#[test]
fn unicode_escape_diagnostics_identify_the_failure_in_each_expression_context() {
    for (literal, reason) in [
        (
            r"'\u{D800}'",
            "Unicode escape cannot represent a surrogate (U+D800..U+DFFF)",
        ),
        (r"'\u{110000}'", "Unicode escape exceeds U+10FFFF"),
        (
            r"'\u{0000041}'",
            "Unicode escape accepts at most six hexadecimal digits",
        ),
        (
            r"'\u{}'",
            "Unicode escape requires at least one hexadecimal digit",
        ),
    ] {
        for (prefix, suffix) in [("    fix ch = ", ""), ("    log info `${", "}`")] {
            let source = format!("fn main() -> u8 {{\n{prefix}{literal}{suffix}\nreturn 0 }}");
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(
                error.message,
                format!("{reason} [char.literal]"),
                "{source}"
            );
            assert_eq!(
                (error.line, error.column),
                (2, prefix.len() + 1),
                "{source}"
            );
        }
    }
}

#[test]
fn mutable_loop_inputs_have_a_diagnostic_shared_by_text_and_ranges() {
    for keyword in ["for", "rev"] {
        for (declaration, input) in [
            ("var value = `abc`", "value"),
            ("var value = 3", "0..<value"),
        ] {
            let source = format!(
                "fn main() -> u8 {{\n{declaration}\n{keyword} ch in {input} {{}}\nreturn 0 }}"
            );
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(
                error.message,
                "loop inputs cannot reference var bindings [for.bound-reference]"
            );
            assert_eq!(error.line, 3);
            assert_eq!(error.column, if input == "value" { 11 } else { 15 });
        }
    }
}
