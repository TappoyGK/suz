use crate::support::{Mode, Outcome, extract, validate_inventory};

pub(super) const SMALL: &str = r#"### lex-001 example

主根拠：`[src.encoding]`。

自由な説明。

**ソース：**

```suz
fn main() -> u8 { return 0 }
```

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

"#;

const ONE_CASE_INVENTORY: &str = r#"## 目次と件数

| 分野 | ID接頭辞 | 対応する仕様 | 件数 |
|---|---|---|---:|
| 例 | `lex` | 任意 | 1 |
| 合計 | | | 1 |

### 結果別件数

| 結果 | 件数 |
|---|---:|
| 受理 | 1 |
| コンパイルエラー | 0 |
| 実行時パニック | 0 |
| 診断設定の比較 | 0 |
| 合計 | 1 |

"#;

#[test]
fn split_catalog_checks_cross_file_duplicates_and_missing_cases() {
    use crate::{support::catalog, test_util::TemporaryDirectory};
    use std::fs;

    let directory = TemporaryDirectory::new();
    fs::write(directory.0.join("spec-cases.md"), ONE_CASE_INVENTORY).unwrap();
    let cases = directory.0.join("spec-cases");
    fs::create_dir(&cases).unwrap();
    fs::write(cases.join("lex.md"), SMALL).unwrap();
    assert_eq!(catalog::load(&directory.0).unwrap().len(), 1);

    fs::write(cases.join("duplicate.md"), SMALL).unwrap();
    assert!(
        catalog::load(&directory.0)
            .unwrap_err()
            .contains("duplicate ID across files")
    );
    fs::remove_file(cases.join("duplicate.md")).unwrap();
    fs::remove_file(cases.join("lex.md")).unwrap();
    assert!(
        catalog::load(&directory.0)
            .unwrap_err()
            .contains("inventory and cases disagree")
    );
}

#[test]
fn malformed_markdown_is_rejected() {
    for (input, message) in [
        (format!("{SMALL}{SMALL}"), "duplicate ID"),
        (SMALL.replace("```suz", "```rust"), "expected suz fence"),
        (SMALL.replace("lex-001", "../bad"), "invalid case heading"),
        (SMALL.replacen("\n```\n", "\n", 1), "unclosed fence"),
        (
            SMALL.replace(
                "**ソース：**\n\n```suz\nfn main() -> u8 { return 0 }\n```",
                "",
            ),
            "missing source",
        ),
        (
            format!("{SMALL}\n**ソース：**\n\n```suz\nother\n```\n"),
            "duplicate source",
        ),
        (
            SMALL.replace("主根拠：`[src.encoding]`。", ""),
            "missing primary rule",
        ),
        (
            SMALL.replace("**ソース：**", "**ソースの誤記：**"),
            "unlabelled fence",
        ),
    ] {
        let error = extract(&input).unwrap_err();
        assert!(error.contains(message), "expected {message}, got {error}");
    }
}

#[test]
fn malformed_hex_is_rejected() {
    for digits in ["f", "ff gg", "é0", ""] {
        let input = SMALL.replace(
            "**ソース：**\n\n```suz\nfn main() -> u8 { return 0 }\n```",
            &format!("**保存用のバイト列（16進数）：**\n\n```text\n{digits}\n```"),
        );
        assert!(extract(&input).unwrap_err().contains("invalid hex"));
    }
}

#[test]
fn headings_and_labels_inside_source_are_not_metadata() {
    let input = SMALL.replace(
        "fn main() -> u8 { return 0 }",
        "### not-a-case\n主根拠：`[not.metadata]`。\n**期待結果：**",
    );
    let s = extract(&input).unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s["lex-001"].rule, "src.encoding");
    assert!(s["lex-001"].bytes.starts_with(b"### not-a-case\n"));
}

#[test]
fn malformed_expectation_tables_are_rejected() {
    let row = "| 通常 | 正常終了 | 0 | 0 |";
    for (input, message) in [
        (
            SMALL.replace("**期待結果：**", "自由な文章"),
            "missing expectation table",
        ),
        (
            SMALL.replace("| 設定 |", "| モード |"),
            "invalid table header",
        ),
        (
            SMALL.replace("|---|---|---:|---:|", "|---|---|oops|---|"),
            "invalid table separator",
        ),
        (SMALL.replace(row, ""), "empty expectation table"),
        (
            SMALL.replace(row, &format!("{row}\n{row}")),
            "duplicate mode",
        ),
        (
            SMALL.replace(row, "| 不明 | 正常終了 | 0 | 0 |"),
            "unknown mode",
        ),
        (
            SMALL.replace(row, "| 通常 | 不明 | 0 | 0 |"),
            "unknown outcome",
        ),
        (
            SMALL.replace(row, "| 通常 | 正常終了 | 256 | 0 |"),
            "integer out of range",
        ),
        (
            SMALL.replace(row, "| 通常 | 正常終了 | — | 0 |"),
            "invalid exit code",
        ),
        (
            SMALL.replace(row, "| 通常 | コンパイルエラー | 0 | 0 |"),
            "invalid exit code",
        ),
        (
            SMALL.replace(row, "| 通常 | 正常終了 | 0 | -1 |"),
            "invalid unsigned integer",
        ),
        (
            SMALL.replace(row, "| 通常 | 正常終了 | 0 | 0 | extra |"),
            "4 table columns",
        ),
        (
            format!(
                "{SMALL}\n**期待結果：**\n\n| 設定 | 結果 | 終了コード | 警告数 |\n|---|---|---|---|\n{row}\n"
            ),
            "duplicate expectation table",
        ),
    ] {
        let error = extract(&input).unwrap_err();
        assert!(error.contains(message), "expected {message}, got {error}");
    }
}

