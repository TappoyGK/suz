use std::{path::Path, process::Command};

pub fn compiler() -> Command {
    let mut command = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()));
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime");
    command
        .arg("-I")
        .arg(&runtime)
        .arg(runtime.join("suz_runtime.c"))
        .arg(runtime.join("suz_text.c"))
        .arg(runtime.join("suz_parse.c"))
        .arg(runtime.join("suz_parse_float.c"))
        .arg(runtime.join("suz_parse_struct.c"))
        .arg(runtime.join("suz_env.c"))
        .arg("-lm");
    command
}
