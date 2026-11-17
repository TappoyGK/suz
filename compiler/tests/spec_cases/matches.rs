use crate::{integer_bounds::INTEGER_BOUNDS, support::batch::Batch, test_util::TemporaryDirectory};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn match_preserves_integer_patterns_and_loop_control() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (ty, min, max) in INTEGER_BOUNDS {
        source.push_str(&format!(
            r#"{{
    fix low: {ty} = {min}
    fix high: {ty} = {max}
    var result: u8 = 0
    match low {{
        {max} => {{ return 1 }}
        {min} => {{ result = 1 }}
        _ => {{ return 2 }}
    }}
    match high {{
        {min} => {{ return 3 }}
        {max} => {{ result +%= 1 }}
        _ => {{ return 4 }}
    }}
    if result != 2 {{ return 5 }}
    fix selected: u8 = matchexpr high {{
        {min} => 1
        {max} => {{ 2 }}
        _ => 3
    }}
    if selected != 2 {{ return 11 }}
}}
"#
        ));
    }
    source.push_str(
        r#"var count: u8 = 0
    for i in 0..<4 {
        match i {
            0 => { continue }
            2 => { break }
            _ => { count +%= 1 }
        }
        count +%= 1
    }
    if count != 2 { return 6 }
    loop {
        match true {
            false => { return 7 }
            true => { break }
        }
    }
    var target: u8 = 0
    match target {
        0 => { target = 1 }
        1 => { return 8 }
        _ => { return 9 }
    }
    fix inferred = matchexpr true {
        true => 7
        false => target
    }
    fix proof: u8 = inferred
    if proof != 7 { return 12 }
    fix float_value = matchexpr true {
        true => 1
        false => 2.5
    }
    if float_value != 1.0 { return 13 }
    var tick: u8 = 0
    while matchexpr tick {
        3 => false
        _ => true
    } {
        tick +%= 1
    }
    if tick != 3 { return 14 }
    fix nested: u8 = matchexpr (matchexpr true { true => 2
        false => 1 }) {
        1 => 1
        2 => matchexpr false { false => 3
            true => 4 }
        _ => 5
    }
    if nested != 3 { return 15 }
    match target == 1 {
        false => { return 10 }
        true => { return 0 }
    }
}
"#,
    );
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("match.c");
    fs::write(&path, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let batch = Batch::build(&directory.0, &[(0, path)], 1).unwrap();
    let output = batch.run(0).unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn discarded_expressions_and_match_targets_keep_panics_and_branch_selection() {
    let sources = [
        "fn main() -> u8 { fix zero: u8 = 0\nmatch 1 / zero { _ => {} }\nreturn 0 }",
        "fn main() -> u8 { fix zero: u8 = 0\nmatch true { true => 1 / zero\nfalse => 0 }\nreturn 0 }",
        "fn main() -> u8 { fix zero: u8 = 0\nreturn matchexpr 1 / zero { _ => 0 } }",
        "fn main() -> u8 { fix zero: u8 = 0\nreturn matchexpr true { true => 1 / zero\nfalse => 0 } }",
        "fn main() -> u8 { fix zero: u8 = 0\nmatch false { true => 1 / zero\nfalse => false }\nif false { 1 / zero }\nreturn 0 }",
        "fn main() -> u8 { fix zero: u8 = 0\nreturn matchexpr false { true => 1 / zero\nfalse => matchexpr 123 { _ => 0 } } }",
    ];
    let directory = TemporaryDirectory::new();
    let inputs: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let path = directory.0.join(format!("{index}.c"));
            fs::write(&path, suz::compile(source.as_bytes()).unwrap()).unwrap();
            (index, path)
        })
        .collect();
    let batch = Batch::build(&directory.0, &inputs, 3).unwrap();
    for index in 0..sources.len() {
        let output = batch.run(index).unwrap();
        assert!(output.stdout.is_empty());
        assert_eq!(output.status.code(), Some(if index < 4 { 101 } else { 0 }));
        if index < 4 {
            assert!(
                String::from_utf8(output.stderr)
                    .unwrap()
                    .ends_with("[rt.div-zero]\n")
            );
        } else {
            assert!(output.stderr.is_empty());
        }
    }
}

#[test]
fn match_patterns_check_constant_forms_types_and_arithmetic_diagnostics() {
    for (body, rule) in [
        (
            "fix n = matchexpr true { true => 1\nfalse => false }\nreturn 0",
            "type.expected",
        ),
        (
            "return matchexpr true { true => {}\nfalse => 0 }",
            "block.value",
        ),
        (
            "return matchexpr true { true => break\nfalse => 0 }",
            "expr.jump-prohibited",
        ),
        (
            "fix n: u16 = 2\nreturn matchexpr (n as u8 or return 1) { _ => 0 }",
            "expr.jump-prohibited",
        ),
        (
            "fix n: u16 = 2\nreturn matchexpr true { true => n as u8 or return 1\nfalse => 0 }",
            "expr.jump-prohibited",
        ),
        (
            "var n = 2\nfor i in 0..<(matchexpr n { _ => 3 }) {}\nn = 1\nreturn 0",
            "for.bound-reference",
        ),
        ("1 2\nreturn 0", "src.statement-end"),
        (
            "1 match true { true => 1\nfalse => 2 }\nreturn 0",
            "src.statement-end",
        ),
        ("match 1.0 { _ => {} }\nreturn 0", "match.target"),
        (
            "match true { 1 => {}\nfalse => {} }\nreturn 0",
            "type.expected",
        ),
        (
            "fix flag = true\nmatch true { true || flag => {}\nfalse => {} }\nreturn 0",
            "const.expression",
        ),
        (
            "match 0 { 1 / 0 => {}\n_ => {} }\nreturn 0",
            "arith.div-zero-always",
        ),
        (
            "fix n: u8 = 0\nmatch n { 256 => {}\n_ => {} }\nreturn 0",
            "lit.range",
        ),
        (
            "match true { true => {}\nfalse => { return 0 } }",
            "fn.fallthrough",
        ),
        (
            "fix n: u16 = 1\nmatch (n as u8 or return 2) { _ => {} }\nreturn 0",
            "expr.or-return-position",
        ),
        (
            "fix n: u16 = 1\nn as u8 or return 2\nreturn 0",
            "expr.or-return-position",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message().contains(rule), "{source}: {error:?}");
    }
    let source = br#"fix SELECTED: i64 = 1 + 1
fn main() -> u8 {
    match SELECTED {
        SELECTED => 1 + 1
        1 + 2 => 0
        _ => 0
    }
    return 0
}
"#;
    let warnings = suz::compile_with_mode(source, suz::ArithmeticMode::Warn)
        .unwrap()
        .warnings;
    assert_eq!(
        warnings.iter().map(|w| w.line).collect::<Vec<_>>(),
        [1, 4, 5]
    );
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert_eq!(error.line, 1);
    let source = br#"fn main() -> u8 {
    return matchexpr 1 + 1 {
        1 + 1 => 2 + 2
        _ => 0
    }
}
"#;
    let warnings = suz::compile_with_mode(source, suz::ArithmeticMode::Warn)
        .unwrap()
        .warnings;
    assert_eq!(
        warnings.iter().map(|w| w.line).collect::<Vec<_>>(),
        [2, 3, 3]
    );
    let source =
        b"fix VALUE = matchexpr true { true => 1\nfalse => 2 }\nfn main() -> u8 { return 0 }";
    assert!(
        suz::compile(source)
            .unwrap_err()
            .message()
            .contains("const.expression")
    );
}
