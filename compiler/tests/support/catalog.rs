use super::{
    Outcome, Result, Source,
    cases::valid_id,
    extract,
    markdown::{number, skip_blank, table_cells, table_separator, unique},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub fn load(docs: &Path) -> Result<BTreeMap<String, Source>> {
    let index = docs.join("spec-cases.md");
    let index =
        fs::read_to_string(&index).map_err(|error| format!("{}: {error}", index.display()))?;
    let directory = docs.join("spec-cases");
    let mut paths = fs::read_dir(&directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    paths.retain(|path| path.extension().is_some_and(|extension| extension == "md"));
    paths.sort();
    let mut sources = BTreeMap::new();
    for path in paths {
        let markdown =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let cases = extract(&markdown).map_err(|error| format!("{}: {error}", path.display()))?;
        for (id, source) in cases {
            if sources.insert(id.clone(), source).is_some() {
                return Err(format!(
                    "{}: duplicate ID across files: {id}",
                    path.display()
                ));
            }
        }
    }
    validate_inventory(&index, &sources)?;
    Ok(sources)
}

// 二つの目録は同じ表の形式を使い、見出しと行のキーだけが異なる。
pub fn validate_inventory(markdown: &str, sources: &BTreeMap<String, Source>) -> Result<()> {
    let mut groups = BTreeMap::<&str, usize>::new();
    for id in sources.keys() {
        *groups.entry(id.split_once('-').unwrap().0).or_default() += 1;
    }
    validate_count_table(
        markdown,
        "## 目次と件数",
        &["分野", "ID接頭辞", "対応する仕様", "件数"],
        |row| {
            row[1]
                .strip_prefix('`')
                .and_then(|s| s.strip_suffix('`'))
                .ok_or_else(|| "invalid inventory prefix".into())
        },
        &groups,
    )?;

    const COMPARISON: &str = "診断設定の比較";
    let mut outcomes: BTreeMap<_, _> = [Outcome::Exit, Outcome::CompileError, Outcome::Panic]
        .iter()
        .map(|outcome| (outcome.inventory_name(), 0))
        .chain([(COMPARISON, 0)])
        .collect();
    for source in sources.values() {
        let category = if source.runs.len() > 1 {
            COMPARISON
        } else {
            source.runs[0].result.inventory_name()
        };
        *outcomes.get_mut(category).unwrap() += 1;
    }
    validate_count_table(
        markdown,
        "### 結果別件数",
        &["結果", "件数"],
        |row| Ok(row[0]),
        &outcomes,
    )?;
    for id in retired_ids(markdown)? {
        let (prefix, _) = split_id(&id)?;
        if !groups.contains_key(prefix) {
            return Err(format!("unknown retired ID prefix: {id}"));
        }
        if sources.contains_key(&id) {
            return Err(format!("reused retired ID: {id}"));
        }
    }
    Ok(())
}

pub fn retired_ids(markdown: &str) -> Result<BTreeSet<String>> {
    let mut lines = markdown.lines().skip_while(|line| *line != "## 欠番一覧");
    lines.next().ok_or("missing retired IDs section")?;
    let lines: Vec<_> = lines.take_while(|line| !line.starts_with('#')).collect();
    let mut i = 0;
    while i < lines.len() && !lines[i].starts_with('|') {
        i += 1;
    }
    let header = ["欠番ID", "移管先テスト"];
    if table_cells(
        lines.get(i).ok_or("missing retired IDs table")?,
        header.len(),
    )? != header
    {
        return Err("invalid retired IDs header".into());
    }
    i += 1;
    table_separator(&lines, &mut i, header.len())?;
    let mut retired = BTreeSet::new();
    while i < lines.len() && lines[i].starts_with('|') {
        let row = table_cells(lines[i], header.len())?;
        i += 1;
        if row[1].is_empty() {
            return Err("empty retired ID destination".into());
        }
        for token in row[0].split('・') {
            if let Some((start, end)) = token.split_once('〜') {
                let start = parse_retired_id(start)?;
                let end = parse_retired_id(end)?;
                let (start_prefix, start_num) = split_id(start)?;
                let (end_prefix, end_num) = split_id(end)?;
                if start_prefix != end_prefix || start_num >= end_num {
                    return Err(format!("invalid retired ID range: {token}"));
                }
                for n in start_num..=end_num {
                    let id = format!("{start_prefix}-{n:03}");
                    if !retired.insert(id.clone()) {
                        return Err(format!("duplicate retired ID: {id}"));
                    }
                }
            } else {
                let id = parse_retired_id(token)?;
                if !retired.insert(id.to_owned()) {
                    return Err(format!("duplicate retired ID: {id}"));
                }
            }
        }
    }
    if lines[i..].iter().any(|line| line.starts_with('|')) {
        return Err("unexpected table row after retired IDs table".into());
    }
    Ok(retired)
}

fn parse_retired_id(token: &str) -> Result<&str> {
    let id = token
        .strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .ok_or_else(|| format!("invalid retired ID token: {token}"))?;
    if !valid_id(id) {
        return Err(format!("invalid retired ID: {id}"));
    }
    Ok(id)
}

fn split_id(id: &str) -> Result<(&str, u16)> {
    let (prefix, num_str) = id
        .split_once('-')
        .ok_or_else(|| format!("invalid ID: {id}"))?;
    let num = num_str
        .parse::<u16>()
        .map_err(|_| format!("invalid ID number: {id}"))?;
    Ok((prefix, num))
}

pub fn next_ids(docs: &Path, target_prefix: Option<&str>) -> Result<Vec<String>> {
    let sources = load(docs)?;
    let index = fs::read_to_string(docs.join("spec-cases.md"))
        .map_err(|error| format!("spec-cases.md: {error}"))?;
    let retired = retired_ids(&index)?;
    let mut max_by_prefix = BTreeMap::<String, u16>::new();
    for id in sources.keys().chain(&retired) {
        let (prefix, num) = split_id(id)?;
        let entry = max_by_prefix.entry(prefix.to_owned()).or_default();
        if num > *entry {
            *entry = num;
        }
    }
    let format_next = |prefix: &str, max: u16| -> Result<String> {
        let next_num = max + 1;
        if next_num > 999 {
            return Err(format!("ID number overflow for prefix: {prefix}"));
        }
        Ok(format!("{prefix}-{next_num:03}"))
    };
    if let Some(prefix) = target_prefix {
        let max = max_by_prefix
            .get(prefix)
            .copied()
            .ok_or_else(|| format!("unknown prefix: {prefix}"))?;
        Ok(vec![format_next(prefix, max)?])
    } else {
        max_by_prefix
            .into_iter()
            .map(|(prefix, max)| format_next(&prefix, max))
            .collect()
    }
}

fn validate_count_table<'a>(
    markdown: &'a str,
    heading: &str,
    header: &[&str],
    key: impl Fn(&[&'a str]) -> Result<&'a str>,
    actual: &BTreeMap<&str, usize>,
) -> Result<()> {
    let mut lines = markdown.lines().skip_while(|line| *line != heading);
    lines.next().ok_or("missing inventory")?;
    let lines: Vec<_> = lines.take_while(|line| !line.starts_with('#')).collect();
    let mut i = 0;
    skip_blank(&lines, &mut i);
    if table_cells(lines.get(i).ok_or("missing inventory table")?, header.len())? != header {
        return Err("invalid inventory header".into());
    }
    i += 1;
    table_separator(&lines, &mut i, header.len())?;
    let mut declared = BTreeMap::new();
    let mut total = None;
    while i < lines.len() && lines[i].starts_with('|') {
        let row = table_cells(lines[i], header.len())?;
        i += 1;
        let count = number::<usize>(row[header.len() - 1])?;
        if row[0] == "合計" {
            if row[1..row.len() - 1].iter().any(|cell| !cell.is_empty()) {
                return Err("invalid inventory total".into());
            }
            unique(&mut total, count, "inventory total")?;
        } else if declared.insert(key(&row)?, count).is_some() {
            return Err("duplicate inventory category".into());
        }
    }
    if &declared != actual || total != Some(actual.values().sum()) {
        return Err("inventory and cases disagree".into());
    }
    Ok(())
}
