use super::{
    Result, Source, extract,
    markdown::{cells, number, separator, skip_blank, unique},
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
    Ok(())
}
