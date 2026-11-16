use crate::test_util::compile_and_run_c23;
use suz::ArithmeticMode;

#[test]
fn generic_calls_share_candidates_and_generate_only_used_instances() {
    let source = r#"fn clamp(x: int, lo: int, hi: int) -> int {
    return ifexpr x < lo { lo } elif x > hi { hi } else { x }
}
fn square(x: int) -> int { return x * x }
fn zero() -> int { return 0 }
fn first(x: int, y: float) -> int { return x }
fn scale(x: float) -> float { return x * 0.5 }
fn unused(x: int) -> int { return x }
fn main() -> u8 {
    fix a: u8 = 200
    fix wide: u16 = clamp((square(a)), 0, 50_000)
    fix narrow: u8 = clamp(zero(), 0, 10)
    fix plain = zero()
    fix f: f32 = scale(4.0)
    fix g: f64 = scale(f)
    fix mixed = first(a, f)
    fix expression: f32 = scale(4.0) + 0.5
    fix again = clamp(narrow, 0, 10)
    log info `${wide},${narrow},${plain},${f},${g},${mixed},${expression},${again}`
    return 0
}"#;
    let result = compile_and_run_c23(source, "generic.suz");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "info generic.suz:19: 40000,0,0,2.0,1.0,200,2.5,0\n"
    );
}

#[test]
fn generic_local_types_casts_and_mutual_recursion_use_concrete_types() {
    let source = r#"fn even(n: int) -> bool {
    if n == 0 { return true }
    return odd(n - 1)
}
fn odd(n: int) -> bool {
    if n == 0 { return false }
    return even(n - 1)
}
fn count(limit: int) -> int {
    var n: int = 0
    while n < limit { n += 1 }
    return n as int
}
fn convert(n: u16) -> int { return n as int or 7 }
fn wrap(n: u16) -> int { return n as% int }
fn main() -> u8 {
    fix a: u8 = 4
    fix b: u16 = 5
    fix narrow: u8 = convert(300)
    fix wrapped: u8 = wrap(300)
    fix wide: u32 = convert(300)
    return ifexpr even(a) && odd(b) && count(a) == 4 && narrow == 7 && wrapped == 44 && wide == 300 { 0 } else { 1 }
}"#;
    let result = compile_and_run_c23(source, "recursive.suz");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn generic_numeric_types_reject_invalid_places_names_and_instantiations() {
    // @ marks the exact expected diagnostic position and is removed before parsing.
    for (marked, rule) in [
        (
            "const @N: int = 1\nfn main() -> u8 { return 0 }",
            "num.places",
        ),
        (
            "struct Item { @value: float }\nfn main() -> u8 { return 0 }",
            "num.places",
        ),
        (
            "fn main() -> u8 { fix @n: int = 0\nreturn 0 }",
            "num.places",
        ),
        (
            "fn main() -> u8 { fix n = 1 @as int\nreturn 0 }",
            "num.places",
        ),
        (
            "fn f(n: int) -> int { fix @x: float = 0.0\nreturn n }\nfn main() -> u8 { return 0 }",
            "num.fn-undeclared",
        ),
        (
            "fn f(n: int) -> int { fix x = n @as float\nreturn n }\nfn main() -> u8 { return 0 }",
            "num.fn-undeclared",
        ),
        (
            "fn f(n: int) -> int { return @missing }\nfn main() -> u8 { return 0 }",
            "decl.local-scope",
        ),
        (
            "fn f(n: int) -> int { loop {} @}\nfn main() -> u8 { return 0 }",
            "fn.value-return-required",
        ),
        (
            "fn f(n: int) -> int { if n == 0 { return n } @}\nfn main() -> u8 { return 0 }",
            "fn.fallthrough",
        ),
        (
            "fn f(n: int) -> u8 { return n }\nfn main() -> u8 { fix n: u16 = 300\nreturn @f(n) }",
            "num.fn-body",
        ),
        (
            "fn f(n: int) -> int { return n }\nfn main() -> u8 { fix n = f(@1.0)\nreturn 0 }",
            "num.fn-convert",
        ),
        (
            "fn f(n: float) -> float { return n }\nfn main() -> u8 { fix n: i64 = 1\nfix x = f(@n)\nreturn 0 }",
            "num.fn-convert",
        ),
        ("fn main() -> u8 { fix @int = 1\nreturn 0 }", "src.ident"),
        (
            "fn @float() -> u8 { return 0 }\nfn main() -> u8 { return 0 }",
            "src.ident",
        ),
        (
            "fn f(@int: u8) -> u8 { return 0 }\nfn main() -> u8 { return 0 }",
            "src.ident",
        ),
        (
            "struct Item { @float: u8 }\nfn main() -> u8 { return 0 }",
            "struct.field",
        ),
        (
            "fn f(n: int) -> int { var @count = 0\nreturn n }\nfn main() -> u8 { return 0 }",
            "var.never-reassigned",
        ),
        (
            "enum Color: u8 { Red }\nfn f(n: int) -> int { fix color = @Color.Blue\nreturn n }\nfn main() -> u8 { return 0 }",
            "enum.access",
        ),
        (
            "fn f(n: int) -> int { fix value = expr { @return n\nn }\nreturn value }\nfn main() -> u8 { return 0 }",
            "expr.jump-prohibited",
        ),
        (
            "struct Item { value: u8 }\nfn f(n: int) -> int { fix item = @Item {}\nreturn n }\nfn main() -> u8 { return 0 }",
            "struct.literal",
        ),
    ] {
        marked_error(marked, rule);
    }
    // [num.fn-main] The ordinary entry-point contract owns this diagnostic.
    marked_error("fn main() -> @int { return 0 }", "prog.main");
    // Type-dependent errors in an unused function are deferred until a call.
    suz::compile(b"fn small(n: int) -> u8 { return n }\nfn main() -> u8 { return 0 }").unwrap();
}

