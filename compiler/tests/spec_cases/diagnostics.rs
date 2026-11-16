use super::format::SMALL;
use crate::support::{self, Mode, extract};

#[test]
fn diagnostic_expectations_are_strict() {
    let negative = SMALL.replace("正常終了 | 0", "コンパイルエラー | —");
    let table = r#"
**コンパイル診断：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 通常 | `[decl.shadowing]` | 4 | 13 |
"#;
    let input = format!("{negative}{table}");
    let cases = extract(&input).unwrap();
    let expected = cases["lex-001"].runs[0].diagnostic.as_ref().unwrap();
    let path = std::path::Path::new("example.suz");
    let correct = "example.suz:4:13: name already exists [decl.shadowing]\n";
    assert!(expected.matches(correct, path));
    for wrong in [
        correct.replace(":4:", ":5:"),
        correct.replace(":13:", ":12:"),
        correct.replace("decl.shadowing", "decl.duplicate"),
        correct.replace("example", "other"),
        format!("{correct}{correct}"),
        correct.replace("name already", "warn: name already"),
    ] {
        assert!(!expected.matches(&wrong, path), "{wrong}");
    }
    for invalid in [
        format!("{SMALL}{table}"),
        format!("{input}{table}"),
        input.replace("| 4 | 13 |", "| 0 | 13 |"),
        input.replace("| 4 | 13 |", "| 4 | 0 |"),
        input.replace("`[decl.shadowing]`", "decl.shadowing"),
        input.replace("| 通常 | `[", "| 強制 | `["),
        input.replace("| 規則 |", "| reason |"),
        input.replace("| 通常 | `[decl.shadowing]` | 4 | 13 |", ""),
        input.replace(
            "| 通常 | `[decl.shadowing]` | 4 | 13 |",
            "| 通常 | `[decl.shadowing]` | 4 | 13 |\n| 通常 | `[decl.shadowing]` | 4 | 13 |",
        ),
    ] {
        assert!(extract(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn warning_table_is_parsed_and_validated() {
    let input = r#"### diag-001 warning example

主根拠：`[diagnostic.arithmetic]`。

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 警告 | 正常終了 | 0 | 2 |

**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 2 | 21 |
| 警告 | `[diagnostic.arithmetic]` | 4 | 17 |

**ソース：**

```suz
fn main() -> u8 {
    fix small: i8 = -128
    return 0
    fix other = -1
}
```
"#;
    let cases = extract(input).unwrap();
    let runs = &cases["diag-001"].runs;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].mode, Mode::Warn);
    assert_eq!(runs[0].warning_count, 2);
    assert_eq!(
        runs[0].warnings,
        [
            support::ExpectedDiagnostic {
                rule: "diagnostic.arithmetic".into(),
                line: 2,
                column: 21,
            },
            support::ExpectedDiagnostic {
                rule: "diagnostic.arithmetic".into(),
                line: 4,
                column: 17,
            },
        ]
    );

    // Markdownの検証契約に違反するケースを、一項目ずつ変えて試す。
    for invalid in [
        input.replace(
            "**ソース：**",
            r#"**警告：**

| 設定 | 規則 | 行 | 列 |
|---|---|---:|---:|
| 警告 | `[diagnostic.arithmetic]` | 2 | 21 |

**ソース：**"#,
        ),
        input.replace("| 0 | 2 |", "| 0 | 1 |"),
        input.replace("| 0 | 2 |", "| 0 | 0 |"),
        input.replace("| 4 | 17 |", "| 2 | 21 |"),
        input.replace("| 警告 | `[", "| 強制 | `["),
        input.replace("diagnostic.arithmetic", "decl.shadowing"),
    ] {
        assert!(extract(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn warning_diagnostics_match_locations_and_counts() {
    let warnings = &[
        support::ExpectedDiagnostic {
            rule: "diagnostic.arithmetic".into(),
            line: 2,
            column: 21,
        },
        support::ExpectedDiagnostic {
            rule: "diagnostic.arithmetic".into(),
            line: 4,
            column: 17,
        },
    ];
    let path = std::path::Path::new("test.suz");
    let first = "test.suz:2:21: warn: integer negation [diagnostic.arithmetic]\n";
    let second = "test.suz:4:17: warn: integer negation [diagnostic.arithmetic]\n";
    assert!(support::warnings_match(warnings, &[first, second], path));
    assert!(support::warnings_match(warnings, &[second, first], path));
    for wrong in [
        vec![first],
        vec![first, first],
        vec![first, second, first],
        vec![
            first,
            "test.suz:4:18: warn: integer negation [diagnostic.arithmetic]\n",
        ],
        vec![
            first,
            "test.suz:4:17: integer negation [diagnostic.arithmetic]\n",
        ],
        vec![
            first,
            "test.suz:4:17: warn: integer negation [other.rule]\n",
        ],
        vec![
            first,
            "other.suz:4:17: warn: integer negation [diagnostic.arithmetic]\n",
        ],
    ] {
        assert!(
            !support::warnings_match(warnings, &wrong, path),
            "{wrong:?}"
        );
    }
}
