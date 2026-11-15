use crate::test_util::compile_and_run_c23;
use suz::{ArithmeticMode, CompilationOptions};

#[test]
fn exact_arithmetic_preserves_large_intermediates_and_storage_boundaries() {
    let source = r#"fn later() -> u8 { log warn `unexpected`
return 1 }
fn main() -> u8 {
    fix a: u8 = 200
    fix b: u8 = 100
    fix sum: u16 = a + b
    fix scaled: u8 = a * b / 250
    fix narrow: u8 = a + b or 7
    fix inner: u16 = ((a + b) or 255) * 2
    fix f: f32 = a + b
    fix rounded = (a + b) as f32
    fix wrap = (a + b) as% u8
    fix negative: i16 = -a
    fix max: u64 = 18_446_744_073_709_551_615
    fix cancel: u64 = max * max / max
    fix tiny: u8 = (max - max) + 1
    fix zero: u8 = (a / (b % 1) + later()) or 3
    fix selected = matchexpr a + b { 300 => 4
        _ => 9 }
    log info `${sum},${scaled},${narrow},${inner},${f},${rounded},${wrap},${negative},${cancel},${tiny},${zero},${selected}`
    log info `${max * max}`
    return 0
}"#;
    let out = compile_and_run_c23(source, "numeric.suz");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "info numeric.suz:20: 300,80,7,510,300.0,300.0,44,-200,18446744073709551615,1,3,4\ninfo numeric.suz:21: 340282366920938463426481119284349108225\n"
    );
}

#[test]
fn numeric_comparisons_keep_integer_precision_and_ieee_special_values() {
    let source = r#"const EXACT = 18_446_744_073_709_551_615 * 18_446_744_073_709_551_615 > 9_007_199_254_740_992.0
fn main() -> u8 {
    fix i: u64 = 9_007_199_254_740_993
    fix f: f64 = 9_007_199_254_740_992.0
    fix minus: i64 = -1
    fix zero: u8 = 0
    fix small: f64 = 0.5
    fix huge: f64 = 1.0e300
    fix max: u64 = 18_446_744_073_709_551_615
    log info `${EXACT},${i > f},${f < i},${i != f},${minus < max},${zero == -0.0},${zero < small},${max * max < huge}`
    log info `${i < nan},${i >= nan},${nan <= i},${nan != i},${i < inf},${-inf < minus}`
    return 0
}"#;
    let out = compile_and_run_c23(source, "compare.suz");
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "info compare.suz:10: true,true,true,true,true,true,true,true\ninfo compare.suz:11: false,false,false,true,true,true\n"
    );
}

#[test]
fn exact_width_and_storage_rules_reject_invalid_programs() {
    for (body, rule) in [
        ("fix x: u64 = 1\nfix y = x * x * x", "arith.width"),
        (
            "fix x: u64 = 1\nfix y: u8 = (x * x * x / 0) or 0",
            "arith.width",
        ),
        (
            "fix x: u8 = 2\nfix y = x / (x % 1)",
            "arith.div-zero-always",
        ),
        ("fix x: u16 = 1\nfix y: u8 = x", "type.expected"),
        ("fix x: u32 = 1\nfix y: f32 = x * 2", "arith.store-float"),
        (
            "fix x: u32 = 1\nfix y: f32 = 2.0\nfix z = x + y",
            "arith.mix",
        ),
        ("fix x: u64 = 1\nfix y: i8 = 2\nfix z = x + y", "conv.join"),
        (
            "fix x: u8 = 1\nfix y: u16 = 2\nfix z = x & y",
            "width.same-type",
        ),
        (
            "fix x: u8 = 1\nfix y = (x + 255) as u8",
            "expr.cast-handler",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let err = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            err.message.contains(&format!("[{rule}]")),
            "{source}: {err}"
        );
    }
}

#[test]
fn comparison_warnings_and_allow_items_are_independent_of_arithmetic_modes() {
    let source=b"fn main() -> u8 {\nfix x: u8 = 1\nfix always = x + 1 < 1000 allow [diagnostic.constant-comparison: intentional], [diagnostic.arithmetic: bounded]\nreturn 0\n}";
    for mode in [ArithmeticMode::Default, ArithmeticMode::Warn] {
        let options = CompilationOptions {
            arithmetic_mode: mode,
            log_allow: true,
            ..CompilationOptions::default()
        };
        let warnings = suz::check_source_with_options(source, &options).unwrap();
        assert_eq!(
            warnings.len(),
            if mode == ArithmeticMode::Warn { 2 } else { 1 }
        );
        assert!(warnings.iter().all(|w| w.allowed_reason.is_some()));
        assert!(warnings.iter().any(|w| {
            w.message
                .contains("always true [diagnostic.constant-comparison]")
                && w.column == 20
        }));
    }
    let err = suz::check_source(source, ArithmeticMode::Deny).unwrap_err();
    assert!(err.message.contains("[diagnostic.arithmetic]"));
}

