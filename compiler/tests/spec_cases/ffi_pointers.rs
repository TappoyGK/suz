use crate::test_util::TemporaryDirectory;
use std::{fs, process::Command};

#[test]
fn c_pointer_calls_borrow_original_arena_values_in_argument_order() {
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("fixture.h"),
        r#"
#include <stdint.h>
struct sample { int32_t value; };
static inline int32_t read_sample(const struct sample *p) { return p->value; }
static inline void set_sample(struct sample *p, int32_t value) { p->value = value; }
static inline void set_native(int *p) { *p = 17; }
static inline void set_byte(uint8_t *p) { *p = 255; }
static inline void set_float(double *p) { *p = 1.5; }
static inline int32_t read_number(const int32_t *p) { return *p; }
static inline void set_number(int32_t *p) { *p = 6; }
"#,
    )
    .unwrap();
    fs::write(
        directory.0.join("main.suz"),
        r#"
extern "fixture.h" {
    struct sample as Sample { value: i32 }
    fn read_sample(p: *const Sample) -> i32
    fn set_sample(p: *Sample, value: i32) -> void
    fn set_native(p: *_int) -> void
    fn set_byte(p: *u8) -> void
    fn set_float(p: *f64) -> void
    fn read_number(p: *const i32) -> i32
    fn set_number(p: *i32) -> void
}
struct Holder { p: ptr Sample }
fn forward(p: ptr Sample) -> void { trusted { set_sample(p, 11) } }
arena fn make() -> ptr Sample { return new Sample { value: 0 } }
fn main() -> u8 {
    arena {
        fix ptr first = make()
        fix ptr second = make()
        var ptr selected = first
        var evaluations = 0
        trusted {
            set_sample(expr { evaluations += 1
                selected }, expr { selected = second
                evaluations += 1
                9 })
            if read_sample(first) != 9 || read_sample(second) != 0 { return 1 }
        }
        if evaluations != 2 || first.value != 9 { return 2 }
        fix holder = Holder { p: first }
        forward(holder.p)
        if first.value != 11 { return 3 }
        fix ptr optional: ptr? Sample = first
        let p = optional { trusted { set_sample(p, 12) } } else { return 4 }
        if first.value != 12 { return 5 }
        arena {
            fix ptr inner = make()
            forward(inner)
            trusted { set_sample(first, 13) }
            if inner.value != 11 { return 6 }
        }
        if first.value != 13 { return 7 }
        fix native: _int = 0
        fix ptr n = new native
        fix ptr byte = new (0 as u8)
        fix ptr floating = new 0.0
        fix ptr number = new (3 as i32)
        trusted {
            set_native(n)
            set_byte(byte)
            set_float(floating)
            set_number(number)
            if read_number(number) != 6 { return 8 }
            set_sample(make(), 4)
        }
        if .n != 17 || .byte != 255 || .floating != 1.5 { return 9 }
    }
    return 0
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suz"))
        .current_dir(&directory.0)
        .args(["run", "main.suz", "--include-dir", "."])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn c_pointer_headers_require_exact_pointees_and_const() {
    let directory = TemporaryDirectory::new();
    fs::write(directory.0.join("fixture.h"), "#include <stdint.h>\nstruct sample { int32_t value; };\nstruct other { int32_t value; };\nvoid take(const struct sample *);\nvoid mutate(struct sample *);\nvoid number(const int32_t *);\nvoid unrelated(struct other *);\n").unwrap();
    for (signature, accepted) in [
        ("fn take(p: *const Sample) -> void", true),
        ("fn take(p: *Sample) -> void", false),
        ("fn mutate(p: *Sample) -> void", true),
        ("fn mutate(p: *const Sample) -> void", false),
        ("fn number(p: *const i32) -> void", true),
        ("fn number(p: *const u32) -> void", false),
        ("fn number(p: *i32) -> void", false),
        ("fn unrelated(p: *Sample) -> void", false),
    ] {
        let source = format!(
            "extern \"fixture.h\" {{\nstruct sample as Sample {{ value: i32 }}\n{signature}\n}}\nfn main() -> u8 {{ return 0 }}"
        );
        fs::write(
            directory.0.join("program.c"),
            suz::compile(source.as_bytes()).unwrap(),
        )
        .unwrap();
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
            .arg(directory.0.join("program.c"))
            .output()
            .unwrap();
        assert_eq!(output.status.success(), accepted, "{signature}: {output:?}");
        if !accepted {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("tool.ffi-header"),
                "{output:?}"
            );
        }
    }
}

#[test]
fn c_pointer_calls_reject_wrong_types_nullability_and_untrusted_calls() {
    let declarations = "struct Local { value: i32 }\nextern \"fixture.h\" {\nstruct sample as Sample { value: i32 }\nstruct other as Other { value: i32 }\nfn take(p: *const Sample) -> void\nfn number(p: *i32) -> void\n}";
    for (body, rule) in [
        (
            "fix ptr p = new Sample { value: 0 }\ntake(p)",
            "ffi.trusted",
        ),
        (
            "fix ptr p: ptr? Sample = new Sample { value: 0 }\ntrusted { take(p) }",
            "type.expected",
        ),
        ("trusted { take(null) }", "arena.null"),
        ("trusted { take(Sample { value: 0 }) }", "type.expected"),
        (
            "fix ptr p = new Local { value: 0 }\ntrusted { take(p) }",
            "type.expected",
        ),
        (
            "fix ptr p = new Other { value: 0 }\ntrusted { take(p) }",
            "type.expected",
        ),
        ("trusted { take(c_bytes `x`) }", "type.expected"),
        (
            "fix n: _int = 0\nfix ptr p = new n\ntrusted { number(p) }",
            "type.expected",
        ),
    ] {
        let source = format!("{declarations}\nfn main() -> u8 {{ arena {{ {body}\n}}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{body}: {error}");
    }
}

#[test]
fn typed_c_pointers_are_limited_to_supported_extern_parameters() {
    for ty in [
        "ptr i32", "*void", "*Local", "*bool", "*len_t", "*char", "*text", "*symbol", "*ptr i32",
        "**i32",
    ] {
        let source = format!(
            "struct Local {{ n: i32 }}\nextern \"f.h\" {{ fn take(p: {ty}) -> void }}\nfn main() -> u8 {{ return 0 }}"
        );
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
    for declaration in [
        "extern \"f.h\" { fn get() -> *const i32 }",
        "extern \"f.h\" { fn get() -> ptr i32 }",
        "struct Local { p: *const i32 }",
        "fn take(p: *const i32) -> void {}",
        "fn get() -> *const i32 {}",
        "fnshape Take(*const i32) -> void",
    ] {
        let source = format!("{declaration}\nfn main() -> u8 {{ return 0 }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}
