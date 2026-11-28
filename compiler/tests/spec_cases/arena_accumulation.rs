//! 確保先だけでなく、反復外への到達と関数内部の一時確保を区別する。
fn warnings(extra: &str, body: &str) -> Vec<suz::Warning> {
    let source = format!(
        "struct Node {{ value: i32, next: mut? Node }}\nfn make() -> mut Node {{ return new Node {{ value: 1, next: nil }} }}\n{extra}\nfn main() -> u8 {{ {body}\nreturn 0 }}"
    );
    suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default)
        .unwrap_or_else(|error| panic!("{error}\n{source}"))
        .into_iter()
        .filter(|warning| warning.rule() == "arena.loop-allocation")
        .collect()
}

#[test]
fn iteration_lifetime_is_distinct_from_allocation_arena() {
    for (body, expected) in [
        ("loop { fix p = make()\nbreak }", 1),
        ("arena { loop { fix p = make()\nbreak } }", 1),
        ("loop { arena { fix p = make() }\nbreak }", 0),
        (
            "loop { fix p = new Node { value: 1, next: nil }\nbreak }",
            1,
        ),
        (
            "var head: mut? Node = nil\nloop { head = make()\nbreak }",
            0,
        ),
        (
            "var head: mut? Node = nil\nloop { fix p = make()\nhead = new Node { value: 2, next: p }\nbreak }",
            0,
        ),
        (
            "loop { fix p = make()\nfix holder = new Node { value: 2, next: p }\nbreak }",
            2,
        ),
        ("loop { fix p = make()\nreturn 0 }", 1),
    ] {
        assert_eq!(warnings("", body).len(), expected, "{body}");
    }
}

#[test]
fn summaries_separate_temporary_allocations_from_retained_results() {
    for (extra, body, expected) in [
        (
            "fn work[T](value: T) -> void { loop { fix p = new value\nbreak } }",
            "work(1 as i32)",
            1,
        ),
        (
            "fnshape Act() -> void\nfn run(f: fn Act) -> void { f() }",
            "run(fn []() -> void { loop { fix p = make()\nbreak } })",
            1,
        ),
        (
            "error Err { Failed }\nfn work() -> void or Err { fix p = make() }\nfn wrap() -> void { work() or ignore [failure is unused] }",
            "loop { wrap()\nbreak }",
            1,
        ),
        (
            "fn work() -> void { fix p = make() }\nfn wrap() -> void { work() }",
            "loop { wrap()\nbreak }",
            1,
        ),
        (
            "fn work() -> void { arena { fix p = make() } }",
            "loop { work()\nbreak }",
            0,
        ),
        (
            "fn work() -> mut Node { fix scratch = make()\nreturn make() }",
            "var head: mut? Node = nil\nloop { head = work()\nbreak }",
            1,
        ),
        (
            "fn work() -> mut Node { return make() }",
            "var head: mut? Node = nil\nloop { head = work()\nbreak }",
            0,
        ),
        (
            "fn work() -> mut Node { loop { return make() } }",
            "fix p = work()",
            0,
        ),
    ] {
        assert_eq!(warnings(extra, body).len(), expected, "{extra}\n{body}");
    }
}

#[test]
fn callback_and_capture_effects_reach_loop_warnings() {
    let extra = "fnshape Get() -> mut Node\nfn invoke(f: fn Get) -> mut Node { return f() }\nstruct Handler { f: static fn Get }";
    for (body, expected) in [
        ("loop { fix p = invoke(fn make)\nbreak }", 1),
        (
            "loop { fix p = invoke(fn []() -> mut Node { return make() })\nbreak }",
            1,
        ),
        (
            "fix h = Handler { f: fn make }\nloop { fix p = h.f()\nbreak }",
            1,
        ),
        (
            "var p: mut? Node = nil\nloop { p = invoke(fn make)\nbreak }",
            0,
        ),
    ] {
        assert_eq!(warnings(extra, body).len(), expected, "{body}");
    }
}

#[test]
fn keep_and_allow_suppress_only_their_own_allocations() {
    for keep in ["keep ", ""] {
        let extra = format!(
            "extern \"host.h\" {{ fn save({keep}p: *i32) -> void }}\nfn retain(p: mut i32) -> void {{ trusted {{ save(p) }} }}"
        );
        assert_eq!(
            warnings(&extra, "loop { fix p = new (1 as i32)\nretain(p)\nbreak }").len(),
            usize::from(keep.is_empty())
        );
    }
    assert!(
        warnings(
            "",
            "loop { fix p = make() allow [arena.loop-allocation: bounded iteration]\nbreak }"
        )
        .is_empty()
    );
}

#[test]
fn loop_conditions_captures_and_nested_arenas_preserve_escape_routes() {
    for (extra, body, expected) in [
        (
            "fn test() -> bool { fix p = make()\nreturn false }",
            "while test() { break }",
            1,
        ),
        (
            "fn count() -> i32 { fix p = make()\nreturn 2 }",
            "for i in 0..<count() { fix x = i }",
            0,
        ),
        (
            "fnshape Act() -> void\nfn run(f: fn Act) -> void { f() }",
            "var p: mut? Node = nil\nloop { run(fn [var p]() -> void { p = make() })\nbreak }",
            0,
        ),
        (
            "fnshape Act() -> void\nfn run(f: fn Act) -> void { f() }",
            "loop { var p: mut? Node = nil\nrun(fn [var p]() -> void { p = make() })\nbreak }",
            1,
        ),
        (
            "",
            "loop { arena { loop { fix p = make()\nbreak } }\nbreak }",
            1,
        ),
        (
            "",
            "fix holder = make()\nloop { fix p = make()\nholder#next = p\nbreak }",
            0,
        ),
        (
            "fn work[T](value: T) -> mut T { return new value }",
            "loop { fix p = work(1 as i32)\nbreak }",
            1,
        ),
    ] {
        assert_eq!(warnings(extra, body).len(), expected, "{extra}\n{body}");
    }
}

