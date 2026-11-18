use crate::test_util::compile_and_run_c23;

#[test]
fn function_values_pass_through_bindings_fields_and_calls() {
    let source = r#"
fnshape Transform(i32) -> i32
fnshape Alias(i32) -> i32
fnshape Choose(bool) -> static fn Transform
fnshape Visit(Node) -> i32
struct Node { value: i32, visit: static fn Visit }
struct Holder { action: static fn Alias }
fix ID: static fn Transform = fn identity
var CURRENT: static fn Transform = fn identity
fn identity(x: int) -> int { return x }
fn twice(x: i32) -> i32 { return x * 2 }
fn truth(x: bool) -> bool { return x }
fn opposite(x: bool) -> bool { return not x }
fn visit_node(node: Node) -> i32 { return node.value }
fn choose(first: bool) -> static fn Transform {
    return ifexpr first { fn identity } else { fn twice }
}
fn apply(action: static fn Alias, x: i32) -> i32 { return action(x) }
fn returned() -> static fn Transform {
    fix n = 300 as u8 or return fn identity
    return fn twice
}
fn main() -> u8 {
    var inferred_bool = fn truth
    inferred_bool = fn opposite
    if inferred_bool(true) { return 7 }
    fix inferred = fn twice
    fix copy = inferred
    var local = inferred
    local = fn identity
    CURRENT = fn twice
    fix chooser: static fn Choose = fn choose
    fix holder = Holder { action: fn identity }
    fix changed = holder with { action: fn twice }
    fix node = Node { value: 7, visit: fn visit_node }
    fix selected: static fn Transform = matchexpr 0 { _ => fn identity }
    fix blocked: static fn Transform = expr { (fn identity) }
    if ID(3) != 3 || CURRENT(3) != 6 || copy(3) != 6 || local(3) != 3 { return 1 }
    if chooser(true)(3) != 3 || choose(false)(3) != 6 { return 2 }
    if holder.action(3) != 3 || changed.action(3) != 6 || node.visit(node) != 7 { return 3 }
    if apply(fn identity, 3) != 3 || selected(3) != 3 || blocked(3) != 3 { return 4 }
    fix captured = expr { copy(3) }
    if captured != 6 { return 6 }
    if returned()(3) != 3 || (fn twice)(3) != 6 { return 5 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "function-values.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn function_values_capture_the_target_before_evaluating_arguments() {
    let source = r#"
fnshape Pair(i32, i32) -> i32
struct Holder { action: static fn Pair }
var CURRENT: static fn Pair = fn first
var HOLDER = Holder { action: fn first }
var TRACE: i32 = 0
fn first(a: i32, b: i32) -> i32 { TRACE = TRACE * 10 + 4
return a * 10 + b }
fn second(a: i32, b: i32) -> i32 { return 99 }
fn pick() -> static fn Pair { TRACE = TRACE * 10 + 1
return CURRENT }
fn change() -> i32 { TRACE = TRACE * 10 + 2
CURRENT = fn second
HOLDER.action = fn second
return 2 }
fn next() -> i32 { TRACE = TRACE * 10 + 3
return 3 }
fn main() -> u8 {
    if pick()(change(), next()) != 23 || TRACE != 1234 { return 1 }
    TRACE = 0
    HOLDER.action = fn first
    if HOLDER.action(change(), next()) != 23 || TRACE != 234 { return 2 }
    var local: static fn Pair = fn first
    fix result = local(expr { local = fn second
        2 }, 3)
    if result != 23 || local(2, 3) != 99 { return 3 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "function-order.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn function_values_reject_invalid_types_and_missing_context() {
    let prelude = "fnshape Transform(i32) -> i32\nfn identity(x: int) -> int { return x }\nfn fixed(x: i32) -> i32 { return x }\n";
    for (source, rule) in [
        (
            "fn main() -> u8 { fix f = fn identity\nreturn 0 }",
            "fn.value-generic",
        ),
        (
            "fn main() -> u8 { fix f = ifexpr true { fn identity } else { fn fixed }\nreturn 0 }",
            "fn.value-generic",
        ),
        (
            "fnshape Bad(i32) -> i64\nfn main() -> u8 { fix f: static fn Bad = fn identity\nreturn 0 }",
            "fn.value-generic",
        ),
        (
            "fnshape Bad(f32) -> f32\nfn main() -> u8 { fix f: static fn Bad = fn identity\nreturn 0 }",
            "fn.value-generic",
        ),
        (
            "fn main() -> u8 { fix f: Transform = fn fixed\nreturn 0 }",
            "fn.type-mark",
        ),
        (
            "fn main() -> u8 { fix f: fn i32 = fn fixed\nreturn 0 }",
            "fn.type-mark",
        ),
        (
            "fn main() -> u8 { fix f = fixed\nreturn 0 }",
            "fn.value-mark",
        ),
        (
            "fn main() -> u8 { fix f = fn main\nreturn 0 }",
            "fn.value-from-name",
        ),
        (
            "fn main() -> u8 { fix f = fn fixed\nfix g = fn f\nreturn 0 }",
            "fn.value-mark",
        ),
        (
            "fn main() -> u8 { fix f = fn fixed(1)\nreturn 0 }",
            "fn.value-mark",
        ),
        (
            "fn main() -> u8 { fix f = 1\nf(1)\nreturn 0 }",
            "fn.value-call",
        ),
        ("fn main() -> u8 { (1)(2)\nreturn 0 }", "fn.value-call"),
        (
            "fn main() -> u8 { fix f = fn fixed\nf()\nreturn 0 }",
            "fn.call",
        ),
        (
            "fn main() -> u8 { fix f = fn fixed\nfix eq = f == f\nreturn 0 }",
            "fn.value-compare",
        ),
        (
            "fn main() -> u8 { fix f = fn fixed\nfix label = `${f}`\nreturn 0 }",
            "symbol.interpolation",
        ),
        (
            "fnshape Bad(int) -> int\nfn main() -> u8 { return 0 }",
            "num.places",
        ),
        (
            "fnshape Bad(void) -> void\nfn main() -> u8 { return 0 }",
            "type.void",
        ),
        (
            "fnshape Bad(static fn Bad) -> void\nfn main() -> u8 { return 0 }",
            "fnshape.recursion",
        ),
        (
            "fnshape Ping(static fn Pong) -> void\nfnshape Pong(static fn Ping) -> void\nfn main() -> u8 { return 0 }",
            "fnshape.recursion",
        ),
        (
            "struct Holder { action: static fn Transform }\nfn main() -> u8 { fix h = Holder { action: fn fixed }\nfix same = h == h\nreturn 0 }",
            "fn.value-compare",
        ),
        (
            "struct Holder { action: static fn Transform }\nfn main() -> u8 { fix h = `` as Holder or Holder { action: fn fixed }\nreturn 0 }",
            "symbol.cast",
        ),
        (
            "fn main() -> u8 { fix f = fn fixed\nfix bad = f + f\nreturn 0 }",
            "expr.operator-types",
        ),
        (
            "fn main() -> u8 { var f: static fn Transform = fn fixed\nfor i in f(0)..<1 {}\nf = fn identity\nreturn 0 }",
            "for.bound-reference",
        ),
        (
            "fnshape Transform() -> void\nfn main() -> u8 { return 0 }",
            "decl.duplicate",
        ),
        (
            "fnshape bad() -> void\nfn main() -> u8 { return 0 }",
            "name.type",
        ),
        (
            "fn main() -> u8 { fnshape Local() -> void\nreturn 0 }",
            "decl.location",
        ),
    ] {
        let full = format!("{prelude}{source}");
        let error = suz::compile(full.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("accepted: {source}"));
        assert!(error.to_string().contains(rule), "{source}: {error}");
    }
}

#[test]
fn function_values_specialize_both_numeric_families_and_reuse_instances() {
    let source = r#"
fnshape Mixed(i16, f32) -> f32
fnshape Zero() -> u16
fnshape Action() -> void
fnshape Transform(i32) -> i32
fn mixed(n: int, x: float) -> float { return x }
fn zero() -> int { return 0 }
fn identity(n: int) -> int { return n }
fn action() -> void { return }
fn inside(n: int) -> int {
    fix f: static fn Transform = fn identity
    return n
}
fn main() -> u8 {
    fix mix: static fn Mixed = fn mixed
    fix make: static fn Zero = fn zero
    fix act: static fn Action = fn action
    fix f: static fn Transform = fn identity
    act()
    if mix(1, 2.5) + 0.5 != 3.0 || make() != 0 { return 1 }
    fix n: i32 = 4
    if f(n) != identity(n) || inside(n) != n { return 2 }
    return 0
}
"#;
    let c = suz::compile(source.as_bytes()).unwrap();
    // identityの具体化は、関数値・直接呼び出し・汎用本体内で同じC関数を使う。
    let definitions = c
        .lines()
        .filter(|l| l.starts_with("static inline int32_t suz_fn_") && l.ends_with(" {"))
        .count();
    assert_eq!(
        definitions, 2,
        "identityとinsideの具体化だけを生成する: {c}"
    );
    let output = compile_and_run_c23(source, "function-generic.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn function_values_check_unused_specializations_and_report_the_value_site() {
    for declaration in [
        "fix BAD: static fn Narrow = @fn limited",
        "fn unused() -> void { fix bad: static fn Narrow = @fn limited }",
    ] {
        let marked = format!(
            "fnshape Narrow(u8) -> u8\nfn limited(n: int) -> int {{ return n +% 300 }}\n{declaration}\nfn main() -> u8 {{ return 0 }}"
        );
        let offset = marked.find('@').unwrap();
        let line = marked[..offset].bytes().filter(|b| *b == b'\n').count() + 1;
        let column = marked[..offset].rsplit('\n').next().unwrap().len() + 1;
        let error = suz::compile(marked.replace('@', "").as_bytes()).unwrap_err();
        assert_eq!((error.line, error.column), (line, column), "{error}");
        assert!(
            error.to_string().contains("fn.value-generic-check"),
            "{error}"
        );
        assert!(!error.related.is_empty(), "{error}");
    }
}

#[test]
fn function_values_stop_at_a_panicking_target_or_argument() {
    let prelude = r#"
fnshape Action(u8, u8) -> u8
fn target(a: u8, b: u8) -> u8 { log info `body`
return a }
fn pick() -> static fn Action { log info `target`
return fn target }
fn stop_now() -> u8 { log info `fail`
fix n: u8 = 255
return n + 1 }
fn later() -> u8 { log info `later`
return 1 }
fn bad_pick() -> static fn Action { fix n = stop_now()
return fn target }
"#;
    for (call, expected) in [
        ("pick()(stop_now(), later())", "target"),
        ("bad_pick()(later(), later())", "fail"),
    ] {
        let source = format!("{prelude}\nfn main() -> u8 {{ return {call} }}");
        let output = compile_and_run_c23(&source, "function-panic.suz");
        assert_eq!(output.status.code(), Some(101), "{output:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains(expected) && stderr.contains("panic "),
            "{stderr}"
        );
        assert!(
            !stderr.contains(": later") && !stderr.contains(": body"),
            "{stderr}"
        );
    }
}

#[test]
fn function_values_limit_call_targets_and_resolve_long_type_chains_without_recursion() {
    use std::{fs, process::Command};
    let directory = crate::test_util::TemporaryDirectory::new();
    for count in [253, 254] {
        let target = format!("{}fn fixed{}", "(".repeat(count), ")".repeat(count));
        let source = format!(
            "fn fixed() -> i32 {{ return 1 }}\nfn main() -> u8 {{\nfix value = {target}()\nreturn 0\n}}"
        );
        fs::write(directory.0.join("depth.suz"), source).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_suz"))
            .current_dir(&directory.0)
            .args(["check", "depth.suz"])
            .output()
            .unwrap();
        if count == 253 {
            assert!(output.status.success(), "{output:?}");
        } else {
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("[src.nesting-limit]"), "{stderr}");
            assert!(
                stderr.contains(&format!(
                    "depth.suz:3:{}:",
                    "fix value = ".len() + target.len() + 1
                )),
                "{stderr}"
            );
        }
    }
    // 型の依存は式の深さと異なる。宣言を長く連ねても、型解決に再帰スタックを使わない。
    let mut source = String::new();
    for i in 0..2_000 {
        source.push_str(&format!(
            "fnshape Chain{i}(static fn Chain{}) -> void\n",
            i + 1
        ));
    }
    source.push_str("fnshape Chain2000() -> void\nfn main() -> u8 { return 0 }");
    fs::write(directory.0.join("chain.suz"), &source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .current_dir(&directory.0)
        .args(["check", "chain.suz"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let source = source.replace("fnshape Chain2000()", "fnshape Chain2000(static fn Chain0)");
    fs::write(directory.0.join("chain.suz"), source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .current_dir(&directory.0)
        .args(["check", "chain.suz"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[fnshape.recursion]"),
        "{output:?}"
    );
}

#[test]
fn function_values_distinguish_declarations_from_misplaced_fn_marks() {
    for statement in [
        "fn inner() -> void {}",
        "fn inner(x: i32) -> void {}",
        "fn inner {}",
        "fn inner -> void {}",
        "fn",
    ] {
        let source = format!("fn main() -> u8 {{\n    {statement}\n    return 0\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "2:5: functions and types must be declared at top level [decl.location]"
        );
    }
    for statement in ["fn pick(1)", "fix f = fn pick(1)"] {
        let source = format!(
            "fn pick(n: i32) -> i32 {{ return n }}\nfn main() -> u8 {{\n{statement}\nreturn 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("[fn.value-mark]"), "{error}");
    }
    for binding in ["fix f = fn pick", "fix f = 1", "var f = fn pick"] {
        let source = format!(
            "fn pick(n: i32) -> i32 {{ return n }}\nfn main() -> u8 {{\n{binding}\nfix g = fn f\nreturn 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "4:9: do not prefix a binding with fn; use f directly [fn.value-mark]"
        );
    }
}

#[test]
fn function_values_show_the_required_and_actual_types() {
    for (declarations, annotation, actual) in [
        (
            "fnshape Narrow(i32) -> i32\n",
            ": static fn Narrow",
            "static fn Narrow",
        ),
        ("", "", "static fn(i32) -> i32"),
    ] {
        let source = format!(
            "{declarations}fnshape Wide(i64) -> i32\nfn identity(n: i32) -> i32 {{ return n }}\nfn take(f: static fn Wide) -> void {{}}\nfn main() -> u8 {{\nfix f{annotation} = fn identity\ntake(f)\nreturn 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains(&format!(
                "expected static fn Wide, found {actual} [type.expected]"
            )),
            "{error}"
        );
    }
}

#[test]
fn function_values_report_duplicate_type_names_at_the_existing_collector_site() {
    // 異種の型名の重複は、既存の列挙型→構造体の収集順と同じ位置で報告する。
    for (first, second, line, column) in [
        ("struct Same { value: i32 }", "enum Same: u8 { One }", 1, 8),
        ("enum Same: u8 { One }", "struct Same { value: i32 }", 2, 8),
        ("struct Same { value: i32 }", "fnshape Same() -> void", 1, 8),
        ("fnshape Same() -> void", "struct Same { value: i32 }", 2, 8),
        ("enum Same: u8 { One }", "fnshape Same() -> void", 2, 9),
        ("fnshape Same() -> void", "enum Same: u8 { One }", 1, 9),
        ("fnshape Same() -> void", "fnshape Same(i32) -> void", 2, 9),
    ] {
        let source = format!("{first}\n{second}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("{line}:{column}: duplicate type name [decl.duplicate]")
        );
    }
}

#[test]
fn call_only_functions_preserve_selected_values_and_forward_environments() {
    let source = r#"
fnshape Transform(i32) -> i32
fnshape Alias(i32) -> i32
fnshape Action() -> void
fnshape Make() -> Item
struct Item { value: i32 }
struct Holder { action: static fn Transform }
var CURRENT: static fn Transform = fn identity
var HOLDER = Holder { action: fn identity }
var TRACE: i32 = 0
fn identity(n: int) -> int { return n }
fn twice(n: i32) -> i32 { return n * 2 }
fn pick() -> static fn Transform { TRACE += 1
return CURRENT }
fn change() -> i32 { CURRENT = fn twice
HOLDER.action = fn twice
return 3 }
fn apply(action: fn Transform, n: i32) -> i32 { return action(n) }
fn forward(action: fn Alias, n: i32) -> i32 { return apply(action, n) }
fn generic_forward(action: fn Transform, n: int) -> int { action(1)
return n }
fn pair(a: fn Transform, b: fn Transform, n: i32) -> i32 { return a(n) + b(n) }
fn again(action: fn Transform, n: i32) -> i32 {
    if n == 0 { return action(3) }
    return again(action, n - 1)
}
fn run(action: fn Action) -> void { action() }
fn tick() -> void { TRACE += 10 }
fn make() -> Item { return Item { value: 7 } }
fn create(action: fn Make) -> Item { return action() }
fn main() -> u8 {
    if generic_forward(fn identity, 3) != 3 { return 8 }
    if forward(pick(), change()) != 3 || TRACE != 1 { return 1 }
    HOLDER.action = fn identity
    if apply(HOLDER.action, change()) != 3 { return 2 }
    if pair(fn identity, CURRENT, 3) != 9 { return 3 }
    if again(fn identity, 3) != 3 { return 4 }
    // 関数名から推論した型でも、呼び出し専用の仮引数へ変換できる。
    fix indirect = fn apply
    if indirect(fn identity, 4) != 4 { return 5 }
    run(fn tick)
    if TRACE != 11 || create(fn make).value != 7 { return 6 }
    for i in 0..<10 {
        if forward(ifexpr i == 0 { fn identity } else { fn twice }, 3) != ifexpr i == 0 { 3 } else { 6 } { return 7 }
    }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "call-only.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    // この入力は環境の受け渡しだけを使う。環境のための動的確保が紛れ込むのを検出する。
    let c = suz::compile(source.as_bytes()).unwrap();
    for allocation in ["malloc(", "calloc(", "realloc(", "alloca("] {
        assert!(
            !c.contains(allocation),
            "unexpected allocation: {allocation}"
        );
    }
}

#[test]
fn call_only_functions_reject_storage_and_non_parameter_annotations() {
    let prelude = "fnshape Transform(i32) -> i32\nfn fixed(n: i32) -> i32 { return n }\nfn apply(f: fn Transform) -> void { f(1) }\nfn save(f: static fn Transform) -> void {}\n";
    for source in [
        "fix BAD: fn Transform = fn fixed",
        "var BAD: fn Transform = fn fixed",
        "struct Bad { f: fn Transform }",
        "fnshape Bad() -> fn Transform",
        "fn bad() -> fn Transform { return fn fixed }",
        "fn bad() -> void { fix f: fn Transform = fn fixed }",
        "fn bad() -> void { var f: fn Transform = fn fixed }",
    ] {
        let source = format!("{prelude}{source}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[fn.type-call]"),
            "{source}: {error}"
        );
    }
    for body in [
        "fix copy = f",
        "var copy = f",
        "fix copy = ifexpr true { f } else { fn fixed }",
        "fix copy = matchexpr 0 { _ => f }",
        "fix copy = expr { f }",
        "var copy = fn fixed\ncopy = f",
        "fix copy = Holder { f: f }",
        "save(f)",
        "apply((f))",
        "apply(ifexpr true { f } else { f })",
        "fix result = f == f",
        "fix result = `${f}`",
        "return f",
    ] {
        let source = format!(
            "{prelude}struct Holder {{ f: static fn Transform }}\nfn bad(f: fn Transform) -> static fn Transform {{ {body}\nreturn fn fixed }}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[fn.call-only]"),
            "{source}: {error}"
        );
    }
}

#[test]
fn static_is_reserved_and_only_modifies_function_types() {
    for source in [
        "fn main() -> u8 { fix static = 1\nreturn 0 }",
        "fn static() -> void {}\nfn main() -> u8 { return 0 }",
        "fn bad(static: i32) -> void {}\nfn main() -> u8 { return 0 }",
        "struct Item { static: i32 }\nfn main() -> u8 { return 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
    for ty in ["static i32", "static Item", "static fn i32"] {
        let source = format!(
            "struct Item {{ value: i32 }}\nfn bad(n: {ty}) -> void {{}}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[fn.type-mark]"),
            "{source}: {error}"
        );
    }
}

#[test]
fn call_only_forwarding_reports_signature_mismatches() {
    let source = "fnshape Tr(i32) -> i32\nfnshape Ot(i64) -> i32\nfn take(f: fn Ot) -> void {}\nfn forward(f: fn Tr) -> void { take(f) }\nfn main() -> u8 { return 0 }";
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "4:37: expected fn Ot, found fn Tr [type.expected]"
    );
}

#[test]
fn function_type_parameters_accept_call_only_types() {
    let source = r#"
// 前方参照を解決した後も、fnとstatic fnを区別して同じ形をまとめる。
fnshape Runner(fn Tr) -> i32
fnshape Alias(fn TrAlias) -> i32
fnshape Tr(i32) -> i32
fnshape TrAlias(i32) -> i32
// 関数本体から使われない型も、参照する呼び出し専用のC型を必要とする。
fnshape Unused(fn Never) -> void
fnshape Never() -> void
struct Job { run: static fn Runner }
fn inc(n: i32) -> i32 { return n + 1 }
fn r(f: fn Tr) -> i32 { return f(4) }
fn other(f: fn Tr) -> i32 { return f(5) }
fn saved(run: static fn Alias, f: fn Tr) -> i32 { return run(f) }
fn forwarded(run: fn Runner, f: fn Tr) -> i32 { return run(f) }
fn main() -> u8 {
    fix inferred = fn r
    var named: static fn Alias = inferred
    fix job = Job { run: named }
    named = fn other
    if job.run(fn inc) != 5 || named(fn inc) != 6 { return 1 }
    if saved(inferred, fn inc) != 5 { return 2 }
    if forwarded(job.run, fn inc) != 5 { return 3 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "function-type-parameters.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    for declarations in [
        "fnshape Loop(fn Loop) -> void",
        "fnshape Alpha(fn Beta) -> void\nfnshape Beta(static fn Alpha) -> void",
    ] {
        let source = format!("{declarations}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[fnshape.recursion]"),
            "{source}: {error}"
        );
    }
}
