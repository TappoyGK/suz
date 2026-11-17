use crate::test_util::compile_and_run_c23;

#[test]
fn value_captures_preserve_evaluation_order_and_nested_environments() {
    let source = r#"
fntype Calc(i32) -> i32
fntype Once() -> i32
fntype Action() -> void
struct Point { x: i32 }
var STATE: i32 = 1
fn apply(f: fn Calc, n: i32) -> i32 { return f(n) }
fn run(f: fn Once) -> i32 { return f() }
fn pair(a: fn Once, b: fn Once) -> i32 { return a() * 10 + b() }
fn forward(f: fn Calc, n: i32) -> i32 { return apply(f, n) }
fn execute(f: fn Action) -> void { f() }
fn use_capture(f: fn Calc) -> i32 {
    return run(fn [f]() -> i32 { return forward(f, 4) })
}
fn generic(n: int) -> i32 {
    return run(fn [n]() -> i32 { return n as i32 or 0 })
}
fn main() -> u8 {
    var scale: i32 = 2
    fix first = apply(fn [scale](x: i32) -> i32 { return x * scale }, expr {
        scale = 10
        3
    })
    if first != 6 { return 1 }
    var point = Point { x: 7 }
    fix saved = apply(fn [point](x: i32) -> i32 { return point.x + x }, expr {
        point.x = 20
        1
    })
    if saved != 8 { return 2 }
    fix nested = expr {
        fix n: i32 = 4
        run(fn [n]() -> i32 {
            return expr {
                fix q = n + 1
                run(fn [q]() -> i32 { return q })
            }
        })
    }
    if nested != 5 { return 3 }
    if use_capture(fn [](x: i32) -> i32 { return x + 2 }) != 6 { return 4 }
    if generic(7 as i8) != 7 || generic(9 as u64) != 9 { return 5 }
    execute(fn []() -> void { STATE = 12 })
    if run(fn []() -> i32 { return STATE }) != 12 { return 6 }
    // クロージャのローカル変数は、同名の外側の変数とは別の束縛。
    var local: i32 = 0
    local = run(fn []() -> i32 { var local: i32 = 1
        local = 3
        return local })
    if local != 3 { return 7 }
    for i in 0..<1000 {
        if run(fn [i]() -> i32 { return i as i32 or 0 }) != i { return 8 }
    }
    fix a: i32 = 2
    fix b: i32 = 7
    if pair(fn [a]() -> i32 { return a }, fn [b]() -> i32 { return b }) != 27 { return 9 }
    // グローバルは環境に写さず、後続の実引数による変更後の値を読む。
    if apply(fn [](x: i32) -> i32 { return STATE + x }, expr {
        STATE = 20
        1
    }) != 21 { return 10 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "value-captures.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    // この入力には確保を必要とする言語機能がない。環境のヒープ確保・ループ内allocaを検出する。
    let c = suz::compile(source.as_bytes()).unwrap();
    for allocation in ["malloc(", "calloc(", "realloc(", "alloca("] {
        assert!(
            !c.contains(allocation),
            "unexpected allocation: {allocation}"
        );
    }
}

#[test]
fn closures_reject_escaping_implicit_captures_and_mutating_value_captures() {
    let prelude = "fntype Once() -> i32\nfntype Action() -> void\nstruct Point { x: i32 }\nstruct Holder { f: static fn Once }\nfix TOP: i32 = 1\nfn run(f: fn Once) -> i32 { return f() }\nfn save(f: static fn Once) -> void {}\nfntype Calc(i32) -> i32\nfn apply(f: fn Calc) -> i32 { return f(1) }\n";
    for (body, rule) in [
        ("fix f = fn []() -> i32 { return 1 }", "closure.place"),
        ("var f = fn []() -> i32 { return 1 }", "closure.place"),
        ("save(fn []() -> i32 { return 1 })", "closure.place"),
        (
            "fix h = Holder { f: fn []() -> i32 { return 1 } }",
            "closure.place",
        ),
        ("run((fn []() -> i32 { return 1 }))", "closure.place"),
        ("run(fn []() -> void {})", "closure.place"),
        (
            "fix n: i32 = 1\napply(fn [n](n: i32) -> i32 { return n })",
            "closure.capture-list",
        ),
        ("run(fn []() -> int { return 1 })", "closure.generic"),
        ("run(fn [](n: int) -> i32 { return 1 })", "closure.generic"),
        (
            "fix n = 1\nrun(fn []() -> i32 { return n })",
            "closure.scope",
        ),
        (
            "fix n = 1\nrun(fn [n, n]() -> i32 { return n })",
            "closure.capture-list",
        ),
        (
            "run(fn [missing]() -> i32 { return 1 })",
            "closure.capture-list",
        ),
        (
            "run(fn [TOP]() -> i32 { return 1 })",
            "closure.capture-list",
        ),
        (
            "run(fn [run]() -> i32 { return 1 })",
            "closure.capture-list",
        ),
        (
            "run(fn [Point]() -> i32 { return 1 })",
            "closure.capture-list",
        ),
        (
            "var n: i32 = 1\nrun(fn [n]() -> i32 { n = 2\nreturn n })\nn = 3",
            "closure.capture-value",
        ),
        (
            "fix p = Point { x: 1 }\nrun(fn [p]() -> i32 { p.x += 1\nreturn p.x })",
            "closure.capture-value",
        ),
        (
            "fix n: i32 = 1\nrun(fn []() -> i32 { return run(fn [n]() -> i32 { return n }) })",
            "closure.capture-list",
        ),
        (
            "for i in 0..<1 { run(fn []() -> i32 { break\nreturn 0 }) }",
            "loop.jump-context",
        ),
        (
            "for i in 0..<1 { run(fn []() -> i32 { continue\nreturn 0 }) }",
            "loop.jump-context",
        ),
    ] {
        let source = format!("{prelude}fn main() -> u8 {{ {body}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
}

#[test]
fn closures_keep_return_and_range_boundaries_separate() {
    for (source, rule) in [
        (
            "fntype Once() -> i32\nfn run(f: fn Once) -> i32 { return f() }\nfn main() -> u8 { run(fn []() -> i32 { return 1 }) }",
            "fn.value-return-required",
        ),
        (
            "fntype Once() -> i32\nfn run(f: fn Once) -> i32 { return f() }\nfn main() -> u8 { return run(fn []() -> i32 { fix n = 1 }) as u8 or 0 }",
            "fn.value-return-required",
        ),
        (
            "fntype Once() -> i32\nfn run(f: fn Once) -> i32 { return f() }\nfn main() -> u8 { var n: i32 = 2\nfor i in 0..<run(fn [n]() -> i32 { return n }) {}\nn = 3\nreturn 0 }",
            "for.bound-reference",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
}

#[test]
fn closure_nesting_uses_the_enclosing_depth() {
    let directory = crate::test_util::TemporaryDirectory::new();
    // 本体1段 + 各組の呼び出し・クロージャ・本体が3段ずつ + 最後のリテラル1段。
    for count in [84, 85, 10_000] {
        let expression = format!(
            "{}1{}",
            "run(fn []() -> i32 { return ".repeat(count),
            " })".repeat(count)
        );
        let source = format!(
            "fntype Once() -> i32\nfn run(f: fn Once) -> i32 {{ return f() }}\nfn main() -> u8 {{ fix n = {expression}\nreturn 0 }}"
        );
        std::fs::write(directory.0.join("depth.suz"), source).unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_suz"))
            .current_dir(&directory.0)
            .args(["check", "depth.suz"])
            .output()
            .unwrap();
        if count == 84 {
            assert!(output.status.success(), "{output:?}");
        } else {
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("[src.nesting-limit]"),
                "{output:?}"
            );
        }
    }
}
