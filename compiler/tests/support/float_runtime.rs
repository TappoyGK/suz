use std::{path::Path, process::Command};

pub fn required(source: &str) -> bool {
    [
        "suz_log_f32(",
        "suz_log_f64(",
        "suz_format_f32(",
        "suz_format_f64(",
    ]
    .iter()
    .any(|name| source.contains(name))
}

pub fn link(command: &mut Command, runtime: &Path) {
    command.arg("-iquote").arg(runtime.join("third_party/ryu"));
    for source in [
        "suz_float.c",
        "suz_format_float.c",
        "third_party/ryu/ryu/f2s.c",
        "third_party/ryu/ryu/d2s.c",
    ] {
        command.arg(runtime.join(source));
    }
}

pub fn parser_required(source: &str) -> bool {
    [
        "suz_parse_f32(",
        "suz_parse_f64(",
        "suz_parse_float_bits(",
        "suz_parse_struct(",
    ]
    .iter()
    .any(|name| source.contains(name))
}

pub fn struct_parser_required(source: &str) -> bool {
    source.contains("suz_parse_struct(")
}
