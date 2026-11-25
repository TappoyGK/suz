use crate::test_util::compile_and_run_c23;

#[test]
fn struct_with_preserves_saved_values_order_and_constant_results() {
    let source = r#"struct Point { x: i32, y: i32 }
struct Player { pos: Point, name: symbol, view: text }
struct Wide { x: i64 }
fix RIGHT = ORIGIN with { x: 1 }
fix ORIGIN = Point { x: 0, y: 2 }
fix PLAYER = Player { pos: RIGHT, name: `old`, view: `abc`.[:] }
fix UPDATED = PLAYER with { pos: RIGHT with { y: 3 }, name: `new` }
struct State { point: Point, trace: i32 }
fn base(state: mut State) -> Point { state#trace = state.trace * 10 + 1
    return state.point }
fn mark(state: mut State, n: i32) -> i32 { state#trace = state.trace * 10 + n
    state.point#y = 99
    return n }
fn generic(n: int) -> Point { return ORIGIN with { x: n as i32 or 0 } }
fn accepts(p: Point) -> bool { return p.x == 8 }
fn main() -> u8 {
    arena {
    fix state = new State { point: ORIGIN with { x: 5 }, trace: 0 }
    fix snapshot = base(state) with { x: mark(state, 2) }
    if state.trace != 12 || snapshot.x != 2 || snapshot.y != 2 || state.point.y != 99 { return 1 }
    state#trace = 0
    fix ordered = base(state) with { y: mark(state, 2), x: mark(state, 3) }
    if state.trace != 123 || ordered.x != 3 || ordered.y != 2 { return 2 }
    fix x: u8 = 8
    fix chained = ORIGIN with { x } with { y: 4 }
    if chained.x != 8 || chained.y != 4 || ORIGIN.y != 2 { return 3 }
    fix nested = PLAYER with { pos: PLAYER.pos with { y: 3 }, name: `new` }
    if nested != UPDATED || PLAYER.pos.y != 2 || PLAYER.name != `old` { return 4 }
    var copy = nested
    copy.pos#x = 100
    if nested.pos.x != 1 { return 5 }
    if not accepts(ORIGIN with { x }) || (ORIGIN with { x }).x != 8 { return 6 }
    fix direct = ORIGIN with { x: 9 }.x
    if generic(7).x != 7 || direct != 9 { return 7 }
    fix sliced = PLAYER with { name: `xyz` }.name.[1:] or ``
    if sliced != `yz` || nested.view != `abc` { return 8 }
    fix block = expr { ORIGIN with { x } }
    fix branch = ifexpr x == 8 { block with { y: 7 } } else { ORIGIN }
    if branch.y != 7 { return 9 }
    var local = ORIGIN
    fix saved = local with {
        x: expr { local#y = 20
            10 },
    }
    if saved.x != 10 || saved.y != 2 || local.y != 20 { return 10 }
    // withで作った定数と、その場で書いた定数式の両方をパターンに使える。
    match 1 as i32 { RIGHT.x => {}
        _ => { return 11 } }
    match 2 as i32 { (ORIGIN with { x: 2 }).x => {}
        _ => { return 12 } }
    fix narrow: i8 = -7
    fix wide = (Wide { x: 0 }) with { x: narrow }
    if wide.x != -7 { return 13 }
    fix casted = ORIGIN with { x: 3 }.x as u8 or 9
    fix sum = base(state) with { x: 3 }.x + ORIGIN with { y: 1 }.y * 2
    if casted != 3 || sum != 5 { return 14 }
    return 0
    }
}
"#;
    let output = compile_and_run_c23(source, "with.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn struct_with_rejects_invalid_targets_fields_and_hidden_writes() {
    for (body, rule) in [
        ("fix q = 1 with { x: 2 }", "struct.with-target"),
        ("fix q = p with {}", "struct.with-fields"),
        ("fix q = p with { z: 1 }", "struct.with-fields"),
        ("fix q = p with { x, x: 2 }", "struct.with-fields"),
        ("fix q = p with { x: true }", "type.expected"),
        (
            "fix large: i64 = 1\nfix q = p with { x: large }",
            "type.expected",
        ),
        ("fix q = p with { y }", "decl.local-scope"),
        ("fix q = p with { x: 1, }", "src.trailing-comma"),
        ("fix q = p with {\nx: 1\n}", "src.trailing-comma"),
        ("fix q = p\nwith { x: 1 }", "syntax.grammar"),
        ("fix q = p with\n{ x: 1 }", "syntax.grammar"),
        ("fix q = (p\nwith { x: 1 })", "struct.with-line"),
        ("fix q = p with { x.y: 1 }", "struct.with-nested"),
        ("fix with = 1", "src.ident"),
        ("p = p with { x: expr { p#y = 1\n2 }, }", "assign.rhs-write"),
        (
            "fix q = p with { x: 300 as u8 or return 0 }",
            "expr.or-return-position",
        ),
    ] {
        let source = format!(
            "struct Point {{ x: i32, y: i32 }}\nfn main() -> u8 {{\nfix x: i32 = 1\nvar p = Point {{ x, y: 0 }}\np#x = 2\n{body}\nreturn 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{source}: {error}");
    }
    // 差し替えるフィールドでも、コピー元を評価する際のエラーを無視しない。
    let source = b"struct Point { x: u8 }\nfix BAD = (Point { x: 1 / 0 }) with { x: 1 }\nfn main() -> u8 { return 0 }";
    assert_eq!(
        suz::compile(source).unwrap_err().rule(),
        "arith.div-zero-always"
    );
    // 関数呼び出しは、withのコピー元でも定数式にはならない。
    let source = "struct Point { x: i32 }\nfix ORIGIN = Point { x: 0 }\nfix BAD = id(ORIGIN) with { x: 1 }\nfn id(p: Point) -> Point { return p }\nfn main() -> u8 { return 0 }";
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap_err().rule(),
        "const.expression",
        "{source}"
    );
}

