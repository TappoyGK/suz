//! 関数の契約を、実行結果と領域をまたぐ拒否例の両方で検査する。
use crate::test_util::compile_and_run_c23;

#[test]
fn calls_accept_older_inputs_but_reject_saving_shorter_references() {
    for operation in [
        "link(i, o)",
        "let outer = o { outer#next = i }\n                link(i, o)",
        "fix ptr n = first(.h)",
        "fix ptr n = from_pointer(h)",
    ] {
        let source = format!(
            r#"struct Node {{ v: i32, next: ptr? Node }}
struct Holder {{ node: ptr? Node }}
fn mk(v: i32) -> ptr? Node {{ return new Node {{ v: v, next: null }} }}
fn link(a: ptr? Node, b: ptr? Node) -> void {{ let x = a {{ x#next = b }} }}
fn hold(n: ptr? Node) -> ptr? Holder {{ return new Holder {{ node: n }} }}
fn first(h: Holder) -> ptr? Node {{ return h.node }}
fn from_pointer(h: ptr? Holder) -> ptr? Node {{ let p = h {{ return p.node }}
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
        if operation.contains("outer#next") {
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.rule(), "arena.store", "{error}");
        } else {
            suz::compile(source.as_bytes()).unwrap();
        }
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
        "fix ptr p: ptr? Node = null\ncall(fn [p]() -> i64 { return p.v })",
        "var ptr p: ptr? Node = new Node { v: 1 }\ncall(fn [p]() -> i64 { return p.v })\np = null",
        "var ptr p: ptr? Node = new Node { v: 1 }\ncall(fn [var p]() -> i64 { return p.v })\np = null",
        // 外側の非nullのローカル番号0と、クロージャの通常の仮引数番号0を混同しない。
        "fix ptr p = new Node { v: 1 }\nborrow(fn [p](q: ptr? Node) -> i64 { return q.v })",
    ] {
        let source = format!(
            "struct Node {{ v: i64 }}\nfnshape Act() -> i64\nfnshape Borrow(ptr? Node) -> i64\nfn call(f: fn Act) -> i64 {{ return f() }}\nfn borrow(f: fn Borrow) -> i64 {{ return f(null) }}\nfn main() -> u8 {{ arena {{ {body}\n}}\nreturn 0 }}"
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
struct Box { p: ptr? i32 }
fnshape Read() -> i32
fnshape Update(ptr? i32) -> void
fn invoke(f: fn Read) -> i32 { return f() }
fn update(p: ptr? i32) -> void { let q = p { #q += 1 } }
fn forward(b: Box, f: fn Update) -> i32 {
    f(b.p)
    arena {
        fix ptr copy = new b
        let q = copy.p { return .q }
    }
    return 0
}
fn main() -> u8 {
    arena {
        fix ptr p = new (10 as i32)
        fix box = Box { p: p }
        if forward(box, fn update) != 11 { return 1 }
        if invoke(fn [p]() -> i32 { let q = p { return .q }
            return 0 }) != 11 { return 2 }
        if invoke(fn [box]() -> i32 { let q = box.p { return .q }
            return 0 }) != 11 { return 3 }
        fix ptr after = new (20 as i32)
        if .p != 11 || .after != 20 { return 4 }
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
struct Node { value: i32, next: ptr? Node }
struct Box { p: ptr? Node }
fn make(value: i32, tail: ptr? Node) -> ptr? Node {
    return new Node { value: value, next: tail }
}
fn append(p: ptr? Node, n: i32) -> void {
    let q = p {
        let next = q.next { append(next, n) }
        else { q#next = make(n, null) }
    }
}
fn boxed(p: ptr? Node) -> Box {
    arena { fix ptr scratch = new (99 as i32)
        if .scratch != 99 { return Box { p: null } }
    }
    return Box { p: make(3, p) }
}
fn sum(p: ptr? Node) -> i32 {
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
            let next = q.next { next#next = root }
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
fn parameter_stores_are_checked_by_lifetime_instead_of_blanket_prohibition() {
    for body in [
        "let q = p { q#next = p }",
        "let q = p { q#next = null }",
        "let q = p { #q = Node { next: null } }",
        "let q = p { let r = q.next { r#next = p } }",
        "let q = p { arena { q#next = new Node { next: null } } }",
    ] {
        let source = format!(
            "struct Node {{ next: ptr? Node }}\nfn change(p: ptr? Node) -> void {{ {body} }}\nfn main() -> u8 {{ return 0 }}"
        );
        if body.contains("arena {") {
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.rule(), "arena.store", "{error}");
        } else {
            suz::compile(source.as_bytes()).unwrap();
        }
    }
    for result in ["ptr? i32", "Box"] {
        let value = if result == "Box" {
            "Box { p: null }"
        } else {
            "null"
        };
        let source = format!(
            "struct Box {{ p: ptr? i32 }}\nfn bad() -> {result} {{ return {value} }}\nfn main() -> u8 {{ return 0 }}"
        );
        suz::compile(source.as_bytes()).unwrap();
    }
}

#[test]
fn arena_calls_check_transitive_regions_and_effects_of_callee_allocations() {
    let definitions = "struct Node { next: ptr? Node }\nstruct Box { p: ptr? Node }\nfn attach(p: ptr? Box) -> ptr? Node { fix ptr n = new Node { next: null }\nlet q = p { q#p = n }\nreturn n }\n";
    for (body, rule) in [
        ("attach(null)", "accept"),
        (
            "arena { fix ptr outer = new Node { next: null }\narena { fix ptr b = new Box { p: outer }\nattach(b) } }",
            "accept",
        ),
        (
            "arena { fix ptr outer = new Box { p: null }\narena { attach(outer) } }",
            "arena.store",
        ),
        (
            "arena { var ptr saved: ptr? Node = null\narena { saved = attach(null) } }",
            "arena.store",
        ),
        (
            "arena { var ptr saved: ptr? Node = null\narena { fix ptr b = new Box { p: null }\nattach(b)\nsaved = b.p } }",
            "arena.store",
        ),
        // 後の更新も候補集合に含むため、呼び出し前のnullだけで安全とは判定しない。
        (
            "arena { fix ptr outer = new Node { next: null }\narena { fix ptr b = new Box { p: null }\nattach(b)\nb#p = outer } }",
            "accept",
        ),
    ] {
        let source = format!("{definitions}fn main() -> u8 {{ {body}\nreturn 0 }}");
        if rule == "accept" {
            suz::compile(source.as_bytes()).unwrap();
        } else {
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.rule(), rule, "{body}: {error}");
        }
    }
}

#[test]
fn arena_function_cannot_return_or_store_inner_references() {
    for body in [
        "arena { return new Node { next: null } }",
        "arena { let q = p { q#next = new Node { next: null } } }\nreturn p",
        "arena { fix ptr inner = new Node { next: null }\nfix n = failed() or return inner\nreturn p }",
        "arena { fix ptr inner = new Node { next: null }\nfix n = (300 as i32) as u8 or return inner\nreturn p }",
        "arena { fix ptr inner = new Node { next: null }\nvar n: u8 = 255\nn = n + 1 or return inner\nreturn p }",
    ] {
        let source = format!(
            "struct Node {{ next: ptr? Node }}\nerror Err {{ Bad }}\nfn failed() -> i32 or Err {{ fail Err.Bad }}\nfn bad(p: ptr? Node) -> ptr? Node {{ {body} }}\nfn main() -> u8 {{ return 0 }}"
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
fn functions_share_the_current_arena_through_values_and_closures() {
    for source in [
        "fn main() -> u8 { return 0 }",
        "fn work() -> i32 { return 0 }\nfnshape Work() -> i32\nfn main() -> u8 { fix f: static fn Work = fn work\nreturn 0 }",
        "fn work() -> void { return }\nfn ordinary() -> void { work() }\nfn main() -> u8 { arena { ordinary() }\nreturn 0 }",
        "fn work() -> void { return }\nfnshape Work() -> void\nfn invoke(f: fn Work) -> void { f() }\nfn main() -> u8 { arena { invoke(fn []() -> void { work() }) }\nreturn 0 }",
    ] {
        suz::compile(source.as_bytes()).unwrap();
    }
}

#[test]
fn generic_pointer_parameters_and_results_keep_the_pointee_type() {
    let output = compile_and_run_c23(
        r#"
fn make(value: int) -> ptr? int { return new value }
fn read(p: ptr? int) -> int { let q = p { return .q }
    return 0 }
fn forward(p: ptr? int) -> int { return read(p) }
fn clear(p: ptr? int) -> void { let q = p { #q = 0 } }
fn read_float(p: ptr? float) -> float { let q = p { return .q }
    return 0.0 }
fnshape Read(ptr? i16) -> i16
fn main() -> u8 {
    arena {
        fix ptr p = make(12 as i16)
        if forward(p) != 12 { return 1 }
        fix f: static fn Read = fn read
        if f(p) != 12 { return 2 }
        fix ptr q: ptr? u8 = make(13)
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
    let source = b"fn take(p: ptr? int, n: int) -> void { return }\nfn main() -> u8 { arena { fix ptr p = new (1 as i16)\ntake(p, 2 as i32) }\nreturn 0 }";
    // 数値の幅を広げても、異なる型の実体を指すポインタには変換できない。
    assert!(suz::compile(source).is_err());
}

#[test]
fn fallible_arena_calls_preserve_reference_results_through_handlers() {
    let output = compile_and_run_c23(
        r#"
error Err { Bad }
fn maybe(ok: bool) -> ptr? i32 or Err {
    if ok { return new (17 as i32) }
    fail Err.Bad
}
fn recover_pointer() -> ptr? i32 {
    fix ptr p = maybe(false) or error { _ => { fix ptr fallback = maybe(true) or return null
recover fallback } }
    return p
}
fn main() -> u8 {
    arena {
        fix ptr a = recover_pointer()
        let p = a { if .p != 17 { return 1 } } else { return 2 }
        fix ptr b = maybe(false) or new (23 as i32)
        let p = b { if .p != 23 { return 3 } } else { return 4 }
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
            "error Err {{ Bad }}\nfn failer() -> ptr? i32 or Err {{ fail Err.Bad }}\nfn main() -> u8 {{ arena {{ fix ptr p = failer() {handler}\n}}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("[arena.store]"), "{error}");
    }
}

#[test]
fn reference_captures_and_pointer_chains_allow_safe_stores() {
    let source = b"fnshape Change() -> void\nfn invoke(f: fn Change) -> void { f() }\nfn main() -> u8 { arena { var ptr p: ptr? i32 = null\ninvoke(fn [var p]() -> void { p = null }) }\nreturn 0 }";
    suz::compile(source).unwrap();
    for source in [
        "fn change(p: ptr? ptr? i32) -> void { let q = p { #q = null } }\nfn main() -> u8 { return 0 }",
        "fn change(p: ptr? ptr? i32) -> void { return }\nfn main() -> u8 { arena { fix ptr outer = new (1 as i32)\narena { fix ptr inner = new outer\nchange(inner) } }\nreturn 0 }",
    ] {
        if source.contains("change(inner)") {
            assert_eq!(
                suz::compile(source.as_bytes()).unwrap_err().rule(),
                "type.expected"
            );
        } else {
            suz::compile(source.as_bytes()).unwrap();
        }
    }
}

#[test]
fn pointer_function_types_use_canonical_shapes_and_reject_unresolved_types() {
    let output = compile_and_run_c23(
        r#"
fnshape Same() -> i32
fnshape Alias() -> i32
fnshape Use(ptr? static fn Alias) -> i32
struct Box { f: ptr? static fn Alias }
fn value() -> i32 { return 29 }
fn call(p: ptr? static fn Same) -> i32 {
    let q = p { fix f = .q
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
        "fnshape Bad(ptr? int) -> i32",
        "fnshape Bad(ptr? static fn Bad) -> i32",
    ] {
        let source = format!("{shape}\nfn main() -> u8 {{ return 0 }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn let_evaluates_the_pointer_once_and_selects_the_nonnull_or_null_branch() {
    let source = r#"
fn select(count: ptr? i32, value: ptr? i32) -> ptr? i32 {
    let c = count { #c += 1 }
    return value
}
fn main() -> u8 {
    arena {
        fix ptr count = new (0 as i32)
        fix ptr value = new (7 as i32)
        let p = select(count, value) {
            if .p != 7 { return 1 }
        } else { return 2 }
        let p = select(count, null) { return 3 }
        else { if .count != 2 { return 4 } }
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

#[test]
fn returned_allocation_keeps_proven_store_diagnostics_inside_objection() {
    let source = b"struct Node { v: i32, next: ptr? Node }\n\
fn mk(v: i32) -> ptr Node { return new Node { v: v, next: null } }\n\
fn main() -> u8 {\n\
    arena {\n\
        fix ptr a = new Node { v: 1, next: null }\n\
        arena {\n\
            fix ptr t = mk(2)\n\
            objection [test] { a#next = t }\n\
        }\n\
    }\n\
    return 0\n\
}";
    let error = suz::compile(source).unwrap_err();
    assert_eq!(error.rule(), "arena.store");
    assert!(
        error.message().contains("proven lifetime violation"),
        "{error}"
    );
    assert!(error.message().contains("in member 'next'"), "{error}");
    assert!(
        error
            .related
            .iter()
            .any(|note| note.line == 7 && note.message().contains("this call may allocate")),
        "{error}"
    );
    assert!(
        error
            .related
            .iter()
            .any(|note| note.line == 9 && note.message().contains("its arena ends here")),
        "{error}"
    );
}

#[test]
fn unified_arena_named_functions_allocate_without_an_explicit_block() {
    let source = r#"
struct Node { v: i32, next: ptr? Node }
fn make(v: i32) -> ptr Node { return new Node { v: v, next: null } }
fn identity(p: ptr Node) -> ptr Node { return p }
fn read(p: ptr Node) -> i32 { return p.v }
fn main() -> u8 {
    fix ptr p = make(7)
    if identity(p).v != 7 { return 1 }
    arena { if read(p) != 7 { return 2 } }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "unified-arena.suz");
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn unified_arena_named_calls_check_stores_and_returned_periods() {
    let prefix = r#"
struct Node { v: i32, next: ptr? Node }
fn identity(p: ptr Node) -> ptr Node { return p }
fn attach(p: ptr Node, q: ptr Node) -> void { p#next = q }
"#;
    for (operation, rule) in [
        ("attach(outer, inner)", "arena.store"),
        ("fix ptr p = identity(outer)\np#next = inner", "arena.store"),
        ("saved = identity(outer)", "arena.return-lifetime"),
    ] {
        let source = format!(
            "{prefix}\nfn main() -> u8 {{\nfix ptr outer = new Node {{ v: 1, next: null }}\nvar ptr saved: ptr? Node = null\nsaved = outer\narena {{\nfix ptr inner = new Node {{ v: 2, next: null }}\n{operation}\n}}\nreturn 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains(&format!("[{rule}]")),
            "{operation}: {error}"
        );
    }
}

#[test]
fn unified_arena_removes_function_and_requirement_modifiers() {
    for source in [
        "arena fn f() -> void {}\nfn main() -> u8 { return 0 }",
        "@needs T: arena .make() -> T\nfn f[T](v: T) -> T { return v.make() }\nfn main() -> u8 { return 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err());
    }
}

#[test]
fn unified_arena_callbacks_use_each_calls_actual_function() {
    let source = r#"
struct Node { v: i32, next: ptr? Node }
fnshape Link(ptr Node, ptr Node) -> void
fn apply(f: fn Link, a: ptr Node, b: ptr Node) -> void { f(a, b) }
fn read(a: ptr Node, b: ptr Node) -> void { if a.v == b.v { return } }
fn link(a: ptr Node, b: ptr Node) -> void { a#next = b }
fn main() -> u8 {
    fix ptr outer = new Node { v: 1, next: null }
    fix ptr peer = new Node { v: 2, next: null }
    apply(fn link, outer, peer)
    arena {
        fix ptr inner = new Node { v: 3, next: null }
        apply(fn read, outer, inner)
    }
    return 0
}
"#;
    suz::compile(source.as_bytes()).unwrap();
    let rejected = source.replace(
        "apply(fn read, outer, inner)",
        "apply(fn link, outer, inner)",
    );
    let error = suz::compile(rejected.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("[arena.store]"), "{error}");
}

#[test]
fn unified_arena_closure_reference_captures_use_the_invocation_arena() {
    let source = r#"
struct Node { v: i32 }
fnshape Act() -> void
fn run(f: fn Act) -> void { f() }
fn main() -> u8 {
    var ptr p: ptr? Node = null
    run(fn [var p]() -> void { p = new Node { v: 7 } })
    let n = p { if n.v != 7 { return 1 } } else { return 2 }
    return 0
}
"#;
    let output = compile_and_run_c23(source, "capture-arena.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let rejected = source.replace(
        "fn run(f: fn Act) -> void { f() }",
        "fn run(f: fn Act) -> void { arena { f() } }",
    );
    let error = suz::compile(rejected.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("[arena.store]"), "{error}");
}

#[test]
fn unified_arena_returned_capture_preserves_the_objects_region() {
    let source = r#"
struct Node { v: i32, next: ptr? Node }
fnshape Get() -> ptr Node
fn run(f: fn Get) -> ptr Node { return f() }
fn main() -> u8 {
    fix ptr outer = new Node { v: 1, next: null }
    arena {
        fix ptr inner = new Node { v: 2, next: null }
        fix ptr returned = run(fn [outer]() -> ptr Node { return outer })
        returned#next = inner
    }
    return 0
}
"#;
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("[arena.store]"), "{error}");
    suz::compile(
        source
            .replace("returned#next = inner", "returned#v = inner.v")
            .as_bytes(),
    )
    .unwrap();
}

#[test]
fn unified_arena_static_fields_keep_different_declarations_separate() {
    let source = r#"
struct Node { v: i32, next: ptr? Node }
fnshape Link(ptr Node, ptr Node) -> void
struct BoxA { cb: static fn Link }
struct BoxB { cb: static fn Link }
fn read(a: ptr Node, b: ptr Node) -> void { if a.v == b.v { return } }
fn link(a: ptr Node, b: ptr Node) -> void { a#next = b }
fn main() -> u8 {
    fix a = BoxA { cb: fn link }
    fix b = BoxB { cb: fn read }
    fix ptr outer = new Node { v: 1, next: null }
    arena {
        fix ptr inner = new Node { v: 2, next: null }
        b.cb(outer, inner)
    }
    return 0
}
"#;
    suz::compile(source.as_bytes()).unwrap();
    let error = suz::compile(
        source
            .replace("b.cb(outer, inner)", "a.cb(outer, inner)")
            .as_bytes(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("[arena.store]"), "{error}");
}

#[test]
fn unified_arena_forwarded_and_captured_callbacks_keep_their_environment() {
    let source = r#"
struct Node { v: i32, next: ptr? Node }
fnshape Get() -> ptr Node
fn invoke(f: fn Get) -> ptr Node { return f() }
fn relay(f: fn Get) -> ptr Node {
    return invoke(fn [f]() -> ptr Node { return f() })
}
fn main() -> u8 {
    fix ptr outer = new Node { v: 1, next: null }
    arena {
        fix ptr inner = new Node { v: 2, next: null }
        fix ptr p = relay(fn [outer]() -> ptr Node { return outer })
        p#next = inner
    }
    return 0
}
"#;
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap_err().rule(),
        "arena.store"
    );
    let accepted = source.replace("p#next = inner", "p#v = inner.v");
    let output = compile_and_run_c23(&accepted, "forwarded-capture.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn unified_arena_static_candidates_follow_assignments_parameters_and_returns() {
    let prefix = r#"
struct Node { v: i32, next: ptr? Node }
fnshape Link(ptr Node, ptr Node) -> void
fn read(a: ptr Node, b: ptr Node) -> void { if a.v == b.v { return } }
fn write(a: ptr Node, b: ptr Node) -> void { a#next = b }
fn choose() -> static fn Link { return fn write }
fn apply(f: static fn Link, a: ptr Node, b: ptr Node) -> void { f(a, b) }
"#;
    for operation in [
        "var f: static fn Link = fn read\nf(outer, inner)\nf = fn write",
        "fix f = choose()\nf(outer, inner)",
        "apply(fn read, outer, inner)\napply(fn write, inner, inner)",
    ] {
        let source = format!(
            "{prefix}\nfn main() -> u8 {{\nfix ptr outer = new Node {{ v: 1, next: null }}\narena {{\nfix ptr inner = new Node {{ v: 2, next: null }}\n{operation}\n}}\nreturn 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "arena.store", "{error}");
        assert!(
            error
                .related
                .iter()
                .any(|note| note.message().contains("possible call target: write")),
            "{error}"
        );
        assert!(
            error
                .related
                .iter()
                .any(|note| note.message().contains("initialization or assignment")),
            "{error}"
        );
    }
}

#[test]
fn unified_arena_comparison_methods_share_the_current_arena() {
    let source = r#"
struct Num { v: i32 }
fn make(v: i32) -> ptr Num { return new Num { v: v } }
fn Num.<(b: Num) -> bool {
    fix ptr copy = make(self.v)
    return copy.v < b.v
}
fn main() -> u8 {
    fix a = Num { v: 7 }
    return ifexpr a < (Num { v: 8 }) { 0 } else { 1 }
}
"#;
    let output = compile_and_run_c23(source, "comparison-arena.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn unified_arena_reports_growing_callback_environments_instead_of_looping() {
    let source = r#"
fnshape Act() -> void
fn plain() -> void {}
fn grow(f: fn Act, count: i32) -> void {
    if count == 0 { f()
        return }
    grow(fn [f]() -> void { f() }, count - 1)
}
fn main() -> u8 { grow(fn plain, 2)
    return 0 }
"#;
    // ポインタのないプログラムには寿命解析の資源上限を課さない。
    suz::compile(source.as_bytes()).unwrap();
    let source = format!(
        "struct Node {{ v: i32 }}\n{}",
        source.replace(
            "grow(fn plain, 2)",
            "fix ptr node = new Node { v: 1 }\ngrow(fn [node]() -> void { node#v = 2 }, 2)",
        )
    );
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap_err().rule(),
        "arena.analysis-limit"
    );
}

#[test]
fn unified_arena_function_pointer_aliases_share_candidates_without_type_wide_merging() {
    let source = r#"
struct Node { next: ptr? Node }
fnshape Link(ptr Node, ptr Node) -> void
fn read(a: ptr Node, b: ptr Node) -> void {}
fn write(a: ptr Node, b: ptr Node) -> void { a#next = b }
fn main() -> u8 {
    fix ptr unrelated = new (fn write)
    fix ptr chosen = new (fn read)
    fix ptr alias = chosen
    fix ptr outer = new Node { next: null }
    arena {
        fix ptr inner = new Node { next: null }
        (.chosen)(outer, inner)
    }
    #alias = fn read
    return 0
}
"#;
    suz::compile(source.as_bytes()).unwrap();
    let rejected = source.replace("#alias = fn read", "#alias = fn write");
    assert_eq!(
        suz::compile(rejected.as_bytes()).unwrap_err().rule(),
        "arena.store"
    );
}

#[test]
fn unified_arena_generic_function_fields_filter_candidates_by_concrete_signature() {
    let source = r#"
struct Node { next: ptr? Node }
fnshape Link[T](T, T) -> void
struct Handler[T] { run: static fn Link[T] }
fn write(a: ptr Node, b: ptr Node) -> void { a#next = b }
fn read(a: i32, b: i32) -> void {}
fn main() -> u8 {
    fix pointers = Handler[ptr Node] { run: fn write }
    fix numbers = Handler[i32] { run: fn read }
    numbers.run(1, 2)
    return 0
}
"#;
    suz::compile(source.as_bytes()).unwrap();
}

#[test]
fn callee_objection_cannot_override_a_proven_call_store() {
    for body in [
        "n#next = new Node { v: 2, next: null }",
        "objection [locally assumed safe] { n#next = new Node { v: 2, next: null } }",
    ] {
        for call in [
            "attach(n)",
            "relay(n)",
            "invoke(fn attach, n)",
            "choose()(n)",
            "cycle(n, false)",
            "generic[Node](n)",
            "run(fn [n]() -> void { attach(n) })",
        ] {
            let source = format!(
                r#"
struct Node {{ v: i32, next: ptr? Node }}
fnshape Attach(ptr Node) -> void
fn attach(n: ptr Node) -> void {{ {body} }}
fn relay(n: ptr Node) -> void {{ attach(n) }}
fn invoke(f: static fn Attach, n: ptr Node) -> void {{ f(n) }}
fn choose() -> static fn Attach {{ return fn attach }}
fnshape Act() -> void
fn run(f: fn Act) -> void {{ f() }}
fn cycle(n: ptr Node, flag: bool) -> void {{ if flag {{ back(n) }} else {{ attach(n) }} }}
fn back(n: ptr Node) -> void {{ cycle(n, false) }}
fn generic[T](n: ptr Node) -> void {{ attach(n) }}
fn main() -> u8 {{
    fix ptr n = new Node {{ v: 1, next: null }}
    arena {{ {call} }}
    return 0
}}
"#
            );
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.rule(), "arena.store", "{error}");
            assert!(
                error.to_string().contains("proven lifetime violation"),
                "{error}"
            );
        }
    }
}

#[test]
fn nested_generic_fields_do_not_expand_callable_paths() {
    for ty in ["ptr Node", "static fn Read"] {
        let initial = if ty == "ptr Node" { "outer" } else { "fn read" };
        let source = format!(
            r#"
struct Box[T] {{ value: T }}
struct Node {{ v: i32 }}
fnshape Read(ptr Node) -> i32
fn read(n: ptr Node) -> i32 {{ return n.v }}
fn main() -> u8 {{
    fix ptr outer = new Node {{ v: 1 }}
    fix c = Box[{ty}] {{ value: {initial} }}
    fix b = Box[Box[{ty}]] {{ value: c }}
    return 0
}}
"#
        );
        suz::compile(source.as_bytes()).unwrap();
    }
    let source = r#"
struct Box[T] { value: T }
struct Node { v: i32 }
fn main() -> u8 {
    fix ptr outer = new Node { v: 1 }
    var b = Box[Box[ptr Node]] { value: Box[ptr Node] { value: outer } }
    arena { b.value#value = new Node { v: 2 } }
    return 0
}
"#;
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap_err().rule(),
        "arena.store"
    );
}
