pub mod batch;
mod cases;
pub mod catalog;
mod expectations;
pub mod float_runtime;
mod markdown;
pub mod runner;
pub mod runtime_link;
mod timing;

pub use cases::extract;
pub use catalog::validate_inventory;
pub use expectations::warnings_match;

use std::{collections::BTreeMap, fs, path::Path};

type Result<T> = std::result::Result<T, String>;

#[derive(Debug)]
pub struct Source {
    pub bytes: Vec<u8>,
    pub rule: String,
    pub secondary_rules: Vec<String>,
    pub runs: Vec<Run>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Default,
    Warn,
    Deny,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Exit,
    CompileError,
    Panic,
}

impl Outcome {
    pub fn inventory_name(&self) -> &'static str {
        match self {
            Self::Exit => "受理",
            Self::CompileError => "コンパイルエラー",
            Self::Panic => "実行時パニック",
        }
    }
}

#[derive(Debug)]
pub struct Run {
    pub mode: Mode,
    pub result: Outcome,
    pub code: Option<u8>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stderr_prefix: Option<String>,
    pub warning_count: usize,
    pub diagnostic: Option<ExpectedDiagnostic>,
    pub panic_diagnostic: Option<ExpectedDiagnostic>,
    pub warnings: Vec<ExpectedDiagnostic>,
}

#[derive(Debug, PartialEq)]
pub struct ExpectedDiagnostic {
    pub rule: String,
    pub line: usize,
    pub column: usize,
}

pub fn write_sources(sources: &BTreeMap<String, Source>, directory: &Path) -> std::io::Result<()> {
    for (id, source) in sources {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(format!("{id}.suz")))?;
        std::io::Write::write_all(&mut file, &source.bytes)?;
    }
    Ok(())
}
