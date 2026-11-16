use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
fn length_values_casts_and_mixed_positions_execute() {
    let source = r#"
struct Size { count: len_t }
const PARSED = `2147483647` as len_t or 0
const REJECTED = `2147483648` as len_t or 7
const COUNT = len `あ😀x`
const VIEW = len (`あ😀x`[1:] or ``)
fn identity(x: int) -> int { return x }
fn size(t: text) -> len_t { return len t }
fn parse(t: text) -> len_t { return t as len_t or 7 }
fn parse_size(t: text) -> Size { return t as Size or Size { count: 7 } }
fn narrow(x: u64) -> len_t { return x as len_t or 7 }
fn convert(x: f64) -> len_t { return x as len_t or 7 }
fn produce() -> symbol { log info `once`
return `あ😀` }
fn main() -> u8 {
    if len produce() != 2 { return 17 }
    var counter: len_t = 0
    for i in 0..<COUNT { counter += 1 }
    rev i in 0..<COUNT { counter += 1 }
    if counter != 6 { return 18 }
    if COUNT != 3 || VIEW != 2 { return 1 }
    fix text_value = `あ😀x`
    fix n = size(text_value)
    fix a: i32 = n
    fix b: u32 = n
    fix c: i64 = n
    fix d: u64 = n
    fix e: f64 = n
    fix small: u8 = 1
    fix medium: u16 = 2
    fix l: len_t = small
    fix m: len_t = medium
    if a != 3 || b != 3 || c != 3 || d != 3 || e != 3.0 { return 2 }
    if identity(n) != 3 || l + m != 3 { return 3 }
    fix start: u8 = 1
    fix end: i64 = 3
    if (text_value[start:end] or ``) != `😀x` { return 4 }
    fix max: u64 = 18446744073709551615
    if (text_value[start:max] or `bad`) != `bad` { return 5 }
    fix wide_start: u64 = 0
    if (text_value[wide_start:18446744073709551615] or `bad`) != `bad` { return 24 }
    if (text_value[-1:end] or `bad`) != `bad` { return 6 }
    if len text_value - 1 != 2 || (len text_value[1:] or ``) != 2 { return 7 }
    if len `` != 0 || len `é` != 2 { return 8 }
    fix hi: len_t = 2147483647
    fix zero: len_t = 0
    fix overflow: len_t = hi + 1 or 9
    fix underflow: len_t = zero - 1 or 9
    fix negative: len_t = -n or 9
    if overflow != 9 || underflow != 9 || negative != 9 { return 9 }
    fix shift: u32 = 1 << n
    if shift != 8 { return 10 }
    if parse(`2147483647`) != hi || parse(`2147483648`) != 7 { return 11 }
    if parse(`-0`) != 7 || parse(`-1`) != 7 { return 12 }
    if convert(2147483647.9) != hi || convert(2147483648.0) != 7 { return 13 }
    if convert(-0.5) != 0 || convert(-1.0) != 7 { return 14 }
    fix joined = n + small
    fix check_join: len_t = joined
    fix signed: i8 = -1
    fix signed_join = n + signed
    fix check_signed: i32 = signed_join
    if check_join != 4 || check_signed != 2 { return 15 }
    if `${n}` != `3` { return 16 }
    if PARSED != hi || REJECTED != 7 { return 19 }
    if narrow(2147483647) != hi || narrow(2147483648) != 7 { return 20 }
    if parse_size(`Size { count: 2147483647 }`).count != hi { return 21 }
    if parse_size(`Size { count: 2147483648 }`).count != 7 { return 22 }
    fix bits = n as% u8
    if bits != 3 { return 23 }
    return 0
}
"#;
    let c = suz::compile(source.as_bytes()).unwrap();
    assert!(c.contains("int32_t suz_fn_"));
    assert!(c.contains(".scalars"));
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("length.c");
    fs::write(&path, c).unwrap();
    let result = build_and_run_c(&path, &directory.0.join("length"));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).ends_with(": once\n"));
    assert_eq!(String::from_utf8_lossy(&result.stderr).lines().count(), 1);
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn length_rejects_width_operations_and_invalid_targets() {
    for body in [
        "fix x: len_t = 2147483648",
        "fix x: len_t = -1",
        "fix x: f32 = n",
        "fix x = n & n",
        "fix x = n | n",
        "fix x = n ^ n",
        "fix x = ~n",
        "fix x = n +% n",
        "fix x = n -% n",
        "fix x = n *% n",
        "fix x = n << 1",
        "fix x = n >> 1",
        "fix x = 1 as% len_t",
        "fix x = `1` as% len_t or 0",
        "fix x = len 1",
        "fix len = 1",
        "fix len_t = 1",
    ] {
        let source = format!("fn main() -> u8 {{ fix n: len_t = 2\n{body}\nreturn 0 }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
    assert!(suz::compile(b"enum Count: len_t { One }\nfn main() -> u8 { return 0 }").is_err());
}

#[test]
fn length_generic_width_failures_are_deferred_to_the_call() {
    for expr in [
        "x & 1",
        "x | 1",
        "x ^ 1",
        "~x",
        "x +% 1",
        "x -% 1",
        "x *% 1",
        "x << 1",
        "x >> 1",
        "1 as% int",
        "`1` as% int or 0",
    ] {
        let prefix = format!("fn operation(x: int) -> int {{ return {expr} }}\n");
        let good =
            format!("{prefix}fn main() -> u8 {{ fix x: u32 = 2\nfix y = operation(x)\nreturn 0 }}");
        suz::compile(good.as_bytes()).unwrap_or_else(|e| panic!("{expr}: {e}"));
        let bad = format!(
            "{prefix}fn main() -> u8 {{ fix x: len_t = 2\nfix y = operation(x)\nreturn 0 }}"
        );
        let error = suz::compile(bad.as_bytes()).unwrap_err();
        assert!(error.message().contains("[num.fn-body]"), "{expr}: {error}");
    }
}
