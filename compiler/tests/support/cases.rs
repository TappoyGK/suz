use super::{
    Outcome, Result, Source,
    expectations::{diagnostic_table, table, validate},
    markdown::{block, fence, heading, unique},
};
use std::collections::BTreeMap;

fn valid_id(id: &str) -> bool {
    let Some((group, number)) = id.split_once('-') else {
        return false;
    };
    !group.is_empty()
        && group.bytes().all(|b| b.is_ascii_lowercase())
        && number.len() == 3
        && number.bytes().all(|b| b.is_ascii_digit())
        && number != "000"
}

fn decode_diagram(body: &str) -> Result<Vec<u8>> {
    let joined = body.replace('\n', "");
    let mut rest = joined
        .strip_suffix("<EOF>")
        .ok_or("input diagram must end with <EOF>")?;
    let mut bytes = Vec::new();
    while !rest.is_empty() {
        if rest.starts_with("<EOF>") {
            return Err("input diagram has an early <EOF>".into());
        }
        if let Some(tail) = rest.strip_prefix("<CR>") {
            bytes.push(b'\r');
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("<LF>") {
            bytes.push(b'\n');
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("<FF>") {
            bytes.push(0xff);
            rest = tail;
        } else {
            let size = rest.chars().next().unwrap().len_utf8();
            bytes.extend_from_slice(&rest.as_bytes()[..size]);
            rest = &rest[size..];
        }
    }
    Ok(bytes)
}

fn source(lines: &[&str]) -> Result<Source> {
    let mut bytes = None;
    let mut runs = None;
    let mut rule = None;
    let mut stdout = None;
    let mut stderr = None;
    let mut stderr_prefix = None;
    let mut diagram = None;
    let mut diagnostics = None;
    let mut panic_diagnostics = None;
    let mut warnings = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        match line {
            "**期待結果：**" => unique(&mut runs, table(lines, &mut i)?, "expectation table")?,
            "**コンパイル診断：**" => unique(
                &mut diagnostics,
                diagnostic_table(lines, &mut i)?,
                "diagnostic table",
            )?,
            "**パニック診断：**" => unique(
                &mut panic_diagnostics,
                diagnostic_table(lines, &mut i)?,
                "panic diagnostic table",
            )?,
            "**警告：**" => unique(
                &mut warnings,
                diagnostic_table(lines, &mut i)?,
                "warning table",
            )?,
            "**ソース：**" => {
                let code = block(lines, &mut i, "suz")?;
                if code.is_empty() {
                    return Err("empty source".into());
                }
                unique(&mut bytes, code.into_bytes(), "source")?;
            }
            "**保存用のバイト列（16進数）：**" => {
                let body = block(lines, &mut i, "text")?;
                let digits: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
                if digits.is_empty()
                    || !digits.len().is_multiple_of(2)
                    || !digits.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err("invalid hex input".into());
                }
                let decoded = (0..digits.len())
                    .step_by(2)
                    .map(|n| u8::from_str_radix(&digits[n..n + 2], 16).unwrap())
                    .collect();
                unique(&mut bytes, decoded, "source")?;
            }
            "**標準出力：**" => unique(&mut stdout, block(lines, &mut i, "text")?, "stdout")?,
            "**標準エラー：**" => {
                unique(&mut stderr, block(lines, &mut i, "text")?, "stderr")?
            }
            "**パニック前の標準エラー：**" => unique(
                &mut stderr_prefix,
                block(lines, &mut i, "text")?,
                "panic stderr",
            )?,
            "**入力図：**" => {
                unique(
                    &mut diagram,
                    decode_diagram(&block(lines, &mut i, "text")?)?,
                    "input diagram",
                )?;
            }
            _ => {
                if let Some(value) = line.trim().strip_prefix("主根拠：") {
                    let value = value.trim().trim_end_matches('。').trim();
                    let value = value
                        .strip_prefix("`[")
                        .and_then(|s| s.strip_suffix("]`"))
                        .ok_or("invalid primary rule")?;
                    unique(&mut rule, value.to_owned(), "primary rule")?;
                } else if line.starts_with("```") {
                    return Err("unsupported or unlabelled fence".into());
                }
            }
        }
    }
    let mut runs = runs.ok_or("missing expectation table")?;
    for (mode, diagnostic) in diagnostics.unwrap_or_default() {
        let run = runs
            .iter_mut()
            .find(|r| r.mode == mode && r.result == Outcome::CompileError)
            .ok_or("diagnostic does not apply to a compile-error run")?;
        unique(&mut run.diagnostic, diagnostic, "diagnostic mode")?;
    }
    for (mode, diagnostic) in panic_diagnostics.unwrap_or_default() {
        if !diagnostic.rule.starts_with("rt.") {
            return Err("expected runtime panic rule".into());
        }
        let run = runs
            .iter_mut()
            .find(|r| r.mode == mode && r.result == Outcome::Panic)
            .ok_or("panic diagnostic does not apply to a panic run")?;
        unique(
            &mut run.panic_diagnostic,
            diagnostic,
            "panic diagnostic mode",
        )?;
    }
    for (mode, warning) in warnings.unwrap_or_default() {
        let run = runs
            .iter_mut()
            .find(|r| r.mode == mode)
            .ok_or("warning does not apply to any run")?;
        if !matches!(
            warning.rule.as_str(),
            "diagnostic.arithmetic"
                | "diagnostic.constant-comparison"
                | "diagnostic.integer-division-to-float"
                | "arena.loop-allocation"
                | "array.redundant-handler"
        ) {
            return Err("expected a supported warning rule".into());
        }
        if run.warnings.contains(&warning) {
            return Err("duplicate warning expectation".into());
        }
        run.warnings.push(warning);
    }
    for run in &runs {
        if !run.warnings.is_empty() && run.warnings.len() != run.warning_count {
            return Err("warning table and warning count disagree".into());
        }
    }
    let has_exit = runs.iter().any(|r| r.result == Outcome::Exit);
    let has_panic = runs.iter().any(|r| r.result == Outcome::Panic);
    if (stdout.is_some() && !has_exit && !has_panic)
        || (stderr.is_some() && !has_exit)
        || (stderr_prefix.is_some() && !has_panic)
    {
        return Err("output block does not apply to any run".into());
    }
    for run in &mut runs {
        match run.result {
            Outcome::Exit => {
                run.stdout = Some(stdout.clone().unwrap_or_default());
                run.stderr = Some(stderr.clone().unwrap_or_default());
            }
            Outcome::Panic => {
                run.stdout = Some(stdout.clone().unwrap_or_default());
                run.stderr_prefix = Some(stderr_prefix.clone().unwrap_or_default());
            }
            Outcome::CompileError => {}
        }
    }
    let bytes = bytes.ok_or("missing source")?;
    if let Some(diagram) = diagram
        && diagram != bytes
    {
        return Err("input diagram and source bytes disagree".into());
    }
    Ok(Source {
        bytes,
        rule: rule.ok_or("missing primary rule")?,
        runs,
    })
}

