//! [src.reason] の説明文を読む。通常の字句解析とテンプレート補間で同じ終端・検査を使う。
use super::{Diagnostic, ErrorKind, error};

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Allow,
    Objection,
    Ignore,
}

/// allowだけは規則名とコロンを含む項目を読む。規則名の形式はパーサーが検査する。
pub(super) fn scan(
    source: &str,
    line: usize,
    column: usize,
    kind: Kind,
) -> Result<usize, Diagnostic> {
    let invalid = || {
        error(
            line,
            column,
            ErrorKind::InvalidSource,
            match kind {
                Kind::Allow => crate::diagnostic!(
                    "allow.reason",
                    "expected a non-empty reason without control characters"
                ),
                Kind::Objection => crate::diagnostic!(
                    "arena.objection-reason",
                    "expected a non-empty reason in brackets without control characters"
                ),
                Kind::Ignore => crate::diagnostic!(
                    "err.or-ignore",
                    "expected a non-empty reason in brackets without control characters"
                ),
            },
        )
    };
    for (offset, ch) in source.char_indices().skip(1) {
        if ch <= '\u{1f}' || ch == '\u{7f}' {
            return Err(invalid());
        }
        if ch == ']' {
            let raw = &source[1..offset];
            let reason = match kind {
                Kind::Allow => raw.split_once(':').map(|(_, reason)| reason),
                _ => Some(raw),
            };
            if reason.is_some_and(|s| s.chars().all(char::is_whitespace)) {
                return Err(invalid());
            }
            return Ok(offset + 1);
        }
    }
    if matches!(kind, Kind::Allow) {
        return Err(error(
            line,
            column,
            ErrorKind::InvalidSource,
            crate::diagnostic!("allow.item", "unclosed allow item"),
        ));
    }
    Err(invalid())
}
