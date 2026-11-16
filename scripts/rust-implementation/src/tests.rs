//! [impl.*] コメント・文字列とRust構文を区別して検査情報を抽出する。

use super::*;

#[test]
fn ignores_comment_and_string_lookalikes() {
    let report = scan(
        r###"
// crate::diagnostic!("bad.rule", "bad")
fn f() {
    let a = r##"crate::diagnostic!("bad.rule", "bad")"##;
    let b = "Diagnostic::new(1, 2, 3, 4, 5)";
    let c = b'\'';
}
"###,
    )
    .unwrap();
    assert!(report["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn extracts_decoded_literals_and_expression_rules() {
    let report = scan(
        r#"
fn f() {
    // [good.rule] 条件を確認する。
    return Err(crate::diagnostic!(r"good.rule", concat!("[bad", ".rule]")));
}
fn g() { Diagnostic::new(k, 1, 2, rule, format!("\u{5b}bad.rule]")); }
"#,
    )
    .unwrap();
    let rows = report["diagnostics"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["rule"], "good.rule");
    assert!(
        rows[0]["strings"]
            .as_array()
            .unwrap()
            .contains(&json!("[bad.rule]"))
    );
    assert!(
        rows[0]["context_comments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str().unwrap().contains("条件を確認する"))
    );
    assert!(rows[1]["rule"].is_null());
    assert_eq!(rows[1]["rule_expression"], "rule");
    assert!(
        rows[1]["strings"]
            .as_array()
            .unwrap()
            .contains(&json!("[bad.rule]"))
    );
}

#[test]
fn test_only_subtrees_are_excluded_but_other_cfgs_are_scanned() {
    let report = scan(
        r#"
#[cfg(test)] mod tests { fn f() { diagnostic!("test.rule", "message"); } }
#[cfg(all(test, unix))] fn t() { diagnostic!("test.rule", "message"); }
#[cfg(test)] struct T(crate::codegen::Generator);
#[cfg(any(test, unix))] fn u() { diagnostic!("unix.rule", "message"); }
#[cfg(not(test))] fn p() { diagnostic!("prod.rule", "message"); }
#[cfg(test)] #[path = "test_helpers.rs"] mod helpers;
"#,
    )
    .unwrap();
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 2);
    assert_eq!(report["modules"][0]["test_only"], true);
    assert_eq!(report["modules"][0]["path"], "test_helpers.rs");
    assert!(
        !report["references"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == json!(["crate", "codegen", "Generator"]))
    );
}

#[test]
fn tracks_grouped_imports_aliases_and_inline_modules() {
    let report = scan(
        r#"
use crate::{check::ir::{self, Value as V}, Diagnostic as D, diagnostic as fail};
fn f() { D::new(k, 1, 2, "x.y", "message"); fail!("x.z", "message"); }
mod nested { use super::check; fn f() { check::run(); } mod child; }
"#,
    )
    .unwrap();
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 2);
    assert_eq!(
        report["imports"][0]["path"],
        json!(["crate", "check", "ir"])
    );
    assert_eq!(report["imports"][0]["alias"], "ir");
    assert_eq!(report["modules"][0]["module"], json!(["nested"]));
}

#[test]
fn diagnostic_imports_can_follow_their_uses() {
    let report = scan(
        r#"
fn f() { D::new(k, 1, 2, "a.b", "message"); fail!("a.c", "message"); }
use crate::{Diagnostic as D, diagnostic as fail};
"#,
    )
    .unwrap();
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 2);
    assert_eq!(report["imports"].as_array().unwrap().len(), 2);
}

#[test]
fn finds_diagnostics_and_dependencies_inside_macro_tokens() {
    let report = scan(
        r#"
macro_rules! make {
    () => { crate::diagnostic!("macro.rule", "message") };
}
fn f() { outer!(Diagnostic::new(k, 1, 2, "x.y", "message"), super::exact::Exact); }
"#,
    )
    .unwrap();
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 2);
    assert!(
        report["references"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["path"] == json!(["super", "exact", "Exact"]))
    );
}

#[test]
fn skip_attributes_need_adjacent_nonempty_comments() {
    let report = scan(
        r#"
// 表の列を揃えるため。
#[rustfmt::skip] fn table() {}
#[cfg_attr(any(), rustfmt::skip)] fn missing() {}
/* 一覧を読みやすく保つため。 */
#[rustfmt::skip] fn block_comment() {}
"#,
    )
    .unwrap();
    let skips = report["skips"].as_array().unwrap();
    assert_eq!(skips.len(), 3);
    assert!(skips[0]["comment"].as_str().unwrap().contains("列を揃える"));
    assert_eq!(skips[1]["comment"], "");
    assert!(skips[2]["comment"].as_str().unwrap().contains("読みやすく"));
}

#[test]
fn parse_errors_are_not_silently_ignored() {
    assert!(scan("fn f( {").is_err());
}
