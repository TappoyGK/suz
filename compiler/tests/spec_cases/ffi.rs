use crate::test_util::{TemporaryDirectory, compile_and_run_c23};
use std::{fs, process::Command};

#[test]
fn ffi_calls_and_c_integer_operations() {
    let result = compile_and_run_c23(
        r#"
extern "stdlib.h" { fn abs(value: _int) -> _int }
extern "math.h" { fn log as natural_log(value: f64) -> f64 }
struct Boxed { value: _int }
fnshape Transform(_int) -> _int
fn magnitude(value: _int) -> _int { unsafe { return abs(value) } }
fn from_generic(value: int) -> _int { return value as _int or 0 }
fn to_generic(value: _int, fallback: int) -> int { return value as int or fallback }
fn main() -> u8 {
    if from_generic(7 as i8) != 7 { return 17 }
    fix generic: i8 = to_generic(from_generic(7 as u16), 0)
    if generic != 7 { return 18 }
    fix wrapper: static fn Transform = fn magnitude
    fix value = wrapper(-7)
    fix box = Boxed { value: value }
    if box.value != 7 || 6 >= value { return 1 }
    fix converted = value as i32 or return 2
    if converted != 7 { return 3 }
    unsafe {
        if natural_log(1.0) != 0.0 { return 4 }
        { if abs(-2) != 2 { return 5 } }
    }
    fix small: _char = 65
    if small != 65 { return 6 }
    fix unsigned = -1 as% _uint
    if unsigned <= 0 { return 7 }
    if (unsigned as i8 or 9) != 9 { return 8 }
    fix bits: u64 = 18446744073709551615
    fix signed = bits as% _long
    if signed != -1 { return 9 }
    if (signed as i64 or 0) != -1 { return 10 }
    if (signed as% u8) != 255 { return 11 }
    fix zero = -0.75 as _size_t or 2
    if zero != 0 { return 12 }
    fix truncated = -2147483648.5 as _int or 0
    if truncated != -2147483648 { return 13 }
    fix invalid = nan as _int or 13
    if invalid != 13 { return 14 }
    fix inf_value = inf as _ulong or 14
    if inf_value != 14 { return 15 }
    fix floated = value as f64
    if floated != 7.0 { return 16 }
    return 0
}
"#,
        "ffi.suz",
    );
    assert_eq!(result.status.code(), Some(0), "{result:?}");
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
}

#[test]
fn ffi_rejects_unsafe_escape_and_unsupported_operations() {
    let declaration = "extern \"stdlib.h\" { fn abs(value: _int) -> _int }\n";
    for body in [
        "fix x = abs(-1)",
        "unsafe { fix callback = fn abs }",
        "fix callback = fn abs",
        "unsafe { }\nfix x = abs(-1)",
        "fix x: _int = 1\nfix y = x + 1",
        "fix x: _int = 1\nfix y = x & x",
        "fix x: _int = 1\nlog info `${x}`",
        "fix x: _int = 1\nfix y: _long = 1\nfix z = x == y",
        "fix x: _char = 'a'",
        "fix x: _char = 65\nfix y = x == 'a'",
        "fix x: _int = 1\nfix y = x as i32",
        "fix x: _int = 1\nfix y = x as% f64",
        "fix x: _int = 1\nfix y = x as% len_t",
        "fix x: _int = 1\nfix y = x as bool or false",
        "fix x = `1` as _int or 0",
        "fix abs = 1",
    ] {
        let source = format!("{declaration}fn main() -> u8 {{\n{body}\nreturn 0\n}}");
        assert!(
            suz::compile(source.as_bytes()).is_err(),
            "accepted: {source}"
        );
    }
    let source = format!(
        "{declaration}fn helper() -> _int {{ return abs(-1) }}\nfn main() -> u8 {{ unsafe {{ fix x = helper() }}\nreturn 0 }}"
    );
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.message().contains("ffi.unsafe"), "{error:?}");
    let closure = format!(
        "{declaration}fnshape Action() -> _int\nfn apply(action: fn Action) -> _int {{ return action() }}\nfn main() -> u8 {{\nunsafe {{ fix x = apply(fn []() -> _int {{ return abs(-1) }}) }}\nreturn 0\n}}"
    );
    let error = suz::compile(closure.as_bytes()).unwrap_err();
    assert!(error.message().contains("ffi.unsafe"), "{error:?}");
}

#[test]
fn ffi_rejects_invalid_declarations() {
    for declaration in [
        "extern \"stdlib.h\" { fn abs(value: _int) -> _int { return value } }",
        "extern \"stdlib.h\" { fn abs(value: _int) -> _int or Bad }",
        "extern \"stdlib.h\" { fn abs(value: text) -> _int }",
        "extern \"stdlib.h\" { fn abs(value: len_t) -> _int }",
        "extern \"stdlib.h\" { fn abs(value: char) -> _int }",
        "extern \"stdlib.h\" { fn abs(value: int) -> _int }",
        "extern \"stdlib.h\" { fn abs(value: _int) -> text }",
        "extern \"math.h\" { fn log(value: f64) -> f64 }",
        "extern \"stdlib.h\" { fn abs as main(value: _int) -> _int }",
        "extern \"../stdlib.h\" { fn abs(value: _int) -> _int }",
        "extern \"stdlib.h\" { fn abs(value: _int) -> _int\nfn abs as magnitude(value: _int) -> _int }",
        "extern \"stdlib.h\" { fn abs(value: _int) -> _int }\nextern \"other.h\" { fn abs as other(value: _int) -> _int }",
        "extern \"stdlib.h\" { fn abs(value: _int) -> _int }\nfn abs(value: _int) -> _int { return value }",
        "extern \"stdlib.h\" { fn abs(value: _int) -> _int }\nimpl fn abs { return value }",
    ] {
        let source = format!("{declaration}\nfn main() -> u8 {{ return 0 }}");
        assert!(
            suz::compile(source.as_bytes()).is_err(),
            "accepted: {source}"
        );
    }
}