#[test]
fn generic_cast_requirements_do_not_depend_on_the_called_type() {
    for body in [
        "return x as u8",
        "fix alias = x\nreturn alias as u8",
        "var alias: int = 0\nalias = x\nreturn alias as u8",
        "return (x + 1) as u8",
        "for n in 0..<x { fix narrowed = n as u8 }\nreturn 0",
    ] {
        let source = format!(
            "fn small(x: int) -> u8 {{ {body} }}\nfn main() -> u8 {{ fix x: u8 = 1\nreturn small(x) }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message().contains("[num.fn-cast]"),
            "{source}\n{error:?}"
        );
    }
    for source in [
        "fn f(x: int) -> int { return x as int }\nfn main() -> u8 { return f(1) }",
        "fn f(x: float) -> float { return x as float }\nfn main() -> u8 { fix x: f32 = f(1.0)\nreturn 0 }",
        "fn f(x: int) -> float { return x as float }\nfn main() -> u8 { fix x: f64 = f(1)\nreturn 0 }",
    ] {
        suz::compile(source.as_bytes()).unwrap();
    }
}

#[test]
fn generic_comparison_warnings_require_the_same_result_for_all_types() {
    let source = r#"fn positive(x: int) -> bool { return x >= 0 }
fn impossible(x: int) -> bool { return x > 18446744073709551615 }
fn main() -> u8 {
    fix x: u8 = 1
    fix a = positive(x)
    fix b = impossible(x)
    fix y: i64 = 1
    fix c = impossible(y)
    return 0
}"#;
    let warnings = suz::check_source(source.as_bytes(), ArithmeticMode::Default).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 2);
    assert!(
        warnings[0]
            .message()
            .contains("[diagnostic.constant-comparison]")
    );
}

#[test]
fn generic_resolution_respects_family_and_concrete_result_boundaries() {
    for ty in [
        "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
    ] {
        let family = if ty.starts_with('f') { "float" } else { "int" };
        let source = format!(
            "fn identity(x: {family}) -> {family} {{ return x }}\nfn main() -> u8 {{ fix a: {ty} = 1\nfix b: {ty} = identity(identity(a))\nreturn 0 }}"
        );
        suz::compile(source.as_bytes()).unwrap();
    }
    // A fixed return type separates the two choices: inner remains u8 even
    // though the outer function receives and returns u16.
    let source = b"fn inner(x: int) -> u16 { return x as u16 or 0 }\nfn outer(x: int) -> int { return x }\nfn main() -> u8 { fix a: u8 = 1\nfix b: u16 = outer(inner(a))\nreturn 0 }";
    suz::compile(source).unwrap();
    let source = b"fn first(a: int, b: int) -> int { return a }\nfn main() -> u8 { fix a: i64 = 1\nfix b: u64 = 1\nfix c = first(a, b)\nreturn 0 }";
    let error = suz::compile(source).unwrap_err();
    assert!(error.message().contains("[num.fn-resolve]"));
}

