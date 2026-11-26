use crate::test_util::compile_and_run_c23;

#[test]
fn objection_handles_correlated_source_and_destination_choices() {
    let source = r#"
struct Node { next: mut? Node }
fn connect(flag: bool) -> void {
    arena {
        fix outer = new Node { next: nil }
        arena {
            fix inner = new Node { next: nil }
            fix destination = ifexpr flag { outer } else { inner }
            fix source = ifexpr flag { outer } else { inner }
            objection [both choices use the same flag and therefore the same object] {
                let p = destination { p#next = source }
            }
        }
    }
}
fn main() -> u8 { connect(true)
    connect(false)
    return 0 }
"#;
    assert_eq!(
        compile_and_run_c23(source, "correlated.suz").status.code(),
        Some(0)
    );
}

#[test]
fn objection_accepts_a_safe_overwrite_without_changing_runtime_lifetimes() {
    let source = r#"
struct Node { v: i32, next: mut? Node }
fn main() -> u8 {
    arena {
        fix outer = new Node { v: 1, next: nil }
        arena {
            fix inner = new Node { v: 2, next: nil }
            var selected = outer
            selected = inner
            objection [selected was replaced by inner] {
                let p = selected { p#next = inner }
            }
            let p = inner.next { if p.v != 2 { return 1 } }
            else { return 2 }
        }
        if outer.v != 1 { return 3 }
    }
    return 0
}
"#;
    let rejected = source.replace("objection [selected was replaced by inner]", "");
    assert!(
        suz::compile(rejected.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("[arena.store]")
    );
    let output = compile_and_run_c23(source, "objection.suz");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        !suz::compile(source.as_bytes())
            .unwrap()
            .contains("selected was replaced")
    );
}

#[test]
fn objection_never_removes_proven_lifetime_violations() {
    for operation in [
        "saved = inner",
        "fix copy = inner\nsaved = copy",
        "fix maybe: mut? Node = inner\nlet p = maybe { saved = p }",
        "saved = new Node { next: nil }",
        "saved = ifexpr true { inner } else { new Node { next: nil } }",
        "outer#next = inner",
    ] {
        let source = format!(
            "struct Node {{ next: mut? Node }}\nfn main() -> u8 {{ arena {{ var saved: mut? Node = nil\nfix outer = new Node {{ next: nil }}\narena {{ fix inner = new Node {{ next: nil }}\nobjection [claimed safety] {{ {operation} }}\n}}\n}}\nreturn 0 }}"
        );
        // savedを使わない例でもvarの再代入規則を満たす。
        let source = source.replace(
            "var saved: mut? Node = nil",
            "var saved: mut? Node = nil\nsaved = nil",
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[arena.store]"),
            "{operation}: {error}"
        );
        assert!(
            error.message().contains("proven lifetime violation"),
            "{operation}: {error}"
        );
        assert_eq!(error.related.len(), 2);
    }
    let source = b"fn bad() -> mut? i32 { arena { objection [claim] { return new (1 as i32) } } }\nfn main() -> u8 { return 0 }";
    assert!(
        suz::compile(source)
            .unwrap_err()
            .message()
            .contains("proven lifetime violation")
    );
}

#[test]
fn a_nilable_source_is_not_a_proven_violation() {
    let source = r#"
struct Node { next: mut? Node }
fn main() -> u8 {
    arena {
        var saved: mut? Node = nil
        arena {
            fix inner = new Node { next: nil }
            fix maybe: mut? Node = ifexpr true { nil } else { inner }
            objection [the true branch yields nil] { saved = maybe }
        }
    }
    return 0
}
"#;
    assert_eq!(
        compile_and_run_c23(source, "nilable.suz").status.code(),
        Some(0)
    );
    let source = source.replace("objection [the true branch yields nil]", "");
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("[arena.store]"), "{error}");
    assert!(!error.message().contains("proven"), "{error}");
}

#[test]
fn objection_effects_remain_visible_and_permission_ends_at_the_block() {
    let source = r#"struct Node { next: mut? Node }
fn main() -> u8 {
    arena {
        var saved: mut? Node = nil
        fix outer = new Node { next: nil }
        arena {
            fix inner = new Node { next: nil }
            var selected = outer
            selected = inner
            objection [selected is inner] {
                objection [same guarantee] {
                    let p = selected { p#next = inner }
                }
            }
            saved = outer.next
        }
    }
    return 0
}"#;
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("[arena.store]"), "{error}");
    assert_eq!(error.line, 15, "{error}");
}

#[test]
fn objection_does_not_override_explicit_contracts_or_callee_checks() {
    for (source, rule) in [
        (
            "fn main() -> u8 { objection [claim] { fix p: mut? i32 = nil\nfix n = .p }\nreturn 0 }",
            "arena.nil-deref",
        ),
        (
            "fn bad(flag: bool) -> mut? i32 { arena { fix p: mut? i32 = ifexpr flag { new (1 as i32) } else { nil }\nreturn p } }\nfn main() -> u8 { arena { objection [caller guarantee] { bad(false) } }\nreturn 0 }",
            "arena.store",
        ),
        (
            "fnshape Act() -> void\nfn run(f: fn Act) -> void { f() }\nfn main() -> u8 { objection [caller guarantee] { run(fn []() -> void { arena { var saved: mut? i32 = nil\narena { fix p: mut? i32 = ifexpr true { nil } else { new (1 as i32) }\nsaved = p } } }) }\nreturn 0 }",
            "arena.store",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains(&format!("[{rule}]")), "{error}");
    }
    let source = "fn main() -> u8 { objection [claim] { fix n: bool = 1 }\nreturn 0 }";
    assert!(suz::compile(source.as_bytes()).is_err());
}

#[test]
fn objection_requires_a_literal_reason_and_does_not_generate_runtime_text() {
    for reason in [
        "",
        "[]",
        "[   ]",
        "[　]",
        "name",
        "\"template\"",
        "\"old syntax\"",
        "[line\nbreak]",
        "[tab\t]",
        "[unclosed",
        "[ends]extra]",
    ] {
        let source = format!("fn main() -> u8 {{ objection {reason} {{ return 0 }} }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{reason}");
    }
    let annotation = r#"objection [日本語 \n path and "quote" ${not_an_expression} // { `]"#;
    let source = format!("fn main() -> u8 {{ {annotation} {{ return 0 }} }}");
    let plain = source.replace(annotation, &" ".repeat(annotation.chars().count()));
    assert_eq!(
        suz::compile(source.as_bytes()).unwrap(),
        suz::compile(plain.as_bytes()).unwrap()
    );
}

#[test]
fn objection_reason_inside_interpolation_is_not_template_syntax() {
    let source = r#"fnshape Number() -> u8
fn value(f: fn Number) -> u8 { return f() }
fn main() -> u8 {
    fix message: symbol = "${value(fn []() -> u8 { objection [} ` ${ and { are annotation text] { return 42 } })}"
    log info "${message}"
    return 0
}"#;
    assert_eq!(
        compile_and_run_c23(source, "interpolated-objection.suz")
            .status
            .code(),
        Some(0)
    );
}
