use crate::{integer_bounds::INTEGER_BOUNDS, test_util::compile_and_run_c23};

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn enum_conversions_use_variant_ranges_and_preserve_integer_boundaries() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (base, _, _) in INTEGER_BOUNDS {
        source.push_str(&format!("{{\nfix value = Kind{base}.B\n"));
        for (ty, min, max) in INTEGER_BOUNDS {
            source.push_str(&format!(
                "fix ordinal_{ty}: {ty} = value as {ty}\nif ordinal_{ty} != 1 {{ return 1 }}\n"
            ));
            source.push_str(&format!(
                "fix zero_{ty}: {ty} = 0\nfix one_{ty}: {ty} = 1\nfix max_{ty}: {ty} = {max}\n"
            ));
            source.push_str(&format!("if (zero_{ty} as Kind{base} or Kind{base}.B) != Kind{base}.A || (one_{ty} as Kind{base} or Kind{base}.A) != Kind{base}.B || (max_{ty} as Kind{base} or Kind{base}.A) != Kind{base}.A {{ return 2 }}\n"));
            if min < 0 {
                source.push_str(&format!("fix min_{ty}: {ty} = {min}\nif (min_{ty} as Kind{base} or Kind{base}.B) != Kind{base}.B {{ return 3 }}\n"));
            }
        }
        source.push_str("}\n");
    }
    source.push_str("return 0\n}\n");
    for (base, _, _) in INTEGER_BOUNDS {
        source.push_str(&format!("enum Kind{base}: {base} {{ A, B }}\n"));
    }
    let output = compile_and_run_c23(&source, "enums.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn enum_values_work_with_constants_calls_blocks_and_lazy_handlers() {
    let source = r#"fix CHOSEN: Mode = INVALID as Mode or Mode.B
fix INVALID: u16 = 256
fix EQUAL = CHOSEN == Mode.B == (1 as Mode)
fn narrow(raw: i64) -> Mode {
    fix mode = raw as Mode or return Mode.B
    return mode
}
fn main() -> u8 {
    var visits: u8 = 0
    fix success = (expr {
        visits += 1
        0
    }) as Mode or expr {
        visits += 10
        Mode.B
    }
    fix failure = (expr {
        visits += 1
        256
    }) as Mode or expr {
        visits += 10
        Mode.B
    }
    if visits != 12 || success != Mode.A || failure != Mode.B || not EQUAL { return 1 }
    fix first: Mode = narrow(-1)
    var copy = first
    copy = Mode.A
    if first != Mode.B || copy != Mode.A { return 2 }
    fix selected = matchexpr CHOSEN {
        Mode.A => Mode.B
        Mode.B => expr {
            fix inner = narrow(0)
            inner
        }
    }
    log info `${selected}/${CHOSEN}`
    match selected {
        Mode.A => { return 0 }
        Mode.B => { return 3 }
    }
}
enum Mode: u8 { A, B }
"#;
    let output = compile_and_run_c23(source, "enums.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "info enums.suz:{}: A/B\n",
            source
                .lines()
                .position(|line| line.contains("log info"))
                .unwrap()
                + 1
        )
    );
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn enum_numbering_and_narrowing_cover_signed_and_unsigned_limits() {
    for (base, _, max) in INTEGER_BOUNDS.into_iter().filter(|(_, _, max)| *max <= 255) {
        let variants = (0..=max)
            .map(|i| format!("V{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "enum Code: {base} {{ {variants} }}\nfn main() -> u8 {{ return Code.V{max} as u8 }}"
        );
        assert_eq!(
            compile_and_run_c23(&source, "enums.suz").status.code(),
            Some(max as i32)
        );
        let invalid = source.replace(&format!("V{max} }}"), &format!("V{max}, Overflow }}"));
        let error = suz::compile(invalid.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().contains("[enum.variant]"));
    }
    // 符号付きの格納型を使っても、Cの比較に符号の混在を持ち込んではならない。
    let variants = (0..=128)
        .map(|i| format!("V{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        r#"enum Code: i64 {{ {variants} }}
fix LOW = Code.V127 as i8
fix HIGH = Code.V128 as i8 or -1
fn convert(value: Code) -> i8 {{
    fix ordinal = value as i8 or return -1
    return ordinal
}}
fn main() -> u8 {{
    if LOW != 127 || HIGH != -1 || convert(Code.V127) != 127 || convert(Code.V128) != -1 {{ return 1 }}
    return 0
}}
"#
    );
    assert_eq!(
        compile_and_run_c23(&source, "enums.suz").status.code(),
        Some(0)
    );
}

#[test]
fn enum_types_remain_nominal_and_require_checked_conversions() {
    let declarations = "enum First: u8 { A, B }\nenum Second: u8 { A, B }\n";
    for (body, rule) in [
        ("fix wrong: Second = First.A", "type.expected"),
        ("First.A == Second.A", "type.expected"),
        ("First.A < First.B", "operator.overload"),
        ("First.A + First.B", "operator.overload"),
        ("fix wrong: First = 0", "type.expected"),
        ("fix raw: u8 = 0\nraw as First", "expr.cast-handler"),
        ("256 as First", "const.cast-check"),
        ("First.A as Second or Second.A", "expr.as"),
        ("First.A as First", "expr.as"),
        ("First.A as f64", "expr.as"),
        ("0.0 as First or First.A", "expr.as"),
        ("First.A as% u8", "expr.as-trunc"),
        ("0 as% First", "expr.as-trunc"),
        ("First.Missing", "enum.access"),
        ("fix First = 0", "name.value"),
        (
            "match First.A {\nFirst.A => {}\n0 as First => {}\nFirst.B => {}\n}",
            "match.pattern",
        ),
    ] {
        let source = format!("{declarations}fn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}: {error}"
        );
        assert!(
            error.message().contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
        if body.starts_with("fix wrong: Second") {
            assert!(error.message().contains("expected Second, found First"));
        }
    }
    for declaration in ["struct First { value: u8 }", "enum First: i8 { A }"] {
        let source = format!("{declarations}{declaration}\nfn main() -> u8 {{ return 0 }}");
        assert!(
            suz::compile(source.as_bytes())
                .unwrap_err()
                .message()
                .contains("[decl.duplicate]")
        );
    }
}