#[test]
fn generic_unused_bodies_share_ordinary_diagnostics_and_report_independent_errors() {
    for body in [
        "fix n = 1\nreturn n",
        "fix x = n\n{ fix x = n }\nreturn n",
        "n = 1\nreturn n",
        "var x = n\nreturn n",
        "return missing",
        "break\nreturn n",
        "fix t: text = 5\nreturn n",
    ] {
        let template =
            format!("fn f(n: int) -> int {{\n{body}\n}}\nfn main() -> u8 {{ return 0 }}");
        let concrete = template.replace(": int", ": u8").replace("-> int", "-> u8");
        let a = suz::compile(template.as_bytes()).unwrap_err();
        let b = suz::compile(concrete.as_bytes()).unwrap_err();
        assert_eq!(
            (a.line, a.column, &a.message()),
            (b.line, b.column, &b.message()),
            "{template}"
        );
        assert!(a.related.is_empty());
    }
    // A type-dependent failure earlier in some substitutions must not hide a
    // later type-independent error. The binding's declared type is retained.
    let source = b"fn f(n: int) -> u8 {\nfix a: u8 = n\nfix t: text = 5\nreturn a\n}\nfn main() -> u8 { return 0 }";
    let error = suz::compile(source).unwrap_err();
    assert_eq!(
        (error.rule(), error.line, error.column),
        ("type.expected", 3, 15)
    );
    let source = b"fn predicate(x: f64) -> bool { return true }\nfn f(n: int) -> int {\nif predicate(n) {\nfix t: text = 5\n}\nreturn n\n}\nfn main() -> u8 { return 0 }";
    let error = suz::compile(source).unwrap_err();
    assert_eq!(
        (error.rule(), error.line, error.column),
        ("type.expected", 4, 15)
    );
    let source = b"fn f(n: int) -> int {\nfor i in n..<300 {\nfix t: text = 5\n}\nreturn n\n}\nfn main() -> u8 { return 0 }";
    let error = suz::compile(source).unwrap_err();
    assert_eq!(
        (error.rule(), error.line, error.column),
        ("type.expected", 3, 15)
    );
    for control in [
        "while predicate(n) {\nfix t: text = 5\n}",
        "if n == 0 {} elif predicate(n) {\nfix t: text = 5\n}",
        "match n {\n300 => { fix t: text = 5 }\n_ => {}\n}",
    ] {
        let source = format!(
            "fn predicate(x: f64) -> bool {{ return true }}\nfn f(n: int) -> int {{\n{control}\nreturn n\n}}\nfn main() -> u8 {{ return 0 }}"
        );
        marked_error(&source.replace("text = 5", "text = @5"), "type.expected");
    }
    for (expression, rule) in [
        ("x @as u8", "num.fn-cast"),
        ("`5` @as% int", "symbol.cast-handler"),
        ("t @as% int", "symbol.cast-handler"),
        ("x @as% float", "expr.as-trunc"),
        ("y @as int", "num.fn-cast"),
    ] {
        let source = format!(
            "fn f(x: int, y: float, t: text) -> int {{\n    fix value = {expression}\n    return x\n}}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = marked_error(&source, rule);
        assert!(error.related.is_empty());
    }
    let source = b"fn f(x: int) -> bool {\nfix y: u8 = 5\nreturn y >= 0\n}\nfn divide(x: int) -> f64 {\nreturn (x / 2) * 1.0\n}\nfn main() -> u8 { return 0 }";
    let warnings = suz::check_source(source, ArithmeticMode::Default).unwrap();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings
            .iter()
            .any(|w| w.line == 3 && w.rule() == "diagnostic.constant-comparison")
    );
    assert!(
        warnings
            .iter()
            .any(|w| { w.line == 6 && w.rule() == "diagnostic.integer-division-to-float" })
    );
}

#[test]
fn generic_instantiation_errors_retain_the_call_chain_and_body_rule() {
    let source = b"fn inner(x: int) -> u8 {\n    return x\n}\nfn outer(x: int) -> u8 {\n    return inner(x)\n}\nfn main() -> u8 {\n    fix a: u16 = 3\n    return outer(a)\n}";
    let error = suz::compile(source).unwrap_err();
    assert_eq!(
        (error.rule(), error.line, error.column),
        ("num.fn-body", 9, 12)
    );
    assert_eq!(error.related.len(), 2);
    assert_eq!(
        (
            error.related[0].rule(),
            error.related[0].line,
            error.related[0].column
        ),
        ("num.fn-body", 5, 12)
    );
    assert_eq!(
        (
            error.related[1].rule(),
            error.related[1].line,
            error.related[1].column
        ),
        ("type.expected", 2, 12)
    );
    assert_eq!(error.message().matches('[').count(), 1);
}

