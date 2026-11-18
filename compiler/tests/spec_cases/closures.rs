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
fn captured_function_parameters_remain_call_only() {
    // キャプチャー後も保存やstatic fnへの変換を許さず、環境が呼び出しの外へ逃げるのを防ぐ。
    for misuse in ["fix g = f", "save(f)"] {
        let source = format!(
            "fntype Calc(i32) -> i32\n\
             fn apply(f: fn Calc) -> i32 {{ return f(1) }}\n\
             fn save(f: static fn Calc) -> void {{}}\n\
             fn wrap(f: fn Calc) -> i32 {{\n\
                 return apply(fn [f](x: i32) -> i32 {{ {misuse}\nreturn x }})\n\
             }}\n\
             fn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[fn.call-only]"),
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

#[test]
fn reference_captures_write_the_original_binding() {
    let source = r#"
fntype Action() -> void
fntype Once() -> i32
struct Point { x: i32, y: i32 }
fn execute(f: fn Action) -> void { f() }
fn run(f: fn Once) -> i32 { return f() }
fn pair(a: fn Action, b: fn Action) -> void { a()
b() }
fn after(f: fn Once, ignored: i32) -> i32 { return f() }
fn generic(n: int) -> i32 {
    var value = n
    execute(fn [var value]() -> void { value += 1 })
    return value as i32 or 0
}
fn main() -> u8 {
    var total: i32 = 0
    execute(fn [var total]() -> void { total = 3 })
    if total != 3 { return 1 }
    pair(fn [var total]() -> void { total += 2 }, fn [var total]() -> void { total *= 3 })
    if total != 15 { return 2 }
    // 作成時には値を読まず、後続の実引数の評価後に読む。
    if after(fn [var total]() -> i32 { return total }, expr { total = 20
        0 }) != 20 { return 3 }
    var point = Point { x: 1, y: 2 }
    execute(fn [var point]() -> void { point.x = 4
        point.y += 3 })
    if point.x != 4 || point.y != 5 { return 4 }
    // 値ブロックとクロージャをまたいでも同じ変数へ書く。
    fix nested = expr {
        execute(fn [var total]() -> void {
            execute(fn [var total]() -> void { total += 1 })
            fix ignored = expr { total += 2
                0 }
        })
        total
    }
    if nested != 23 { return 5 }
    // 参照キャプチャーを値で再キャプチャーした場合は、その位置の値を保持する。
    if run(fn [var total]() -> i32 {
        return after(fn [total]() -> i32 { return total }, expr { total = 30
            0 })
    }) != 23 || total != 30 { return 6 }
    if generic(3 as i8) != 4 || generic(5 as u64) != 6 { return 7 }
    for i in 0..<1000 {
        execute(fn [var total]() -> void { total += 1 })
    }
    if total != 1030 { return 8 }
    // 別フィールドへの書き込みは右辺でも許される。
    point.x = run(fn [var point]() -> i32 { point.y += 1
        return 9 })
    if point.x != 9 || point.y != 6 { return 9 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "reference-captures.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let c = suz::compile(source.as_bytes()).unwrap();
    for allocation in ["malloc(", "calloc(", "realloc(", "alloca("] {
        assert!(
            !c.contains(allocation),
            "unexpected allocation: {allocation}"
        );
    }
}

#[test]
fn reference_captures_enforce_mutability_and_assignment_boundaries() {
    let prelude = "fntype Once() -> i32\nfntype Action() -> void\nstruct Point { x: i32 }\nfn run(f: fn Once) -> i32 { return f() }\nfn execute(f: fn Action) -> void { f() }\n";
    for (body, rule) in [
        (
            "fix n: i32 = 1\nrun(fn [var n]() -> i32 { return n })",
            "closure.capture-list",
        ),
        (
            "for i in 0..<1 { run(fn [var i]() -> i32 { return i as i32 or 0 }) }",
            "closure.capture-list",
        ),
        (
            "var n: i32 = 1\nrun(fn [n]() -> i32 { return run(fn [var n]() -> i32 { return n }) })\nn = 2",
            "closure.capture-list",
        ),
        (
            "var n: i32 = 1\nrun(fn [n, var n]() -> i32 { return n })\nn = 2",
            "closure.capture-list",
        ),
        (
            "var n: i32 = 1\nrun(fn [var n]() -> i32 { return n })",
            "var.never-reassigned",
        ),
        (
            "var n: i32 = 1\nn = run(fn [var n]() -> i32 { n += 1\nreturn n })",
            "assign.rhs-write",
        ),
        (
            "var n: i32 = 1\nn = run(fn [var n]() -> i32 { return run(fn [var n]() -> i32 { n = 2\nreturn n }) })",
            "assign.rhs-write",
        ),
        (
            "var p = Point { x: 1 }\np.x = run(fn [var p]() -> i32 { p.x = 2\nreturn 0 })",
            "assign.rhs-write",
        ),
        (
            "var n: i32 = 1\nfor i in 0..<run(fn [var n]() -> i32 { n = 2\nreturn n }) {}",
            "for.bound-reference",
        ),
        (
            "execute(fn []() -> void { var n: i32 = 1\nexecute(fn [var n]() -> void {}) })",
            "var.never-reassigned",
        ),
    ] {
        let source = format!("{prelude}fn main() -> u8 {{ {body}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
    let source = format!(
        "{prelude}fn bad(n: i32) -> i32 {{ return run(fn [var n]() -> i32 {{ return n }}) }}\nfn main() -> u8 {{ return 0 }}"
    );
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(
        error.to_string().contains("[closure.capture-list]"),
        "{error}"
    );
}