#[test]
fn labelled_output_is_exact_and_prose_is_not_parsed() {
    let input = format!(
        "{SMALL}\n**標準出力：**\n\n```text\nhello\n\n```\n\n**標準エラー：**\n\n```text\n### not-a-case\n**期待結果：**\n```\n"
    );
    let s = extract(&input).unwrap();
    let run = &s["lex-001"].runs[0];
    assert_eq!(run.stdout.as_deref(), Some("hello\n\n"));
    assert_eq!(
        run.stderr.as_deref(),
        Some("### not-a-case\n**期待結果：**\n")
    );
    let s = extract(&SMALL.replace(
        "自由な説明。",
        "**期待：** 実行時パニック。終了コード 101。",
    ))
    .unwrap();
    assert_eq!(s["lex-001"].runs[0].result, Outcome::Exit);
    assert_eq!(s["lex-001"].runs[0].code, Some(0));
}

#[test]
fn malformed_output_blocks_are_rejected() {
    for (suffix, message) in [
        (
            "\n**標準エラー：**\nintervening prose\n```text\nx\n```\n",
            "expected text fence",
        ),
        (
            "\n**標準エラー：**\n```text\nx\n```\n**標準エラー：**\n```text\ny\n```\n",
            "duplicate stderr",
        ),
        (
            "\n**標準エラーの誤記：**\n```text\nx\n```\n",
            "unlabelled fence",
        ),
        (
            "\n**パニック前の標準エラー：**\n```text\nx\n```\n",
            "does not apply",
        ),
    ] {
        assert!(
            extract(&format!("{SMALL}{suffix}"))
                .unwrap_err()
                .contains(message)
        );
    }
}

#[test]
fn cases_and_modes_come_from_the_document() {
    // A new category and different case counts need no parser changes.
    let row = "| 通常 | 正常終了 | 0 | 0 |";
    let second = SMALL.replace("lex-001", "example-042").replace(
        row,
        "| 警告 | 正常終了 | 7 | 2 |\n| 強制 | コンパイルエラー | — | 0 |",
    );
    let s = extract(&format!("{SMALL}\n{second}")).unwrap();
    assert_eq!(s.len(), 2);
    let runs = &s["example-042"].runs;
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].mode, Mode::Warn);
    assert_eq!(runs[0].code, Some(7));
    assert_eq!(runs[0].warning_count, 2);
    assert_eq!(runs[1].mode, Mode::Deny);
    assert_eq!(runs[1].result, Outcome::CompileError);
    assert_eq!(runs[1].code, None);
    assert!(runs[1].stdout.is_none());
    // The group name does not select modes.
    assert_eq!(
        extract(&SMALL.replace("lex-001", "diag-001")).unwrap()["diag-001"]
            .runs
            .len(),
        1
    );
}

#[test]
fn inventory_is_checked_against_the_parsed_cases() {
    let doc = format!("{ONE_CASE_INVENTORY}{SMALL}");
    let cases = extract(&doc).unwrap();
    validate_inventory(&doc, &cases).unwrap();
    for bad in [
        doc.replace("任意 | 1", "任意 | 2"),
        doc.replace("合計 | | | 1", "合計 | | | 2")
            .replace("| 受理 | 1 |", "| 受理 | 2 |")
            .replace("| 合計 | 1 |", "| 合計 | 2 |"),
        doc.replace("`lex`", "`other`"),
        doc.replace("| 受理 | 1 |", "| 受理 | 0 |")
            .replace("| コンパイルエラー | 0 |", "| コンパイルエラー | 1 |"),
        doc.replace("| 合計 | 1 |", "| 合計 | 2 |"),
        doc.replace("| 受理 | 1 |", "| 受理 | 1 |\n| 受理 | 1 |"),
        doc.replace("### 結果別件数", "### その他"),
        doc.replace("| 実行時パニック | 0 |\n", ""),
        doc.replace("| 受理 | 1 |", "| 受理 |"),
    ] {
        assert!(validate_inventory(&bad, &cases).is_err());
    }
    let extended = format!(
        "{}{}",
        doc.replace("任意 | 1", "任意 | 2")
            .replace("合計 | | | 1", "合計 | | | 2")
            .replace("| 受理 | 1 |", "| 受理 | 2 |")
            .replace("| 合計 | 1 |", "| 合計 | 2 |"),
        SMALL.replace("lex-001", "lex-002")
    );
    validate_inventory(&extended, &extract(&extended).unwrap()).unwrap();
    // Two settings count as one comparison case even when both exit normally.
    let compared = format!(
        "{}{}",
        ONE_CASE_INVENTORY
            .replace("| 受理 | 1 |", "| 受理 | 0 |")
            .replace("| 診断設定の比較 | 0 |", "| 診断設定の比較 | 1 |"),
        SMALL.replace(
            "| 通常 | 正常終了 | 0 | 0 |",
            "| 通常 | 正常終了 | 0 | 0 |\n| 警告 | 正常終了 | 0 | 0 |"
        )
    );
    validate_inventory(&compared, &extract(&compared).unwrap()).unwrap();
}