#[test]
fn generic_result_conversion_and_arithmetic_contexts_follow_their_own_rules() {
    let prefix = "fn id(x: int) -> int { return x }\nfn half(x: float) -> float { return x / 2.0 }\nfn square(x: int) -> int { return x * x }\n";
    for (body, rule) in [
        ("fix a: i64 = 1\nfix f: f64 = id(a)", Some("num.fn-convert")),
        (
            "fix a: f32 = 1.0\nfix f = id(half(a))",
            Some("num.fn-convert"),
        ),
        ("fix a: u8 = 1\nfix f: f64 = id(a)", None),
        ("fix a: u8 = 1\nfix f: u8 = id(a + a)", None),
        ("fix f: u8 = id(1 + 2)", Some("num.fn-convert")),
    ] {
        let source = format!("{prefix}fn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let result = suz::compile(source.as_bytes());
        if let Some(rule) = rule {
            assert_eq!(result.unwrap_err().rule(), rule, "{source}");
        } else {
            result.unwrap();
        }
    }
    for (expression, annotation) in [("square(a)", ""), ("square(a) + 1", ": u16")] {
        let source = format!(
            "{prefix}fn main() -> u8 {{\nfix a: u8 = 200\nfix result{annotation} = {expression}\nreturn 0\n}}"
        );
        let result = compile_and_run_c23(&source, "arithmetic-context.suz");
        // square is instantiated with u8; 200 * 200 panics before any outer widening.
        assert_eq!(result.status.code(), Some(101));
    }
}

#[test]
fn generic_boundaries_preserve_inner_errors_and_reject_non_numeric_values() {
    let declarations = "fn f(n: int) -> int { return n }\nfn g(n: u8) -> u8 { return n }\nfn h(n: float) -> float { return n }\nfn pick(n: int, flag: bool) -> int { return n }\n";
    for (body, rule) in [
        ("fix n = f(g(@true))", "type.expected"),
        ("fix a: u8 = 1\nfix n = f(a + g(@true))", "type.expected"),
        ("fix n = pick(1, @3)", "type.expected"),
        ("fix n = f(@true)", "type.expected"),
        ("fix t: text = `x`\nfix n = f(@t)", "type.expected"),
        ("fix a: u8 = 1\nfix t: text = @f(a)", "type.expected"),
        ("fix n = f(@h(1.0))", "num.fn-convert"),
    ] {
        let marked = format!("{declarations}fn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = marked_error(&marked, rule);
        if rule == "num.fn-convert" {
            assert!(
                error
                    .message()
                    .starts_with("floating-point values cannot be passed to an int parameter")
            );
        }
    }
}

#[test]
fn generic_unused_checks_report_body_positions_and_return_shapes() {
    for (body, rule) in [
        ("return @true", "type.expected"),
        ("fix b: bool = @n\nreturn n", "type.expected"),
        ("fix t: text = @n\nreturn n", "type.expected"),
        ("while @n { fix b: bool = n }\nreturn n", "while.loop"),
        (
            "if n == 0 {} elif @n { fix b: bool = n }\nreturn n",
            "if.condition",
        ),
    ] {
        let source = format!("fn f(n: int) -> int {{\n{body}\n}}\nfn main() -> u8 {{ return 0 }}");
        assert!(marked_error(&source, rule).related.is_empty());
    }
    for (body, rule) in [
        ("fix a: u8 = n", "fn.value-return-required"),
        ("fix a: u8 = n\nif n == 0 { return n }", "fn.fallthrough"),
    ] {
        let source = format!("fn f(n: int) -> int {{\n{body}\n@}}\nfn main() -> u8 {{ return 0 }}");
        marked_error(&source, rule);
    }
}

#[test]
fn generic_unused_arithmetic_sites_apply_warning_modes_and_allow() {
    let source = "fn f(n: int) -> int { return n + 1 }\nfn main() -> u8 { return 0 }";
    let warnings = suz::check_source(source.as_bytes(), ArithmeticMode::Warn).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!((warnings[0].line, warnings[0].column), (1, 32));
    assert!(suz::check_source(source.as_bytes(), ArithmeticMode::Deny).is_err());
    let allowed = source.replace(
        "return n + 1",
        "return n + 1 allow [diagnostic.arithmetic: small]",
    );
    for mode in [ArithmeticMode::Default, ArithmeticMode::Warn] {
        suz::check_source(allowed.as_bytes(), mode).unwrap();
    }
    let error = suz::check_source(allowed.as_bytes(), ArithmeticMode::Deny).unwrap_err();
    assert_eq!(
        (error.rule(), error.line, error.column),
        ("diagnostic.arithmetic", 1, 32)
    );
}

// A single marker helper keeps rule, physical location and wording checks separate.
fn marked_error(marked: &str, rule: &str) -> suz::Diagnostic {
    let offset = marked.find('@').unwrap();
    let prefix = &marked[..offset];
    let location = (
        prefix.bytes().filter(|&b| b == b'\n').count() + 1,
        prefix.rsplit('\n').next().unwrap().chars().count() + 1,
    );
    let error = suz::compile(marked.replace('@', "").as_bytes()).unwrap_err();
    assert_eq!(
        (error.rule(), error.line, error.column),
        (rule, location.0, location.1),
        "{marked}: {error}"
    );
    error
}

#[test]
fn generic_check_reports_symbolic_types_and_preserves_ordinary_error_order() {
    for (marked, rule, message) in [
        (
            "fn f(n: int) -> int { return @true }\nfn main() -> u8 { return 0 }",
            "type.expected",
            "expected int, found bool",
        ),
        (
            "fn f(n: float) -> float { return @true }\nfn main() -> u8 { return 0 }",
            "type.expected",
            "expected float, found bool",
        ),
        (
            "fn f(n: int) -> int { fix b: bool = @n\nreturn n }\nfn main() -> u8 { return 0 }",
            "type.expected",
            "expected bool, found int",
        ),
        (
            "fn f(n: int) -> int { var x: int = 0\nx = @true\nreturn n }\nfn main() -> u8 { return 0 }",
            "type.expected",
            "expected int, found bool",
        ),
        (
            "fn main() -> u8 { fix b: bool = @1\nreturn 0 }\nfn f(n: int) -> int { return true }",
            "type.expected",
            "integer literal",
        ),
        (
            "fn f(n: int) -> int { fix t: text = @5\nfix b = n as u8\nreturn n }\nfn main() -> u8 { return 0 }",
            "type.expected",
            "integer literal",
        ),
        (
            "fn f(n: int) -> int { var x: int = 0\nfix b: bool = @n\nreturn n }\nfn main() -> u8 { return 0 }",
            "type.expected",
            "expected bool, found int",
        ),
        (
            "fn f(n: int) -> u16 { return 1 }\nfn main() -> u8 { fix a: u8 = 1\nfix y: u8 = @f(a)\nreturn 0 }",
            "type.expected",
            "expected u8, found u16",
        ),
        (
            "fn f(n: float) -> void {}\nfn main() -> u8 { fix a: u64 = 1\nf(@a)\nreturn 0 }",
            "num.fn-convert",
            "cannot be passed as",
        ),
    ] {
        let error = marked_error(marked, rule);
        assert!(error.message().contains(message), "{error}");
        assert!(error.related.is_empty());
    }
}

#[test]
fn generic_deferred_rules_do_not_hide_independent_expression_errors() {
    for (body, rule) in [
        ("fix k = (n +% 300) + @true", "expr.operator-types"),
        ("fix k = g(n +% 300, @true)", "type.expected"),
        (
            "fix k = ifexpr n == 0 { n +% 300 } else { @true }",
            "type.expected",
        ),
    ] {
        let source = format!(
            "fn g(x: int, y: int) -> int {{ return x }}\nfn f(n: int) -> int {{\n{body}\nreturn n\n}}\nfn main() -> u8 {{ return 0 }}"
        );
        marked_error(&source, rule);
    }
    let prefix = "fn f(n: int) -> int { return n +% 300 }\n";
    suz::compile(format!("{prefix}fn main() -> u8 {{ return 0 }}").as_bytes()).unwrap();
    let error = marked_error(
        &format!("{prefix}fn main() -> u8 {{ fix n: u8 = 1\nreturn @f(n) }}"),
        "num.fn-body",
    );
    assert_eq!(
        (
            error.related[0].rule(),
            error.related[0].line,
            error.related[0].column
        ),
        ("lit.range", 1, 35)
    );
}

#[test]
fn generic_rule_judgments_agree_with_scalar_checks() {
    for (family, types, expressions) in [
        (
            "int",
            &["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"][..],
            &[
                "x + 1",
                "x * x * x",
                "x +% 300",
                "x as u8 or 0",
                "x as% u8",
                "x + true",
                "x && true",
                "x as bool",
                "x as float",
                "ifexpr x == 0 { x } else { 300 }",
                "(1 as int) as u8",
                "`abc`[x] or 'a'",
                "x << x",
                "~x",
                "-x",
            ][..],
        ),
        (
            "float",
            &["f32", "f64"][..],
            &[
                "x + 1.0",
                "x * 1.0",
                "x as i32 or 0",
                "x + true",
                "x && true",
                "x as% u8",
                "(1.0 as float or 0.0) as i32",
            ][..],
        ),
    ] {
        for expression in expressions {
            let template = format!(
                "fn f(x: {family}) -> void {{ fix value = {expression} }}\nfn main() -> u8 {{ return 0 }}"
            );
            let generic = suz::compile(template.as_bytes());
            let concrete: Vec<_> = types
                .iter()
                .map(|ty| {
                    let source = template
                        .replace(&format!(": {family}"), &format!(": {ty}"))
                        .replace(&format!("as {family}"), &format!("as {ty}"));
                    suz::compile(source.as_bytes())
                })
                .collect();
            if concrete.iter().all(Result::is_ok) {
                generic.unwrap_or_else(|error| {
                    panic!("all concrete types accept {template}: {error}")
                });
            } else if let Err(first) = &concrete[0]
                && concrete
                    .iter()
                    .all(|r| r.as_ref().is_err_and(|e| e.rule() == first.rule()))
            {
                assert!(generic.is_err(), "every concrete type rejects {template}");
            }
        }
    }
}

#[test]
fn generic_constant_patterns_keep_type_dependent_values_and_duplicates() {
    for pattern in [
        "1 as int",
        "(1.0 as float or 0.0) as int or 0",
        "`1` as int or 0",
    ] {
        let source = format!(
            "fn f(n: int, x: float) -> int {{ match n {{ {pattern} => {{ return n }}\n_ => {{ return 0 }} }} }}\nfn main() -> u8 {{ return 0 }}"
        );
        suz::compile(source.as_bytes()).unwrap_or_else(|e| panic!("{source}: {e}"));
    }
    let source = "fn f(n: int) -> int { match n {\n256 as% int => { return n }\n0 => { return n }\n_ => { return n }\n} }\n";
    suz::compile(format!("{source}fn main() -> u8 {{ return 0 }}").as_bytes()).unwrap();
    marked_error(
        &format!("{source}fn main() -> u8 {{ fix n: u8 = 1\nreturn @f(n) }}"),
        "num.fn-body",
    );
    suz::compile(
        format!("{source}fn main() -> u8 {{ fix n: u16 = 1\nfix result = f(n)\nreturn 0 }}")
            .as_bytes(),
    )
    .unwrap();
}

#[test]
fn generic_deferred_calls_precede_later_local_errors_and_sibling_calls() {
    let prefix = "fn bad(n: int) -> int { fix w: i64 = n\nreturn n }\n";
    // A shared dependency already requested by a later sibling still belongs
    // to the earlier call path. A recursive edge cannot postpone later checks.
    for declarations in [
        "fn f(n: int) -> int { fix w: i64 = n\nreturn n }",
        "fn f(n: int) -> int { fix w = bad(n)\nreturn n }",
        "fn f(n: int) -> int { fix w = f(n)\nfix y: i64 = n\nreturn n }",
    ] {
        let marked = format!(
            "{prefix}fn main() -> u8 {{\nfix a: u64 = 1\nfix k = @f(a)\nfix other = bad(a)\nfix b: bool = 1\nreturn 0\n}}\n{declarations}"
        );
        let error = marked_error(&marked, "num.fn-body");
        assert_eq!(error.related.last().unwrap().rule(), "type.expected");
    }
    // A shared unconditional body error stays at its declaration, even if the
    // generic function is declared after its first call.
    marked_error(
        "fn main() -> u8 { fix n = f(1)\nreturn 0 }\nfn f(n: int) -> int { return @true }",
        "type.expected",
    );
    // Errors in arguments are checked before the callee's deferred rules.
    marked_error(
        "fn f(n: int, b: bool) -> int { fix w: i64 = n\nreturn n }\nfn main() -> u8 { fix a: u64 = 1\nfix k = f(a, @3)\nreturn 0 }",
        "type.expected",
    );
}