#[test]
fn struct_with_requires_parentheses_in_control_targets() {
    for body in [
        "if p with { x: 1 }.x == 1 {}",
        "while p with { x: 1 }.x == 1 { break }",
        "for i in 0..<p with { x: 1 }.x {}",
        "rev i in 0..<p with { x: 1 }.x {}",
        "match p with { x: 1 }.x { _ => {} }",
        "fix q = ifexpr p with { x: 1 }.x == 1 { 0 } else { 1 }",
        "fix q = matchexpr p with { x: 1 }.x { _ => 0 }",
        "if false {} elif p with { x: 1 }.x == 1 {}",
    ] {
        let source = format!(
            "struct Point {{ x: i32 }}\nfn main() -> u8 {{\nfix p = Point {{ x: 0 }}\n{body}\nreturn 0\n}}"
        );
        assert_eq!(
            suz::compile(source.as_bytes()).unwrap_err().rule(),
            "expr.struct-in-cond"
        );
        let grouped = source.replace("p with { x: 1 }", "(p with { x: 1 })");
        suz::compile(grouped.as_bytes()).unwrap();
    }
}

#[test]
fn struct_shorthand_preserves_values_conversions_and_evaluation_order() {
    let source = r#"struct Pair { x: u16, y: u16 }
struct Box { pair: Pair, label: symbol }
struct Index { i: len_t }
fn make(x: u8, y: u16) -> Pair { return Pair { y, x } }
fn main() -> u8 {
    var x: u8 = 1
    fix y: u16 = 300
    // 後の初期値が変数を書き換えても、省略形で既に読んだ値は変わらない。
    fix first = Pair { x, y: expr { x = 2
        y }, }
    fix second = Pair { y: expr { x = 3
        y }, x, }
    if first != make(1, y) || second != make(3, y) { return 1 }
    var pair = Pair {
        x,
        y,
    }
    fix label: symbol = `original`
    fix boxed = Box { pair, label }
    pair#x = 4
    if boxed.pair.x != 3 || boxed.label != label { return 2 }
    for i in 0..<(2 as len_t) {
        fix index = Index { i }
        if index.i != i { return 3 }
    }
    rev i in 0..<(2 as len_t) {
        fix index = Index { i }
        if index.i != i { return 4 }
    }
    // 文字列の中の省略形は名前参照にならず、従来どおり変換に失敗する。
    fix input = `Pair { x, y }`
    fix fallback = make(9, 9)
    if (input as Pair or fallback) != fallback { return 5 }
    fix view = input.[:]
    if (view as Pair or fallback) != fallback { return 6 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "shorthand.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn struct_shorthand_uses_existing_name_field_and_type_checks() {
    let declarations = "struct Pair { x: u8, y: u8 }\nfn x() -> u8 { return 1 }\n";
    for (setup, literal) in [
        ("fix y: u8 = 2", "Pair { x, y }"), // 関数名だけでは値にならない。
        ("fix x: u8 = 1", "Pair { x, y }"), // 未定義の値。
        ("fix x: u8 = 1\nfix y: u8 = 2", "Pair { x, x: 2, y }"),
        ("fix x: u8 = 1", "Pair { x }"),
        ("fix z: u8 = 1\nfix y: u8 = 2", "Pair { z, y }"),
        ("fix x = true\nfix y: u8 = 2", "Pair { x, y }"),
    ] {
        let source =
            format!("{declarations}fn main() -> u8 {{\n{setup}\nfix p = {literal}\nreturn 0\n}}");
        let explicit = source
            .replace("{ x,", "{ x: x,")
            .replace("{ x }", "{ x: x }")
            .replace("{ z,", "{ z: z,")
            .replace(", y }", ", y: y }");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        let expected = suz::compile(explicit.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), expected.rule(), "{source}: {error}");
    }
    let error = suz::compile(
        b"struct Item { missing: u8 }\nfn main() -> u8 {\nfix p = Item { missing }\nreturn 0\n}",
    )
    .unwrap_err();
    assert_eq!((error.line, error.column), (3, 16));
    assert!(error.message().contains("missing"));
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn nested_struct_constants_and_equality_preserve_scalar_semantics() {
    let source = r#"struct Outer { inner: Inner, mode: Mode }
struct Inner { value: f32, flag: bool }
enum Mode: u8 { A, B }
fix FIRST = Outer { mode: Mode.B, inner: Inner { value: 1.5, flag: true } }
fix EQUAL = FIRST == Outer { inner: Inner { flag: true, value: 1.5 }, mode: Mode.B }
fix BAD = Inner { value: nan, flag: false }
fix UNEQUAL = BAD != BAD
fix FIELD = FIRST.inner.value
fn copy(value: Outer) -> Outer { return value }
fn main() -> u8 {
    fix value = copy(FIRST)
    fix selected = ifexpr EQUAL { value } else { FIRST }
    if not EQUAL || not UNEQUAL || FIELD != 1.5 || selected != FIRST { return 1 }
    fix nested = Outer { inner: BAD, mode: Mode.A }
    if nested == nested || not (nested != nested) { return 2 }
    var calls: u8 = 0
    fix result = FIRST == nested == expr {
        calls += 1
        FIRST
    }
    if result || calls != 0 { return 3 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "structs.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn field_compound_assignment_saves_old_value_and_handles_failure() {
    let source = r#"struct Pair { x: u8, y: u8 }
struct Box { pair: Pair }
fn failing() -> u8 {
    var value = Box { pair: Pair { x: 255, y: 0 } }
    value.pair#x += expr {
        value.pair#y = 7
        255 as u8
    } + 1 or return expr {
        ifexpr value.pair.x == 255 && value.pair.y == 7 { 0 } else { 1 }
    }
    return 2
}
fn main() -> u8 {
    var value = Box { pair: Pair { x: 10, y: 0 } }
    value.pair#x += expr {
        value.pair#y = 7
        value.pair.x
    }
    if value.pair.x != 20 || value.pair.y != 7 { return 3 }
    value.pair#x = value.pair.x * 255 or 8
    if value.pair.x != 8 { return 4 }
    value.pair#x = 300 as u8 or return failing()
    return 5
}
"#;
    let output = compile_and_run_c23(source, "structs.suz");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}

#[test]
fn struct_literals_require_parentheses_in_each_control_target() {
    for body in [
        "if Item { x: 0 }.x == 0 {}",
        "while Item { x: 0 }.x == 0 { break }",
        "for i in 0..<Item { x: 1 }.x {}",
        "match Item { x: 0 }.x { _ => {} }",
        "fix x = ifexpr Item { x: 0 }.x == 0 { 0 } else { 1 }",
        "fix x = matchexpr Item { x: 0 }.x { _ => 0 }",
        "if false {} elif Item { x: 0 }.x == 0 {}",
    ] {
        let source = format!("struct Item {{ x: u8 }}\nfn main() -> u8 {{\n{body}\nreturn 0\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message().contains("[expr.struct-in-cond]"),
            "{source}: {error}"
        );
        let grouped = source
            .replace("Item { x: 0 }", "(Item { x: 0 })")
            .replace("Item { x: 1 }", "(Item { x: 1 })");
        suz::compile(grouped.as_bytes()).unwrap();
    }
}

#[test]
fn struct_fields_reject_invalid_declarations_access_and_overlapping_writes() {
    for (declarations, body, rule) in [
        ("struct Item { x: u8, x: u8 }", "", "struct.field"),
        ("struct Item { x }", "", "struct.field"),
        ("struct Item { x: Item }", "", "struct.recursion"),
        (
            "struct Item { x: u8 }",
            "fix p = Item { z: 1 }",
            "struct.literal",
        ),
        (
            "struct Item { x: u8 }",
            "fix p = Item { x: 1 }\np.z",
            "struct.access",
        ),
        (
            "struct Item { x: u8 }",
            "var p = Item { x: 1 }\np#z = 0",
            "struct.access",
        ),
        (
            "struct Item { x: u8 }",
            "fix p = Item { x: 1 }\np < p",
            "operator.overload",
        ),
        (
            "struct Item { x: u8 }",
            "fix p = Item { x: 1 }\np + p",
            "operator.overload",
        ),
        (
            "struct Item { x: u8 }",
            "var p = Item { x: 1 }\np#x = expr {\np#x = 2\n0\n}",
            "assign.rhs-write",
        ),
        (
            "struct Item { x: u8 }",
            "var p = Item { x: 1 }\np#x = ifexpr false { expr {\np#x = 2\n0\n} } else { 0 }",
            "assign.rhs-write",
        ),
        (
            "struct Item { x: u8 }\nfix P = Item { x: 1 / 0 }",
            "",
            "arith.div-zero-always",
        ),
        (
            "struct Item { x: u8 }\nfix P = Item { x: P.x }",
            "",
            "const.initializer",
        ),
    ] {
        let source = format!("{declarations}\nfn main() -> u8 {{\n{body}\nreturn 0\n}}");
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
    }
}

#[test]
fn struct_value_parameters_preserve_copy_semantics_under_aliasing_and_mutation() {
    let source = r#"struct Inner { x: i64, y: i64 }
struct Big { a: Inner, b: i64, c: i64, d: i64 }
fnshape Mutate() -> void
fnshape SumSame(Big, Big) -> i64
fnshape ReadBig(Big) -> i64
fnshape Once() -> i64

fn mutate_and_read_ptr(v: Big, p: mut Big) -> i64 {
    p#b = 999
    p.a#x = 888
    return v.a.x + v.b
}

fn mutate_via_closure(v: Big, mutate: fn Mutate) -> i64 {
    mutate()
    return v.a.x + v.b
}

fn mutate_inner_via_closure(v: Inner, mutate: fn Mutate) -> i64 {
    mutate()
    return v.x + v.y
}

fn run_once(f: fn Once) -> i64 {
    return f()
}

fn sum_same(a: Big, b: Big) -> i64 {
    return a.a.x + b.b + a.c + b.d
}

fn Big#check_self_copy(mutate: fn Mutate) -> i64 {
    return mutate_via_closure(self, mutate)
}

fn Big#mutate_in_place() -> void {
    self#b = 777
    self.a#x = 666
}

fn apply_static(f: static fn SumSame, a: Big, b: Big) -> i64 {
    return f(a, b)
}

fn apply_fn(f: fn ReadBig, a: Big) -> i64 {
    return f(a)
}

fn identity[T](v: T) -> T {
    return v
}

fn Big.score() -> i64 {
    return self.a.x + self.b
}

fn Big.+(other: Big) -> Big {
    return Big {
        a: Inner { x: self.a.x + other.a.x, y: self.a.y + other.a.y },
        b: self.b + other.b,
        c: self.c + other.c,
        d: self.d + other.d,
    }
}

fn Big.<(other: Big) -> bool {
    return self.b < other.b
}

fn main() -> u8 {
    // 1. ポインタ参照外し + 同じ領域の mut ポインタを同時渡し。
    fix p = new Big { a: Inner { x: 10, y: 20 }, b: 30, c: 40, d: 50 }
    fix r: ref Big = p
    if mutate_and_read_ptr(.p, p) != 40 { return 1 }
    if p.a.x != 888 || p.b != 999 { return 2 }
    // ref ポインタ経由の参照外しでも、別名の mut ポインタで書き換わらない。
    p.a#x = 11
    p#b = 22
    if mutate_and_read_ptr(.r, p) != 33 { return 3 }

    // 2. var 束縛とそのフィールドを、参照キャプチャーで書き換えるクロージャと同時に渡す。
    var v = Big { a: Inner { x: 1, y: 2 }, b: 3, c: 4, d: 5 }
    if mutate_via_closure(v, fn [var v]() -> void {
        v#b = 100
        v.a#x = 200
    }) != 4 { return 4 }
    if v.a.x != 200 || v.b != 100 { return 5 }

    if mutate_inner_via_closure(v.a, fn [var v]() -> void {
        v.a#x = 500
        v.a#y = 600
    }) != 202 { return 6 }

    // 3. 参照キャプチャーした var 構造体をクロージャの中から値渡しする。
    var cap = Big { a: Inner { x: 7, y: 8 }, b: 9, c: 1, d: 2 }
    if run_once(fn [var cap]() -> i64 {
        return mutate_via_closure(cap, fn [var cap]() -> void {
            cap#b = 50
            cap.a#x = 60
        })
    }) != 16 { return 7 }
    if cap.a.x != 60 || cap.b != 50 { return 8 }

    // 4. # メソッドの self を値仮引数へ渡した後に self が書き換わる。
    var target = Big { a: Inner { x: 5, y: 6 }, b: 7, c: 8, d: 9 }
    fix before = target#check_self_copy(fn [var target]() -> void {
        target#mutate_in_place()
    })
    if before != 12 { return 9 }
    if target.a.x != 666 || target.b != 777 { return 10 }

    // 5. 同じ fix 構造体を 2 つの仮引数へ同時渡し（const T *restrict の読み取り専用エイリアス）。
    fix fixed = Big { a: Inner { x: 1, y: 2 }, b: 3, c: 4, d: 5 }
    if sum_same(fixed, fixed) != 13 { return 11 }
    if apply_static(fn sum_same, fixed, fixed) != 13 { return 12 }
    if apply_fn(fn [fixed](x: Big) -> i64 { return x.score() + fixed.b }, fixed) != 7 { return 13 }

    // 6. ジェネリック具体化・.メソッド・演算子メソッド（+ と <）。
    fix copy = identity(fixed)
    fix doubled = copy + fixed
    if not (fixed < doubled) || doubled.score() != 8 { return 14 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "struct-params.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
}
