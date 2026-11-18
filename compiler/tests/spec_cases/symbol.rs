use crate::test_util::compile_and_run_c23;

#[test]
fn templates_preserve_constant_dependencies_and_nested_value_scopes() {
    let source = r#"fix LABEL = `${PRODUCT} ${COUNT} ${READY}`
fix PRODUCT = `Suz`
fix COUNT: u8 = 3
fix READY = true
struct Label { value: symbol }
fix ITEM = Label { value: LABEL }
fix SAME = ITEM == Label { value: `Suz 3 true` }
fn main() -> u8 {
    if not SAME { return 1 }
    fix count: u8 = 2
    fix first = `${`nested ${count}`}`
    fix captured = expr { `${count}` }
    if first != `nested 2` { return 3 }
    if captured != `2` { return 2 }
    return 0
}"#;
    let result = compile_and_run_c23(source, "text.suz");
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn template_contents_keep_diagnostics_and_scope_checks() {
    for (source, rule) in [
        (
            "fix A = `${B}`\nfix B = `${A}`\nfn main() -> u8 { return 0 }",
            "const.initializer",
        ),
        ("fn main() -> u8 { fix text = 1\nreturn 0 }", "src.ident"),
        (
            "fn main() -> u8 { fix value = `${expr { return 1 }}`\nreturn 0 }",
            "expr.jump-prohibited",
        ),
        (
            "fn main() -> u8 { var n = 2\nfor i in 0..<`${n}` { n += 1 }\nreturn 0 }",
            "for.bound-reference",
        ),
        (
            "fix VALUE = `${1 / 0}`\nfn main() -> u8 { return 0 }",
            "arith.div-zero-always",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.message().contains(rule), "{source}: {error:?}");
    }
    let source = "fn main() -> u8 { fix value = `${1 + 2}`\nreturn 0 }";
    let result =
        suz::compile_with_source_name(source.as_bytes(), suz::ArithmeticMode::Warn, "text.suz")
            .unwrap();
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].column, source.find('+').unwrap() + 1);
}

fn float_templates(values: impl IntoIterator<Item = (String, &'static str)>) {
    let mut source = String::new();
    let mut body = String::from("fn main() -> u8 {\n");
    for (index, (literal, ty)) in values.into_iter().enumerate() {
        let literal = if matches!(literal.as_str(), "NaN" | "nan") {
            "nan".into()
        } else if let Some((mantissa, exponent)) = literal.split_once('e') {
            format!(
                "{}{}e{exponent}",
                mantissa,
                if mantissa.contains('.') { "" } else { ".0" }
            )
        } else {
            literal
        };
        source.push_str(&format!(
            "fix VALUE_{index}: {ty} = {literal}\nfix SAVED_{index} = `${{VALUE_{index}}}`\n"
        ));
        body.push_str(&format!("{{\nfix dynamic = VALUE_{index}\nif SAVED_{index} != `${{dynamic}}` {{ log info `mismatch {index}: ${{SAVED_{index}}} ${{dynamic}}`\nreturn 1 }}\n}}\n"));
    }
    body.push_str("return 0\n}\n");
    source.push_str(&body);
    let result = compile_and_run_c23(&source, "float-text.suz");
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
}

#[test]
fn constant_and_dynamic_float_templates_agree_at_format_boundaries() {
    let doubles = [
        0.0,
        -0.0,
        1e-6,
        f64::from_bits(1e-6f64.to_bits() + 1),
        1e21,
        f64::from_bits(1e21f64.to_bits() - 1),
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::from_bits(1),
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let singles = [
        // 正確な十進の中点: 1048576.25と1048576.75。
        f32::from_bits(0x4980_0002),
        f32::from_bits(0x4980_0006),
        0.0,
        -0.0,
        1e-6,
        1e21,
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::from_bits(1),
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    float_templates(
        doubles
            .into_iter()
            .map(|v| (format!("{v:e}"), "f64"))
            .chain(singles.into_iter().map(|v| (format!("{v:e}"), "f32"))),
    );
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn constant_and_dynamic_float_templates_agree_for_generated_values() {
    let mut state = 0x917d_38ca_4b20_f561_u64;
    let values: Vec<_> = (0..256)
        .flat_map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            [
                (format!("{:e}", f64::from_bits(state)), "f64"),
                (format!("{:e}", f32::from_bits(state as u32)), "f32"),
            ]
        })
        .collect();
    float_templates(values);
}
