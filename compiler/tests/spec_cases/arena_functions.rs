//! 関数の契約を、実行結果と領域をまたぐ拒否例の両方で検査する。
use crate::test_util::compile_and_run_c23;

#[test]
fn invalid_inner_arena_call_is_reported_at_its_argument_not_an_outer_call() {
    for (operation, argument) in [
        ("link(i, o)", "argument 2"),
        (
            "let outer = o { outer.next = i }\n                link(i, o)",
            "argument 2",
        ),
        ("fix ptr n = first(*h)", "argument 1.node"),
        ("fix ptr n = from_pointer(h)", "argument 1"),
    ] {
        let source = format!(
            r#"struct Node {{ v: i32, next: ptr Node }}
struct Holder {{ node: ptr Node }}
arena fn mk(v: i32) -> ptr Node {{ return new Node {{ v: v, next: null }} }}
arena fn link(a: ptr Node, b: ptr Node) -> void {{ let x = a {{ x.next = b }} }}
arena fn hold(n: ptr Node) -> ptr Holder {{ return new Holder {{ node: n }} }}
arena fn first(h: Holder) -> ptr Node {{ return h.node }}
arena fn from_pointer(h: ptr Holder) -> ptr Node {{ let p = h {{ return p.node }}
    return null }}
fn main() -> u8 {{
    arena {{
        fix ptr o = mk(1)
        fix ptr maybe_h = hold(o)
        let h = maybe_h {{
            arena {{
                fix ptr i = mk(2)
                {operation}
            }}
        }}
    }}
    return 0
}}"#
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("[arena.fn-call]"), "{error}");
        assert_eq!(
            error.line,
            15 + operation.lines().count(),
            "{operation}: {error}"
        );
        assert!(error.message().contains(argument), "{operation}: {error}");
        let allocation_line = if operation.contains("from_pointer") {
            12
        } else {
            11
        };
        assert_eq!(
            error.related[0].line, allocation_line,
            "{operation}: {error}"
        );
    }
}

#[test]
fn value_captures_preserve_nonnull_pointers_with_independent_local_ids() {
    let output = compile_and_run_c23(
        r#"
struct Node { v: i64 }
fnshape Act() -> i64
fnshape Add(i64) -> i64
fn call(f: fn Act) -> i64 { return f() }
fn apply(f: fn Add) -> i64 { return f(2) }
fn main() -> u8 {
    arena {
        fix ptr o = new Node { v: 7 }
        fix ptr alias = o
        if call(fn [o]() -> i64 { return o.v }) != 7 { return 1 }
        if apply(fn [alias](x: i64) -> i64 { return alias.v + x }) != 9 { return 2 }
        if call(fn [o]() -> i64 { return call(fn [o]() -> i64 { return o.v }) }) != 7 { return 3 }
        let p = o {
            if call(fn [p]() -> i64 { return p.v }) != 7 { return 4 }
        }
    }
    return 0
}
"#,
        "nonnull-capture.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    for body in [
        "fix ptr p: ptr Node = null\ncall(fn [p]() -> i64 { return p.v })",
        "var ptr p = new Node { v: 1 }\ncall(fn [p]() -> i64 { return p.v })\np = null",
        "var ptr p = new Node { v: 1 }\ncall(fn [var p]() -> i64 { return p.v })\np = null",
        // 外側の非nullのローカル番号0と、クロージャの通常の仮引数番号0を混同しない。
        "fix ptr p = new Node { v: 1 }\nborrow(fn [p](q: ptr Node) -> i64 { return q.v })",
    ] {
        let source = format!(
            "struct Node {{ v: i64 }}\nfnshape Act() -> i64\nfnshape Borrow(ptr Node) -> i64\nfn call(f: fn Act) -> i64 {{ return f() }}\nfn borrow(f: fn Borrow) -> i64 {{ return f(null) }}\nfn main() -> u8 {{ arena {{ {body}\n}}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[arena.null-deref]"),
            "{body}: {error}"
        );
    }
}