#[test]
fn warning_location_and_allow_logging_identify_the_operation() {
    let source = b"fn main() -> u8 {\nfor i in 0..<2 {\nfix p = new (1 as i32) allow [arena.loop-allocation: bounded loop]\n}\nreturn 0 }";
    let options = suz::CompilationOptions {
        log_allow: true,
        ..Default::default()
    };
    let warnings = suz::check_source_with_options(source, &options).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3);
    assert_eq!(warnings[0].allowed_reason.as_deref(), Some("bounded loop"));
    let error = suz::check_source(
        b"fn main() -> u8 { fix x = 1 allow [arena.loop-allocation: no allocation]\nreturn 0 }",
        suz::ArithmeticMode::Default,
    )
    .unwrap_err();
    assert_eq!(error.rule(), "allow.unused");
}

#[test]
fn allocating_comparison_methods_report_each_operator() {
    let source = b"struct Value { n: i32 }\nfn Value.<(other: Value) -> bool { fix p = new self\nreturn self.n < other.n }\nfn main() -> u8 {\nfix v = Value { n: 1 }\nloop {\nfix compared = v < v < v allow [arena.loop-allocation: bounded comparison]\nbreak\n}\nreturn 0 }";
    let options = suz::CompilationOptions {
        log_allow: true,
        ..Default::default()
    };
    let warnings = suz::check_source_with_options(source, &options).unwrap();
    assert_eq!(warnings.len(), 2);
    assert!(
        warnings
            .iter()
            .all(|w| w.line == 7 && w.allowed_reason.is_some())
    );
    assert_ne!(warnings[0].column, warnings[1].column);
}

#[test]
fn allocation_routes_preserve_forwarding_and_callable_candidates() {
    let extra = "fn work() -> void { fix p = make() }\nfn wrap() -> void { work() }\nfnshape Act() -> void\nfn run(f: fn Act) -> void { f() }\nstruct Handler { f: static fn Act }";
    for body in [
        "loop { wrap()\nbreak }",
        "loop { run(fn wrap)\nbreak }",
        "fix h = Handler { f: fn wrap }\nloop { h.f()\nbreak }",
    ] {
        let found = warnings(extra, body);
        assert_eq!(found.len(), 1);
        let notes = &found[0].related;
        for line in [2, 3, 4] {
            assert!(
                notes.iter().any(|note| note.line == line),
                "{body}: {notes:?}"
            );
        }
        if body.contains("fn wrap") {
            assert!(
                notes
                    .iter()
                    .any(|note| note.message().contains("possible call target")),
                "{notes:?}"
            );
            assert!(
                notes
                    .iter()
                    .any(|note| note.message().contains("supplies the call target")),
                "{notes:?}"
            );
        }
    }
}

#[test]
fn cli_prints_allocation_route_notes() {
    let directory = crate::test_util::TemporaryDirectory::new();
    let path = directory.0.join("route.suz");
    std::fs::write(&path, "fn make() -> void { fix p = new (1 as i32) }\nfn wrap() -> void { make() }\nfn main() -> u8 { loop { wrap()\nbreak }\nreturn 0 }").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_suz"))
        .arg("check")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("note:")
            && stderr.contains("route.suz:2:")
            && stderr.contains("route.suz:1:"),
        "{stderr}"
    );
}

#[test]
fn direct_function_arguments_point_to_the_function_value_expression() {
    let source = "fn work() -> void { fix p = new (1 as i32) }\nfnshape Act() -> void\nfn run(f: fn Act) -> void { f() }\nfn main() -> u8 {\nfor i in 0..<3 { run(fn work) }\nreturn 0 }";
    let found = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default).unwrap();
    let note = found[0]
        .related
        .iter()
        .find(|note| note.message().contains("supplies the call target"))
        .unwrap();
    assert_eq!(
        (note.line, note.column),
        (
            5,
            source.lines().nth(4).unwrap().find("fn work").unwrap() + 1
        )
    );
}

#[test]
fn long_allocation_routes_bound_each_warning_and_keep_the_allocation_site() {
    for depth in [16, 17, 256] {
        let mut source = String::from("fn c0x() -> void { fix p = new (1 as i32) }\n");
        for i in 1..depth {
            source.push_str(&format!("fn c{i}x() -> void {{ c{}x() }}\n", i - 1));
        }
        source.push_str("fn main() -> u8 { loop {\n");
        for _ in 0..32 {
            source.push_str(&format!("c{}x()\n", depth - 1));
        }
        source.push_str("break }\nreturn 0 }");
        let found = suz::check_source(source.as_bytes(), suz::ArithmeticMode::Default).unwrap();
        assert_eq!(found.len(), 32);
        for warning in found {
            assert!(
                warning.related.len() <= 18,
                "depth={depth}: {:?}",
                warning.related
            );
            assert_eq!(warning.related.last().unwrap().line, 1);
            let omitted = warning
                .related
                .iter()
                .find(|note| note.message().contains("calls omitted"));
            assert_eq!(omitted.is_some(), depth > 16);
            if let Some(note) = omitted {
                assert!(
                    note.message()
                        .contains(&format!("{} intermediate", depth - 16))
                );
            }
        }
    }
}
