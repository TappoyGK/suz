use super::{
    Outcome, Result, Source, extract,
    markdown::{cells, number, separator, skip_blank, table_cells, table_separator, unique},
};
use std::{collections::BTreeMap, fs, path::Path};

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

// The inventory declares the groups and counts; no group names or case counts live here.
pub fn validate_inventory(markdown: &str, sources: &BTreeMap<String, Source>) -> Result<()> {
    let mut actual = BTreeMap::<&str, usize>::new();
    for id in sources.keys() {
        *actual.entry(id.split_once('-').unwrap().0).or_default() += 1;
    }
    let mut declared = BTreeMap::new();
    let mut total = None;
    let mut lines = markdown.lines().skip_while(|line| *line != "## 目次と件数");
    lines.next().ok_or("missing inventory")?;
    let lines: Vec<_> = lines.take_while(|line| !line.starts_with("## ")).collect();
    let mut i = 0;
    skip_blank(&lines, &mut i);
    if cells(lines.get(i).ok_or("missing inventory table")?)?
        != ["分野", "ID接頭辞", "対応する仕様", "件数"]
    {
        return Err("invalid inventory header".into());
    }
    i += 1;
    separator(&lines, &mut i)?;
    while i < lines.len() && lines[i].starts_with('|') {
        let row = cells(lines[i])?;
        i += 1;
        let count = number::<usize>(row[3])?;
        if row[0] == "合計" {
            if !row[1].is_empty() || !row[2].is_empty() {
                return Err("invalid inventory total".into());
            }
            unique(&mut total, count, "inventory total")?;
        } else {
            let prefix = row[1]
                .strip_prefix('`')
                .and_then(|s| s.strip_suffix('`'))
                .ok_or("invalid inventory prefix")?;
            if declared.insert(prefix, count).is_some() {
                return Err("duplicate inventory prefix".into());
            }
        }
    }
    if declared != actual || total != Some(sources.len()) {
        return Err("inventory and cases disagree".into());
    }
    validate_outcome_inventory(markdown, sources)
}

fn validate_outcome_inventory(markdown: &str, sources: &BTreeMap<String, Source>) -> Result<()> {
    let mut actual = BTreeMap::from([
        ("受理", 0),
        ("コンパイルエラー", 0),
        ("実行時パニック", 0),
        ("診断設定の比較", 0),
    ]);
    for source in sources.values() {
        let category = if source.runs.len() > 1 {
            "診断設定の比較"
        } else {
            match source.runs[0].result {
                Outcome::Exit => "受理",
                Outcome::CompileError => "コンパイルエラー",
                Outcome::Panic => "実行時パニック",
            }
        };
        *actual.get_mut(category).unwrap() += 1;
    }
    let mut lines = markdown
        .lines()
        .skip_while(|line| *line != "### 結果別件数");
    lines.next().ok_or("missing outcome inventory")?;
    let lines: Vec<_> = lines.take_while(|line| !line.starts_with('#')).collect();
    let mut i = 0;
    skip_blank(&lines, &mut i);
    if table_cells(lines.get(i).ok_or("missing outcome inventory table")?, 2)? != ["結果", "件数"]
    {
        return Err("invalid outcome inventory header".into());
    }
    i += 1;
    table_separator(&lines, &mut i, 2)?;
    let mut declared = BTreeMap::new();
    let mut total = None;
    while i < lines.len() && lines[i].starts_with('|') {
        let row = table_cells(lines[i], 2)?;
        i += 1;
        let count = number::<usize>(row[1])?;
        if row[0] == "合計" {
            unique(&mut total, count, "outcome inventory total")?;
        } else if declared.insert(row[0], count).is_some() {
            return Err("duplicate outcome inventory category".into());
        }
    }
    if declared != actual || total != Some(sources.len()) {
        return Err("outcome inventory and cases disagree".into());
    }
    Ok(())
}
