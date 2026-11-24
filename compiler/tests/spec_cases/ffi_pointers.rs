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
struct Holder { p: mut Sample }
fn forward(p: mut Sample) -> void { trusted { set_sample(p, 11) } }
fn make() -> mut Sample { return new Sample { value: 0 } }
fn main() -> u8 {
    arena {
        fix first = make()
        fix second = make()
        var selected = first
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
        fix optional: mut? Sample = first
        let p = optional { trusted { set_sample(p, 12) } } else { return 4 }
        if first.value != 12 { return 5 }
        arena {
            fix inner = make()
            forward(inner)
            trusted { set_sample(first, 13) }
            if inner.value != 11 { return 6 }
        }
        if first.value != 13 { return 7 }
        fix native: _int = 0
        fix n = new native
        fix byte = new (0 as u8)
        fix floating = new 0.0
        fix number = new (3 as i32)
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
        ("fix p = new Sample { value: 0 }\ntake(p)", "ffi.trusted"),
        (
            "fix p: mut? Sample = new Sample { value: 0 }\ntrusted { take(p) }",
            "type.expected",
        ),
        ("trusted { take(nil) }", "arena.nil"),
        ("trusted { take(Sample { value: 0 }) }", "type.expected"),
        (
            "fix p = new Local { value: 0 }\ntrusted { take(p) }",
            "type.expected",
        ),
        (
            "fix p = new Other { value: 0 }\ntrusted { take(p) }",
            "type.expected",
        ),
        ("trusted { take(c_bytes `x`) }", "type.expected"),
        (
            "fix n: _int = 0\nfix p = new n\ntrusted { number(p) }",
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
        "mut i32", "*void", "*Local", "*bool", "*len_t", "*char", "*text", "*symbol", "*mut i32",
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

#[test]
fn keep_allows_registration_until_release_inside_an_explicit_arena() {
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("fixture.h"),
        r#"
#include <stdint.h>
struct state { int32_t value; };
static struct state *saved;
static const double *reading;
static inline void attach(struct state *p, const double *q) { saved = p; reading = q; }
static inline void tick(void) { saved->value += 1; }
static inline double read_saved(void) { return *reading; }
static inline void detach(void) { saved = 0; reading = 0; }
static inline int active(void) { return saved != 0 || reading != 0; }
"#,
    )
    .unwrap();
    fs::write(
        directory.0.join("main.suz"),
        r#"
extern "fixture.h" {
    struct state as State { value: i32 }
    fn attach(keep p: *State, keep q: *const f64) -> void
    fn tick() -> void
    fn read_saved() -> f64
    fn detach() -> void
    fn active() -> _int
}
fn register(p: mut State, q: mut f64) -> void { trusted { attach(p, q) } }
fnshape Action() -> void
fn invoke(f: fn Action) -> void { f() }
fn make() -> mut State { return new State { value: 0 } }
fn main() -> u8 {
    for i in 0..<2 {
        arena {
            fix p = make()
            fix q = new 2.5
            invoke(fn [p, q]() -> void { register(p, q) })
            // 登録した関数・クロージャ・trustedを抜けても、実体のArenaは生きている。
            trusted { tick() }
            if p.value != 1 { return 1 }
            #q = 3.5
            trusted {
                if read_saved() != 3.5 { return 2 }
                detach()
            }
        }
        trusted { if active() != 0 { return 3 } }
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
fn keep_is_a_c_pointer_modifier_not_a_reserved_name() {
    // Cが保持するバイト列も、型付きポインタと同じく反復外へ残る実体として扱う。
    for (keep, warnings) in [("keep ", 0), ("", 1)] {
        let source = format!(
            "extern \"retain.h\" {{ fn retain({keep}bytes: *const void) -> void }}\nfn main() -> u8 {{ for i in 0..<2 {{ fix t = textbuf `held`\ntrusted {{ retain(c_bytes t) }} }}\nreturn 0 }}"
        );
        let compiled = suz::compile_with_source_name(
            source.as_bytes(),
            suz::ArithmeticMode::Default,
            "keep.suz",
        )
        .unwrap();
        assert_eq!(compiled.warnings.len(), warnings);
        if let Some(warning) = compiled.warnings.first() {
            assert_eq!(warning.rule(), "arena.loop-allocation");
        }
    }
    for ty in ["i32", "_int", "f64", "bool", "State"] {
        let source = format!(
            "extern \"f.h\" {{\nstruct state as State {{ value: i32 }}\nfn attach(keep p: {ty}) -> void\n}}\nfn main() -> u8 {{ return 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "ffi.keep", "{ty}: {error}");
    }
    // 修飾語と仮引数名が同じ綴りでも、コロンの位置で区別する。
    suz::compile(b"extern \"f.h\" {\nfn scalar(keep: _int) -> void\nfn attach(keep keep: *i32) -> void\n}\nfn main() -> u8 { fix keep = 1\nreturn keep as u8 or 1 }").unwrap();
    for declaration in [
        "extern \"f.h\" { fn attach(keep keep p: *i32) -> void }",
        "extern \"f.h\" { fn attach(keep p: ptr i32) -> void }",
    ] {
        let source = format!("{declaration}\nfn main() -> u8 {{ return 0 }}");
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn keep_on_suz_parameters_explains_the_extern_only_modifier() {
    for declaration in [
        "fn wrapper(keep p: mut i32) -> void {}",
        "fn wrapper(keep p: mut i32) -> void {}",
        "fn wrapper() -> void { fix f = fn [](keep p: mut i32) -> void {} }",
    ] {
        let source = format!("{declaration}\nfn main() -> u8 {{ return 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), "ffi.keep", "{source}: {error}");
        assert!(
            error
                .to_string()
                .contains("only allowed on extern C pointer parameters")
        );
    }
    suz::compile(b"fn value(keep: i32) -> i32 { return keep }\nfn main() -> u8 { return value(0) as u8 or 1 }").unwrap();
}

#[test]
fn keep_preserves_call_type_trusted_and_suz_lifetime_checks() {
    for (body, rule) in [
        ("fix p = new (1 as i32)\nattach(p)", "ffi.trusted"),
        (
            "fix p: mut? i32 = new 1\ntrusted { attach(p) }",
            "type.expected",
        ),
        ("trusted { attach(nil) }", "arena.nil"),
        (
            "fix p = new (1 as u32)\ntrusted { attach(p) }",
            "type.expected",
        ),
        (
            "var p: mut? i32 = nil\ntrusted { arena { p = new (1 as i32)\nlet q = p { attach(q) } } }",
            "arena.store",
        ),
    ] {
        let source = format!(
            "extern \"f.h\" {{ fn attach(keep p: *i32) -> void }}\nfn main() -> u8 {{ arena {{ {body}\n}}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{body}: {error}");
    }
}

#[test]
fn c_const_pointer_accepts_ref_and_mut_while_mutable_c_pointer_rejects_ref() {
    let directory = TemporaryDirectory::new();
    fs::write(
        directory.0.join("fixture.h"),
        r#"
#include <stdint.h>
struct sample { int32_t value; };
static inline int32_t read_sample(const struct sample *p) { return p->value; }
static inline void set_sample(struct sample *p, int32_t value) { p->value = value; }
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
}
fn main() -> u8 {
    arena {
        fix m = new Sample { value: 5 }
        fix r: ref Sample = m
        fix ro: ref? Sample = r
        trusted {
            set_sample(m, 9)
            if read_sample(m) != 9 || read_sample(r) != 9 { return 1 }
            let q = ro {
                if read_sample(q) != 9 { return 2 }
            } else {
                return 3
            }
        }
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

    for (body, rule) in [
        (
            "fix r: ref Sample = new Sample { value: 1 }\ntrusted { set_sample(r, 2) }",
            "type.expected",
        ),
        (
            "fix r: ref? Sample = new Sample { value: 1 }\ntrusted { fix n = read_sample(r) }",
            "type.expected",
        ),
    ] {
        let source = format!(
            "extern \"fixture.h\" {{\nstruct sample as Sample {{ value: i32 }}\nfn read_sample(p: *const Sample) -> i32\nfn set_sample(p: *Sample, value: i32) -> void\n}}\nfn main() -> u8 {{ arena {{ {body} }}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert_eq!(error.rule(), rule, "{body}: {error}");
    }
}
