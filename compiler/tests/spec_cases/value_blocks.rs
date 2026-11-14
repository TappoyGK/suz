use crate::test_util::compile_and_run_c11;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn value_blocks_preserve_scope_mutations_and_evaluation_order() {
    let source = r#"fn main() -> u8 {
    var isolated: u8 = 0
    expr {
        isolated = 1
        0
    }
    if isolated != 1 { return 12 }
    var count: u8 = 0
    fix seed: u8 = 2
    fix nested = expr {
        fix local = seed
        expr {
            count += local
            count
        }
    }
    if nested != 2 || count != 2 { return 1 }
    fix sum: u8 = expr {
        count += 1
        count
    } + expr {
        count *= 2
        count
    }
    if sum != 9 || count != 6 { return 2 }
    fix selected = ifexpr false {
        count = 100
        0
    } elif true {
        fix narrow: u8 = 7
        count += 1
        narrow
    } else {
        count = 200
        0
    }
    if selected != 7 || count != 7 { return 3 }
    fix matched = matchexpr expr {
        count += 1
        count
    } {
        8 => {
            fix local: u8 = 9
            count += 1
            local
        }
        _ => {
            count = 200
            0
        }
    }
    if matched != 9 || count != 9 { return 4 }
    false && expr {
        count = 100
        true
    }
    true || expr {
        count = 100
        false
    }
    if count != 9 { return 5 }
    fix chain = 0 < expr {
        count += 1
        count
    } < 20 < expr {
        count += 1
        count
    } < expr {
        count = 200
        count
    }
    if chain || count != 11 { return 6 }
    while expr {
        count -= 1
        count > 9
    } {}
    if count != 9 { return 7 }
    fix converted = seed as u8 or expr {
        count = 200
        0
    }
    fix failed = 300 as u8 or expr {
        count += 1
        count
    }
    if converted != 2 || failed != 10 || count != 10 { return 8 }
    fix divided: u8 = (1 / 0) or expr {
        count += 1
        count
    }
    if divided != 11 { return 9 }
    for i in 0..<expr {
        fix end: u8 = 2
        end
    } {
        count += expr {
            fix copy = i
            copy
        }
    }
    if count != 12 { return 10 }
    fix floating = ifexpr true {
        fix local: f32 = 1.5
        local
    } else { 2.5 }
    fix boolean = expr {
        fix local = true
        local
    }
    fix result: u8 = expr {
        123
        ifexpr floating == 1.5 && boolean { 0 } else { 11 }
    }
    return result
}
"#;
    let output = compile_and_run_c11(source, "blocks.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn value_blocks_enforce_scope_control_and_assignment_constraints() {
    for (body, rule) in [
        ("fix x = expr {}", "block.value"),
        ("fix x = expr { fix local = 1 }", "block.value"),
        ("fix x = expr { if true { 1 } }", "block.value"),
        (
            "fix x = expr {\nfix local = 1\nlocal\n}\nlocal",
            "decl.local-scope",
        ),
        (
            "fix local = 1\nfix x = expr {\nfix local = 2\nlocal\n}",
            "decl.shadowing",
        ),
        (
            "fix x = expr {\nfix local = 1\nfix local = 2\nlocal\n}",
            "decl.duplicate",
        ),
        (
            "fix x = expr {\nvar local = 1\nlocal\n}",
            "var.never-reassigned",
        ),
        ("fix outer = 1\nexpr {\nouter = 2\n0\n}", "var.fix-assign"),
        (
            "var outer = 1\nouter = expr {\nouter += 1\n0\n}",
            "assign.rhs-write",
        ),
        (
            "var outer = 1\nouter += ifexpr false {\nouter = 2\n0\n} else { 0 }",
            "assign.rhs-write",
        ),
        (
            "expr {\nwhile false { return 1 }\n0\n}",
            "expr.jump-prohibited",
        ),
        ("expr {\nloop { break }\n0\n}", "expr.jump-prohibited"),
        (
            "expr {\nfor i in 0..<1 { continue }\n0\n}",
            "expr.jump-prohibited",
        ),
        (
            "expr {\n{\nfix x = 300 as u8 or return 1\n}\n0\n}",
            "expr.jump-prohibited",
        ),
        (
            "expr {\nvar x: u8 = 0\nx += 300 as u8 or return 1\nx\n}",
            "expr.jump-prohibited",
        ),
        (
            "for i in 0..<expr {\nvar end = 1\nend += 1\nend\n} {}",
            "for.bound-reference",
        ),
        (
            "var end = 1\nfor i in 0..<expr {\nfix copy = end\ncopy\n} {}\nend = 2",
            "for.bound-reference",
        ),
        (
            "fix x: u8 = expr {\nfix local: i64 = 1\nlocal\n}",
            "type.expected",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}: {error}"
        );
        assert!(
            error.message.contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
}

#[test]
fn value_block_arithmetic_diagnostics_include_statements_and_final_value_once() {
    let source = b"fn main() -> u8 {\nfix result: u8 = expr {\nvar count: u8 = 1\ncount += 1\ncount + 1\n}\nreturn result\n}";
    let compiled = suz::compile_with_mode(source, suz::ArithmeticMode::Warn).unwrap();
    let positions: Vec<_> = compiled
        .warnings
        .iter()
        .map(|w| (w.line, w.column))
        .collect();
    assert_eq!(positions, [(4, 7), (5, 7)]);
    let error = suz::compile_with_mode(source, suz::ArithmeticMode::Deny)
        .err()
        .unwrap();
    assert_eq!((error.line, error.column), (4, 7));
    assert!(error.message.contains("[diagnostic.arithmetic]"));
}
