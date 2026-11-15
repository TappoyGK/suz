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
    let c = suz::compile(source.as_bytes()).unwrap();
    let count_functions = |c: &str| {
        c.lines()
            .filter(|line| {
                line.starts_with("static inline ")
                    && line.contains(" suz_fn_")
                    && line.ends_with(" {")
            })
            .count()
    };
    let without_unused = source.replace("fn unused(x: int) -> int { return x }\n", "");
    assert_eq!(
        count_functions(&c),
        count_functions(&suz::compile(without_unused.as_bytes()).unwrap())
    );
    let repeated = source.replace(
        "    fix again =",
        "    fix duplicate = clamp(narrow, 0, 10)\n    fix again =",
    );
    assert_eq!(
        count_functions(&c),
        count_functions(&suz::compile(repeated.as_bytes()).unwrap())
    );
    assert!(count_functions(&c) > 1);
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
        let offset = marked.find('@').unwrap();
        let prefix = &marked[..offset];
        let line = prefix.bytes().filter(|b| *b == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap().chars().count() + 1;
        let source = marked.replacen('@', "", 1);
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            (error.rule(), error.line, error.column),
            (rule, line, column),
            "{source}: {error:?}"
        );
    }
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
            error.message.contains("[num.fn-cast]"),
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
            .message
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
    assert!(error.message.contains("[num.fn-resolve]"));
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
            (a.line, a.column, &a.message),
            (b.line, b.column, &b.message),
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
    for (expression, rule) in [
        ("x as u8", "num.fn-cast"),
        ("`5` as% int", "symbol.cast-handler"),
        ("t as% int", "symbol.cast-handler"),
        ("x as% float", "expr.as-trunc"),
        ("y as int", "num.fn-cast"),
    ] {
        let source = format!(
            "fn f(x: int, y: float, t: text) -> int {{\n    fix value = {expression}\n    return x\n}}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{source}: {error:?}");
        assert_eq!(error.line, 2);
        assert!(error.related.is_empty());
    }
    let source = b"fn f(x: int) -> bool {\nfix y: u8 = 5\nreturn y >= 0\n}\nfn divide(x: int) -> f64 {\nreturn (x / 2) * 1.0\n}\nfn main() -> u8 { return 0 }";
    let warnings = suz::check_source(source, ArithmeticMode::Default).unwrap();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings
            .iter()
            .any(|w| w.line == 3 && w.message.ends_with("[diagnostic.constant-comparison]"))
    );
    assert!(warnings.iter().any(|w| {
        w.line == 6
            && w.message
                .ends_with("[diagnostic.integer-division-to-float]")
    }));
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
    assert_eq!(error.message.matches('[').count(), 1);
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
    for expression in ["square(a)", "square(a) + 1"] {
        let annotation = if expression.contains('+') {
            ": u16"
        } else {
            ""
        };
        let source = format!(
            "{prefix}fn main() -> u8 {{\nfix a: u8 = 200\nfix result{annotation} = {expression}\nreturn 0\n}}"
        );
        let result = compile_and_run_c23(&source, "arithmetic-context.suz");
        assert_eq!(result.status.code(), Some(101));
    }
}