#[test]
fn input_diagrams_are_compared_with_bytes_for_any_id() {
    for (diagram, hex, expected) in [
        ("a<CR><LF><FF><EOF>", "61 0d 0a ff", b"a\r\n\xff".as_slice()),
        ("z<EOF>", "7a", b"z".as_slice()),
        ("<CR><EOF>", "0d", b"\r".as_slice()),
        ("あ<LF>\nb<EOF>", "e3 81 82 0a 62", "あ\nb".as_bytes()),
    ] {
        let input = SMALL.replace("**ソース：**\n\n```suz\nfn main() -> u8 { return 0 }\n```", &format!("**入力図：**\n\n```text\n{diagram}\n```\n\n**保存用のバイト列（16進数）：**\n\n```text\n{hex}\n```"));
        assert_eq!(extract(&input).unwrap()["lex-001"].bytes, expected);
        let bad = input.replace(&format!("\n{hex}\n"), "\n00\n");
        assert!(extract(&bad).unwrap_err().contains("disagree"));
        assert!(
            extract(&input.replace("<EOF>", ""))
                .unwrap_err()
                .contains("must end")
        );
        assert!(
            extract(&input.replace("<EOF>", "<EOF><EOF>"))
                .unwrap_err()
                .contains("early")
        );
    }
}

#[test]
fn panic_expectation_is_read_from_the_table() {
    let input = SMALL.replace(
        "| 通常 | 正常終了 | 0 | 0 |",
        "| 通常 | パニック | 101 | 0 |",
    );
    let input =
        format!("{input}\n**パニック前の標準エラー：**\n\n```text\nlogged before panic\n```\n");
    let cases = extract(&input).unwrap();
    let run = &cases["lex-001"].runs[0];
    assert_eq!(run.result, Outcome::Panic);
    assert_eq!(run.code, Some(101));
    assert_eq!(run.stderr_prefix.as_deref(), Some("logged before panic\n"));
    assert!(run.stderr.is_none());
    assert!(run.panic_diagnostic.is_none());
}

#[test]
fn explanatory_sections_do_not_become_cases() {
    let document = r#"# Examples

### Format guide

This fenced example is documentation, not a test case.

````markdown
### fake-001 example
主根拠：`[fake.rule]`。
```suz
fn main() -> u8 { return 7 }
```
````

~~~text
### fake-002 example
主根拠：`[fake.rule]`。
~~~

## Cases

### lex-001 example ###

主根拠：  `[src.encoding]`

#### Explanation within the case

The following program returns zero.

**期待結果：**

| 設定 | 結果 | 終了コード | 警告数 |
|---|---|---:|---:|
| 通常 | 正常終了 | 0 | 0 |

**ソース：**

```suz
fn main() -> u8 { return 0 }
```

### Further reading

An unlabelled code block outside a case is ordinary documentation.

```text
This must not be read as case data.
```

## Appendix

**期待結果：**

This is not an expectation table.
"#;
    let cases = extract(document).unwrap();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases["lex-001"].rule, "src.encoding");
    assert_eq!(cases["lex-001"].bytes, b"fn main() -> u8 { return 0 }\n");

    // Every heading at the case level or above closes the case.
    for heading in ["# Appendix", "## Appendix", "### Further reading"] {
        let input = document.replace("### Further reading", heading);
        assert_eq!(extract(&input).unwrap().len(), 1);
    }
    for (from, to, reason) in [
        (
            "### lex-001 example ###",
            "### ../bad example",
            "invalid case heading",
        ),
        (
            "### lex-001 example ###",
            "### lex-001",
            "invalid case heading",
        ),
        ("主根拠：  `[src.encoding]`", "", "missing primary rule"),
        (
            "**ソース：**",
            "## Appendix\n\n**ソース：**",
            "missing source",
        ),
    ] {
        let error = extract(&document.replace(from, to)).unwrap_err();
        assert!(error.contains(reason), "{error}");
    }
}
