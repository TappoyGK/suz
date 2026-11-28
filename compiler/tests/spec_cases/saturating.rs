use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

fn compile_and_run(source: &[u8], name: &str) -> std::process::Output {
    let directory = TemporaryDirectory::new();
    let path = directory.0.join(format!("{name}.c"));
    let c = suz::compile(source).unwrap_or_else(|e| panic!("compile failed for {name}: {e:?}"));
    fs::write(&path, c).unwrap();
    build_and_run_c(&path, &directory.0.join(name))
}

#[test]
fn saturating_arithmetic_and_unary_negate_work_at_runtime_and_compile_time() {
    let source = br#"fix C_U8_ADD: u8 = (250 as u8) +| (20 as u8)
fix C_U8_SUB: u8 = (5 as u8) -| (20 as u8)
fix C_U8_MUL: u8 = (20 as u8) *| (20 as u8)
fix C_I8_ADD_HI: i8 = (100 as i8) +| (50 as i8)
fix C_I8_ADD_LO: i8 = (-100 as i8) +| (-50 as i8)
fix C_I8_SUB_HI: i8 = (100 as i8) -| (-50 as i8)
fix C_I8_SUB_LO: i8 = (-100 as i8) -| (50 as i8)
fix C_I8_MUL_HI: i8 = (-128 as i8) *| (-1 as i8)
fix C_I8_MUL_LO: i8 = (100 as i8) *| (-2 as i8)
fix C_I8_NEG_MIN: i8 = -| (-128 as i8)
fix C_I8_NEG_VAL: i8 = -| (42 as i8)
fix C_I64_ADD: i64 = 9223372036854775807 +| 1
fix C_I64_SUB: i64 = (-9223372036854775807 - 1) -| 1
fix C_I64_MUL: i64 = (-9223372036854775807 - 1) *| -1
fix C_I64_NEG: i64 = -| (-9223372036854775807 - 1)
fix C_U64_MUL: u64 = 18446744073709551615 *| 18446744073709551615

fn main() -> u8 {
    if C_U8_ADD != 255 || C_U8_SUB != 0 || C_U8_MUL != 255 { return 1 }
    if C_I8_ADD_HI != 127 || C_I8_ADD_LO != -128 { return 2 }
    if C_I8_SUB_HI != 127 || C_I8_SUB_LO != -128 { return 3 }
    if C_I8_MUL_HI != 127 || C_I8_MUL_LO != -128 { return 4 }
    if C_I8_NEG_MIN != 127 || C_I8_NEG_VAL != -42 { return 5 }
    if C_I64_ADD != 9223372036854775807 || C_I64_SUB != -9223372036854775807 - 1 { return 6 }
    if C_I64_MUL != 9223372036854775807 || C_I64_NEG != 9223372036854775807 { return 7 }
    if C_U64_MUL != 18446744073709551615 { return 8 }

    var u: u8 = 250
    u +|= 20
    if u != 255 { return 9 }
    u -|= 250
    if u != 5 { return 10 }
    u -|= 20
    if u != 0 { return 11 }
    u = 20
    u *|= 20
    if u != 255 { return 12 }

    var s: i16 = 30000
    s +|= 10000
    if s != 32767 { return 13 }
    s = -30000
    s -|= 10000
    if s != -32768 { return 14 }
    if (-| s) != 32767 { return 15 }
    s *|= -1
    if s != 32767 { return 16 }

    fix big_i: i64 = 9223372036854775807
    if (big_i +| 10) != 9223372036854775807 { return 17 }
    fix min_i: i64 = -9223372036854775807 - 1
    if (min_i -| 10) != -9223372036854775807 - 1 { return 18 }
    if (min_i *| -1) != 9223372036854775807 { return 19 }
    if (min_i *| 2) != -9223372036854775807 - 1 { return 20 }
    if (-| min_i) != 9223372036854775807 { return 21 }

    fix big_u: u64 = 18446744073709551615
    if (big_u +| 1) != 18446744073709551615 { return 22 }
    if ((0 as u64) -| 1) != 0 { return 23 }
    if (big_u *| 2) != 18446744073709551615 { return 24 }

    // precedence: *| > +| / -|
    fix p: u8 = (2 as u8) +| (10 as u8) *| (30 as u8)
    if p != 255 { return 25 }
    return 0
}
"#;
    let output = compile_and_run(source, "sat-arith");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn saturating_left_shift_works_at_runtime_and_compile_time() {
    let source = br#"fix C_U8_OK: u8 = (3 as u8) <<| 2
fix C_U8_SAT: u8 = (100 as u8) <<| 2
fix C_U8_HUGE: u8 = (1 as u8) <<| 100
fix C_U8_ZERO: u8 = (0 as u8) <<| 100
fix C_I8_POS: i8 = (64 as i8) <<| 1
fix C_I8_NEG_EXACT: i8 = (-64 as i8) <<| 1
fix C_I8_NEG_SAT: i8 = (-65 as i8) <<| 1
fix C_I8_HUGE_POS: i8 = (1 as i8) <<| 100
fix C_I8_HUGE_NEG: i8 = (-1 as i8) <<| 100
fix C_I8_FALLBACK: i8 = (1 as i8) <<| -1 or 77

fn main() -> u8 {
    if C_U8_OK != 12 || C_U8_SAT != 255 || C_U8_HUGE != 255 || C_U8_ZERO != 0 { return 1 }
    if C_I8_POS != 127 || C_I8_NEG_EXACT != -128 || C_I8_NEG_SAT != -128 { return 2 }
    if C_I8_HUGE_POS != 127 || C_I8_HUGE_NEG != -128 || C_I8_FALLBACK != 77 { return 3 }

    var u: u8 = 3
    fix s_u: u8 = 2
    u <<|= s_u
    if u != 12 { return 4 }
    u <<|= 10 as u8
    if u != 255 { return 5 }

    fix z: u8 = 0
    if (z <<| 64 as u8) != 0 { return 6 }

    fix neg_shift: i32 = -1
    var s: i8 = 1
    if (s <<| neg_shift or 99) != 99 { return 7 }
    fix big_shift: i32 = 100
    if (s <<| big_shift or 99) != 127 { return 8 }
    s = -1
    if (s <<| big_shift or 99) != -128 { return 9 }
    s = -64
    if (s <<| (1 as u8)) != -128 { return 10 }
    s = -65
    if (s <<| (1 as u8)) != -128 { return 11 }
    s = 0
    if (s <<| big_shift or 99) != 0 { return 12 }

    fix s64: i64 = -1
    if (s64 <<| (63 as u8)) != -9223372036854775807 - 1 { return 13 }
    fix p64: i64 = 1
    if (p64 <<| (63 as u8)) != 9223372036854775807 { return 14 }
    return 0
}
"#;
    let output = compile_and_run(source, "sat-shl");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn saturating_cast_integers_floats_and_text_work() {
    let source = br#"fix C_I32_TO_I16_HI: i16 = (50000 as i32) as| i16
fix C_I32_TO_I16_LO: i16 = (-50000 as i32) as| i16
fix C_I32_TO_U8_LO: u8 = (-5 as i32) as| u8
fix C_EXACT_SAT: i8 = (100 + 100) as| i8
fix C_F64_TO_I16_HI: i16 = 50000.9 as| i16
fix C_F64_TO_I16_LO: i16 = -50000.9 as| i16
fix C_F64_TO_I16_INF: i16 = inf as| i16
fix C_F64_TO_I16_NINF: i16 = -inf as| i16
fix C_F64_TO_I16_NAN: i16 = nan as| i16 or -123
fix C_TEXT_HI: u8 = "99999999999999999999999999" as| u8 or 7
fix C_TEXT_NEG_U8: u8 = "-5" as| u8 or 7
fix C_TEXT_LO_I8: i8 = "-999" as| i8 or 7
fix C_TEXT_BAD: u8 = "12x" as| u8 or 7
fix C_TEXT_BAD_NEG: u8 = "-12x" as| u8 or 7

fn float_or_return(x: f64) -> i16 {
    fix v = x as| i16 or return -77
    return v
}

fn main() -> u8 {
    if C_I32_TO_I16_HI != 32767 || C_I32_TO_I16_LO != -32768 || C_I32_TO_U8_LO != 0 { return 1 }
    if C_EXACT_SAT != 127 { return 2 }
    if C_F64_TO_I16_HI != 32767 || C_F64_TO_I16_LO != -32768 { return 3 }
    if C_F64_TO_I16_INF != 32767 || C_F64_TO_I16_NINF != -32768 || C_F64_TO_I16_NAN != -123 { return 4 }
    if C_TEXT_HI != 255 || C_TEXT_NEG_U8 != 0 || C_TEXT_LO_I8 != -128 { return 5 }
    if C_TEXT_BAD != 7 || C_TEXT_BAD_NEG != 7 { return 6 }

    fix a: i32 = 100
    fix b: i32 = 200
    if ((a + b) as| i8) != 127 { return 7 }
    fix neg_i: i32 = -42
    if (neg_i as| u8) != 0 { return 8 }
    fix big_u: u64 = 18446744073709551615
    if (big_u as| i64) != 9223372036854775807 { return 9 }

    fix f_nan: f64 = nan
    fix f_inf: f64 = inf
    fix f_ninf: f64 = -inf
    fix f_frac: f64 = -12.9
    fix f_neg_small: f64 = -0.9
    fix f_i64_hi: f64 = 9223372036854775808.0
    fix f_u64_hi: f64 = 18446744073709551616.0
    if (f_nan as| i16 or -9) != -9 { return 10 }
    if (f_inf as| i16 or -9) != 32767 { return 11 }
    if (f_ninf as| i16 or -9) != -32768 { return 12 }
    if (f_frac as| i16 or -9) != -12 { return 13 }
    if (f_neg_small as| u8 or 9) != 0 { return 14 }
    if (f_i64_hi as| i64 or 0) != 9223372036854775807 { return 15 }
    if (f_u64_hi as| u64 or 0) != 18446744073709551615 { return 16 }
    if float_or_return(f_nan) != -77 || float_or_return(f_inf) != 32767 { return 17 }

    allow [diagnostic.float-sat-nan: audio output treats NaN as silence] {
        if (f_nan as| i16) != 0 { return 18 }
        if (f_inf as| i16) != 32767 { return 19 }
    }

    fix t_hi: text = "999"
    fix t_lo: text = "-50"
    fix t_bad: text = "-50z"
    if (t_hi as| u8 or 1) != 255 || (t_lo as| u8 or 1) != 0 || (t_bad as| u8 or 1) != 1 { return 20 }

    fix z0: f64 = 0.0
    var side: i8 = 3
    fix ordered: i8 = (expr {
        side = 7
        0.0 / z0
    }) as| i8 or side
    if ordered != 7 { return 21 }
    return 0
}
"#;
    let output = compile_and_run(source, "sat-cast");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn saturating_diagnostics_and_warnings_are_reported() {
    // float as| int without or warns with diagnostic.float-sat-nan
    let warn_src =
        b"fn f(x: f32) -> i16 {\n    return x as| i16\n}\nfn main() -> u8 { return 0 }\n";
    let warnings = suz::compile_with_mode(warn_src, suz::ArithmeticMode::Default)
        .unwrap()
        .warnings;
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].message().contains("diagnostic.float-sat-nan"),
        "{:?}",
        warnings[0]
    );
    assert_eq!((warnings[0].line, warnings[0].column), (2, 14));

    // Constant non-NaN float as| int does NOT warn
    let const_ok = b"fn main() -> u8 {\n    fix v: i16 = 100.5 as| i16\n    return ifexpr v == 100 { 0 } else { 1 }\n}\n";
    let warnings = suz::compile_with_mode(const_ok, suz::ArithmeticMode::Default)
        .unwrap()
        .warnings;
    assert!(warnings.is_empty(), "{warnings:?}");

    // Constant NaN float as| int without or warns
    let const_nan = b"fn main() -> u8 {\n    fix v: i16 = nan as| i16\n    return ifexpr v == 0 { 0 } else { 1 }\n}\n";
    let warnings = suz::compile_with_mode(const_nan, suz::ArithmeticMode::Default)
        .unwrap()
        .warnings;
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0].message().contains("diagnostic.float-sat-nan"),
        "{:?}",
        warnings[0]
    );

    // Unsigned shift count on <<| is infallible: accepted under Deny, rejects or handler
    let unsigned_shl =
        b"fn f(n: u8) -> i8 {\n    return (1 as i8) <<| n\n}\nfn main() -> u8 { return 0 }\n";
    suz::compile_with_mode(unsigned_shl, suz::ArithmeticMode::Deny).unwrap();
    let unsigned_shl_or =
        b"fn f(n: u8) -> i8 {\n    return (1 as i8) <<| n or 5\n}\nfn main() -> u8 { return 0 }\n";
    let err = suz::compile(unsigned_shl_or).unwrap_err();
    assert!(err.message().contains("expr.arithmetic-handler"), "{err:?}");

    // Invalid usages
    for (src, expected_rule) in [
        (
            "fn main() -> u8 { fix a: u8 = -| (1 as u8)\n return a }",
            "expr.operator-types",
        ),
        (
            "fn main() -> u8 { fix a: len_t = 1\n fix b: len_t = a +| a\n return 0 }",
            "expr.operator-types",
        ),
        (
            "fn main() -> u8 { fix a: f64 = 1.0 +| 2.0\n return 0 }",
            "expr.operator-types",
        ),
        (
            "fn main() -> u8 { fix a: f64 = 1.0 <<| 1\n return 0 }",
            "expr.operator-types",
        ),
        (
            "fn main() -> u8 { fix a: len_t = (1 as i32) as| len_t\n return 0 }",
            "expr.as-sat",
        ),
        (
            "fn main() -> u8 { fix a: u8 = (1 as i32) as| u8 or 0\n return a }",
            "expr.cast-sat-handler-syntax",
        ),
        (
            "fn main() -> u8 { fix a: u8 = \"10\" as| u8\n return a }",
            "symbol.cast-handler",
        ),
        (
            "fn main() -> u8 { fix a: f64 = (1 as i32) as| f64\n return 0 }",
            "expr.as-sat",
        ),
        (
            "fix BAD: i8 = (1 as i8) <<| -1\nfn main() -> u8 { return 0 }",
            "const.evaluation",
        ),
    ] {
        let err = suz::compile(src.as_bytes()).unwrap_err();
        assert!(
            err.message().contains(expected_rule),
            "expected {expected_rule} for `{src}`, got: {err:?}"
        );
    }
}

