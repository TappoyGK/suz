use std::{path::Path, process::Command};

pub fn required(source: &str) -> bool {
    source.contains("suz_log_f32(") || source.contains("suz_log_f64(")
}

pub fn link(command: &mut Command, runtime: &Path) {
    command.arg("-I").arg(runtime.join("third_party/ryu"));
    for source in [
        "suz_float.c",
        "third_party/ryu/ryu/f2s.c",
        "third_party/ryu/ryu/d2s.c",
    ] {
        command.arg(runtime.join(source));
    }
}
