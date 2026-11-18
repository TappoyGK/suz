use std::{env, path::PathBuf, process::Command};

fn run(command: &mut Command) {
    let result = command.output().expect("could not start the C build tool");
    assert!(
        result.status.success(),
        "C formatter build failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn main() {
    let runtime = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../runtime");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let compiler = env::var_os("CC").unwrap_or_else(|| "clang".into());
    let archiver = env::var_os("AR").unwrap_or_else(|| "ar".into());
    let mut objects = Vec::new();
    for (index, source) in [
        "suz_format_float.c",
        "suz_parse.c",
        "suz_parse_float.c",
        "suz_parse_struct.c",
        "third_party/ryu/ryu/f2s.c",
        "third_party/ryu/ryu/d2s.c",
    ]
    .iter()
    .enumerate()
    {
        let object = output.join(format!("format-{index}.o"));
        run(Command::new(&compiler)
            .args([
                "-std=c23",
                "-O2",
                "-fhonor-nans",
                "-fhonor-infinities",
                "-ffp-contract=off",
                "-fdenormal-fp-math=ieee",
            ])
            .arg("-iquote")
            .arg(&runtime)
            .arg("-iquote")
            .arg(runtime.join("third_party/ryu"))
            .arg("-c")
            .arg(runtime.join(source))
            .arg("-o")
            .arg(&object));
        objects.push(object);
    }
    run(Command::new(archiver)
        .arg("crs")
        .arg(output.join("libsuz_format.a"))
        .args(objects));
    println!("cargo:rustc-link-search=native={}", output.display());
    println!("cargo:rustc-link-lib=static=suz_format");
    for input in [
        "suz_format_float.c",
        "suz_parse.c",
        "suz_parse_float.c",
        "suz_parse_struct.c",
        "suz_runtime.h",
        "suz_parse.h",
        "suz_char.h",
        "suz_parse_struct.h",
        "suz_config.h",
        "third_party/ryu/ryu",
    ] {
        println!("cargo:rerun-if-changed={}", runtime.join(input).display());
    }
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");
}
