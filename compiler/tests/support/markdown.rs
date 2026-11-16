use super::Result;

pub(super) fn unique<T>(slot: &mut Option<T>, value: T, label: &str) -> Result<()> {
    if slot.replace(value).is_some() {
        return Err(format!("duplicate {label}"));
    }
    Ok(())
}

pub(super) fn skip_blank(lines: &[&str], i: &mut usize) {
    while *i < lines.len() && lines[*i].trim().is_empty() {
        *i += 1;
    }
}

pub(super) fn block(lines: &[&str], i: &mut usize, language: &str) -> Result<String> {
    skip_blank(lines, i);
    if lines.get(*i) != Some(&format!("```{language}").as_str()) {
        return Err(format!("expected {language} fence after label"));
    }
    *i += 1;
    let start = *i;
    while *i < lines.len() && lines[*i] != "```" {
        if lines[*i].starts_with("```") {
            return Err("nested fence".into());
        }
        *i += 1;
    }
    if *i == lines.len() {
        return Err("unclosed fence".into());
    }
    let content = if start == *i {
        String::new()
    } else {
        format!("{}\n", lines[start..*i].join("\n"))
    };
    *i += 1;
    Ok(content)
}

pub(super) fn cells(line: &str) -> Result<Vec<&str>> {
    table_cells(line, 4)
}

pub(super) fn table_cells(line: &str, columns: usize) -> Result<Vec<&str>> {
    let inner = line
        .strip_prefix('|')
        .and_then(|s| s.strip_suffix('|'))
        .ok_or("invalid table row")?;
    let values: Vec<_> = inner.split('|').map(str::trim).collect();
    if values.len() != columns {
        return Err(format!("expected {columns} table columns"));
    }
    Ok(values)
}

pub(super) fn number<T: std::str::FromStr>(text: &str) -> Result<T> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid unsigned integer".into());
    }
    text.parse().map_err(|_| "integer out of range".into())
}

pub(super) fn separator(lines: &[&str], i: &mut usize) -> Result<()> {
    table_separator(lines, i, 4)
}

pub(super) fn table_separator(lines: &[&str], i: &mut usize, columns: usize) -> Result<()> {
    let separator = table_cells(lines.get(*i).ok_or("missing table separator")?, columns)?;
    if separator.iter().any(|c| {
        let dashes = c.trim_matches(':');
        dashes.len() < 3 || !dashes.bytes().all(|b| b == b'-')
    }) {
        return Err("invalid table separator".into());
    }
    *i += 1;
    Ok(())
}

// Only ATX headings delimit sections; fenced content is always data.
pub(super) fn heading(line: &str) -> Option<(usize, &str)> {
    let text = line.trim_start_matches(' ');
    if line.len() - text.len() > 3 {
        return None;
    }
    let level = text.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &text[level..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let title = rest.trim();
    let without_hashes = title.trim_end_matches('#');
    let title = if without_hashes.is_empty() || without_hashes.ends_with([' ', '\t']) {
        without_hashes.trim_end()
    } else {
        title
    };
    Some((level, title))
}

pub(super) fn fence(line: &str) -> Option<(u8, usize, &str)> {
    let text = line.trim_start_matches(' ');
    if line.len() - text.len() > 3 {
        return None;
    }
    let marker = *text.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let length = text.bytes().take_while(|b| *b == marker).count();
    (length >= 3).then_some((marker, length, &text[length..]))
}
