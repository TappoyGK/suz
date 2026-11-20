use crate::test_util::compile_and_run_c23;

#[test]
fn operator_methods_preserve_calls_types_and_integer_boundaries() {
    let output = compile_and_run_c23(
        r#"
struct Vec2 { x: f32, y: f32 }
fn Vec2.+(v: Vec2) -> Vec2 { return Vec2 { x: self.x + v.x, y: self.y + v.y } }
fn Vec2.*(k: f32) -> Vec2 { return Vec2 { x: self.x * k, y: self.y * k } }
fn Vec2.*(v: Vec2) -> f32 { return self.x * v.x + self.y * v.y }
fn Vec2.<(v: Vec2) -> bool { return self.x < v.x }
fn Vec2.<=(v: Vec2) -> bool { return self.x <= v.x }
struct Count { n: i32 }
fn Count.*(k: int) -> Count { return Count { n: self.n * k } }
fn Count.*(k: f32) -> Count { return Count { n: 99 } }
fn Count.+(v: Count) -> i32 { return self.n + v.n }
fn twice(n: int) -> int { return n * 2 }
fn half(n: float) -> float { return n / 2.0 }
fn main() -> u8 {
    fix a = Vec2 { x: 2.0, y: 4.0 }
    fix b = a * half(1.0)
    if b != (Vec2 { x: 1.0, y: 2.0 }) { return 1 }
    fix c = a + b * 2
    if c.x != 4.0 { return 2 }
    if not (b < a <= c == c) { return 3 }
    if a * b != 10.0 { return 4 }
    fix n = Count { n: 3 }
    if (n * twice(2)).n != 12 { return 5 }
    if (n * 0.5).n != 99 { return 6 }
    fix small: u8 = (n + n) + 250 or 7
    if small != 7 { return 7 }
    return 0
}
"#,
        "operators.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn operator_methods_sequence_operands_and_short_circuit_chains() {
    let output = compile_and_run_c23(
        r#"
struct Number { v: i32 }
fn Number.+(n: Number) -> Number { return Number { v: self.v + n.v } }
fn Number.-(n: Number) -> Number { return Number { v: self.v - n.v } }
fn Number.<(n: Number) -> bool { return self.v < n.v }
fn Number.<=(n: Number) -> bool { return self.v <= n.v }
fn make(counter: ptr i32, v: i32) -> Number {
    #counter = .counter * 10 + v
    return Number { v: v }
}
fn main() -> u8 {
    arena {
        fix ptr counter = new (0 as i32)
        fix result = make(counter, 1) + make(counter, 2) - make(counter, 3)
        if .counter != 123 || result.v != 0 { return 1 }
        #counter = 0
        if not (make(counter, 1) < make(counter, 2) <= make(counter, 3)) { return 2 }
        if .counter != 123 { return 3 }
        #counter = 0
        if make(counter, 3) < make(counter, 2) <= make(counter, 1) { return 4 }
        if .counter != 32 { return 5 }
        fix ptr p = new Number { v: 2 }
        if (.p + .p).v != 4 { return 6 }
    }
    return 0
}
"#,
        "operator-order.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn operator_methods_reject_invalid_definitions_and_calls() {
    for (declaration, body, rule) in [
        (
            "fn Num.==(n: Num) -> bool { return true }",
            "",
            "operator.no-equality",
        ),
        (
            "fn Num#+(n: Num) -> Num { return n }",
            "",
            "operator.method",
        ),
        (
            "arena fn Num.+(n: Num) -> Num { return n }",
            "",
            "operator.method",
        ),
        ("fn Num.+(n: Num) -> void {}", "", "operator.method"),
        (
            "fn Num.<(n: Num) -> Num { return n }",
            "",
            "operator.method",
        ),
        (
            "fn Num.+(n: Num) -> Num or Err { return n }",
            "",
            "operator.method",
        ),
        (
            "fn Num.*(n: i32) -> Num { return self }\nfn Num.*(n: i64) -> Num { return self }",
            "",
            "operator.overload",
        ),
        (
            "fn Num.*(n: int) -> Num { return self }\nfn Num.*(n: i32) -> Num { return self }",
            "",
            "operator.overload",
        ),
        (
            "fn Num.*(n: f32) -> Num { return self }\nfn Num.*(n: f64) -> Num { return self }",
            "",
            "operator.overload",
        ),
        (
            "fn Num.+(n: Num) -> Num { return n }\nfn Num.+(n: Num) -> i32 { return 0 }",
            "",
            "operator.overload",
        ),
        (
            "fn Num.+(n: Num) -> Num { return n }",
            "fix a = n + n or n",
            "operator.arith-boundary",
        ),
        (
            "fn Num.+(n: Num) -> i32 { return 1 }",
            "fix a = n + n or 0",
            "operator.arith-boundary",
        ),
        (
            "fn Num.*(n: i32) -> Num { return self }",
            "fix a = n * 0.5",
            "operator.overload",
        ),
        (
            "fn Num.<(p: ptr? Num) -> bool { return true }",
            "arena { fix ptr p = new n\nif n < p { return 1 } }",
            "operator.overload",
        ),
        ("", "fix a = n % n", "expr.operator-types"),
        (
            "fn Num.+(n: Num) -> Num { return n }",
            "arena { fix ptr p = new n\nfix a = p + n }",
            "expr.operator-types",
        ),
    ] {
        let source = format!(
            "struct Num {{ v: i32 }}\nerror Err {{ Bad }}\n{declaration}\nfn main() -> u8 {{ fix n = Num {{ v: 1 }}\n{body}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{source}\n{error}");
    }
}

#[test]
fn operator_methods_resolve_numeric_families_without_defaulting_literals() {
    let output = compile_and_run_c23(
        r#"
struct Scalar { v: f32 }
fn Scalar.*(v: f32) -> Scalar { return Scalar { v: self.v * v } }
fn Scalar.<(v: f32) -> bool { return self.v < v }
struct Choice { tag: i32 }
fn Choice.*(v: int) -> Choice { return Choice { tag: 1 } }
fn Choice.*(v: float) -> Choice { return Choice { tag: 2 } }
fn choose(v: int) -> i32 { return (Choice { tag: 0 } * v).tag }
fn main() -> u8 {
    fix s = Scalar { v: 2.0 }
    fix ready = s.v == 2.0
    fix a = s * (ifexpr ready { 0.5 } else { 0.25 })
    fix b = s * (matchexpr ready { true => 0.5
false => 0.25 })
    if a.v != 1.0 || b.v != 1.0 { return 1 }
    fix n: i16 = 3
    if (s * n).v != 6.0 { return 2 }
    if not (s < 3.0 < 4.0) { return 3 }
    if choose(1 as u8) != 1 || choose(2 as i32) != 1 { return 4 }
    fix c = Choice { tag: 0 }
    if (c * (ifexpr ready { 2 } else { 3 })).tag != 1 { return 5 }
    if (c * (ifexpr ready { 2 } else { 0.5 })).tag != 2 { return 6 }
    fix divisor: i32 = 0
    if (c * (n / divisor or 1)).tag != 1 { return 7 }
    return 0
}
"#,
        "operator-inference.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn operator_methods_support_enums_and_exact_pointer_overloads() {
    let output = compile_and_run_c23(
        r#"
enum Step: u8 { Low, High }
fn Step.+(n: Step) -> Step { return n }
fn Step.<(n: Step) -> bool { return self as u8 < n as u8 }
struct Num { v: i32 }
fn Num.+(p: ptr Num) -> i32 { return p.v }
fn Num.+(p: ptr? Num) -> i32 { let q = p { return q.v + 1 }
return 0 }
fn main() -> u8 {
    if Step.Low + Step.High != Step.High { return 1 }
    if not (Step.Low < Step.High) { return 2 }
    arena {
        fix n = Num { v: 4 }
        fix ptr p = new n
        fix ptr q: ptr? Num = p
        if n + p != 4 || n + q != 5 { return 3 }
    }
    return 0
}
"#,
        "operator-types.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

// APIの診断分類だけではCLIの内部エラー扱いへの退行を見逃すため、終了コードも確認する。
#[test]
fn operator_methods_in_constants_are_source_errors() {
    let directory = crate::test_util::TemporaryDirectory::new();
    for (declarations, initializer) in [
        (
            "fn Num.-() -> i32 { return 1 }",
            "fix VALUE = -(Num { v: 1 })",
        ),
        (
            "fn Num.+(n: i32) -> i32 { return n }",
            "fix VALUE: i32 = Num { v: 1 } + 1",
        ),
        (
            "fn Num.*(n: Num) -> Num { return n }",
            "fix VALUE = Num { v: 1 } * Num { v: 2 }",
        ),
        (
            "fn Num.<(n: Num) -> bool { return true }",
            "fix VALUE = Num { v: 1 } < Num { v: 2 } < Num { v: 3 }",
        ),
    ] {
        let source = format!(
            "struct Num {{ v: i32 }}\n{declarations}\n{initializer}\nfn main() -> u8 {{ return 0 }}"
        );
        let path = directory.0.join("constant.suz");
        std::fs::write(&path, &source).unwrap();
        for command in ["check", "run"] {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_suz"))
                .arg(command)
                .arg(&path)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1), "{source}\n{output:?}");
            let diagnostic = String::from_utf8(output.stderr).unwrap();
            assert!(diagnostic.contains("[const.expression]"), "{diagnostic}");
        }
    }
}

#[test]
fn operator_methods_report_accepted_types_without_float_to_integer_fallback() {
    for overload in ["", "fn Num.*(n: Tag) -> Num { return self }"] {
        let source = format!(
            "struct Num {{ v: i32 }}\nstruct Tag {{ v: i32 }}\nfn Num.*(n: i32) -> Num {{ return self }}\n{overload}\nfn main() -> u8 {{ fix a = Num {{ v: 1 }} * 0.5\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert_eq!(error.rule(), "operator.overload");
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("i32"), "{diagnostic}");
        if !overload.is_empty() {
            assert!(diagnostic.contains("Tag"), "{diagnostic}");
        }
    }
    let error = suz::compile(b"struct Num { v: i32 }\nfn Num.*(n: i32) -> Num { return self }\nfn main() -> u8 { fix k: i64 = 1\nfix a = Num { v: 1 } * k\nreturn 0 }").unwrap_err();
    assert_eq!(error.rule(), "type.expected", "{error}");
}

#[test]
fn operator_methods_comparison_chains_do_not_generate_block_helpers() {
    let source = b"struct Num { v: i32 }\nfn Num.<(n: Num) -> bool { return self.v < n.v }\nfn ordered(a: Num, b: Num, c: Num) -> bool { return a < b < c == c }\nfn main() -> u8 { return 0 }";
    let compiled = suz::compile(source).unwrap();
    assert!(!compiled.contains("suz_block_"));
}

#[test]
fn missing_operator_diagnostics_include_the_symbol_without_any_definitions() {
    for (declaration, value) in [
        ("struct Num { v: i32 }", "Num { v: 1 }"),
        ("enum Num: u8 { A, B }", "Num.A"),
    ] {
        for operator in ["+", "-", "*", "/", "<", "<=", ">", ">="] {
            let source = format!(
                "{declaration}\nfn main() -> u8 {{ fix a = {value}\nfix b = a {operator} a\nreturn 0 }}"
            );
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
            assert_eq!(error.rule(), "operator.overload");
            assert!(
                error.to_string().contains(&format!(
                    "no matching operator {operator}; accepted right-hand types: none"
                )),
                "{source}\n{error}"
            );
        }
    }
}

#[test]
fn unary_minus_methods_preserve_precedence_evaluation_and_result_types() {
    let output = compile_and_run_c23(
        r#"
struct Num { v: i32 }
fn Num.-() -> Num { return Num { v: -self.v } }
fn Num.-(n: Num) -> Num { return Num { v: self.v - n.v } }
fn make(counter: ptr i32, v: i32) -> Num {
    #counter = .counter * 10 + v
    return Num { v: v }
}
struct Scalar { v: i32 }
fn Scalar.-() -> i32 { return -self.v }
struct Fraction { v: f32 }
fn Fraction.-() -> f32 { return -self.v }
enum Direction: u8 { Left, Right }
fn Direction.-() -> Direction {
    return ifexpr self == Direction.Left { Direction.Right } else { Direction.Left }
}
fn main() -> u8 {
    arena {
        fix ptr counter = new (0 as i32)
        fix n = -make(counter, 1) - make(counter, 2)
        if n.v != -3 || .counter != 12 { return 1 }
        #counter = 0
        if (-(-make(counter, 3))).v != 3 || .counter != 3 { return 2 }
        fix ptr p = new Num { v: 4 }
        if (-(.p)).v != -4 { return 3 }
    }
    fix s = Scalar { v: 3 }
    fix k: u8 = (-s) + 260 or 7
    if k != 7 { return 4 }
    if -(-s) != 3 { return 5 }
    fix f = Fraction { v: 2.0 }
    if (-f) + 0.5 != -1.5 { return 6 }
    if -Direction.Left != Direction.Right { return 7 }
    return 0
}
"#,
        "unary-operators.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn unary_minus_methods_reject_invalid_definitions_and_call_contexts() {
    for (declaration, body, rule) in [
        (
            "fn Num.-() -> Num { return self }\nfn Num.-() -> i32 { return 0 }",
            "",
            "operator.unary-minus",
        ),
        ("fn Num.-() -> void {}", "", "operator.method"),
        ("fn Num#-() -> Num { return self }", "", "operator.method"),
        (
            "arena fn Num.-() -> Num { return self }",
            "",
            "operator.method",
        ),
        (
            "fn Num.-() -> Num or Err { return self }",
            "",
            "operator.method",
        ),
        ("fn Num.+() -> Num { return self }", "", "operator.method"),
        (
            "fn Num.-(a: Num, b: Num) -> Num { return a }",
            "",
            "operator.method",
        ),
        ("", "fix a = -n", "operator.unary-minus"),
        (
            "fn Num.-(a: Num) -> Num { return a }",
            "fix a = -n",
            "operator.unary-minus",
        ),
        (
            "fn Num.-() -> Num { return self }",
            "fix a = n - n",
            "operator.overload",
        ),
        (
            "fn Num.-() -> Num { return self }",
            "fix a = -n or n",
            "operator.arith-boundary",
        ),
        (
            "fn Num.-() -> i32 { return 1 }",
            "fix a = (-n) or 0",
            "operator.arith-boundary",
        ),
        (
            "fn Num.-() -> Num { return self }",
            "arena { fix ptr p = new n\nfix a = -p }",
            "expr.operator-types",
        ),
    ] {
        let source = format!(
            "struct Num {{ v: i32 }}\nerror Err {{ Bad }}\n{declaration}\nfn main() -> u8 {{ fix n = Num {{ v: 1 }}\n{body}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}\n{error}"
        );
        assert_eq!(error.rule(), rule, "{source}\n{error}");
    }
    let error = suz::compile(b"struct Ref { p: ptr i32 }\nfn Ref.-() -> ptr i32 { return self.p }\nfn main() -> u8 { return 0 }").unwrap_err();
    assert_eq!(error.rule(), "arena.parameter", "{error}");
}

#[test]
fn unary_minus_generates_the_same_call_as_a_named_method() {
    let source = "struct Num { v: f32 }\nfn Num.-() -> Num {\nreturn Num { v: -self.v }\n}\nfn main() -> u8 {\nfix a = Num { v: 2.0 }\nfix b = -a\nreturn ifexpr b.v == -2.0 { 0 } else { 1 }\n}";
    let named = source
        .replace("Num.-()", "Num.neg()")
        .replace("b = -a", "b = a.neg()");
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap(),
        suz::compile(named.as_bytes()).unwrap()
    );
}
