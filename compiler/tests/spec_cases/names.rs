#[test]
fn declarations_reject_invalid_name_shapes_at_the_name() {
    let constants = ["count", "Count", "_COUNT", "COUNT_", "COUNT__MAX"];
    let values = [
        "Count",
        "COUNT",
        "someCount",
        "_count",
        "count_",
        "count__max",
    ];
    let types = ["item", "ITEM", "I", "I2", "IOError", "Item_Value", "_Item"];
    let variants = ["ready", "Ready_Now", "_Ready", "Ready_"];
    for (prefix, suffix, names, rule) in [
        (
            "const ",
            " = 1\nfn main() -> u8 { return 0 }",
            constants.as_slice(),
            "name.constant",
        ),
        (
            "fn ",
            "() -> void {}\nfn main() -> u8 { return 0 }",
            values.as_slice(),
            "name.value",
        ),
        (
            "fn f(",
            ": u8) -> void {}\nfn main() -> u8 { return 0 }",
            values.as_slice(),
            "name.value",
        ),
        (
            "struct Item { ",
            ": u8 }\nfn main() -> u8 { return 0 }",
            values.as_slice(),
            "name.value",
        ),
        (
            "fn main() -> u8 { fix ",
            " = 1\nreturn 0 }",
            values.as_slice(),
            "name.value",
        ),
        (
            "fn main() -> u8 { var ",
            " = 1\nreturn 0 }",
            values.as_slice(),
            "name.value",
        ),
        (
            "fn main() -> u8 { for ",
            " in 0..<1 {}\nreturn 0 }",
            values.as_slice(),
            "name.value",
        ),
        (
            "struct ",
            " { value: u8 }\nfn main() -> u8 { return 0 }",
            types.as_slice(),
            "name.type",
        ),
        (
            "enum ",
            ": u8 { Ready }\nfn main() -> u8 { return 0 }",
            types.as_slice(),
            "name.type",
        ),
        (
            "enum State: u8 { ",
            " }\nfn main() -> u8 { return 0 }",
            variants.as_slice(),
            "name.variant",
        ),
        (
            "fn main() -> u8 { fix result = expr { fix ",
            " = 0\n0 }\nreturn 0 }",
            values.as_slice(),
            "name.value",
        ),
    ] {
        for name in names {
            let source = format!("{prefix}{name}{suffix}");
            let error = suz::compile(source.as_bytes()).unwrap_err();
            assert_eq!(
                (error.line, error.column),
                (1, prefix.len() + 1),
                "{source}: {error}"
            );
            assert!(
                error.message().contains(&format!("[{rule}]")),
                "{source}: {error}"
            );
        }
    }
}

#[test]
fn valid_name_boundaries_and_exact_references_are_preserved() {
    for source in [
        "const N: u8 = 1\nconst N_2 = N\nfn main() -> u8 { return N_2 }",
        "struct Vec2D { x2: u8 }\nfn main() -> u8 { fix v = Vec2D { x2: 0 }\nreturn v.x2 }",
        "enum Key: u8 { A, F1, ESC, PageUp }\nfn main() -> u8 { match Key.ESC { Key.A => {}\nKey.F1 => {}\nKey.ESC => {}\nKey.PageUp => {} }\nreturn 0 }",
        "fn main() -> u8 { fix value: u8 = matchexpr 1 { _ => 0 }\nreturn value }",
    ] {
        suz::compile(source.as_bytes()).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
    for (source, rule) in [
        (
            "const COUNT: u8 = 0\nfn main() -> u8 { return count }",
            "decl.local-scope",
        ),
        (
            "struct Item { value: u8 }\nfn main() -> u8 { fix item = Item { value: 0 }\nreturn item.Value }",
            "struct.access",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.message().contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
}
