use crate::integer_bounds::INTEGER_BOUNDS;
use suz::{ArithmeticMode, Diagnostic};

fn check(source: &str) -> Result<Vec<suz::Warning>, Diagnostic> {
    suz::check_source(source.as_bytes(), ArithmeticMode::Default)
}

#[test]
fn generic_instantiations_agree_with_explicit_scalar_functions() {
    // [num.fn-check-judge], [num.fn-body]: an accepted generic body must have
    // the same acceptance as a scalar copy for each actual substitution.
    // A body that passes generic checking can fail after substitution only
    // at a judgment deferred by that generic check.
    // Casts here either have handlers or are always safe, so the universal
    // generic cast policy does not intentionally differ from the scalar policy.
    for expression in [
        "n + x",
        "n * x",
        "n / x",
        "x / n",
        "n == x",
        "n < x",
        "n +% 300",
        "n << 1",
        "~n",
        "-n",
        "n + true",
        "x && true",
        "(n + 1) * x",
        "x + (n / 2)",
        "n as float or 0.0",
        "x as int or 0",
        "ifexpr true { n } else { x }",
        "ifexpr true { x } else { n }",
        "ifexpr n == 0 { n } else { 300 }",
        "expr { fix a = n +% 300\na }",
        "`v=${n},${x}`",
        "`abc`[n] or 'x'",
        "n as u8 or 0",
        "x as f32 or 0.0",
    ] {
        let generic = format!("fn f(n: int, x: float) -> void {{ fix r = {expression} }}\n");
        let unused = check(&format!("{generic}fn main() -> u8 {{ return 0 }}"));
        let mut all_scalar_valid = true;
        for (integer, _, _) in INTEGER_BOUNDS {
            for float in ["f32", "f64"] {
                let main = format!(
                    "fn main() -> u8 {{ fix n: {integer} = 1\nfix x: {float} = 1.0\nf(n,x)\nreturn 0 }}"
                );
                let scalar = generic
                    .replace(": int", &format!(": {integer}"))
                    .replace(": float", &format!(": {float}"))
                    .replace("as int", &format!("as {integer}"))
                    .replace("as float", &format!("as {float}"));
                let ordinary = check(&format!("{scalar}{main}"));
                let instantiated = check(&format!("{generic}{main}"));
                all_scalar_valid &= ordinary.is_ok();
                if unused.is_ok() {
                    assert_eq!(
                        instantiated.is_ok(),
                        ordinary.is_ok(),
                        "{expression}, {integer}/{float}: generic={instantiated:?}, scalar={ordinary:?}"
                    );
                }
            }
        }
        if all_scalar_valid {
            unused.unwrap_or_else(|error| panic!("all scalar types accept {expression}: {error}"));
        }
    }
}