pub fn extract(markdown: &str) -> Result<BTreeMap<String, Source>> {
    let mut cases = BTreeMap::new();
    let mut current: Option<&str> = None;
    let mut section = Vec::new();
    let mut has_rule = false;
    let mut open_fence: Option<(u8, usize)> = None;
    let finish = |title: &str,
                  lines: &[&str],
                  has_rule: bool,
                  cases: &mut BTreeMap<String, Source>|
     -> Result<()> {
        let mut parts = title.splitn(2, char::is_whitespace);
        let id = parts.next().unwrap_or_default();
        // 主根拠のラベルがあれば、IDが不正でも例として識別する。
        if !valid_id(id) && !has_rule {
            return Ok(());
        }
        let title = parts.next().unwrap_or_default().trim();
        if !valid_id(id) || title.is_empty() {
            return Err(format!("invalid case heading: {id} {title}"));
        }
        let parsed = source(lines).map_err(|e| format!("{id}: {e}"))?;
        validate(&parsed).map_err(|e| format!("{id}: {e}"))?;
        if cases.insert(id.to_owned(), parsed).is_some() {
            return Err(format!("duplicate ID: {id}"));
        }
        Ok(())
    };
    for line in markdown.lines() {
        if let Some((marker, length)) = open_fence {
            if let Some((closing_marker, closing_length, tail)) = fence(line)
                && closing_marker == marker
                && closing_length >= length
                && tail.trim().is_empty()
            {
                open_fence = None;
            }
        } else if let Some((marker, length, _)) = fence(line) {
            open_fence = Some((marker, length));
        } else {
            if let Some((level, title)) = heading(line)
                && level <= 3
            {
                if let Some(current) = current.take() {
                    finish(current, &section, has_rule, &mut cases)?;
                }
                section.clear();
                has_rule = false;
                current = (level == 3).then_some(title);
                continue;
            }
            if current.is_some() && line.trim().starts_with("主根拠：") {
                has_rule = true;
            }
        }
        if current.is_some() {
            section.push(line);
        }
    }
    if open_fence.is_some() {
        return Err("unclosed fence".into());
    }
    if let Some(current) = current {
        finish(current, &section, has_rule, &mut cases)?;
    }
    if cases.is_empty() {
        return Err("no cases".into());
    }
    Ok(cases)
}
