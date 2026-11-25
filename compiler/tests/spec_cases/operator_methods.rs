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
fn make(counter: mut i32, v: i32) -> Number {
    #counter = .counter * 10 + v
    return Number { v: v }
}
fn main() -> u8 {
    arena {
        fix counter = new (0 as i32)
        fix result = make(counter, 1) + make(counter, 2) - make(counter, 3)
        if .counter != 123 || result.v != 0 { return 1 }
        #counter = 0
        if not (make(counter, 1) < make(counter, 2) <= make(counter, 3)) { return 2 }
        if .counter != 123 { return 3 }
        #counter = 0
        if make(counter, 3) < make(counter, 2) <= make(counter, 1) { return 4 }
        if .counter != 32 { return 5 }
        fix p = new Number { v: 2 }
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
            "fn Num.<(p: mut? Num) -> bool { return true }",
            "arena { fix p = new n\nif n < p { return 1 } }",
            "operator.overload",
        ),
        ("", "fix a = n % n", "operator.overload"),
        (
            "fn Num.+(n: Num) -> Num { return n }",
            "arena { fix p = new n\nfix a = p + n }",
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
fn Num.+(p: mut Num) -> i32 { return p.v }
fn Num.+(p: mut? Num) -> i32 { let q = p { return q.v + 1 }
return 0 }
fn main() -> u8 {
    if Step.Low + Step.High != Step.High { return 1 }
    if not (Step.Low < Step.High) { return 2 }
    arena {
        fix n = Num { v: 4 }
        fix p = new n
        fix q: mut? Num = p
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
            "fn Num.len() -> len_t { return 1 }",
            "fix VALUE = len (Num { v: 1 })",
        ),
        (
            "fn Num.not() -> bool { return false }",
            "fix VALUE = not (Num { v: 1 })",
        ),
        (
            "fn Num.~() -> i32 { return 1 }",
            "fix VALUE = ~(Num { v: 1 })",
        ),
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
        for operator in [
            "+", "-", "*", "/", "%", "&", "|", "^", "<<", ">>", "<", "<=", ">", ">=",
        ] {
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
fn make(counter: mut i32, v: i32) -> Num {
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
        fix counter = new (0 as i32)
        fix n = -make(counter, 1) - make(counter, 2)
        if n.v != -3 || .counter != 12 { return 1 }
        #counter = 0
        if (-(-make(counter, 3))).v != 3 || .counter != 3 { return 2 }
        fix p = new Num { v: 4 }
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
            "operator.unary",
        ),
        ("fn Num.-() -> void {}", "", "operator.method"),
        ("fn Num#-() -> Num { return self }", "", "operator.method"),
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
        ("", "fix a = -n", "operator.unary"),
        (
            "fn Num.-(a: Num) -> Num { return a }",
            "fix a = -n",
            "operator.unary",
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
            "arena { fix p = new n\nfix a = -p }",
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
    suz::compile(b"struct Ref { p: mut i32 }\nfn Ref.-() -> mut i32 { return self.p }\nfn main() -> u8 { return 0 }").unwrap();
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

#[test]
fn compound_operator_methods_update_locals_fields_and_reference_captures() {
    let output = compile_and_run_c23(
        r#"
struct Num { v: i32 }
struct Body { pos: Num }
fn Num.+(n: Num) -> Num { return Num { v: self.v + n.v } }
fn Num.-(n: Num) -> Num { return Num { v: self.v - n.v } }
fn Num.*(k: int) -> Num { return Num { v: self.v * k } }
fn Num./(k: i32) -> Num { return Num { v: self.v / k } }
enum Step: u8 { Low, High }
fn Step.+(n: Step) -> Step { return n }
fnshape Action() -> void
fn execute(f: fn Action) -> void { f() }
fn main() -> u8 {
    var n = Num { v: 10 }
    n += Num { v: 2 }
    n -= Num { v: 4 }
    n *= 3
    n /= 2
    if n.v != 12 { return 1 }
    var body = Body { pos: n }
    body#pos += Num { v: 3 }
    if body.pos.v != 15 { return 2 }
    execute(fn [var n, var body]() -> void {
        n *= 2
        body#pos -= Num { v: 5 }
    })
    if n.v != 24 || body.pos.v != 10 { return 3 }
    var captured = Num { v: 1 }
    execute(fn [var captured]() -> void { captured += Num { v: 2 } })
    if captured.v != 3 { return 7 }
    var step = Step.Low
    step += Step.High
    if step != Step.High { return 4 }
    arena {
        fix p = new Body { pos: Num { v: 5 } }
        p#pos *= 3
        if p.pos.v != 15 { return 5 }
        fix q = new Num { v: 8 }
        #q /= 2
        if q.v != 4 { return 6 }
    }
    return 0
}
"#,
        "compound-operators.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn compound_operator_methods_save_destination_and_read_before_rhs() {
    let output = compile_and_run_c23(
        r#"
struct Num { v: i32 }
struct Body { pos: Num }
fn Num.+(n: Num) -> Num { return Num { v: self.v + n.v } }
fn select(p: mut Body, count: mut i32) -> mut Body {
    #count = .count * 10 + 1
    return p
}
fn main() -> u8 {
    arena {
        fix count = new (0 as i32)
        fix first = new Body { pos: Num { v: 10 } }
        fix second = new Body { pos: Num { v: 20 } }
        var p = first
        select(p, count)#pos += expr {
            p = second
            first#pos = Num { v: 99 }
            #count = .count * 10 + 2
            Num { v: 3 }
        }
        if .count != 12 || first.pos.v != 13 || second.pos.v != 20 || p.pos.v != 20 { return 1 }
        p#pos += expr {
            p = first
            second#pos = Num { v: 88 }
            Num { v: 4 }
        }
        if second.pos.v != 24 || p.pos.v != 13 { return 2 }
        fix n = new Num { v: 7 }
        #n += expr { #n = Num { v: 77 }
            Num { v: 2 } }
        if n.v != 9 { return 3 }
    }
    return 0
}
"#,
        "compound-operator-order.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn compound_operator_methods_require_same_result_type_and_mutable_target() {
    for (method, statement, rule) in [
        (
            "fn Num.+(n: Num) -> Num { return n }",
            "fix n = Num { v: 1 }\nn += n",
            "var.fix-assign",
        ),
        (
            "fn Num.+(n: Num) -> Num { return n }",
            "fix b = Body { pos: Num { v: 1 } }\nb#pos += Num { v: 2 }",
            "var.fix-assign",
        ),
        (
            "fn Num.+(n: Num) -> Num { return n }",
            "var n = Num { v: 1 }\nn += expr { n = Num { v: 2 }\nNum { v: 3 } }",
            "assign.rhs-write",
        ),
        (
            "fn Num.+(n: Num) -> i32 { return 1 }",
            "var n = Num { v: 1 }\nn += n",
            "operator.compound",
        ),
        (
            "fn Num.+(n: Num) -> Other { return Other { v: 1 } }",
            "var b = Body { pos: Num { v: 1 } }\nb#pos += b.pos",
            "operator.compound",
        ),
        (
            "fn Num.*(n: int) -> int { return n }",
            "var n = Num { v: 1 }\nn *= 2",
            "operator.compound",
        ),
        (
            "fn Num.+(n: Num) -> i32 { return 1 }",
            "arena { fix p = new Num { v: 1 }\n#p += .p }",
            "operator.compound",
        ),
        (
            "fn Num.+(n: Num) -> i32 { return 1 }",
            "arena { fix p = new Body { pos: Num { v: 1 } }\np#pos += p.pos }",
            "operator.compound",
        ),
        (
            "fn Num.+(n: Num) -> i32 { return 1 }",
            "var n = Num { v: 1 }\nexecute(fn [var n]() -> void { n += n })",
            "operator.compound",
        ),
    ] {
        let source = format!(
            "struct Num {{ v: i32 }}\nstruct Other {{ v: i32 }}\nstruct Body {{ pos: Num }}\nfnshape Action() -> void\nfn execute(f: fn Action) -> void {{ f() }}\n{method}\nfn main() -> u8 {{ {statement}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.kind,
            suz::ErrorKind::InvalidSource,
            "{source}\n{error}"
        );
        assert_eq!(error.rule(), rule, "{source}\n{error}");
    }
}

#[test]
fn compound_operator_methods_preserve_rhs_return_handlers() {
    let output = compile_and_run_c23(
        r#"
struct Num { v: i32 }
fn Num.*(k: i32) -> Num { return Num { v: self.v * k } }
error Err { Bad }
fn amount(d: i32) -> i32 or Err {
    if d == 0 { fail Err.Bad }
    return 6 / d
}
fn update(p: mut Num, d: i32) -> u8 {
    #p *= (expr { #p = Num { v: 99 }
        6 } / d or return 7)
    return 0
}
fn local(d: i32) -> i32 {
    var n = Num { v: 7 }
    n *= amount(d) or return -1
    return n.v
}
fn main() -> u8 {
    arena {
        fix p = new Num { v: 7 }
        if update(p, 2) != 0 || p.v != 21 { return 1 }
        #p = Num { v: 7 }
        if update(p, 0) != 7 || p.v != 99 { return 2 }
    }
    if local(2) != 21 || local(0) != -1 { return 3 }
    return 0
}
"#,
        "compound-operator-handlers.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn operator_methods_extend_value_operations_and_compound_assignments() {
    let output = compile_and_run_c23(
        r#"
struct Bits { v: u32 }
fn Bits.%(n: u32) -> Bits { return Bits { v: self.v % n } }
fn Bits.&(n: u32) -> Bits { return Bits { v: self.v & n } }
fn Bits.|(n: u32) -> Bits { return Bits { v: self.v | n } }
fn Bits.^(n: u32) -> Bits { return Bits { v: self.v ^ n } }
fn Bits.<<(n: u32) -> Bits { return Bits { v: self.v << n } }
fn Bits.>>(n: u32) -> Bits { return Bits { v: self.v >> n } }
fn Bits.<<(n: Bits) -> Bits { return Bits { v: self.v << n.v } }
fn Bits.~() -> Bits { return Bits { v: ~self.v } }
fn Bits.not() -> bool { return self.v == 0 }
fn Bits.len() -> len_t { return self.v as len_t or 0 }
fn make(p: mut i32, n: u32) -> Bits { #p += 1
return Bits { v: n } }
fn main() -> u8 {
    var b = Bits { v: 31 }
    b %= 16
    b &= 7
    b |= 8
    b ^= 3
    b <<= 2
    b >>= 1
    if b.v != 24 { return 1 }
    if (b << (Bits { v: 1 })).v != 48 { return 2 }
    if (b & 15 | 1 ^ 2).v != 11 { return 3 }
    if (~~b).v != 24 { return 4 }
    if not b { return 5 }
    if not (not (Bits { v: 0 })) { return 6 }
    fix size: len_t = len b + 1
    if size != 25 { return 7 }
    arena {
        fix count = new (0 as i32)
        if (len make(count, 3)) != 3 || .count != 1 { return 8 }
        if not make(count, 1) || .count != 2 { return 9 }
        if (~make(count, 0)).v != 4294967295 || .count != 3 { return 10 }
        fix p = new b
        #p %= 5
        #p <<= 1
        #p |= 1
        if (.p).v != 9 { return 11 }
        if (len .p) != 9 { return 12 }
    }
    return 0
}
"#,
        "extended-operators.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn operator_methods_extended_results_drive_outer_type_inference() {
    let output = compile_and_run_c23(
        r#"
struct Num { v: i32 }
fn Num.~() -> float { return 0.5 }
fn Num.%(n: f32) -> f32 { return n }
fn Num.&(n: Num) -> i32 { return self.v + n.v }
fn Num.>>(n: int) -> int { return n * 2 }
fn Num.^(n: i32) -> i32 { return n }
fn main() -> u8 {
    fix n = Num { v: 3 }
    fix a: f32 = (~n) + 0.5
    if a != 1.0 { return 1 }
    if n % 0.5 != 0.5 { return 2 }
    fix b: u8 = (n & n) + 250 or 7
    if b != 7 { return 3 }
    fix c: i32 = n >> 4
    if c != 8 { return 4 }
    if n ^ (6 / 2 or 1) != 3 { return 5 }
    return 0
}
"#,
        "extended-operator-types.suz",
    );
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn operator_methods_enforce_eligibility_and_unary_contracts() {
    for (decl, body, rule) in [
        ("fn Num.not() -> i32 { return 0 }", "", "operator.method"),
        ("fn Num.len() -> i32 { return 0 }", "", "operator.method"),
        (
            "fn Num.~(n: Num) -> Num { return n }",
            "",
            "operator.method",
        ),
        (
            "fn Num.not(n: Num) -> bool { return true }",
            "",
            "operator.method",
        ),
        (
            "fn Num.len(n: Num) -> len_t { return 0 }",
            "",
            "operator.method",
        ),
        (
            "fn Num.~() -> Num { return self }\nfn Num.~() -> Num { return self }",
            "",
            "operator.unary",
        ),
        (
            "fn Num.not() -> bool { return true }",
            "fix a = ~n",
            "operator.unary",
        ),
        (
            "fn Num.~() -> Num { return self }",
            "fix a = ~n or n",
            "operator.arith-boundary",
        ),
        (
            "fn Num.len() -> len_t { return 0 }",
            "fix a = len n or 0",
            "operator.arith-boundary",
        ),
        (
            "fn Num.%(k: i32) -> i32 { return k }",
            "var b = n\nb %= 2",
            "operator.compound",
        ),
        (
            "fn Num.<<(k: i32) -> Num { return self }",
            "fix b = n << 0.5",
            "operator.overload",
        ),
        (
            "fn Num.as%(k: Num) -> Num { return self }",
            "",
            "operator.method",
        ),
        (
            "fn Num.&&(k: Num) -> bool { return true }",
            "",
            "operator.method",
        ),
        ("fn Num.as() -> i32 { return 0 }", "", "operator.method"),
        (
            "fn Num.==(k: Num) -> bool { return true }",
            "",
            "operator.no-equality",
        ),
    ] {
        let source = format!(
            "struct Num {{ v: i32 }}\n{decl}\nfn main() -> u8 {{ fix n = Num {{ v: 1 }}\n{body}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{source}\n{error}");
    }
}

#[test]
fn missing_unary_operator_diagnostics_distinguish_user_types_from_builtins() {
    for (declaration, value) in [
        ("struct Num { v: i32 }", "Num { v: 1 }"),
        ("enum Num: u8 { A, B }", "Num.A"),
    ] {
        // 定義の有無によって変わる検査経路の両方で、同じ診断を要求する。
        for extra in ["", "fn Num.+(n: Num) -> Num { return n }"] {
            for op in ["-", "-%", "~", "not", "len"] {
                let source = format!(
                    "{declaration}\n{extra}\nfn main() -> u8 {{ fix a = {value}\nfix b = {op} a\nreturn 0 }}"
                );
                let error = suz::compile(source.as_bytes()).unwrap_err();
                assert_eq!(error.rule(), "operator.unary", "{source}\n{error}");
                assert!(
                    error
                        .to_string()
                        .contains(&format!("no unary operator {op} defined for this type")),
                    "{source}\n{error}"
                );
            }
        }
    }
    for (op, value, rule) in [
        ("~", "1.0", "expr.operator-types"),
        ("not", "1", "expr.not"),
        ("len", "1", "len.targets"),
        ("~", "p", "expr.operator-types"),
        ("not", "p", "expr.not"),
        ("len", "p", "len.targets"),
    ] {
        let source = format!(
            "fn main() -> u8 {{ arena {{ fix p = new (1 as i32)\nfix b = {op} {value} }}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{source}\n{error}");
    }
}

#[test]
fn wrapping_shift_and_euclid_operator_methods_and_needs_work() {
    let output = compile_and_run_c23(
        r#"struct Phase { raw: u8 }

fn Phase.+%(other: Phase) -> Phase { return Phase { raw: self.raw +% other.raw } }
fn Phase.-%(other: Phase) -> Phase { return Phase { raw: self.raw -% other.raw } }
fn Phase.-%() -> Phase { return Phase { raw: -%self.raw } }
fn Phase.*%(k: u8) -> Phase { return Phase { raw: self.raw *% k } }
fn Phase.%%(period: u8) -> Phase { return Phase { raw: self.raw %% period or 0 } }
fn Phase.<<%(n: u8) -> Phase { return Phase { raw: self.raw <<% n } }
fn Phase.>>%(n: u8) -> Phase { return Phase { raw: self.raw >>% n } }
fn Phase.>>>(n: u8) -> Phase { return Phase { raw: self.raw >>> n or 0 } }
fn Phase.>>>%(n: u8) -> Phase { return Phase { raw: self.raw >>>% n } }

@needs T: +%(T) -> T
@needs T: -%() -> T
fn wrap_add_neg[T](a: T, b: T) -> T {
    return a +% (-%b)
}

@needs T: <<%(u8) -> T
@needs T: >>>%(u8) -> T
fn wrap_roundtrip[T](x: T) -> T {
    return (x <<% 1) >>>% 1
}

@needs T: %%(T) -> T
fn euclid_mod[T](a: T, b: T) -> T {
    return a %% b
}

fn main() -> u8 {
    var p = Phase { raw: 250 }
    p +%= Phase { raw: 10 }
    if p.raw != 4 { return 1 }
    p -%= Phase { raw: 10 }
    if p.raw != 250 { return 2 }
    if (-%p).raw != 6 { return 3 }
    p *%= 2
    if p.raw != 244 { return 4 }
    p %%= 10
    if p.raw != 4 { return 5 }
    p <<%= 2
    if p.raw != 16 { return 6 }
    p >>%= 1
    if p.raw != 8 { return 7 }
    p >>>= 1
    if p.raw != 4 { return 8 }
    p >>>%= 9
    if p.raw != 2 { return 9 }

    // 構造体と組み込み型の両方で @needs が機能する
    fix g_phase = wrap_add_neg(Phase { raw: 10 }, Phase { raw: 15 })
    if g_phase.raw != 251 { return 10 }
    fix g_u8: u8 = wrap_add_neg(10 as u8, 15 as u8)
    if g_u8 != 251 { return 11 }
    fix g_rt_phase = wrap_roundtrip(Phase { raw: 129 })
    if g_rt_phase.raw != 1 { return 12 }
    fix g_rt_i8: i8 = wrap_roundtrip(-127 as i8)
    if g_rt_i8 != 1 { return 13 }
    fix g_euc_f64: f64 = euclid_mod(-0.25, 1.0)
    if g_euc_f64 != 0.75 { return 14 }

    return 0
}
"#,
        "wrap-op-methods",
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}
