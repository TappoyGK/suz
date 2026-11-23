#[test]
fn hash_in_receiver_fields_reports_method_call_syntax() {
    for call in [
        "v#pos#shift(1)",
        "(v)#pos#shift(1)",
        "v#pos.read()",
        "v#inner.pos#shift(1)",
        "fix n = v#pos.read()",
    ] {
        let source = format!(
            "struct Pos {{ x: i32 }}\nstruct Ve {{ pos: Pos }}\nfn Pos#shift(d: i32) -> void {{ self#x += d }}\nfn Pos.read() -> i32 {{ return self.x }}\nfn main() -> u8 {{\nvar v = Ve {{ pos: Pos {{ x: 1 }} }}\n{call}\nreturn 0\n}}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "method.call", "{call}: {error}");
        assert!(
            error
                .to_string()
                .contains("use . to access receiver fields before calling a method"),
            "{error}"
        );
    }
    // 同じ記号の並びでも、代入先なら既存の代入規則で診断する。
    let error = suz::compile(
        b"struct Pos { x: i32 }\nstruct Ve { pos: Pos }\nfn main() -> u8 {\nvar v = Ve { pos: Pos { x: 1 } }\nv#pos#x = 2\nreturn 0\n}",
    )
    .unwrap_err();
    assert_eq!(error.rule(), "assign.target");
}

#[test]
fn methods_reject_invalid_receivers_names_and_writes() {
    for (declaration, body, rule) in [
        ("fn Counter.value() -> i32 { return 0 }", "", "method.name"),
        ("fn Counter.get() -> i32 { return 0 }", "", "method.name"),
        ("fn Counter#get() -> void {}", "", "method.name"),
        ("fn i32.get() -> i32 { return 0 }", "", "method.target-type"),
        (
            "fn Counter.bad() -> void { self#value = 1 }",
            "",
            "fn.parameter",
        ),
        (
            "",
            "fix c = Counter { value: 1 }\nc#add(1)",
            "method.write-target",
        ),
        ("", "(Counter { value: 1 })#add(1)", "method.write-target"),
        ("", "fix c = Counter { value: 1 }\nc.add(1)", "method.call"),
        ("", "var c = Counter { value: 1 }\nc#get()", "method.call"),
        (
            "",
            "fix c = Counter { value: 1 }\nc.unknown()",
            "method.call",
        ),
        ("", "fix n = self", "method.self-keyword"),
        (
            "",
            "arena { fix p: mut? Counter = new Counter { value: 1 }\np#add(1) }",
            "arena.null-deref",
        ),
        (
            "",
            "var c = Counter { value: 1 }\nc#value = expr { c#add(1)\n2 }",
            "assign.rhs-write",
        ),
        (
            "fn bad(c: Counter) -> void { c#add(1) }",
            "",
            "method.write-target",
        ),
    ] {
        let source = format!(
            "struct Counter {{ value: i32 }}\nfn Counter.get() -> i32 {{ return self.value }}\nfn Counter#add(n: i32) -> void {{ self#value += n }}\n{declaration}\nfn main() -> u8 {{ {body}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{declaration}\n{body}: {error}");
    }
}

#[test]
fn method_receivers_keep_lifetimes_capture_and_range_restrictions() {
    for (declarations, body, rule) in [
        ("fn Node#clear() -> void { self#next = null }", "", "accept"),
        (
            "fn Node#clear() -> void { self = Node { value: 0, next: null } }",
            "",
            "accept",
        ),
        (
            "fn Node.bad() -> mut? Node { return self.next }",
            "",
            "accept",
        ),
        (
            "fn Node.read() -> i32 { return invoke(fn []() -> i32 { return self.value }) }",
            "",
            "closure.scope",
        ),
        (
            "fn Node.read() -> i32 { return invoke(fn [var self]() -> i32 { return self.value }) }",
            "",
            "closure.capture-list",
        ),
        (
            "fn Node#bad() -> i32 { return invoke(fn [self]() -> i32 { self#value = 2\nreturn self.value }) }",
            "",
            "closure.capture-value",
        ),
        ("", "fix f = fn Node.count", "method.no-fn-value"),
        (
            "",
            "fix n = Node { value: 1, next: null }\nfix f = fn n.count",
            "method.no-fn-value",
        ),
        (
            "",
            "var n = Node { value: 1, next: null }\nfor i in 0..<n.count() {}\nn#bump()",
            "for.bound-reference",
        ),
        (
            "",
            "var n = Node { value: 1, next: null }\nfor i in 0..<n#bump() {}",
            "for.bound-reference",
        ),
        (
            "",
            "arena { fix outer = new Node { value: 1, next: null }\narena { outer#append() } }",
            "arena.store",
        ),
        ("fn Node .bad() -> void {}", "", "method.decl"),
    ] {
        let source = format!(
            "struct Node {{ value: i32, next: mut? Node }}\nfnshape Read() -> i32\nfn invoke(f: fn Read) -> i32 {{ return f() }}\nfn Node.count() -> i32 {{ return self.value }}\nfn Node#bump() -> i32 {{ self#value += 1\nreturn self.value }}\nfn Node#append() -> void {{ self#next = new Node {{ value: 2, next: null }} }}\n{declarations}\nfn main() -> u8 {{ {body}\nreturn 0 }}"
        );
        if rule == "accept" {
            suz::compile(source.as_bytes()).unwrap();
        } else {
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(error.rule(), rule, "{declarations}\n{body}: {error}");
        }
    }
}