#[test]
fn ffi_header_checks_exact_types_even_for_unused_declarations() {
    let directory = TemporaryDirectory::new();
    // この検査はSuz型検査だけでは成立しない。実際のCヘッダーとの型互換性を検証する。
    fs::write(directory.0.join("fixture.h"), "#include <stdint.h>\n#include <stddef.h>\nint exact(int);\nint variadic(int, ...);\n#define macro(x) (x)\n").unwrap();
    for (signature, success) in [
        ("fn exact(value: _int) -> _int", true),
        ("fn exact() -> _int", false),
        ("fn exact(value: _long) -> _int", false),
        ("fn exact(value: _int) -> _long", false),
        ("fn variadic(value: _int) -> _int", false),
        ("fn macro(value: _int) -> _int", false),
    ] {
        let source =
            format!("extern \"fixture.h\" {{ {signature} }}\nfn main() -> u8 {{ return 0 }}");
        let c = suz::compile(source.as_bytes()).unwrap();
        let path = directory.0.join("program.c");
        fs::write(&path, c).unwrap();
        let output = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
            .args([
                "-std=c23",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
                "-I",
            ])
            .arg(&directory.0)
            .arg("-I")
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime"))
            .arg(path)
            .output()
            .unwrap();
        assert_eq!(output.status.success(), success, "{signature}: {output:?}");
        if !success {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("ffi.header-type"),
                "{output:?}"
            );
        }
    }
}

#[test]
fn ffi_scalar_abi_and_argument_evaluation() {
    let directory = TemporaryDirectory::new();
    let mappings = [
        ("i8", "int8_t", "-128"),
        ("i16", "int16_t", "-32768"),
        ("i32", "int32_t", "-2147483648"),
        ("i64", "int64_t", "-9223372036854775808"),
        ("u8", "uint8_t", "255"),
        ("u16", "uint16_t", "65535"),
        ("u32", "uint32_t", "4294967295"),
        ("u64", "uint64_t", "18446744073709551615"),
        ("f32", "float", "1.5"),
        ("f64", "double", "-2.5"),
        ("bool", "_Bool", "true"),
        ("_int", "int", "-1"),
        ("_uint", "unsigned int", "1"),
        ("_long", "long", "-1"),
        ("_ulong", "unsigned long", "1"),
        ("_size_t", "size_t", "1"),
        ("_char", "char", "65"),
    ];
    let mut header = String::from("#include <stdint.h>\n#include <stddef.h>\n");
    let mut source = String::from("extern \"fixture.h\" {\n");
    let mut body = String::new();
    for (index, (suz, c, value)) in mappings.iter().enumerate() {
        header.push_str(&format!(
            "static inline {c} echo{index}({c} value) {{ return value; }}\n"
        ));
        source.push_str(&format!("fn echo{index}(value: {suz}) -> {suz}\n"));
        body.push_str(&format!(
            "if echo{index}({value}) != {value} {{ return 1 }}\n"
        ));
    }
    header.push_str("static int count;\nstatic inline int next(void) { return ++count; }\nstatic inline int ordered(int a, int b) { return a == 1 && b == 2 && count == 2; }\nstatic inline void clear(void) { count = 0; }\n");
    source.push_str("fn next() -> _int\nfn ordered(a: _int, b: _int) -> _int\nfn clear() -> void\n}\nfn main() -> u8 {\nunsafe {\nclear()\nif ordered(next(), next()) != 1 { return 2 }\n");
    source.push_str(&body);
    source.push_str("}\nreturn 0\n}\n");
    fs::write(directory.0.join("fixture.h"), header).unwrap();
    let path = directory.0.join("program.c");
    fs::write(&path, suz::compile(source.as_bytes()).unwrap()).unwrap();
    let executable = directory.0.join("program");
    let built = crate::support::runtime_link::compiler()
        .args(["-std=c23", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(&directory.0)
        .arg(&path)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(built.status.success(), "{built:?}");
    let result = Command::new(executable).output().unwrap();
    assert_eq!(result.status.code(), Some(0), "{result:?}");
}

#[test]
fn ffi_literals_check_the_target_c_range() {
    // Cのcharは符号にかかわらず8ビット、unsigned intに負数は入らない。
    // 値を切り詰めてCへ渡す実装を検出する。
    for (ty, value) in [("_char", "256"), ("_uint", "-1")] {
        let source = format!("fn main() -> u8 {{\nfix x: {ty} = {value}\nreturn 0\n}}");
        let result = compile_and_run_c23(&source, "ffi-range.suz");
        assert_eq!(result.status.code(), Some(101), "{result:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("ffi.literal-range"),
            "{result:?}"
        );
    }
}
