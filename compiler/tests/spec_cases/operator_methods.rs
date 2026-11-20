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
            "type.expected",
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
    let error = suz::compile(b"struct Num { v: i32 }\nfn Num.+(n: Num) -> Num { return n }\nfix VALUE = Num { v: 1 } + Num { v: 2 }\nfn main() -> u8 { return 0 }").unwrap_err();
    assert_eq!(error.rule(), "const.initializer", "{error}");
}
