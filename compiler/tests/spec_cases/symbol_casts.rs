use crate::integer_bounds::INTEGER_BOUNDS;
use crate::test_util::compile_and_run_c11;

fn check_program(source: &str) {
    let result = compile_and_run_c11(source, "text-casts.suz");
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
}

#[test]
fn symbol_integer_casts_check_every_width_in_constants_and_at_runtime() {
    let mut source = String::new();
    let mut body = String::from("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        for (index, value) in [min - 1, min, 0, max, max + 1].into_iter().enumerate() {
            let expected = if (min..=max).contains(&value) {
                value
            } else {
                7
            };
            let wrapped = (value - min).rem_euclid(max - min + 1) + min;
            for (mode, operator, expected) in
                [("checked", "as", expected), ("wrapped", "as%", wrapped)]
            {
                let name = format!("{ty}_{mode}_{index}").to_ascii_uppercase();
                source.push_str(&format!("const {name} = `{value}` {operator} {ty} or 7\n"));
                body.push_str(&format!("{{\nfix input = `{value}`\nfix actual = input {operator} {ty} or 7\nif actual != {expected} || {name} != {expected} {{ return 1 }}\n}}\n"));
            }
        }
    }
    body.push_str("return 0\n}\n");
    source.push_str(&body);
    check_program(&source);
}

#[test]
fn symbol_casts_reject_incomplete_formats_and_preserve_lazy_constant_evaluation() {
    let invalid = [
        "",
        "-",
        "+1",
        " 1",
        "1 ",
        "1 0",
        "1\\0",
        "1.0",
        "1e2",
        "0x10",
        "_1",
        "1_",
        "1__0",
        "-_1",
        "--1",
        "(1)",
        "1+1",
        "1//comment",
        "１",
    ];
    let mut source = String::from(
        r#"const GOOD = `1` as u8 or (1 / 0)
const BAD = `bad` as u8 or 7
const WRAPPED = `-1` as% u8 or (1 / 0)
const YES = `true` as bool or false
const NO = `false` as bool or true
fn main() -> u8 {
    if GOOD != 1 || BAD != 7 || WRAPPED != 255 || not YES || NO { return 1 }
    fix separated = `0_0_1_2` as u8 or 7
    fix minus_zero = `-0` as u8 or 7
    fix signed_zero = `-0` as i8 or 7
    if separated != 12 || minus_zero != 7 || signed_zero != 0 { return 2 }
"#,
    );
    for input in invalid {
        for operator in ["as", "as%"] {
            source.push_str(&format!("{{\nfix input = `{input}`\nfix result = input {operator} i64 or 7\nif result != 7 {{ return 3 }}\n}}\n"));
        }
    }
    for input in ["true ", "True", "TRUE", "1", "0", "false\\0", "", " false"] {
        source.push_str(&format!("{{\nfix input = `{input}`\nfix a = input as bool or false\nfix b = input as bool or true\nif a || not b {{ return 4 }}\n}}\n"));
    }
    // More digits than any machine integer, with a result known independently:
    // 10^128 is divisible by 2^64, so the last digit is the entire residue.
    let large = format!("1{}1", "0".repeat(127));
    source.push_str(&format!("fix large = `{large}` as% u64 or 7\nfix negative = `-{large}` as% i64 or 7\nif large != 1 || negative != -1 {{ return 5 }}\n"));
    source.push_str("return 0\n}\n");
    check_program(&source);
}

#[test]
fn symbol_cast_handlers_obey_type_and_control_constraints() {
    for (source, rule) in [
        (
            "fn main() -> u8 { fix n = 1 as% u8 or 0\nreturn n }",
            "expr.cast-wrap-handler-syntax",
        ),
        (
            "fn main() -> u8 { fix n = `1` as u8 or false\nreturn n }",
            "type.expected",
        ),
        (
            "fn main() -> u8 { fix n = 1 as bool or false\nreturn 0 }",
            "expr.as",
        ),
        (
            "fn main() -> u8 { fix n = `1` as text or ``\nreturn 0 }",
            "symbol.cast",
        ),
        (
            "fn main() -> u8 { fix n = (`1` as% u8 or return 1) + 1\nreturn n }",
            "expr.or-return-position",
        ),
        (
            "fn main() -> u8 { fix n = expr { `1` as% u8 or return 1 }\nreturn n }",
            "expr.jump-prohibited",
        ),
        (
            "const N = `bad` as% u8 or (1 / 0)\nfn main() -> u8 { return N }",
            "const.evaluation",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.message.contains(rule), "{source}: {error:?}");
    }
    check_program(
        r#"fn parse(value: symbol) -> bool {
    fix result = value as bool or return false
    return result
}
fn accumulate(value: symbol) -> u8 {
    var count: u8 = 2
    count += value as% u8 or return 7
    return count
}
fn main() -> u8 {
    if not parse(`true`) || parse(`false`) || parse(`bad`) { return 1 }
    if accumulate(`257`) != 3 || accumulate(`bad`) != 7 { return 2 }
    return 0
}"#,
    );
}
