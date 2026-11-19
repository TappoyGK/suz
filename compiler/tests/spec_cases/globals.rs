use crate::test_util::TemporaryDirectory;
use std::{fs, process::Command};

#[test]
fn top_level_var_is_rejected_even_with_allow_or_assignments() {
    for declaration in [
        "var N = 1",
        "var N: i32 = 1",
        "var N = 1 allow [diagnostic.global-var: process state]",
        "var ptr p: ptr? i32 = null",
    ] {
        let source = format!(
            "{declaration}\nfn unused() -> void {{ N = 2 }}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "decl.top-level", "{source}: {error}");
        assert_eq!((error.line, error.column), (1, 1));
    }
}

#[test]
fn top_level_fix_keeps_constant_rules_and_rejects_removed_allow_rule() {
    for (declaration, rule) in [
        ("const N = 1", "decl.top-level"),
        ("fix bad = 1", "name.top-level-binding"),
        ("fix N = 1\nfix N = 2", "decl.duplicate"),
        ("fix N = N", "const.initializer"),
        (
            "fix N = get()\nfn get() -> i32 { return 1 }",
            "const.expression",
        ),
        ("fix N = 1\nfn bad() -> void { N = 2 }", "const.decl"),
        (
            "fix N = 1 allow [diagnostic.global-var: removed rule]",
            "allow.rule-check",
        ),
    ] {
        let source = format!("{declaration}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{source}: {error}");
    }
    // constは予約語に戻さず、前方参照を含む定数は維持する。
    suz::compile(
        b"fix N: u8 = M + 1\nfix M: u8 = 1\nfn main() -> u8 { fix const: u8 = N\nreturn const }",
    )
    .unwrap();
}

#[test]
fn removed_global_policy_option_is_rejected_by_all_source_commands() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("valid.suz");
    fs::write(&source, "fn main() -> u8 { return 0 }").unwrap();
    for command in ["check", "build", "run", "emit-c"] {
        let output = Command::new(env!("CARGO_BIN_EXE_suz"))
            .args([command, source.to_str().unwrap(), "--deny-global-var"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{command}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("--deny-global-var"));
        let help = Command::new(env!("CARGO_BIN_EXE_suz"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(help.status.success());
        assert!(!String::from_utf8_lossy(&help.stdout).contains("--deny-global-var"));
    }
}
