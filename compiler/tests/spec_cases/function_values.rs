use crate::test_util::compile_and_run_c23;

#[test]
fn function_values_pass_through_bindings_fields_and_calls() {
    let source = r#"
fntype Transform(i32) -> i32
fntype Alias(i32) -> i32
fntype Choose(bool) -> fn Transform
fntype Visit(Node) -> i32
struct Node { value: i32, visit: fn Visit }
struct Holder { action: fn Alias }
fix ID: fn Transform = fn identity
var CURRENT: fn Transform = fn identity
fn identity(x: int) -> int { return x }
fn twice(x: i32) -> i32 { return x * 2 }
fn truth(x: bool) -> bool { return x }
fn opposite(x: bool) -> bool { return not x }
fn visit_node(node: Node) -> i32 { return node.value }
fn choose(first: bool) -> fn Transform {
    return ifexpr first { fn identity } else { fn twice }
}
fn apply(action: fn Alias, x: i32) -> i32 { return action(x) }
fn returned() -> fn Transform {
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
    fix chooser: fn Choose = fn choose
    fix holder = Holder { action: fn identity }
    fix changed = holder with { action: fn twice }
    fix node = Node { value: 7, visit: fn visit_node }
    fix selected: fn Transform = matchexpr 0 { _ => fn identity }
    fix blocked: fn Transform = expr { (fn identity) }
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
fntype Pair(i32, i32) -> i32
struct Holder { action: fn Pair }
var CURRENT: fn Pair = fn first
var HOLDER = Holder { action: fn first }
var TRACE: i32 = 0
fn first(a: i32, b: i32) -> i32 { TRACE = TRACE * 10 + 4
return a * 10 + b }
fn second(a: i32, b: i32) -> i32 { return 99 }
fn pick() -> fn Pair { TRACE = TRACE * 10 + 1
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
    var local: fn Pair = fn first
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
    let prelude = "fntype Transform(i32) -> i32\nfn identity(x: int) -> int { return x }\nfn fixed(x: i32) -> i32 { return x }\n";
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
            "fntype Bad(i32) -> i64\nfn main() -> u8 { fix f: fn Bad = fn identity\nreturn 0 }",
            "fn.value-generic",
        ),
        (
            "fntype Bad(f32) -> f32\nfn main() -> u8 { fix f: fn Bad = fn identity\nreturn 0 }",
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
            "fn.value-from-name",
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
            "fntype Bad(int) -> int\nfn main() -> u8 { return 0 }",
            "num.places",
        ),
        (
            "fntype Bad(void) -> void\nfn main() -> u8 { return 0 }",
            "type.void",
        ),
        (
            "fntype Bad(fn Bad) -> void\nfn main() -> u8 { return 0 }",
            "fntype.recursion",
        ),
        (
            "fntype Ping(fn Pong) -> void\nfntype Pong(fn Ping) -> void\nfn main() -> u8 { return 0 }",
            "fntype.recursion",
        ),
        (
            "struct Holder { action: fn Transform }\nfn main() -> u8 { fix h = Holder { action: fn fixed }\nfix same = h == h\nreturn 0 }",
            "fn.value-compare",
        ),
        (
            "struct Holder { action: fn Transform }\nfn main() -> u8 { fix h = `` as Holder or Holder { action: fn fixed }\nreturn 0 }",
            "symbol.cast",
        ),
        (
            "fn main() -> u8 { fix f = fn fixed\nfix bad = f + f\nreturn 0 }",
            "expr.operator-types",
        ),
        (
            "fn main() -> u8 { var f: fn Transform = fn fixed\nfor i in f(0)..<1 {}\nf = fn identity\nreturn 0 }",
            "for.bound-reference",
        ),
        (
            "fntype Transform() -> void\nfn main() -> u8 { return 0 }",
            "decl.duplicate",
        ),
        (
            "fntype bad() -> void\nfn main() -> u8 { return 0 }",
            "name.type",
        ),
        (
            "fn main() -> u8 { fntype Local() -> void\nreturn 0 }",
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
fntype Mixed(i16, f32) -> f32
fntype Zero() -> u16
fntype Action() -> void
fntype Transform(i32) -> i32
fn mixed(n: int, x: float) -> float { return x }
fn zero() -> int { return 0 }
fn identity(n: int) -> int { return n }
fn action() -> void { return }
fn inside(n: int) -> int {
    fix f: fn Transform = fn identity
    return n
}
fn main() -> u8 {
    fix mix: fn Mixed = fn mixed
    fix make: fn Zero = fn zero
    fix act: fn Action = fn action
    fix f: fn Transform = fn identity
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
        "fix BAD: fn Narrow = @fn limited",
        "fn unused() -> void { fix bad: fn Narrow = @fn limited }",
    ] {
        let marked = format!(
            "fntype Narrow(u8) -> u8\nfn limited(n: int) -> int {{ return n +% 300 }}\n{declaration}\nfn main() -> u8 {{ return 0 }}"
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
fntype Action(u8, u8) -> u8
fn target(a: u8, b: u8) -> u8 { log info `body`
return a }
fn pick() -> fn Action { log info `target`
return fn target }
fn fail() -> u8 { log info `fail`
fix n: u8 = 255
return n + 1 }
fn later() -> u8 { log info `later`
return 1 }
fn bad_pick() -> fn Action { fix n = fail()
return fn target }
"#;
    for (call, expected) in [
        ("pick()(fail(), later())", "target"),
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
        source.push_str(&format!("fntype Chain{i}(fn Chain{}) -> void\n", i + 1));
    }
    source.push_str("fntype Chain2000() -> void\nfn main() -> u8 { return 0 }");
    fs::write(directory.0.join("chain.suz"), &source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .current_dir(&directory.0)
        .args(["check", "chain.suz"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let source = source.replace("fntype Chain2000()", "fntype Chain2000(fn Chain0)");
    fs::write(directory.0.join("chain.suz"), source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .current_dir(&directory.0)
        .args(["check", "chain.suz"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[fntype.recursion]"),
        "{output:?}"
    );
}
