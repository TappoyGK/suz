use super::{
    Outcome, Result, Source,
    expectations::{parse_result_line, parse_rule_token, validate},
    markdown::{block, fence, heading, unique},
};
use std::collections::BTreeMap;

pub(super) fn valid_id(id: &str) -> bool {
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
    let mut runs = Vec::new();
    let mut rule = None;
    let mut secondary = None;
    let mut stdout = None;
    let mut stderr = None;
    let mut stderr_prefix = None;
    let mut diagram = None;
    let mut in_meta = true;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if in_meta {
            if line.trim().is_empty() {
                in_meta = false;
                continue;
            }
            if let Some(value) = line.strip_prefix("- 主根拠：") {
                let value = parse_rule_token(value).map_err(|_| "invalid primary rule")?;
                unique(&mut rule, value, "primary rule")?;
            } else if let Some(value) = line.strip_prefix("- 補助根拠：") {
                if value.is_empty() {
                    return Err("invalid secondary rule".into());
                }
                let mut tokens = Vec::new();
                for token in value.split(['・', '、']) {
                    let parsed = parse_rule_token(token).map_err(|_| "invalid secondary rule")?;
                    if tokens.contains(&parsed) {
                        return Err("duplicate secondary rule id".into());
                    }
                    tokens.push(parsed);
                }
                unique(&mut secondary, tokens, "secondary rule")?;
            } else if line.starts_with("- 結果") {
                runs.push(parse_result_line(line)?);
            } else {
                return Err("invalid metadata line".into());
            }
            continue;
        }
        match line {
            "**期待結果：**" | "**コンパイル診断：**" | "**パニック診断：**" | "**警告：**" =>
            {
                return Err("legacy table format is no longer supported".into());
            }
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
            "**標準出力：**" => {
                let content = block(lines, &mut i, "text")?;
                if content.is_empty() {
                    return Err("empty output block".into());
                }
                unique(&mut stdout, content, "stdout")?;
            }
            "**標準エラー：**" => {
                let content = block(lines, &mut i, "text")?;
                if content.is_empty() {
                    return Err("empty output block".into());
                }
                unique(&mut stderr, content, "stderr")?;
            }
            "**パニック前の標準エラー：**" => {
                let content = block(lines, &mut i, "text")?;
                if content.is_empty() {
                    return Err("empty output block".into());
                }
                unique(&mut stderr_prefix, content, "panic stderr")?;
            }
            "**入力図：**" => {
                unique(
                    &mut diagram,
                    decode_diagram(&block(lines, &mut i, "text")?)?,
                    "input diagram",
                )?;
            }
            _ => {
                let trimmed = line.trim();
                if trimmed.starts_with("- 主根拠：")
                    || trimmed.starts_with("- 補助根拠：")
                    || trimmed.starts_with("- 結果：")
                    || trimmed.starts_with("- 結果（")
                {
                    return Err("metadata line must appear in header block".into());
                } else if trimmed.starts_with("**") && trimmed.ends_with("：**") {
                    return Err("unsupported or unlabelled fence".into());
                } else if line == "```suz" {
                    i -= 1;
                    let code = block(lines, &mut i, "suz")?;
                    if code.is_empty() {
                        return Err("empty source".into());
                    }
                    unique(&mut bytes, code.into_bytes(), "source")?;
                } else if fence(line).is_some() {
                    return Err("unsupported or unlabelled fence".into());
                }
            }
        }
    }
    if runs.is_empty() {
        return Err("missing result line".into());
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
    let rule = rule.ok_or("missing primary rule")?;
    let secondary_rules = secondary.unwrap_or_default();
    if secondary_rules.contains(&rule) {
        return Err("secondary rule duplicates primary rule".into());
    }
    Ok(Source {
        bytes,
        rule,
        secondary_rules,
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
            let trimmed = line.trim();
            if current.is_some() && trimmed.starts_with("- 主根拠：") {
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
