use crate::support::{Mode, Outcome, extract, validate_inventory};

pub(super) const SMALL: &str = r#"### lex-001 example
- 主根拠：`[src.encoding]`
- 結果：正常終了

自由な説明。

```suz
fn main() -> u8 { return 0 }
```

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
        (SMALL.replace("```suz", "```rust"), "unlabelled fence"),
        (SMALL.replace("lex-001", "../bad"), "invalid case heading"),
        (SMALL.replacen("\n```\n", "\n", 1), "unclosed fence"),
        (
            SMALL.replace("```suz\nfn main() -> u8 { return 0 }\n```", ""),
            "missing source",
        ),
        (format!("{SMALL}\n```suz\nother\n```\n"), "duplicate source"),
        (
            SMALL.replace("- 主根拠：`[src.encoding]`\n", ""),
            "missing primary rule",
        ),
        (
            SMALL.replace("```suz", "**ソースの誤記：**\n\n```suz"),
            "unlabelled fence",
        ),
    ] {
        let error = extract(&input).unwrap_err();
        assert!(error.contains(message), "expected {message}, got {error}");
    }
}

#[test]
fn legacy_table_format_is_rejected() {
    for label in [
        "**期待結果：**",
        "**コンパイル診断：**",
        "**パニック診断：**",
        "**警告：**",
    ] {
        let legacy = format!(
            "### lex-001 example\n- 主根拠：`[src.encoding]`\n- 結果：正常終了\n\n```suz\nfn main() -> u8 {{ return 0 }}\n```\n\n{label}\n"
        );
        assert!(
            extract(&legacy)
                .unwrap_err()
                .contains("legacy table format is no longer supported"),
            "expected legacy error for {label}"
        );
    }
}

#[test]
fn malformed_hex_is_rejected() {
    for digits in ["f", "ff gg", "é0", ""] {
        let input = SMALL.replace(
            "```suz\nfn main() -> u8 { return 0 }\n```",
            &format!("**保存用のバイト列（16進数）：**\n\n```text\n{digits}\n```"),
        );
        assert!(extract(&input).unwrap_err().contains("invalid hex"));
    }
}

#[test]
fn headings_and_labels_inside_source_are_not_metadata() {
    let input = SMALL.replace(
        "fn main() -> u8 { return 0 }",
        "### not-a-case\n- 主根拠：`[not.metadata]`\n- 結果：パニック `[rt.div-zero]` 1:1",
    );
    let s = extract(&input).unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s["lex-001"].rule, "src.encoding");
    assert!(s["lex-001"].bytes.starts_with(b"### not-a-case\n"));
}

#[test]
fn labelled_output_is_exact_and_prose_is_not_parsed() {
    let input = format!(
        "{SMALL}\n**標準出力：**\n\n```text\nhello\n\n```\n\n**標準エラー：**\n\n```text\n### not-a-case\n- 結果：正常終了\n```\n"
    );
    let s = extract(&input).unwrap();
    let run = &s["lex-001"].runs[0];
    assert_eq!(run.stdout.as_deref(), Some("hello\n\n"));
    assert_eq!(
        run.stderr.as_deref(),
        Some("### not-a-case\n- 結果：正常終了\n")
    );
    let s = extract(&SMALL.replace(
        "自由な説明。",
        "実行時パニック。終了コード 101。\n\n- 箇条書きの説明。\n- 結果は、名前のエラーになる。\n  - 結果として拒否される。\n結果：は説明文の中で書ける。\n\n| 列A | 列B |\n|---|---|\n| 1 | 2 |",
    ))
    .unwrap();
    assert_eq!(s["lex-001"].runs[0].result, Outcome::Exit);
    assert_eq!(s["lex-001"].runs[0].code, Some(0));

    // 補助根拠の区切り（・と、）、空白だけの行によるメタ行かたまり終了、CRLF改行を受理する。
    let extra = "### lex-001 example\r\n- 主根拠：`[src.encoding]`\r\n- 補助根拠：`[lex.a]`・`[lex.b]`、`[lex.c]`\r\n- 結果：正常終了\r\n   \r\n説明。\r\n\r\n```suz\r\nfn main() -> u8 { return 0 }\r\n```\r\n";
    let s = extract(extra).unwrap();
    assert_eq!(s["lex-001"].secondary_rules, ["lex.a", "lex.b", "lex.c"]);
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
        ("\n**標準出力：**\n\n```text\n```\n", "empty output block"),
        ("\n**標準エラー：**\n\n```text\n```\n", "empty output block"),
    ] {
        assert!(
            extract(&format!("{SMALL}{suffix}"))
                .unwrap_err()
                .contains(message)
        );
    }
    let panic_empty = format!(
        "{}\n**パニック前の標準エラー：**\n\n```text\n```\n",
        SMALL.replace("- 結果：正常終了", "- 結果：パニック `[rt.div-zero]` 1:1")
    );
    assert!(
        extract(&panic_empty)
            .unwrap_err()
            .contains("empty output block")
    );
}

