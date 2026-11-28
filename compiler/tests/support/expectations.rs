use super::{
    ExpectedDiagnostic, Mode, Outcome, Result, Run, Source,
    markdown::{cells, number, separator, skip_blank},
};
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

fn parse_rule_token(token: &str) -> Result<String> {
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
    let pos_part = pos_part.trim();
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
        (Mode::Default, body.trim())
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
        (mode, body.trim())
    } else {
        return Err("invalid result line".into());
    };

    let (outcome_str, warnings) = if let Some((left, right)) = body.split_once(" / ") {
        if right.contains(" / ") {
            return Err("invalid result line".into());
        }
        let warn_str = right
            .strip_prefix("警告：")
            .ok_or("invalid warning specification")?
            .trim();
        if warn_str.is_empty() {
            return Err("empty warning list".into());
        }
        let mut warnings = Vec::new();
        for item in warn_str.split(',') {
            let diag = parse_rule_position(item.trim())?;
            if !is_supported_warning_rule(&diag.rule) {
                return Err("expected a supported warning rule".into());
            }
            if warnings.contains(&diag) {
                return Err("duplicate warning expectation".into());
            }
            warnings.push(diag);
        }
        (left.trim(), warnings)
    } else {
        (body, Vec::new())
    };

    let (result, code, diagnostic, panic_diagnostic) = if outcome_str == "正常終了" {
        (Outcome::Exit, Some(0), None, None)
    } else if let Some(code_str) = outcome_str.strip_prefix("正常終了 ") {
        (
            Outcome::Exit,
            Some(number::<u8>(code_str.trim())?),
            None,
            None,
        )
    } else if let Some(diag_str) = outcome_str.strip_prefix("コンパイルエラー ") {
        (
            Outcome::CompileError,
            None,
            Some(parse_rule_position(diag_str.trim())?),
            None,
        )
    } else if let Some(diag_str) = outcome_str.strip_prefix("パニック ") {
        let diag = parse_rule_position(diag_str.trim())?;
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

pub(super) fn diagnostic_table(
    lines: &[&str],
    i: &mut usize,
) -> Result<Vec<(Mode, ExpectedDiagnostic)>> {
    skip_blank(lines, i);
    if cells(lines.get(*i).ok_or("missing diagnostic table")?)? != ["設定", "規則", "行", "列"]
    {
        return Err("invalid diagnostic table header".into());
    }
    *i += 1;
    separator(lines, i)?;
    let mut entries = Vec::new();
    while *i < lines.len() && lines[*i].starts_with('|') {
        let values = cells(lines[*i])?;
        *i += 1;
        let mode = match values[0] {
            "通常" => Mode::Default,
            "警告" => Mode::Warn,
            "強制" => Mode::Deny,
            _ => return Err("unknown diagnostic mode".into()),
        };
        let rule = values[1]
            .strip_prefix("`[")
            .and_then(|s| s.strip_suffix("]`"))
            .filter(|s| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_lowercase() || b".-".contains(&b))
            })
            .ok_or("invalid diagnostic rule")?
            .to_owned();
        let line = number::<usize>(values[2])?;
        let column = number::<usize>(values[3])?;
        if line == 0 || column == 0 {
            return Err("diagnostic positions must be positive".into());
        }
        entries.push((mode, ExpectedDiagnostic { rule, line, column }));
    }
    if entries.is_empty() {
        return Err("empty diagnostic table".into());
    }
    Ok(entries)
}

pub(super) fn table(lines: &[&str], i: &mut usize) -> Result<Vec<Run>> {
    skip_blank(lines, i);
    let header = cells(lines.get(*i).ok_or("missing expectation table")?)?;
    if header != ["設定", "結果", "終了コード", "警告数"] {
        return Err("invalid table header".into());
    }
    *i += 1;
    separator(lines, i)?;
    let mut runs = Vec::new();
    while *i < lines.len() && lines[*i].starts_with('|') {
        let values = cells(lines[*i])?;
        *i += 1;
        let mode = match values[0] {
            "通常" => Mode::Default,
            "警告" => Mode::Warn,
            "強制" => Mode::Deny,
            _ => return Err("unknown mode".into()),
        };
        let result = match values[1] {
            "正常終了" => Outcome::Exit,
            "コンパイルエラー" => Outcome::CompileError,
            "パニック" => Outcome::Panic,
            _ => return Err("unknown outcome".into()),
        };
        let code = if values[2] == "—" {
            None
        } else {
            Some(number::<u8>(values[2])?)
        };
        let warning_count = number(values[3])?;
        runs.push(Run {
            mode,
            result,
            code,
            stdout: None,
            stderr: None,
            stderr_prefix: None,
            warning_count,
            diagnostic: None,
            panic_diagnostic: None,
            warnings: Vec::new(),
        });
    }
    if runs.is_empty() {
        return Err("empty expectation table".into());
    }
    Ok(runs)
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
