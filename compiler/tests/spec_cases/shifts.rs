use crate::integer_bounds::INTEGER_BOUNDS;
use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn shifts_keep_left_types_and_accept_every_integer_count_type() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (left, min, max) in INTEGER_BOUNDS {
        let width = (max - min + 1).ilog2();
        for (right, _, count_max) in INTEGER_BOUNDS {
            source.push_str(&format!("{{\nvar value: {left} = 3\nfix count: {right} = 1\nfix huge: {right} = {count_max}\nif value << count != 6 || value >> count != 1 {{ return 1 }}\nvalue <<= count\nvalue >>= count\nif value != 3 || (value << huge or 7) != 7 || (value >> huge or 7) != 7 {{ return 2 }}\n}}\n"));
        }
        source.push_str(&format!("{{\nfix lo: {left} = {min}\nfix hi: {left} = {max}\nif lo >> 0 != lo || hi << 0 != hi {{ return 3 }}\nif (hi << 1 or 7) != 7 || (hi >> {width} or 7) != 7 {{ return 4 }}\n"));
        if min < 0 {
            source.push_str(&format!("fix negative: {left} = -7\nfix minus_two: {left} = -2\nif negative >> 1 != -4 || lo >> {} != -1 {{ return 5 }}\nif (-1 << {}) != lo || (minus_two << {} or 7) != 7 {{ return 6 }}\n", width - 1, width - 1, width - 1));
        } else {
            source.push_str(&format!(
                "fix one: {left} = 1\nif hi >> {} != 1 || (one << {}) != {} {{ return 7 }}\n",
                width - 1,
                width - 1,
                1_i128 << (width - 1)
            ));
        }
        source.push_str("}\n");
    }
    source.push_str(
        r#"fix count: u8 = 8
    fix inferred = 1 << count
    if inferred != 256 { return 8 }
    fix independent: u8 = 1 << 256 or 7
    if independent != 7 { return 9 }
    if 1 << 2 + 1 != 8 || 1 | 2 << 2 != 9 || 1 << 3 >> 2 != 2 { return 10 }
    if 1 + 1 << 2 != 8 || 8 >> 1 + 1 != 2 || 2 * 3 << 1 != 12 { return 11 }
    return 0
}
"#,
    );
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("shifts.c");
    fs::write(&path, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let output = build_and_run_c(&path, &directory.0.join("shifts"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn shifts_check_types_diagnostics_and_constant_evaluation() {
    for body in [
        "fix value = 1 << 1.0\nreturn 0",
        "fix value = 1.0 >> 1\nreturn 0",
        "fix value = true << 1\nreturn 0",
        "fix value = 1 >> false\nreturn 0",
        "fix count: f32 = 1.0\nfix value = 1 << count\nreturn 0",
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().contains("expr.operator-types"), "{error:?}");
    }
    for declaration in [
        "fix VALUE: u8 = 1 << 8",
        "fix VALUE: i64 = 1 << 63",
        "fix VALUE: u64 = 1 >> 64",
        "fix VALUE: i8 = 1 >> -1",
        "fix VALUE: i8 = -2 << 7",
        "fix VALUE: u8 = ((1 << 8) >> 1) or 7",
    ] {
        let source = format!("{declaration}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message().contains("const.evaluation"),
            "{source}: {error:?}"
        );
    }
    let source = b"fn main() -> u8 {\nvar value: u8 = 1\nvalue <<= 1\nfix next = value >> 1 or 7\nreturn next\n}";
    let warnings = suz::compile_with_mode(source, suz::ArithmeticMode::Warn)
        .unwrap()
        .warnings;
    assert_eq!(
        warnings
            .iter()
            .map(|w| (w.line, w.column))
            .collect::<Vec<_>>(),
        [(3, 7)]
    );
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert_eq!((error.line, error.column), (3, 7));
    let source = br#"fix COUNT: u8 = 8
fix INFERRED = 1 << COUNT
fix MINIMUM: i64 = -1 << 63
fix SIGN_FILL: i64 = MINIMUM >> 63
fix LARGEST_BIT: u64 = 1 << 63
fix FALLBACK: u8 = 1 << 256 or 7
fix LAZY: u8 = 1 << 1 or (1 / (0 as u8))
fix SHORT = true || (1 << 64 == 0)
fn main() -> u8 {
    return ifexpr INFERRED == 256 && SIGN_FILL == -1 && LARGEST_BIT == 9223372036854775808 && FALLBACK == 7 && LAZY == 2 && SHORT { 0 } else { 1 }
}
"#;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("constant-shifts.c");
    fs::write(&path, suz::compile(source).unwrap()).unwrap();
    let output = build_and_run_c(&path, &directory.0.join("constant-shifts"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn logical_and_wrapping_shifts_work_at_runtime_and_compile_time() {
    // 論理右シフト >>>（検査あり）と、マスク・パニックなしの <<%・>>%・>>>%（および複合代入）
    let source = br#"fix C_LSHR_I8: i8 = (-2 as i8) >>> 1
fix C_LSHR_I8_ZERO: i8 = (-128 as i8) >>> 0
fix C_LSHR_U8: u8 = (240 as u8) >>> 4
fix C_WSHL_U8: u8 = (129 as u8) <<% 1
fix C_WSHL_I8: i8 = (64 as i8) <<% 1
fix C_WSHL_MASK: u8 = (1 as u8) <<% 9
fix C_WSHL_NEG: u8 = (1 as u8) <<% -1
fix C_WSHR_I8: i8 = (-7 as i8) >>% 1
fix C_WSHR_I8_MASK: i8 = (-7 as i8) >>% 9
fix C_WSHR_U8: u8 = (240 as u8) >>% 4
fix C_WLSHR_I8: i8 = (-2 as i8) >>>% 1
fix C_WLSHR_I8_MASK: i8 = (-2 as i8) >>>% 9
fix C_WLSHR_I8_ZERO: i8 = (-128 as i8) >>>% 8
fix C_WLSHR_I64: i64 = (-1 as i64) >>>% 63

fn main() -> u8 {
    if C_LSHR_I8 != 127 || C_LSHR_I8_ZERO != -128 || C_LSHR_U8 != 15 { return 1 }
    if C_WSHL_U8 != 2 || C_WSHL_I8 != -128 || C_WSHL_MASK != 2 || C_WSHL_NEG != 128 { return 2 }
    if C_WSHR_I8 != -4 || C_WSHR_I8_MASK != -4 || C_WSHR_U8 != 15 { return 3 }
    if C_WLSHR_I8 != 127 || C_WLSHR_I8_MASK != 127 || C_WLSHR_I8_ZERO != -128 || C_WLSHR_I64 != 1 { return 4 }

    var s8: i8 = -2
    if (s8 >>> 1) != 127 || (s8 >>> 0) != -2 || (s8 >>> 8 or 7) != 7 || (s8 >>> -1 or 7) != 7 { return 5 }
    s8 >>>= 1
    if s8 != 127 { return 6 }

    var u: u8 = 129
    u <<%= 9
    if u != 2 { return 7 }
    u <<%= -1
    if u != 0 { return 8 }

    var neg: i8 = -128
    if (neg >>% 0) != -128 || (neg >>% 8) != -128 || (neg >>% 1) != -64 || (neg >>% 7) != -1 { return 9 }
    neg >>%= 1
    if neg != -64 { return 10 }

    var lneg: i8 = -2
    lneg >>>%= 9
    if lneg != 127 { return 11 }

    fix big: i64 = -1
    if (big >>> 63) != 1 || (big >>>% 63) != 1 || (big >>>% 127) != 1 { return 12 }
    if (big <<% 63) != (-1 << 63) { return 13 }

    // exact arithmetic expression as shift count
    fix a: i32 = 10
    fix b: i32 = 1
    fix exact_wshl: u8 = (1 as u8) <<% (a - b)
    if exact_wshl != 2 { return 14 }
    fix exact_wlshr: i8 = (-2 as i8) >>>% (a - b)
    if exact_wlshr != 127 { return 15 }

    return 0
}
"#;
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("extended-shifts.c");
    fs::write(&path, suz::compile(source).unwrap()).unwrap();
    let output = build_and_run_c(&path, &directory.0.join("extended-shifts"));
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());

    // % 付きシフトは --deny-unhandled-arithmetic 下でも or 不要
    let no_warn = br#"fn main() -> u8 {
    var x: u32 = 1
    x <<%= 13
    x >>%= 17
    x >>>%= 5
    fix y: i32 = 0 -% 2
    fix z = (y <<% 1) +% (y >>% 1) +% (y >>>% 1)
    return ifexpr x == 0 && z != 0 { 0 } else { 1 }
}
"#;
    suz::compile_with_mode(no_warn, suz::ArithmeticMode::Deny).unwrap();

    // >>> は --deny-unhandled-arithmetic の診断対象
    let warn_lshr =
        b"fn main() -> u8 {\n    fix x: i8 = -2\n    fix y = x >>> 1\n    return 0\n}\n";
    assert!(suz::compile_with_mode(warn_lshr, suz::ArithmeticMode::Deny).is_err());

    // % 付きシフトへの or 付与はコンパイルエラー
    for expr in ["(x <<% 1) or 0", "(x >>% 1) or 0", "(x >>>% 1) or 0"] {
        let bad = format!(
            "fn main() -> u8 {{\n    fix x: u8 = 1\n    fix y = {expr}\n    return y\n}}\n"
        );
        let error = suz::compile(bad.as_bytes()).unwrap_err();
        assert!(
            error.message().contains("expr.arithmetic-handler"),
            "{expr}: {error:?}"
        );
    }

    // len_t は左辺に使えない
    for op in [">>>", "<<%", ">>%", ">>>%"] {
        let bad = format!(
            "fn main() -> u8 {{\n    fix n: len_t = 4\n    fix y = n {op} 1\n    return 0\n}}\n"
        );
        let error = suz::compile(bad.as_bytes()).unwrap_err();
        assert!(
            error.message().contains("expr.operator-types"),
            "{op}: {error:?}"
        );
    }

    // 定数式での >>> 範囲外は const.evaluation エラー
    for decl in [
        "fix V: i8 = (-2 as i8) >>> 8",
        "fix V: i8 = (-2 as i8) >>> -1",
    ] {
        let bad = format!("{decl}\nfn main() -> u8 {{ return 0 }}\n");
        let error = suz::compile(bad.as_bytes()).unwrap_err();
        assert!(
            error.message().contains("const.evaluation"),
            "{decl}: {error:?}"
        );
    }
}
