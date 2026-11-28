use super::{ExpectedDiagnostic, Mode, Outcome, Result, Run, Source, markdown::number};
use std::{collections::BTreeSet, path::Path};

impl ExpectedDiagnostic {
    pub fn matches(&self, stderr: &str, path: &Path) -> bool {
        self.matches_kind(stderr, path, false)
    }

    pub fn matches_panic(&self, stderr: &str, path: &Path) -> bool {
        stderr
            .strip_prefix("panic ")
            .is_some_and(|line| self.matches(line, path))
    }

    fn matches_kind(&self, stderr: &str, path: &Path, warning: bool) -> bool {
        let prefix = format!(
            "{}:{}:{}: {}",
            path.display(),
            self.line,
            self.column,
            if warning { "warn: " } else { "" }
        );
        let Some(message) = stderr
            .strip_suffix('\n')
            .and_then(|s| s.strip_prefix(&prefix))
        else {
            return false;
        };
        self.matches_parts(self.line, self.column, message)
    }

    pub fn matches_parts(&self, line: usize, column: usize, message: &str) -> bool {
        self.line == line
            && self.column == column
            && !message.contains(['\n', '\r'])
            && !message.starts_with("warn:")
            && message
                .strip_suffix(&format!(" [{}]", self.rule))
                .is_some_and(|description| !description.trim().is_empty())
    }
}

pub fn warnings_match(expected: &[ExpectedDiagnostic], actual: &[&str], path: &Path) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    let mut remaining = actual.to_vec();
    for warning in expected {
        let Some(index) = remaining
            .iter()
            .position(|line| warning.matches_kind(line, path, true))
        else {
            return false;
        };
        remaining.remove(index);
    }
    true
}

pub(super) fn is_supported_warning_rule(rule: &str) -> bool {
    matches!(
        rule,
        "diagnostic.arithmetic"
            | "diagnostic.constant-comparison"
            | "diagnostic.integer-division-to-float"
            | "diagnostic.float-sat-nan"
            | "arena.loop-allocation"
            | "array.redundant-handler"
    )
}

pub(super) fn parse_rule_token(token: &str) -> Result<String> {
    token
        .strip_prefix("`[")
        .and_then(|s| s.strip_suffix("]`"))
        .filter(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b".-".contains(&b))
        })
        .map(str::to_owned)
        .ok_or_else(|| "invalid diagnostic rule".into())
}

fn parse_rule_position(text: &str) -> Result<ExpectedDiagnostic> {
    let (rule_part, pos_part) = text.split_once(' ').ok_or("missing diagnostic position")?;
    let rule = parse_rule_token(rule_part)?;
    let (line_str, col_str) = pos_part
        .split_once(':')
        .ok_or("invalid diagnostic position")?;
    let line = number::<usize>(line_str)?;
    let column = number::<usize>(col_str)?;
    if line == 0 || column == 0 {
        return Err("diagnostic positions must be positive".into());
    }
    Ok(ExpectedDiagnostic { rule, line, column })
}

pub(super) fn parse_result_line(line: &str) -> Result<Run> {
    let rest = line.strip_prefix("- 結果").ok_or("invalid result line")?;
    let (mode, body) = if let Some(body) = rest.strip_prefix('：') {
        (Mode::Default, body)
    } else if let Some(after_open) = rest.strip_prefix('（') {
        let (mode_str, body) = after_open
            .split_once("）：")
            .ok_or("invalid result mode syntax")?;
        let mode = match mode_str {
            "通常" => Mode::Default,
            "警告" => Mode::Warn,
            "強制" => Mode::Deny,
            _ => return Err("unknown mode".into()),
        };
        (mode, body)
    } else {
        return Err("invalid result line".into());
    };

    let (outcome_str, warnings) = if let Some((left, right)) = body.split_once(" / ") {
        if right.contains(" / ") {
            return Err("invalid result line".into());
        }
        let warn_str = right
            .strip_prefix("警告：")
            .ok_or("invalid warning specification")?;
        if warn_str.is_empty() {
            return Err("empty warning list".into());
        }
        let mut warnings = Vec::new();
        for item in warn_str.split(", ") {
            let diag = parse_rule_position(item)?;
            if !is_supported_warning_rule(&diag.rule) {
                return Err("expected a supported warning rule".into());
            }
            if warnings.contains(&diag) {
                return Err("duplicate warning expectation".into());
            }
            warnings.push(diag);
        }
        (left, warnings)
    } else {
        (body, Vec::new())
    };

    let (result, code, diagnostic, panic_diagnostic) = if outcome_str == "正常終了" {
        (Outcome::Exit, Some(0), None, None)
    } else if let Some(code_str) = outcome_str.strip_prefix("正常終了 ") {
        let code = number::<u8>(code_str)?;
        if code == 0 {
            return Err("exit code 0 must be omitted".into());
        }
        (Outcome::Exit, Some(code), None, None)
    } else if let Some(diag_str) = outcome_str.strip_prefix("コンパイルエラー ") {
        (
            Outcome::CompileError,
            None,
            Some(parse_rule_position(diag_str)?),
            None,
        )
    } else if let Some(diag_str) = outcome_str.strip_prefix("パニック ") {
        let diag = parse_rule_position(diag_str)?;
        if !diag.rule.starts_with("rt.") {
            return Err("expected runtime panic rule".into());
        }
        (Outcome::Panic, Some(101), None, Some(diag))
    } else {
        return Err("unknown outcome".into());
    };

    Ok(Run {
        mode,
        result,
        code,
        stdout: None,
        stderr: None,
        stderr_prefix: None,
        warning_count: warnings.len(),
        diagnostic,
        panic_diagnostic,
        warnings,
    })
}

pub(super) fn validate(source: &Source) -> Result<()> {
    let mut modes = BTreeSet::new();
    for run in &source.runs {
        if !modes.insert(&run.mode) {
            return Err("duplicate mode".into());
        }
        let valid = match run.result {
            Outcome::Exit => run.code.is_some(),
            Outcome::Panic => run.code == Some(101),
            Outcome::CompileError => run.code.is_none(),
        };
        if !valid {
            return Err("invalid exit code for outcome".into());
        }
    }
    Ok(())
}
