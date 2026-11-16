//! allowの項目を読み取る。理由内の引用符・コメント記号・波括弧を通常のトークンとして解釈しない。
//!
//! 利用元: [`crate::lexer`]、[`crate::lexer::templates`]。
//! 主な対象規則: `[allow.item]`、`[allow.reason]`。

use super::{Diagnostic, ErrorKind, error};

// allowの項目は、通常の字句解析とテンプレート補間の走査では内容を解釈しない。
// 理由内の引用符・コメント記号・波括弧は単なる文字列として扱う。
pub(crate) fn scan(source: &str, line: usize, column: usize) -> Result<usize, Diagnostic> {
    for (offset, ch) in source.char_indices().skip(1) {
        if ch == ']' {
            return Ok(offset + 1);
        }
        if ch <= '\u{1f}' || ch == '\u{7f}' {
            return Err(error(
                line,
                column + source[..offset].chars().count(),
                ErrorKind::InvalidSource,
                crate::diagnostic!(
                    "allow.reason",
                    "control characters are forbidden in an allow item"
                ),
            ));
        }
    }
    Err(error(
        line,
        column,
        ErrorKind::InvalidSource,
        crate::diagnostic!("allow.item", "unclosed allow item"),
    ))
}