#[test]
fn cases_and_modes_come_from_the_document() {
    let second = SMALL.replace("lex-001", "example-042").replace(
        "- 結果：正常終了",
        "- 結果（警告）：正常終了 7 / 警告：`[diagnostic.arithmetic]` 1:1, `[diagnostic.arithmetic]` 1:2\n- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 1:1",
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
    let compared = format!(
        "{}{}",
        ONE_CASE_INVENTORY
            .replace("| 受理 | 1 |", "| 受理 | 0 |")
            .replace("| 診断設定の比較 | 0 |", "| 診断設定の比較 | 1 |"),
        SMALL.replace(
            "- 結果：正常終了",
            "- 結果（通常）：正常終了\n- 結果（警告）：正常終了"
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
        let input = SMALL.replace(
            "```suz\nfn main() -> u8 { return 0 }\n```",
            &format!(
                "**入力図：**\n\n```text\n{diagram}\n```\n\n**保存用のバイト列（16進数）：**\n\n```text\n{hex}\n```"
            ),
        );
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
fn panic_expectation_is_read_from_result_line() {
    let input = SMALL.replace("- 結果：正常終了", "- 結果：パニック `[rt.div-zero]` 2:10");
    let input =
        format!("{input}\n**パニック前の標準エラー：**\n\n```text\nlogged before panic\n```\n");
    let cases = extract(&input).unwrap();
    let run = &cases["lex-001"].runs[0];
    assert_eq!(run.result, Outcome::Panic);
    assert_eq!(run.code, Some(101));
    assert_eq!(run.stderr_prefix.as_deref(), Some("logged before panic\n"));
    assert!(run.stderr.is_none());
    assert_eq!(run.panic_diagnostic.as_ref().unwrap().rule, "rt.div-zero");
}

#[test]
fn explanatory_sections_do_not_become_cases() {
    let document = r#"# Examples

### Format guide

This fenced example is documentation, not a test case.

````markdown
### fake-001 example
- 主根拠：`[fake.rule]`
- 結果：正常終了

```suz
fn main() -> u8 { return 7 }
```
````

~~~text
### fake-002 example
- 主根拠：`[fake.rule]`
~~~

## Cases

### lex-001 example ###
- 主根拠：`[src.encoding]`
- 結果：正常終了

#### Explanation within the case

The following program returns zero.

```suz
fn main() -> u8 { return 0 }
```

### Further reading

An unlabelled code block outside a case is ordinary documentation.

```text
This must not be read as case data.
```

## Appendix

- 結果：正常終了

This is not a case.
"#;
    let cases = extract(document).unwrap();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases["lex-001"].rule, "src.encoding");
    assert_eq!(cases["lex-001"].bytes, b"fn main() -> u8 { return 0 }\n");

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
        ("- 主根拠：`[src.encoding]`\n", "", "missing primary rule"),
        ("```suz", "## Appendix\n\n```suz", "missing source"),
    ] {
        let error = extract(&document.replace(from, to)).unwrap_err();
        assert!(error.contains(reason), "{error}");
    }
}

#[test]
fn compact_format_parses_all_variants_and_rejects_malformed_lines() {
    let doc = r#"### lex-001 exit default
- 主根拠：`[src.encoding]`
- 結果：正常終了

```suz
fn main() -> u8 { return 0 }
```

### lex-002 exit code with warning
- 主根拠：`[src.encoding]`
- 結果：正常終了 7 / 警告：`[diagnostic.arithmetic]` 2:10, `[array.redundant-handler]` 3:5

```suz
fn main() -> u8 { return 7 }
```

### lex-003 compile error
- 主根拠：`[char.literal]`
- 結果：コンパイルエラー `[char.literal]` 2:14

```suz
fn main() -> u8 { return 0 }
```

### lex-004 panic
- 主根拠：`[rt.div-zero]`
- 結果：パニック `[rt.div-zero]` 2:12

```suz
fn main() -> u8 { return 0 }
```

**パニック前の標準エラー：**

```text
before
```

### array-090 multi mode
- 主根拠：`[diagnostic.arithmetic-target]`
- 補助根拠：`[array.bulk-handler]`
- 結果（通常）：正常終了
- 結果（警告）：正常終了 / 警告：`[diagnostic.arithmetic]` 4:16
- 結果（強制）：コンパイルエラー `[diagnostic.arithmetic]` 4:16

```suz
fn main() -> u8 { return 0 }
```
"#;
    let cases = extract(doc).unwrap();
    assert_eq!(cases.len(), 5);
    assert_eq!(cases["lex-001"].runs[0].code, Some(0));
    assert_eq!(cases["lex-001"].runs[0].warning_count, 0);
    assert_eq!(cases["lex-002"].runs[0].code, Some(7));
    assert_eq!(cases["lex-002"].runs[0].warning_count, 2);
    assert_eq!(cases["lex-002"].runs[0].warnings.len(), 2);
    assert_eq!(cases["lex-003"].runs[0].result, Outcome::CompileError);
    assert_eq!(
        cases["lex-003"].runs[0].diagnostic.as_ref().unwrap().column,
        14
    );
    assert_eq!(cases["lex-004"].runs[0].result, Outcome::Panic);
    assert_eq!(cases["lex-004"].runs[0].code, Some(101));
    assert_eq!(
        cases["lex-004"].runs[0].stderr_prefix.as_deref(),
        Some("before\n")
    );
    assert_eq!(cases["array-090"].runs.len(), 3);
    assert_eq!(cases["array-090"].runs[1].mode, Mode::Warn);
    assert_eq!(cases["array-090"].runs[2].mode, Mode::Deny);

    let base = "### lex-001 test\n- 主根拠：`[src.encoding]`\n- 結果：正常終了\n\n```suz\nfn main() -> u8 { return 0 }\n```\n";
    for (bad, reason) in [
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n- 結果（通常）：正常終了",
            ),
            "duplicate mode",
        ),
        (base.replace("- 結果：", "- 結果（未知）："), "unknown mode"),
        (
            base.replace("- 結果：正常終了", "- 結果：コンパイルエラー"),
            "unknown outcome",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：コンパイルエラー `[char.literal]`",
            ),
            "missing diagnostic position",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：コンパイルエラー `[char.literal]` 0:1",
            ),
            "positive",
        ),
        (
            base.replace("- 結果：正常終了", "- 結果：パニック `[char.literal]` 2:1"),
            "runtime panic rule",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了 / 警告：`[char.literal]` 2:1",
            ),
            "supported warning rule",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了 / 警告：`[diagnostic.arithmetic]` 2:1, `[diagnostic.arithmetic]` 2:1",
            ),
            "duplicate warning expectation",
        ),
        (
            base.replace("- 結果：正常終了", "- 結果：正常終了 / 警告："),
            "empty warning list",
        ),
        (
            format!("{base}\n```suz\nfn other() -> void {{}}\n```\n"),
            "duplicate source",
        ),
        (format!("{base}\n```text\nhello\n```\n"), "unlabelled fence"),
        (format!("{base}\n~~~text\nhi\n~~~\n"), "unlabelled fence"),
        (
            format!("{base}\n  ```text\nhi\n  ```\n"),
            "unlabelled fence",
        ),
        (
            base.replace("- 結果：正常終了\n", ""),
            "missing result line",
        ),
        (
            base.replace("- 結果：正常終了", "- 結果：正常終了 0"),
            "exit code 0 must be omitted",
        ),
        (
            base.replace("- 結果：正常終了", "- 結果：正常終了 256"),
            "integer out of range",
        ),
        (
            base.replace("- 結果：正常終了", "- 結果：正常終了 01"),
            "invalid unsigned integer",
        ),
        (
            base.replace("- 結果：正常終了", "- 結果：正常終了  1"),
            "invalid unsigned integer",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：コンパイルエラー `[char.literal]` -1:1",
            ),
            "invalid unsigned integer",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：コンパイルエラー `[char.literal]` a:1",
            ),
            "invalid unsigned integer",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：コンパイルエラー `[char.literal]` 1:b",
            ),
            "invalid unsigned integer",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：コンパイルエラー `[char.literal]` 01:1",
            ),
            "invalid unsigned integer",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：コンパイルエラー  `[char.literal]` 1:1",
            ),
            "invalid diagnostic rule",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：コンパイルエラー `[char.literal]`  1:1",
            ),
            "invalid unsigned integer",
        ),
        (
            base.replace("- 結果：正常終了", "- 結果:正常終了"),
            "invalid result line",
        ),
        (
            base.replace("- 結果：正常終了", "- 結果(通常):正常終了"),
            "invalid result line",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了/警告：`[diagnostic.arithmetic]` 2:1",
            ),
            "unknown outcome",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了 /警告：`[diagnostic.arithmetic]` 2:1",
            ),
            "invalid unsigned integer",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了 / 警告:`[diagnostic.arithmetic]` 2:1",
            ),
            "invalid warning specification",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n-  結果（警告）：正常終了",
            ),
            "invalid metadata line",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n-\t結果（警告）：正常終了",
            ),
            "invalid metadata line",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n-　結果（警告）：正常終了",
            ),
            "invalid metadata line",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n-結果（警告）：正常終了",
            ),
            "invalid metadata line",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n* 結果（警告）：正常終了",
            ),
            "invalid metadata line",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n+ 結果（警告）：正常終了",
            ),
            "invalid metadata line",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n－ 結果（警告）：正常終了",
            ),
            "invalid metadata line",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n- 副根拠：`[src.encoding]`",
            ),
            "invalid metadata line",
        ),
        (
            base.replace("- 主根拠：", "- 主根據："),
            "invalid metadata line",
        ),
        (
            base.replace("- 結果：正常終了", "  - 結果：正常終了"),
            "invalid metadata line",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 結果：正常終了\n結果（強制）：正常終了",
            ),
            "invalid metadata line",
        ),
        (
            base.replace("- 結果：正常終了", "\n- 結果：正常終了"),
            "metadata line must appear in header block",
        ),
        (
            format!("{base}\n- 結果（警告）：正常終了\n"),
            "metadata line must appear in header block",
        ),
        (
            base.replace("`[src.encoding]`", "`[src.encoding]`、`[lex.x]`"),
            "invalid primary rule",
        ),
        (
            base.replace("`[src.encoding]`", "`[src encoding!]`"),
            "invalid primary rule",
        ),
        (
            base.replace("`[src.encoding]`", " `[src.encoding]`"),
            "invalid primary rule",
        ),
        (
            base.replace("`[src.encoding]`", "`[src.encoding]` "),
            "invalid primary rule",
        ),
        (
            base.replace("`[src.encoding]`", "`[src.encoding]`。"),
            "invalid primary rule",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 補助根拠：なにか\n- 結果：正常終了",
            ),
            "invalid secondary rule",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 補助根拠：`[src.encoding]`\n- 補助根拠：`[lex.x]`\n- 結果：正常終了",
            ),
            "duplicate secondary rule",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 補助根拠：`[lex.a]` ・ `[lex.b]`\n- 結果：正常終了",
            ),
            "invalid secondary rule",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 補助根拠：`[lex.a]`・`[lex.a]`\n- 結果：正常終了",
            ),
            "duplicate secondary rule id",
        ),
        (
            base.replace(
                "- 結果：正常終了",
                "- 補助根拠：`[src.encoding]`\n- 結果：正常終了",
            ),
            "secondary rule duplicates primary rule",
        ),
        (
            format!("{base}\n**標準出力：**\n\n```text\nhi\n```\n\n- 結果（警告）：正常終了\n"),
            "metadata line must appear in header block",
        ),
    ] {
        let error = extract(&bad).unwrap_err();
        assert!(error.contains(reason), "expected {reason}, got {error}");
    }
}