#[test]
fn widening_and_exact_comparisons_accept_previously_rejected_programs() {
    for body in [
        "fix zero: u8 = 0\nfix value = -zero or 0\nreturn value",
        "fix x: u16 = 1 as% u8\nreturn 0",
        "fix value: u8 = 1\nfix result = value < 256\nreturn 0",
        "fix value: u8 = 1\nfix result = -1 < value\nreturn 0",
        "fix first: u8 = 1\nfix second: u16 = 1\nfix result = first == second\nreturn 0",
        "fix result = 18446744073709551615 == 18446744073709551615\nreturn 0",
        "fix first: u8 = 3\nfix second: i32 = 4\nfix result = first < 2 < second\nreturn 0",
        "fix top: u8 = 3\nfix result = 256 < 2 < top\nreturn 0",
        "fix value: i8 = -(128)\nreturn 0",
        "fix x: u16 = 3 as u8 or return 7\nreturn 0",
        "fix value: f32 = 1.0\nfix fallback: f32 = 0.0\nfix x = value as f64 or fallback\nreturn 0",
        "fix x: u8 = 1\nfix y: u16 = 2\nfix z = ifexpr true { x } else { y }\nreturn 0",
        "fix x: f32 = 1.0\nfix y: f64 = x\nreturn 0",
        "fix x: f32 = 1.0\nfix y: f64 = 1.0\nreturn ifexpr x == y { 0 } else { 1 }",
        "fix x: f32 = 1.0\nfix y: f64 = 1.0\nfix z = x + y\nreturn 0",
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        suz::compile(source.as_bytes()).unwrap();
    }
}

#[test]
fn constant_exact_operations_check_storage_after_the_whole_expression() {
    for source in [
        "const LO: i8 = -128\nconst BAD = -(-LO) or 7\nfn main() -> u8 { return 0 }",
        "const BAD: i8 = -128 % -1\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = (1 / 0) + 1 or 7\nfn main() -> u8 { return 0 }",
        "const BAD: u8 = ((255 + 1) * 0) or 7\nfn main() -> u8 { return 0 }",
        "const MAX: u8 = 255\nconst BAD = false || (MAX + 1 == 0)\nfn main() -> u8 { return 0 }",
        "const MAX: u8 = 255\nconst BAD = (MAX + 1) as u16\nfn main() -> u8 { return 0 }",
    ] {
        suz::compile(source.as_bytes()).unwrap();
    }
}

#[test]
fn mixed_arithmetic_preserves_integer_subtrees_and_warns_at_each_division() {
    let source = r#"const WIDTH: i32 = 10
fn main() -> u8 {
    fix frame: i32 = 3
    fix frames: i32 = 10
    fix unit: f64 = 0.5
    fix truncated = frame / frames * 2.0
    fix fraction = frame * 2.0 / frames
    fix position = (2 * frame - WIDTH) * unit
    fix small: u8 = 3
    fix single: f32 = small + 0.5
    fix explicit = (frame / frames) as f64 * unit
    fix intended = frame / frames * unit allow [diagnostic.integer-division-to-float: whole frames]
    log info `${truncated},${fraction},${position},${single},${explicit},${intended}`
    return 0
}"#;
    let warnings = suz::check_source(source.as_bytes(), ArithmeticMode::Default).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0]
            .message
            .ends_with("[diagnostic.integer-division-to-float]")
    );
    assert_eq!((warnings[0].line, warnings[0].column), (6, 27));
    let out = compile_and_run_c23(source, "mixed.suz");
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "info mixed.suz:13: 0.0,0.6,-2.0,3.5,0.0,0.0\n"
    );
    for body in [
        "fix n: i32 = 1\nfix f: f32 = 0.5\nfix bad = n + f",
        "fix n: i64 = 1\nfix bad = n + 0.5",
        "fix n: u32 = 1\nfix f: f32 = 0.5\nfix bad = n * 2 + f",
    ] {
        let err = suz::compile(format!("fn main() -> u8 {{\n{body}\nreturn 0\n}}").as_bytes())
            .unwrap_err();
        assert!(
            err.message.contains("[arith.mix]") || err.message.contains("[arith.store-float]"),
            "{err}"
        );
    }
}

#[test]
fn exact_boundaries_agree_in_constants_and_runtime_conversions() {
    let source = r#"const MAX: u64 = 18_446_744_073_709_551_615
const EXPECTED = `${MAX * MAX},${(MAX * MAX) as% u8},${(MAX * MAX) as f32},${(MAX * MAX) as f64 or 0.0}`
fn main() -> u8 {
    fix max: u64 = MAX
    fix min: i64 = -9_223_372_036_854_775_808
    fix actual = `${max * max},${(max * max) as% u8},${(max * max) as f32},${(max * max) as f64 or 0.0}`
    fix negative = (min * max) as i64 or 7
    fix wrap = (min * max) as% i64
    fix kind = matchexpr max * max { MAX * MAX => 1
        _ => 2 }
    fix shifted: u8 = 1 << (max * max) or 3
    return ifexpr EXPECTED == actual && negative == 7 && wrap == min && kind == 1 && shifted == 3 && max * max < inf && min * max < -1.0e30 { 0 } else { 1 }
}"#;
    let out = compile_and_run_c23(source, "bounds.suz");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn float_literal_comparisons_preserve_precision_through_parentheses() {
    let source = r#"fn main() -> u8 {
    fix fraction: f32 = 0.1
    fix whole: i32 = 1
    return ifexpr fraction == (0.1) && fraction == -(-0.1) && fraction == (0.1) < whole { 0 } else { 1 }
}"#;
    let out = compile_and_run_c23(source, "precision.suz");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