#[test]
fn mixed_branch_hints_preserve_scalar_literal_and_operator_rules() {
    for value in [
        "1",
        "-0",
        "300",
        "1.0",
        "n + n",
        "x + x",
        "-x",
        "x / 2",
        "n +% 1",
        "n << 1",
        "~n",
        "n as float or 0",
        "x as int or 0",
        "expr { 1 }",
        "ifexpr true { 1 } else { 2 }",
    ] {
        for expression in [
            format!("ifexpr true {{ {value} }} elif false {{ n }} else {{ x }}"),
            format!("ifexpr true {{ n }} elif false {{ {value} }} else {{ x }}"),
            format!("ifexpr true {{ x }} elif false {{ n }} else {{ {value} }}"),
        ] {
            let body = format!("fn f(n: int, x: float) -> void {{ fix value = {expression} }}\n");
            for (integer, _, _) in INTEGER_BOUNDS {
                for float in ["f32", "f64"] {
                    let main = format!(
                        "fn main() -> u8 {{ fix n: {integer} = 1\nfix x: {float} = 1.0\nf(n, x)\nreturn 0 }}"
                    );
                    let scalar = body
                        .replace(": int", &format!(": {integer}"))
                        .replace(": float", &format!(": {float}"))
                        .replace("as int", &format!("as {integer}"))
                        .replace("as float", &format!("as {float}"));
                    let expected = check(&format!("{scalar}{main}"));
                    let actual = check(&format!("{body}{main}"));
                    assert_eq!(
                        actual.is_ok(),
                        expected.is_ok(),
                        "{expression}, {integer}/{float}: generic={actual:?}, scalar={expected:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn generic_argument_type_hints_preserve_value_block_bindings() {
    // Moving the binding into an expr block preserves its inferred type.
    for (integer, _, _) in INTEGER_BOUNDS {
        for expression in [
            "n",
            "1",
            "1 + 2",
            "n + 2",
            "n +% 300",
            "n << 1",
            "n as u16 or 0",
            "ifexpr true { n } else { 0 }",
            "expr { n }",
            "expr { 1 }",
        ] {
            for annotation in [String::new(), format!(": {integer}")] {
                let prefix = format!(
                    "fn id(v: int) -> int {{ return v }}\nfn main() -> u8 {{ fix n: {integer} = 1\n"
                );
                let flat =
                    format!("{prefix}fix v = {expression}\nfix r{annotation} = id(v)\nreturn 0 }}");
                let nested = format!(
                    "{prefix}fix r{annotation} = id(expr {{ fix v = {expression}\nv }})\nreturn 0 }}"
                );
                let a = check(&flat);
                let b = check(&nested);
                assert_eq!(a.is_ok(), b.is_ok(), "{nested}: flat={a:?}, nested={b:?}");
            }
        }
    }
    // Acceptance alone cannot detect an unintended wider specialization. The
    // wrapping square and f32 rounding make a changed width observable.
    let mut source = String::from(
        "fn square(n: int) -> int { return n *% n }\nfn increment(x: float) -> float { return x + 1.0 }\nfn main() -> u8 {\n",
    );
    for (ty, _, _) in INTEGER_BOUNDS {
        source.push_str(&format!("fix n_{ty}: {ty} = 100\nfix flat_{ty} = square(n_{ty})\nfix nested_{ty} = square(expr {{ fix local_{ty} = n_{ty}\nlocal_{ty} }})\nif flat_{ty} != nested_{ty} {{ return 1 }}\n"));
    }
    for ty in ["f32", "f64"] {
        source.push_str(&format!("fix n_{ty}: {ty} = 16777216.0\nfix flat_{ty} = increment(n_{ty})\nfix nested_{ty} = increment(expr {{ fix local_{ty} = n_{ty}\nlocal_{ty} }})\nif flat_{ty} != nested_{ty} {{ return 2 }}\n"));
    }
    source.push_str("return 0\n}");
    let result = crate::test_util::compile_and_run_c23(&source, "generic-block-width.suz");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn generic_constant_casts_keep_dependencies_inside_concrete_results() {
    // [num.fn-cast]: intermediate concrete types do not erase a constant's
    // dependence on an inner int/float cast. Judge every resulting value.
    for (expression, safe) in [
        ("((1 as% int) as u16 or 0) as u8", true),
        ("(((1 as% int) << 1) as u16 or 0) as u8", true),
        ("((65536 as% int) as u16 or 0) as u8", true),
        ("((300 as% int) as u16 or 0) as u8", false),
        ("((1.0 as float or 0.0) as i64 or 0) as u8", true),
        ("((300.0 as float or 0.0) as i64 or 0) as u8", false),
    ] {
        let source = format!(
            "fn f(n: int, x: float) -> void {{\n    fix r = {expression}\n}}\nfn main() -> u8 {{ return 0 }}"
        );
        let result = check(&source);
        if safe {
            result.unwrap_or_else(|e| panic!("{expression}: {e}"));
        } else {
            let error = result.unwrap_err();
            let column = "    fix r = ".len() + expression.rfind("as u8").unwrap() + 1;
            assert_eq!(
                (error.rule(), error.line, error.column),
                ("num.fn-cast", 2, column)
            );
        }
    }
}

#[test]
fn generic_symbolic_constants_are_substituted_before_formatting() {
    let source = r#"fn render(n: int, x: float) -> symbol {
    return `${(1 as% int) << 1},${((1 as% int) as u16 or 0) as u8},${(1.0 as float or 0.0) + 2.0}`
}
fn main() -> u8 {
    fix a: u8 = 1
    fix b: f32 = 1.0
    log info `${render(a,b)}`
    return 0
}"#;
    let result = crate::test_util::compile_and_run_c23(source, "generic-constants.suz");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "info generic-constants.suz:7: 2,1,3.0\n"
    );
}

#[test]
fn invalid_generic_bodies_preserve_all_earlier_deferred_judgments() {
    // Appending a universal error must not erase an earlier deferred rule.
    // Compare with the same prefix in a valid generic body, whose concrete
    // instance is checked by the ordinary checker.
    for prefix in [
        "fix value = helper(n)",
        "fix value = helper(helper(n))",
        "fix value = helper(1)",
        "f(n, x)\nfix small: u8 = n",
        "fix value: i64 = n",
        "fix value: u8 = n",
        "fix value: f32 = x",
        "fix value: f64 = n",
        "fix value: int = 300",
        "fix value: float = 1.0e300",
        "fix value = n +% 300",
        "fix value = n + x",
        "fix value = x + n",
        "fix value: f32 = n + n",
        "fix wide: u64 = 1\nfix value = n + wide",
        "fix value: u16 = ~n",
        "fix value: u16 = n << 1",
        "fix value = n * n * n",
        "fix value = ifexpr true { n } else { 300 }",
        "fix value = ifexpr true { n } else { x }",
        "fix value = ifexpr true { 1 } elif false { n } else { x }",
        "fix value = ifexpr true { n } elif false { x + x } else { x }",
        "fix value = ifexpr true { n + n } else { x }",
        "fix value = ifexpr true { n } elif false { 1.0 } else { x }",
        "fix value = ifexpr true { n } elif false { ~n } else { x }",
        "fix value = -(ifexpr true { n } elif false { 300 } else { x })",
        "fix value = -(ifexpr true { n } else { expr { fix local = x\nlocal } })",
        "fix value = -(ifexpr true { n } else { x as float or 0 })",
        "fix value = ifexpr true { x } else { n }",
        "fix value = `value=${expr { ifexpr true { x } else { n } }}`",
        "fix value = (ifexpr true { x } else { n }) as f32 or 0",
        "fix value = ((300 as% int)) == x",
        "fix value = (ifexpr true { 300 } else { (300 as% int) }) >= (300 as f32 or 0)",
        "fix value = expr { fix small: i8 = n\nsmall }",
        "for i in n..<300 {}",
        "match n { 300 => {}\n_ => {} }",
        "match n { 0 => {}\n(256 as% int) => {}\n_ => {} }",
        "match n { -0 => {}\n_ => {} }",
        "match n { -1 => {}\n_ => {} }",
        "match n { (-0) => {}\n_ => {} }",
        "match n { 150 + 150 => {}\n_ => {} }",
    ] {
        for (integer, _, _) in INTEGER_BOUNDS {
            for float in ["f32", "f64"] {
                let main = format!(
                    "fn main() -> u8 {{\nfix n: {integer} = 1\nfix x: {float} = 1.0\nf(n, x)\nfix invalid: bool = true\nreturn 0\n}}\n"
                );
                let helper =
                    "\nfn helper(value: int) -> int { fix small: u8 = value\nreturn value }";
                let before = check(&format!(
                    "{main}{helper}\nfn f(n: int, x: float) -> void {{\n{prefix}\n}}"
                ));
                if before.as_ref().is_err_and(|e| e.rule() != "num.fn-body") {
                    continue;
                }
                let after = check(&format!(
                    "{}{helper}\nfn f(n: int, x: float) -> void {{\n{prefix}\nfix impossible: text = 1\n}}",
                    main.replace("bool = true", "bool = 1")
                ))
                .unwrap_err();
                match before {
                    Err(expected) => {
                        assert_eq!(
                            (after.rule(), after.line, after.column),
                            (expected.rule(), expected.line, expected.column),
                            "{prefix}, {integer}/{float}: {after}"
                        );
                        let positions = |error: &Diagnostic| {
                            error
                                .related
                                .iter()
                                .map(|e| {
                                    (
                                        e.rule().to_owned(),
                                        e.line,
                                        e.column,
                                        e.message().to_owned(),
                                    )
                                })
                                .collect::<Vec<_>>()
                        };
                        assert_eq!(
                            positions(&after),
                            positions(&expected),
                            "{prefix}, {integer}/{float}"
                        );
                    }
                    Ok(_) => assert_eq!(
                        (after.rule(), after.line),
                        ("type.expected", 5),
                        "{prefix}, {integer}/{float}: {after}"
                    ),
                }
            }
        }
    }
}