#[test]
fn saturating_operator_methods_and_needs_work() {
    let source = br#"struct Sample {
    v: i16,
}

fn Sample.+|(other: Sample) -> Sample {
    return Sample { v: self.v +| other.v }
}

fn Sample.-|(other: Sample) -> Sample {
    return Sample { v: self.v -| other.v }
}

fn Sample.*|(other: Sample) -> Sample {
    return Sample { v: self.v *| other.v }
}

fn Sample.<<|(shift: u8) -> Sample {
    return Sample { v: self.v <<| shift }
}

fn Sample.-|() -> Sample {
    return Sample { v: -| self.v }
}

@needs T: +|(T) -> T
@needs T: -|() -> T
fn add_and_neg[T](a: T, b: T) -> T {
    return -| (a +| b)
}

fn main() -> u8 {
    var a = Sample { v: 30000 }
    fix b = Sample { v: 10000 }
    a +|= b
    if a.v != 32767 { return 1 }
    a = Sample { v: -30000 }
    a -|= b
    if a.v != -32768 { return 2 }
    fix inv = -| a
    if inv.v != 32767 { return 3 }
    a = Sample { v: 1000 }
    a *|= Sample { v: 100 }
    if a.v != 32767 { return 4 }
    a = Sample { v: 1000 }
    a <<|= 10 as u8
    if a.v != 32767 { return 5 }

    fix g = add_and_neg(Sample { v: -30000 }, Sample { v: -10000 })
    if g.v != 32767 { return 6 }
    return 0
}
"#;
    let output = compile_and_run(source, "sat-op-methods");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}
