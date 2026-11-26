use crate::test_util::compile_and_run_c23;

#[test]
fn symbol_float_casts_share_constant_rounding_and_lazy_handlers() {
    let source = r#"fix MID = "1.0000000596046447753906250000000000001" as f32 or 7.0
fix HUGE = "1.0e1000" as f64 or 7.0
fix TINY = "-1.0e-1000" as f64 or 7.0
fix GOOD = "1.5" as f32 or ((1 as i64) / (0 as u8)) as f32
fn source(value: symbol) -> symbol {
    log info "source"
    return value
}
fn fallback() -> f64 {
    log info "fallback"
    return 7.0
}
fn parse(value: symbol) -> f32 {
    fix parsed = value as f32 or return 7.0
    return parsed
}
fn main() -> u8 {
    fix input = "1.0000000596046447753906250000000000001"
    fix rounded = input as f32 or 7.0
    if MID != 1.00000011920928955078125 || rounded != MID { return 1 }
    if HUGE != 7.0 || 1.0 / TINY != -inf || GOOD != 1.5 { return 2 }
    fix good = source("1.5") as f64 or fallback()
    fix bad = source("1.0e1000") as f64 or fallback()
    if good != 1.5 || bad != 7.0 { return 3 }
    if parse("1.5") != 1.5 || parse("bad") != 7.0 { return 4 }
    return 0
}"#;
    let result = compile_and_run_c23(source, "text-float.suz");
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stderr, b"info text-float.suz:6: source\ninfo text-float.suz:6: source\ninfo text-float.suz:10: fallback\n");
}

#[test]
fn symbol_float_casts_require_handlers_and_check_unselected_values() {
    for (source, rule) in [
        (
            "fix A = \"1.0\" as f32\nfn main() -> u8 { return 0 }",
            "symbol.cast-handler",
        ),
        (
            "fn main() -> u8 { fix a = \"1.0\" as% f32 or 0.0\nreturn 0 }",
            "symbol.cast-wrap",
        ),
        (
            "fn main() -> u8 { fix a = \"1.0\" as f32 or false\nreturn 0 }",
            "type.expected",
        ),
        (
            "fix A = \"bad\" as f32 or ((1 as i64) / 0) as f32\nfn main() -> u8 { return 0 }",
            "arith.div-zero-always",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.message().contains(rule), "{source}: {error:?}");
    }
}