#[test]
fn ordinary_functions_borrow_forward_and_capture_without_rewinding_the_caller() {
    let output = compile_and_run_c23(
        r#"
struct Box { p: ptr i32 }
fnshape Read() -> i32
fnshape Update(ptr i32) -> void
fn invoke(f: fn Read) -> i32 { return f() }
fn update(p: ptr i32) -> void { let q = p { *q += 1 } }
fn forward(b: Box, f: fn Update) -> i32 {
    f(b.p)
    arena {
        fix ptr copy = new b
        let q = copy.p { return *q }
    }
    return 0
}
fn main() -> u8 {
    arena {
        fix ptr p = new (10 as i32)
        fix box = Box { p: p }
        if forward(box, fn update) != 11 { return 1 }
        if invoke(fn [p]() -> i32 { let q = p { return *q }
            return 0 }) != 11 { return 2 }
        if invoke(fn [box]() -> i32 { let q = box.p { return *q }
            return 0 }) != 11 { return 3 }
        fix ptr after = new (20 as i32)
        if *p != 11 || *after != 20 { return 4 }
    }
    return 0
}
"#,
        "borrow.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn arena_functions_build_recursive_graphs_and_return_aggregates_in_the_callers_region() {
    let output = compile_and_run_c23(
        r#"
struct Node { value: i32, next: ptr Node }
struct Box { p: ptr Node }
arena fn make(value: i32, tail: ptr Node) -> ptr Node {
    return new Node { value: value, next: tail }
}
arena fn append(p: ptr Node, n: i32) -> void {
    let q = p {
        let next = q.next { append(next, n) }
        else { q.next = make(n, null) }
    }
}
arena fn boxed(p: ptr Node) -> Box {
    arena { fix ptr scratch = new (99 as i32)
        if *scratch != 99 { return Box { p: null } }
    }
    return Box { p: make(3, p) }
}
fn sum(p: ptr Node) -> i32 {
    let q = p { return q.value + sum(q.next) }
    return 0
}
fn main() -> u8 {
    arena {
        fix ptr root = make(1, null)
        arena {
            fix ptr temporary = make(11, null)
            if sum(temporary) != 11 { return 5 }
        }
        append(root, 2)
        fix box = boxed(root)
        if sum(box.p) != 6 { return 1 }
        let q = root {
            let next = q.next { next.next = root }
        }
        // 循環があっても呼び出し時の到達可能性検査は停止する。
        fix cycle = boxed(root)
        let q = cycle.p { if q.value != 3 { return 2 } }
        else { return 3 }
    }
    return 0
}
"#,
        "arena-functions.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn borrowed_objects_cannot_receive_pointers_even_null_or_whole_struct_copies() {
    for body in [
        "let q = p { q.next = p }",
        "let q = p { q.next = null }",
        "let q = p { *q = Node { next: null } }",
        "let q = p { let r = q.next { r.next = p } }",
        "let q = p { arena { q.next = new Node { next: null } } }",
    ] {
        let source = format!(
            "struct Node {{ next: ptr Node }}\nfn change(p: ptr Node) -> void {{ {body} }}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[arena.parameter]"),
            "{body}: {error}"
        );
        assert!(error.message().contains("next"), "{error}");
        assert_eq!(error.related[0].line, 2);
    }
    for result in ["ptr i32", "Box"] {
        let value = if result == "Box" {
            "Box { p: null }"
        } else {
            "null"
        };
        let source = format!(
            "struct Box {{ p: ptr i32 }}\nfn bad() -> {result} {{ return {value} }}\nfn main() -> u8 {{ return 0 }}"
        );
        assert!(
            suz::compile(source.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("[arena.parameter]")
        );
    }
}

#[test]
fn arena_calls_check_transitive_regions_and_effects_of_callee_allocations() {
    let definitions = "struct Node { next: ptr Node }\nstruct Box { p: ptr Node }\narena fn attach(p: ptr Box) -> ptr Node { fix ptr n = new Node { next: null }\nlet q = p { q.p = n }\nreturn n }\n";
    for (body, rule) in [
        ("attach(null)", "arena.fn-call"),
        (
            "arena { fix ptr outer = new Node { next: null }\narena { fix ptr b = new Box { p: outer }\nattach(b) } }",
            "arena.fn-call",
        ),
        (
            "arena { fix ptr outer = new Box { p: null }\narena { attach(outer) } }",
            "arena.fn-call",
        ),
        (
            "arena { var ptr saved: ptr Node = null\narena { saved = attach(null) } }",
            "arena.store",
        ),
        (
            "arena { var ptr saved: ptr Node = null\narena { fix ptr b = new Box { p: null }\nattach(b)\nsaved = b.p } }",
            "arena.store",
        ),
        // 後の更新も候補集合に含むため、呼び出し前のnullだけで安全とは判定しない。
        (
            "arena { fix ptr outer = new Node { next: null }\narena { fix ptr b = new Box { p: null }\nattach(b)\nb.p = outer } }",
            "arena.fn-call",
        ),
    ] {
        let source = format!("{definitions}fn main() -> u8 {{ {body}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains(&format!("[{rule}]")),
            "{body}: {error}"
        );
        if body != "attach(null)" {
            assert!(!error.related.is_empty(), "{error}");
        }
        if body.contains("p: outer") {
            assert!(error.message().contains("argument 1.p"), "{error}");
        }
    }
}

#[test]
fn arena_function_cannot_return_or_store_inner_references() {
    for body in [
        "arena { return new Node { next: null } }",
        "arena { let q = p { q.next = new Node { next: null } } }\nreturn p",
        "arena { fix ptr inner = new Node { next: null }\nfix n = failed() or return inner\nreturn p }",
        "arena { fix ptr inner = new Node { next: null }\nfix n = (300 as i32) as u8 or return inner\nreturn p }",
        "arena { fix ptr inner = new Node { next: null }\nvar n: u8 = 255\nn = n + 1 or return inner\nreturn p }",
    ] {
        let source = format!(
            "struct Node {{ next: ptr Node }}\nerror Err {{ Bad }}\nfn failed() -> i32 or Err {{ fail Err.Bad }}\narena fn bad(p: ptr Node) -> ptr Node {{ {body} }}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[arena.store]"),
            "{body}: {error}"
        );
        assert_eq!(error.related.len(), 2, "{error}");
    }
}

#[test]
fn arena_functions_cannot_be_values_or_inherit_a_region_through_ordinary_functions() {
    for (source, rule) in [
        ("arena fn main() -> u8 { return 0 }", "arena.fn"),
        (
            "arena fn work() -> i32 { return 0 }\nfnshape Work() -> i32\nfn main() -> u8 { fix f: static fn Work = fn work\nreturn 0 }",
            "arena.fn-value",
        ),
        (
            "arena fn work() -> void { return }\nfn ordinary() -> void { work() }\nfn main() -> u8 { arena { ordinary() }\nreturn 0 }",
            "arena.fn-call",
        ),
        (
            "arena fn work() -> void { return }\nfnshape Work() -> void\nfn invoke(f: fn Work) -> void { f() }\nfn main() -> u8 { arena { invoke(fn []() -> void { work() }) }\nreturn 0 }",
            "arena.fn-call",
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
fn generic_pointer_parameters_and_results_keep_the_pointee_type() {
    let output = compile_and_run_c23(
        r#"
arena fn make(value: int) -> ptr int { return new value }
fn read(p: ptr int) -> int { let q = p { return *q }
    return 0 }
fn forward(p: ptr int) -> int { return read(p) }
fn clear(p: ptr int) -> void { let q = p { *q = 0 } }
fn read_float(p: ptr float) -> float { let q = p { return *q }
    return 0.0 }
fnshape Read(ptr i16) -> i16
fn main() -> u8 {
    arena {
        fix ptr p = make(12 as i16)
        if forward(p) != 12 { return 1 }
        fix f: static fn Read = fn read
        if f(p) != 12 { return 2 }
        fix ptr q: ptr u8 = make(13)
        if read(q) != 13 { return 3 }
        clear(q)
        if read(q) != 0 { return 4 }
        fix ptr real = new (1.5 as f32)
        if read_float(real) != 1.5 { return 5 }
    }
    return 0
}
"#,
        "generic-pointers.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let source = b"fn take(p: ptr int, n: int) -> void { return }\nfn main() -> u8 { arena { fix ptr p = new (1 as i16)\ntake(p, 2 as i32) }\nreturn 0 }";
    // 数値の幅を広げても、異なる型の実体を指すポインタには変換できない。
    assert!(suz::compile(source).is_err());
}

#[test]
fn fallible_arena_calls_preserve_reference_results_through_handlers() {
    let output = compile_and_run_c23(
        r#"
error Err { Bad }
arena fn maybe(ok: bool) -> ptr i32 or Err {
    if ok { return new (17 as i32) }
    fail Err.Bad
}
arena fn recover_pointer() -> ptr i32 {
    fix ptr p = maybe(false) or error { _ => { fix ptr fallback = maybe(true) or return null
recover fallback } }
    return p
}
fn main() -> u8 {
    arena {
        fix ptr a = recover_pointer()
        let p = a { if *p != 17 { return 1 } } else { return 2 }
        fix ptr b = maybe(false) or new (23 as i32)
        let p = b { if *p != 23 { return 3 } } else { return 4 }
    }
    return 0
}
"#,
        "arena-errors.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    for handler in [
        "or error { _ => { arena { recover new (7 as i32) } } }",
        "or error { _ => { arena { fix ptr p = new (7 as i32)\nrecover expr { p } } } }",
    ] {
        let source = format!(
            "error Err {{ Bad }}\narena fn failer() -> ptr i32 or Err {{ fail Err.Bad }}\nfn main() -> u8 {{ arena {{ fix ptr p = failer() {handler}\n}}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("[arena.store]"), "{error}");
    }
}

#[test]
fn reference_captures_and_pointer_chains_keep_the_borrowed_boundary() {
    let source = b"fnshape Change() -> void\nfn invoke(f: fn Change) -> void { f() }\nfn main() -> u8 { arena { var ptr p: ptr i32 = null\ninvoke(fn [var p]() -> void { p = null }) }\nreturn 0 }";
    let error = suz::compile(source).unwrap_err();
    assert!(error.to_string().contains("[arena.parameter]"), "{error}");
    for source in [
        "fn change(p: ptr ptr i32) -> void { let q = p { *q = null } }\nfn main() -> u8 { return 0 }",
        "arena fn change(p: ptr ptr i32) -> void { return }\nfn main() -> u8 { arena { fix ptr outer = new (1 as i32)\narena { fix ptr inner = new outer\nchange(inner) } }\nreturn 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn pointer_function_types_use_canonical_shapes_and_reject_unresolved_types() {
    let output = compile_and_run_c23(
        r#"
fnshape Same() -> i32
fnshape Alias() -> i32
fnshape Use(ptr static fn Alias) -> i32
struct Box { f: ptr static fn Alias }
fn value() -> i32 { return 29 }
fn call(p: ptr static fn Same) -> i32 {
    let q = p { fix f = *q
        return f() }
    return 0
}
fn main() -> u8 {
    arena {
        fix f: static fn Alias = fn value
        fix ptr p = new f
        fix box = Box { f: p }
        fix use_fn: static fn Use = fn call
        if use_fn(box.f) != 29 { return 1 }
    }
    return 0
}
"#,
        "pointer-function-types.suz",
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    for shape in [
        "fnshape Bad(ptr int) -> i32",
        "fnshape Bad(ptr static fn Bad) -> i32",
    ] {
        let source = format!("{shape}\nfn main() -> u8 {{ return 0 }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn let_evaluates_the_pointer_once_and_selects_the_nonnull_or_null_branch() {
    let source = r#"
arena fn select(count: ptr i32, value: ptr i32) -> ptr i32 {
    let c = count { *c += 1 }
    return value
}
fn main() -> u8 {
    arena {
        fix ptr count = new (0 as i32)
        fix ptr value = new (7 as i32)
        let p = select(count, value) {
            if *p != 7 { return 1 }
        } else { return 2 }
        let p = select(count, null) { return 3 }
        else { if *count != 2 { return 4 } }
    }
    return 0
}
"#;
    assert_eq!(
        crate::test_util::compile_and_run_c23(source, "let.suz")
            .status
            .code(),
        Some(0)
    );
}
