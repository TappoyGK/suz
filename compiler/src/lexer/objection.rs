//! objectionの理由はソース上の注釈であり、式や文字列プールの値にはしない。
use super::*;

pub(super) fn scan(source: &str, line: usize, column: usize) -> Result<usize, Diagnostic> {
    let invalid = || {
        error(
            line,
            column,
            ErrorKind::InvalidSource,
            crate::diagnostic!(
                "arena.objection-reason",
                "expected a non-empty reason in brackets without control characters"
            ),
        )
    };
    let mut nonblank = false;
    for (offset, ch) in source.char_indices().skip(1) {
        if ch <= '\u{1f}' || ch == '\u{7f}' {
            return Err(invalid());
        }
        if ch == ']' {
            return if nonblank {
                Ok(offset + 1)
            } else {
                Err(invalid())
            };
        } else {
            nonblank |= !ch.is_whitespace();
        }
    }
    Err(invalid())
}
