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
    assert!(
        !c.contains("suz_fn_5("),
        "unused generic function must not be emitted"
    );
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
    for (source, rule) in [
        (
            "const N: int = 1\nfn main() -> u8 { return 0 }",
            "num.places",
        ),
        (
            "struct Item { value: float }\nfn main() -> u8 { return 0 }",
            "num.places",
        ),
        ("fn main() -> u8 { fix n: int = 0\nreturn 0 }", "num.places"),
        (
            "fn f(n: int) -> int { fix x: float = 0.0\nreturn n }\nfn main() -> u8 { return 0 }",
            "num.fn-undeclared",
        ),
        (
            "fn f(n: int) -> int { return missing }\nfn main() -> u8 { return 0 }",
            "decl.local-scope",
        ),
        (
            "fn f(n: int) -> int { loop {} }\nfn main() -> u8 { return 0 }",
            "fn.value-return-required",
        ),
        (
            "fn f(n: int) -> int { if n == 0 { return n } }\nfn main() -> u8 { return 0 }",
            "fn.fallthrough",
        ),
        (
            "fn f(n: int) -> u8 { return n }\nfn main() -> u8 { fix n: u16 = 300\nreturn f(n) }",
            "num.fn-body",
        ),
        (
            "fn f(n: int) -> int { return n }\nfn main() -> u8 { fix n = f(1.0)\nreturn 0 }",
            "type.expected",
        ),
        (
            "fn f(n: float) -> float { return n }\nfn main() -> u8 { fix n: i64 = 1\nfix x = f(n)\nreturn 0 }",
            "type.expected",
        ),
        (
            "fn f(n: int) -> int { return n }\nfn main() -> u8 { fix int = 1\nreturn 0 }",
            "src.ident",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message.contains(&format!("[{rule}]")),
            "{source}\n{error:?}"
        );
    }
    for (source, rule) in [
        (
            "fn f(n: int) -> int { var count = 0\nreturn n }\nfn main() -> u8 { return 0 }",
            "var.never-reassigned",
        ),
        (
            "enum Color: u8 { Red }\nfn f(n: int) -> int { fix color = Color.Blue\nreturn n }\nfn main() -> u8 { return 0 }",
            "enum.access",
        ),
        (
            "fn f(n: int) -> int { fix value = expr { return n\nn }\nreturn value }\nfn main() -> u8 { return 0 }",
            "expr.jump-prohibited",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message.contains(&format!("[{rule}]")),
            "{source}\n{error:?}"
        );
    }
    let source = b"struct Item { value: u8 }\nfn f(n: int) -> int { fix item = Item {}\nreturn n }\nfn main() -> u8 { return 0 }";
    assert!(
        suz::compile(source)
            .unwrap_err()
            .message
            .contains("[struct.literal]")
    );
    // Type-dependent errors in an unused function are not instantiated.
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
