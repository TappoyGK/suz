use crate::test_util::{TemporaryDirectory, build_and_run_c};
use std::fs;

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn compiled_main_returns_its_value() {
    let directory = TemporaryDirectory::new();
    for value in [0, 7, 101, 255] {
        let source = directory.0.join(format!("return-{value}.suz"));
        let c_file = directory.0.join(format!("return-{value}.c"));
        let executable = directory.0.join(format!("return-{value}"));
        fs::write(&source, format!("fn main() -> u8 {{ return {value} }}")).unwrap();
        fs::write(&c_file, suz::compile(&fs::read(&source).unwrap()).unwrap()).unwrap();
        let result = build_and_run_c(&c_file, &executable);
        assert_eq!(result.status.code(), Some(value));
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
fn check_reports_source_errors() {
    let directory = TemporaryDirectory::new();
    for (name, source, status, reason) in [
        (
            "invalid",
            "fn main() -> u8 { return 256 }",
            1,
            "[lit.range]",
        ),
        (
            "invalid_log",
            "fn main() -> u8 { log info `${noop()}`\nreturn 0 }\nfn noop() -> void {}",
            1,
            "[text.interpolation]",
        ),
    ] {
        let input = directory.0.join(format!("{name}.suz"));
        fs::write(&input, source).unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_suz"))
            .arg("check")
            .arg(&input)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(status));
        assert!(result.stdout.is_empty());
        let diagnostic = String::from_utf8(result.stderr).unwrap();
        assert!(diagnostic.contains(&format!("{}.suz:1:", name)));
        assert!(diagnostic.contains(reason), "{diagnostic}");
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn generated_bindings_handle_c_names_and_large_integer_literals() {
    let source = r#"fn main() -> u8 {
    fix minimum: i64 = -9223372036854775808
    fix maximum: u64 = 18446744073709551615
    fix signed_maximum: i64 = 9223372036854775807
    fix wide: u32 = 4294967295
    fix negative: i32 = -2147483648
    fix int: u8 = 7
    fix suz_main = (int)
    fix suz_local_0 = suz_main
    return suz_local_0
}
"#;
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("bindings.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("bindings"));
    assert_eq!(result.status.code(), Some(7));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn unary_literal_diagnostics_follow_the_selected_mode() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("negative.suz");
    // Both negations are diagnosed even though the second follows a return.
    fs::write(
        &source,
        "fn main() -> u8 {\nfix small: i8 = -128\nreturn 0\nfix other = -1\n}",
    )
    .unwrap();
    for (option, status, warnings) in [
        (None, 0, 0),
        (Some("--warn-unhandled-arithmetic"), 0, 2),
        (Some("--deny-unhandled-arithmetic"), 1, 0),
    ] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_suz"));
        command.arg("check").arg(&source);
        if let Some(option) = option {
            command.arg(option);
        }
        let result = command.output().unwrap();
        assert_eq!(result.status.code(), Some(status));
        assert!(result.stdout.is_empty());
        let diagnostic = String::from_utf8(result.stderr).unwrap();
        assert_eq!(diagnostic.matches(": warning: ").count(), warnings);
        if option.is_some() {
            assert!(diagnostic.contains("negative.suz:2:17:"), "{diagnostic}");
        }
        if warnings > 0 {
            assert!(diagnostic.contains("negative.suz:4:13:"), "{diagnostic}");
        }
        if status != 0 {
            assert!(diagnostic.contains("diagnostic.arithmetic"));
        }
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn assignments_update_values_without_aliasing_copies() {
    let directory = TemporaryDirectory::new();
    for (index, (body, expected)) in [
        ("var value: u8 = 1\nvalue = 255\nreturn value", 255),
        (
            "var value: u8 = 7\nfix copy = value\nvalue = 9\nreturn copy",
            7,
        ),
        (
            "fix initial: u8 = 7\nvar value = initial\nvalue = value\nreturn value",
            7,
        ),
        ("var value: u8 = 7\nreturn value\nvalue = 9", 7),
        (
            "var first: u8 = 1\nvar second: u8 = 2\nfirst = second\nsecond = 3\nreturn first",
            2,
        ),
        ("var value = 0\nvalue = -9223372036854775808\nreturn 0", 0),
    ]
    .into_iter()
    .enumerate()
    {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let c_file = directory.0.join(format!("assign-{index}.c"));
        fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
        let result = build_and_run_c(&c_file, &directory.0.join(format!("assign-{index}")));
        assert_eq!(result.status.code(), Some(expected));
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn nested_blocks_preserve_updates_scope_and_function_returns() {
    let directory = TemporaryDirectory::new();
    for (index, (body, expected)) in [
        ("var result: u8 = 0\n{ fix value: u8 = 3\nresult = value }\n{ fix value: u8 = 7\nresult = value }\nreturn result", 7),
        ("var result: u8 = 0\n{ { result = 7 } }\nreturn result", 7),
        ("{ { return 7 } }\nreturn 9", 7),
        ("{ { return 7 } }", 7),
        ("{}\nreturn 3", 3),
        ("{ fix value: u8 = 1 }\nfix value: u8 = 9\nreturn value", 9),
        ("var value: u8 = 1\n{ { value = value } }\nreturn value", 1),
        ("var result: u8 = 0\n{ var value: u8 = 1\n{ value = 7 }\nresult = value }\nreturn result", 7),
        ("var result: u8 = 0\n{ fix value: u8 = 7\nresult = value }\nfix value: u8 = 9\nreturn result", 7),
    ].into_iter().enumerate() {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let c_file = directory.0.join(format!("block-{index}.c"));
        fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
        let result = build_and_run_c(&c_file, &directory.0.join(format!("block-{index}")));
        assert_eq!(result.status.code(), Some(expected));
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn branches_select_one_body_and_preserve_bool_values() {
    let directory = TemporaryDirectory::new();
    for (index, (body, expected)) in [
        ("if true { return 1 } else { return 2 }", 1),
        ("if false { return 1 } elif true { return 2 } elif true { return 3 } else { return 4 }", 2),
        ("if false { return 1 } elif false { return 2 } else { return 3 }", 3),
        ("if false { return 1 }\nreturn 4", 4),
        ("if true { if false { return 1 } else { return 2 } } else { return 3 }", 2),
        ("var result: u8 = 0\nif false { fix value: u8 = 1\nresult = value } else { fix value: u8 = 2\nresult = value }\nreturn result", 2),
        ("var flag = true\nfix saved = flag\nflag = not flag\nif flag { return 1 } elif not saved { return 2 } elif not not saved { return 3 } else { return 4 }", 3),
        ("var result: u8 = 7\nif false { result = 1 }\nreturn result", 7),
        ("return 7\nif false { return 1 } else { return 2 }", 7),
    ].into_iter().enumerate() {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let c_file = directory.0.join(format!("branch-{index}.c"));
        fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
        let result = build_and_run_c(&c_file, &directory.0.join(format!("branch-{index}")));
        assert_eq!(result.status.code(), Some(expected), "{source}");
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn loops_reevaluate_conditions_and_target_the_innermost_loop() {
    let directory = TemporaryDirectory::new();
    for (index, (body, expected)) in [
        ("var result: u8 = 7\nwhile false { result = 1 }\nreturn result", 7),
        ("var active = true\nvar result: u8 = 0\nwhile active { active = false\nresult = 7 }\nreturn result", 7),
        ("var active = true\nwhile active { active = false\ncontinue\nreturn 1 }\nreturn 7", 7),
        ("var first = true\nloop { if first { first = false\ncontinue }\nbreak }\nreturn 7", 7),
        ("loop { loop { break }\nreturn 7 }", 7),
        ("while true { { break } }\nreturn 7", 7),
        ("while true { return 7 }", 7),
        ("var first = true\nloop { while first { first = false\ncontinue }\nbreak }\nreturn 7", 7),
        ("var result: u8 = 0\nloop { fix value: u8 = 7\nresult = value\nbreak }\nfix value: u8 = 9\nreturn result", 7),
    ].into_iter().enumerate() {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let c_file = directory.0.join(format!("loop-{index}.c"));
        fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
        let result = build_and_run_c(&c_file, &directory.0.join(format!("loop-{index}")));
        assert_eq!(result.status.code(), Some(expected), "{source}");
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn compound_assignments_use_the_current_value_and_preserve_integer_width() {
    let mut source = String::from("fn main() -> u8 {\n");
    for (ty, min, max) in crate::integer_bounds::INTEGER_BOUNDS {
        let doubled_max = if min < 0 { -2 } else { max - 1 };
        for (initial, operator, right, expected) in [
            (max, "+%=", "1", min),
            (min, "-%=", "1", max),
            (max, "*%=", "2", doubled_max),
            (6, "&=", "3", 2),
            (6, "|=", "3", 7),
            (6, "^=", "3", 5),
            (max, "+%=", "value", doubled_max),
            (6, "&=", "value ^ 3", 4),
        ] {
            source.push_str(&format!(
                "{{\nvar value: {ty} = {initial}\nfix original = value\n{{ value {operator} {right} }}\nif value != {expected} || original != {initial} {{ return 1 }}\n}}\n"
            ));
        }
    }
    source.push_str("var count: u8 = 0\nwhile count < 2 { count +%=\n1 }\nreturn count\n}\n");
    let directory = TemporaryDirectory::new();
    let c_file = directory.0.join("compound.c");
    fs::write(&c_file, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let result = build_and_run_c(&c_file, &directory.0.join("compound"));
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn compound_assignments_check_mutability_types_and_statement_position() {
    for (body, rule) in [
        ("fix value: u8 = 0\nvalue +%= 1\nreturn 0", "var.fix-assign"),
        ("value +%= 1\nreturn 0", "decl.local-scope"),
        ("var value: u8 = 0\nvalue +%= 256\nreturn 0", "lit.range"),
        (
            "var value: u8 = 0\nfix other: i8 = 1\nvalue ^= other\nreturn 0",
            "type.expected",
        ),
        (
            "var value = false\nvalue &= true\nreturn 0",
            "expr.operator-types",
        ),
        (
            "var value: u8 = 0\nreturn (value +%= 1)",
            "assign.statement",
        ),
        (
            "var value: u8 = 0\nvalue +%= (value ^= 1)\nreturn 0",
            "assign.statement",
        ),
    ] {
        let source = format!("fn main() -> u8 {{\n{body}\n}}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.kind, suz::ErrorKind::InvalidSource);
        assert!(error.message.contains(rule), "{error:?}");
    }
    // Wrapping and bitwise compound operators are not arithmetic diagnostic targets.
    let source = r#"fn main() -> u8 {
    var count: u8 = 255
    count +%= 1
    count -%= 1
    count *%= 2
    count &= 7
    count |= 1
    count ^= 2
    return count
}
"#;
    for mode in [
        suz::ArithmeticMode::Default,
        suz::ArithmeticMode::Warn,
        suz::ArithmeticMode::Deny,
    ] {
        assert!(
            suz::compile_with_mode(source.as_bytes(), mode)
                .unwrap()
                .warnings
                .is_empty()
        );
    }
}
