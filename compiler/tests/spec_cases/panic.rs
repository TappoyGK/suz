use crate::test_util::{TemporaryDirectory, compile_and_run_c23};
use std::{fs, process::Command};

#[test]
fn panic_reads_values_and_preserves_bytes() {
    let source = "fn main() -> u8 {\n    fix number = 7\n    fix flag = true\n    panic `日本語 ${number} ${flag}\\0終\\n次`\n}";
    let output = compile_and_run_c23(source, "panic.suz");
    assert_eq!(output.status.code(), Some(101));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        "panic panic.suz:4:5: 日本語 7 true\0終\n次\n".as_bytes()
    );
    let error = suz::compile(b"fn main() -> u8 { panic `` }").unwrap_err();
    assert_eq!(
        error.to_string(),
        "1:25: panic requires a non-empty template [panic.stmt]"
    );
}

#[test]
fn panic_flow_is_local_to_each_function_body() {
    let source = r#"
fnshape Once() -> i32
fn run(f: fn Once) -> i32 { return f() }
fn choose(n: u8) -> i32 {
    if n == 0 { return 3 } else { panic `bad` }
}
fn classify(n: u8) -> i32 {
    match n {
        0 => { return choose(n) }
        _ => { panic `bad` }
    }
}
fn generic(n: int) -> int { panic `${n}` }
fn main() -> u8 {
    if classify(0) != 3 { return 1 }
    return run(fn []() -> i32 { panic `closure` }) as u8 or 0
}
"#;
    let output = compile_and_run_c23(source, "flow.suz");
    assert_eq!(output.status.code(), Some(101));
    assert!(String::from_utf8_lossy(&output.stderr).ends_with(": closure\n"));
    for (source, rule) in [
        (
            "fn main() -> u8 { if false { panic `bad` } }",
            "fn.fallthrough",
        ),
        (
            "fnshape Once() -> u8\nfn run(f: fn Once) -> u8 { return f() }\nfn main() -> u8 { run(fn []() -> u8 { panic `inner` }) }",
            "fn.value-return-required",
        ),
        (
            "fn main() -> u8 { panic `stop`\nfix n: bool = 1 }",
            "type.expected",
        ),
        ("fn main() -> u8 { panic(`bad`) }", "panic.stmt"),
        ("fn main() -> u8 { panic 1 }", "panic.stmt"),
        ("fn main() -> u8 { fix panic = 1\nreturn 0 }", "src.ident"),
        ("fn main() -> u8 { log panic `bad`\nreturn 0 }", "log.level"),
        (
            "fn main() -> u8 { log info `a` panic `b` }",
            "src.statement-end",
        ),
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains(&format!("[{rule}]")),
            "{source}: {error}"
        );
    }
    for source in [
        "fn main() -> u8 { return panic `bad` }",
        "fn main() -> u8 { fix n = panic `bad`\nreturn 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn template_depth_counts_template_and_interpolation() {
    let directory = TemporaryDirectory::new();
    // 関数本体1 + テンプレート1 + リテラル1 + 丸括弧の数。
    // 境界超過でプロセスが異常終了しないことも検査する。
    for (count, accepted) in [(253, true), (254, false), (10_000, false)] {
        let source = format!(
            "fn main() -> u8 {{ fix message = `${{{}1{}}}`\nlog info `${{message}}`\nreturn 0 }}",
            "(".repeat(count),
            ")".repeat(count)
        );
        fs::write(directory.0.join("depth.suz"), source).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_suz"))
            .current_dir(&directory.0)
            .args(["check", "depth.suz"])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(if accepted { 0 } else { 1 }),
            "{output:?}"
        );
        if !accepted {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("[src.nesting-limit]"),
                "{output:?}"
            );
        }
    }
}
