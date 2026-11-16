use super::{
    Outcome, Result, Source, extract,
    markdown::{number, skip_blank, table_cells, table_separator, unique},
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

// Both inventories use the same table contract; only headers and row keys vary.
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
    )
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
