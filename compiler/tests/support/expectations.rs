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
    if header != ["設定", "結果", "終了コード", "整数演算の警告数"] {
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
        let arithmetic_warnings = number(values[3])?;
        runs.push(Run {
            mode,
            result,
            code,
            stdout: None,
            stderr: None,
            stderr_prefix: None,
            arithmetic_warnings,
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
