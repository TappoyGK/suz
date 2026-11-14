use crate::support::{self, Outcome, extract, runner};
use crate::test_util::TemporaryDirectory;
use std::{fs, path::Path};

const PANIC: &str = r#"### check-001 panic example

主根拠：`[rt.int-overflow]`。

**期待結果：**

| 設定 | 結果 | 終了コード | 整数演算の警告数 |
|---|---|---:|---:|
| 通常 | パニック | 101 | 0 |
| 警告 | パニック | 101 | 1 |
| 強制 | コンパイルエラー | — | 0 |

**パニック診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[rt.int-overflow]` | 3 | 22 |
| 警告 | `[rt.int-overflow]` | 3 | 22 |

**整数演算の警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 3 | 22 |

**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 強制 | `[diagnostic.arithmetic]` | 3 | 22 |

**ソース：**

```suz
fn main() -> u8 {
    fix top: u8 = 255
    fix result = top + 1
    return 0
}
```
"#;

#[test]
fn panic_tables_validate_modes_rules_and_exit_codes() {
    let cases = extract(PANIC).unwrap();
    let runs = &cases["check-001"].runs;
    assert_eq!(runs[0].result, Outcome::Panic);
    assert_eq!(
        runs[0].panic_diagnostic,
        Some(support::ExpectedDiagnostic {
            rule: "rt.int-overflow".into(),
            line: 3,
            column: 22,
        })
    );
    assert_eq!(runs[1].panic_diagnostic, runs[0].panic_diagnostic);
    assert!(runs[2].panic_diagnostic.is_none());
    for invalid in [
        PANIC.replace("| パニック | 101 |", "| パニック | 0 |"),
        PANIC.replace("| パニック | 101 |", "| 正常終了 | 101 |"),
        PANIC.replace(
            "| 通常 | `[rt.int-overflow]`",
            "| 強制 | `[rt.int-overflow]`",
        ),
        PANIC.replace(
            "| 警告 | `[rt.int-overflow]`",
            "| 通常 | `[rt.int-overflow]`",
        ),
        PANIC.replace("`[rt.int-overflow]`", "`[decl.shadowing]`"),
        format!(
            "{PANIC}\n**パニック診断：**\n\n| 設定 | 規則 | 行 | 列 |\n|---|---|---:|---:|\n| 通常 | `[rt.int-overflow]` | 3 | 22 |\n"
        ),
    ] {
        assert!(extract(&invalid).is_err(), "{invalid}");
    }
    let mut cases = extract(PANIC).unwrap();
    let run = &mut cases.get_mut("check-001").unwrap().runs[0];
    run.panic_diagnostic = None;
    let directory = TemporaryDirectory::new();
    let error = runner::compile(&directory.0, "check-001", 0, run, b"").unwrap_err();
    assert!(error.contains("missing panic diagnostic expectation"));
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
}

#[test]
fn panic_output_requires_the_exact_prefix_one_diagnostic_and_matching_location() {
    let mut cases = extract(PANIC).unwrap();
    let run = &mut cases.get_mut("check-001").unwrap().runs[0];
    let path = Path::new("dir:日本語/check.suz");
    run.stderr_prefix = Some("info before panic\n".into());
    let diagnostic =
        "panic dir:日本語/check.suz:3:22: integer addition overflow [rt.int-overflow]\n";
    let correct = format!("info before panic\n{diagnostic}");
    assert!(runner::check_output(run, Some(101), b"", correct.as_bytes(), path).is_ok());
    let reworded = correct.replace("integer addition overflow", "the result does not fit u8");
    assert!(runner::check_output(run, Some(101), b"", reworded.as_bytes(), path).is_ok());
    for wrong in [
        correct.replace(":3:", ":4:"),
        correct.replace(":22:", ":21:"),
        correct.replace("rt.int-overflow", "rt.div-zero"),
        correct.replace("check.suz", "other.suz"),
        correct.replace("panic dir", "dir"),
        correct.replace("integer addition overflow", ""),
        correct.replace("info before panic\n", ""),
        correct.replace("info before panic", "info different"),
        correct.trim_end_matches('\n').into(),
        format!("{correct}{diagnostic}"),
        format!("{correct}extra output\n"),
        "info before panic\n".into(),
        String::new(),
    ] {
        assert!(
            runner::check_output(run, Some(101), b"", wrong.as_bytes(), path).is_err(),
            "{wrong}"
        );
    }
    for code in [None, Some(0), Some(1)] {
        assert!(runner::check_output(run, code, b"", correct.as_bytes(), path).is_err());
    }
    assert!(runner::check_output(run, Some(101), b"extra", correct.as_bytes(), path).is_err());
    let mut malformed = correct.into_bytes();
    malformed.push(0xff);
    assert!(runner::check_output(run, Some(101), b"", &malformed, path).is_err());
}

#[test]
fn panic_execution_checks_all_modes_and_rejects_normal_return_101() {
    let cases = extract(PANIC).unwrap();
    let directory = TemporaryDirectory::new();
    support::write_sources(&cases, &directory.0).unwrap();
    for (index, run) in cases["check-001"].runs.iter().enumerate() {
        runner::execute(&directory.0, "check-001", index, run).unwrap();
    }
    fs::write(
        directory.0.join("check-001.suz"),
        "fn main() -> u8 { return 101 }\n",
    )
    .unwrap();
    let error =
        runner::execute(&directory.0, "check-001", 3, &cases["check-001"].runs[0]).unwrap_err();
    assert!(error.contains("expected panic"), "{error}");
}

#[test]
fn direct_compilation_rejects_wrong_diagnostics_and_warnings() {
    for mutation in 0..6 {
        let mut cases = extract(PANIC).unwrap();
        let case = cases.get_mut("check-001").unwrap();
        let run = match mutation {
            0 | 1 => &mut case.runs[2],
            _ => &mut case.runs[1],
        };
        match mutation {
            0 => run.diagnostic.as_mut().unwrap().line += 1,
            1 => run.diagnostic.as_mut().unwrap().rule = "lit.range".into(),
            2 => run.warnings[0].column += 1,
            3 => run.warnings[0].rule = "lit.range".into(),
            4 => {
                run.warnings.clear();
                run.arithmetic_warnings = 0;
            }
            5 => {
                run.warnings.push(support::ExpectedDiagnostic {
                    rule: "diagnostic.arithmetic".into(),
                    line: 3,
                    column: 22,
                });
                run.arithmetic_warnings = 2;
            }
            _ => unreachable!(),
        }
        let directory = TemporaryDirectory::new();
        assert!(runner::compile(&directory.0, "check-001", 0, run, &case.bytes).is_err());
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    }
}
