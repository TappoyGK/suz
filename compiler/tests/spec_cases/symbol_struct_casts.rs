use crate::test_util::compile_and_run_c23;

#[test]
fn struct_symbol_casts_preserve_constants_scalar_types_and_lazy_evaluation() {
    let source = r#"enum Mode: u8 { First, Last }
struct Values { low: i8, high: u64, small: f32, wide: f64, flag: bool, mode: Mode, label: symbol }
const INPUT = `Values { low: -128, high: 18446744073709551615, small: 1.0000000596046447753906250000000001, wide: -0, flag: true, mode: Mode.Last, label: \`a\\0b\` }`
const FALLBACK = Values { low: 0, high: 0, small: 0.0, wide: 0.0, flag: false, mode: Mode.First, label: `` }
const SAVED = INPUT as Values or FALLBACK
const FAILED = `bad` as Values or SAVED
const SHORT = false && ((`bad` as Values or Values { low: 1 / (0 as u8), high: 0, small: 0.0, wide: 0.0, flag: false, mode: Mode.First, label: `` }).flag)
fn source(value: symbol) -> symbol { log info `source`
    return value
}
fn fallback() -> Values { log info `fallback`
    return FALLBACK
}
fn main() -> u8 {
    fix good = source(INPUT) as Values or fallback()
    fix bad = source(`bad`) as Values or fallback()
    if good != SAVED || FAILED != SAVED || bad != FALLBACK || SHORT { return 1 }
    if good.low != -128 || good.high != 18446744073709551615 { return 2 }
    if good.small != 1.00000011920928955078125 || 1.0 / good.wide != -inf { return 3 }
    if not good.flag || good.mode != Mode.Last || good.label != `a\0b` { return 4 }
    log info `${good.label}`
    return 0
}"#;
    let output = compile_and_run_c23(source, "text-struct.suz");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stderr, b"info text-struct.suz:8: source\ninfo text-struct.suz:8: source\ninfo text-struct.suz:11: fallback\ninfo text-struct.suz:21: a\0b\n");
}

#[test]
fn struct_symbol_casts_check_whole_input_layout_and_all_escape_forms() {
    let source = r#"enum Mode: u8 { First, Last }
struct Label { name: symbol, mode: Mode }
const DEFAULT = Label { name: `fallback`, mode: Mode.First }
struct Pair { left: Label, right: Label }
const PAIR_INPUT = `Pair { right: Label { mode: Mode.Last, name: \`right\` }, left: Label { name: \`left\`, mode: Mode.First } }`
const PAIR = PAIR_INPUT as Pair or Pair { left: DEFAULT, right: DEFAULT }
fn invalid(value: symbol) -> bool {
    return (value as Label or DEFAULT) == DEFAULT
}
fn main() -> u8 {
    if not invalid(` Label { name: \`ok\`, mode: Mode.Last }`) { return 1 }
    if not invalid(`Label { name: \`ok\`, mode: Mode.Last } `) { return 2 }
    if not invalid(`Label { name: \`ok\`, mode: Last }`) { return 3 }
    if not invalid(`Label { name: \`ok\`, mode: Other.Last }`) { return 4 }
    if not invalid(`Label { name: \`ok\`, mode: Mode.Unknown }`) { return 5 }
    if not invalid(`Label { name: \`ok\`, mode: Mode.Last // comment\n}`) { return 6 }
    if not invalid(`Label {\rname: \`ok\`, mode: Mode.Last,\r}`) { return 7 }
    if not invalid(`Label { name: \`raw\nline\`, mode: Mode.Last }`) { return 8 }
    if not invalid(`Label { name: \`raw\rline\`, mode: Mode.Last }`) { return 9 }
    fix valid = `Label { name: \`a\\n\\r\\t\\\\\\\`\\$\\0\`, mode: Mode.Last }` as Label or return 10
    if valid.name != `a\n\r\t\\\`$\0` { return 11 }
    fix literal = `Label { name: \`\\\${value}\`, mode: Mode.Last }` as Label or return 12
    if literal.name != `\${value}` { return 13 }
    fix separated = `Label\n{\nname\n:\n\`ok\`, mode: Mode\n.\nLast,\n}` as Label or return 14
    if separated.name != `ok` || separated.mode != Mode.Last { return 15 }
    fix input = PAIR_INPUT
    fix paired = input as Pair or return 16
    if paired != PAIR || paired.left.name != `left` || paired.right.name != `right` { return 17 }
    return 0
}"#;
    let output = compile_and_run_c23(source, "text-struct.suz");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn struct_symbol_casts_require_handlers_and_validate_constant_fallbacks() {
    for (body, rule) in [
        (
            "const V = `Item { value: 1 }` as Item\nfn main() -> u8 { return 0 }",
            "symbol.cast-handler",
        ),
        (
            "fn main() -> u8 { fix v = `Item { value: 1 }` as% Item or Item { value: 0 }\nreturn 0 }",
            "symbol.cast-wrap",
        ),
        (
            "fn main() -> u8 { fix v = `Item { value: 1 }` as Item or 0\nreturn 0 }",
            "type.expected",
        ),
        (
            "const V = `bad` as Item or Item { value: 1 / 0 }\nfn main() -> u8 { return 0 }",
            "arith.div-zero-always",
        ),
        (
            "fn main() -> u8 { fix v = expr { `bad` as Item or return 1 }\nreturn 0 }",
            "expr.jump-prohibited",
        ),
    ] {
        let source = format!("struct Item {{ value: u8 }}\n{body}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.message().contains(rule), "{source}: {error:?}");
    }
}
