use crate::test_util::TemporaryDirectory;
use std::{fs, path::Path, process::Command};

#[test]
fn arena_runtime_aligns_rewinds_and_checks_before_mutation() {
    let directory = TemporaryDirectory::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let executable = directory.0.join("arena");
    let built = Command::new(std::env::var_os("CC").unwrap_or_else(|| "clang".into()))
        .args([
            "-std=c23",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fsanitize=undefined",
            "-fno-sanitize-recover=all",
            "-iquote",
        ])
        .arg(root.join("runtime"))
        .arg(root.join("compiler/tests/spec_cases/arena_runtime.c"))
        .arg(root.join("runtime/suz_arena.c"))
        .arg(root.join("runtime/suz_runtime.c"))
        .args(["-lm", "-o"])
        .arg(&executable)
        .output()
        .unwrap();
    assert!(built.status.success(), "{built:?}");
    for (mode, code, message) in [
        ("success", 0, ""),
        (
            "capacity",
            101,
            "arena.suz:8:5: arena capacity exceeded: limit=9, used=1, additional=9 [rt.arena-allocation]",
        ),
        (
            "padding-overflow",
            101,
            "size calculation overflow [rt.arena-allocation]",
        ),
        (
            "end-overflow",
            101,
            "size calculation overflow [rt.arena-allocation]",
        ),
        (
            "init-failure",
            101,
            "arena.suz:4:2: arena allocation failed: requested=128 [rt.arena-storage]",
        ),
    ] {
        let output = Command::new(&executable).arg(mode).output().unwrap();
        assert_eq!(output.status.code(), Some(code), "{mode}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "{mode}: {output:?}"
        );
    }
}

#[test]
fn arena_capacity_cli_and_export_preserve_setting_without_allocating() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("main.suz");
    fs::write(&source, "fn main() -> u8 { return 0 }").unwrap();
    let binary = env!("CARGO_BIN_EXE_suz");
    for (index, value, bytes) in [
        (0, None, 1_073_741_824_u64),
        (1, Some("1"), 1),
        (2, Some("2K"), 2048),
        (3, Some("3M"), 3_145_728),
        (4, Some("2G"), 2_147_483_648),
        (5, Some("18446744073709551615"), u64::MAX),
    ] {
        let output_dir = directory.0.join(format!("export-{index}"));
        let mut command = Command::new(binary);
        command
            .arg("emit-c")
            .arg(&source)
            .arg("-o")
            .arg(&output_dir);
        if let Some(value) = value {
            command.args(["--arena-limit", value]);
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        let program = output_dir.join("program.c");
        let generated = fs::read_to_string(&program).unwrap();
        assert!(generated.contains(&format!("#define SUZ_ARENA_LIMIT UINT64_C({bytes})")));
        assert!(!generated.contains("suz_arena_init("));
        if index == 2 {
            // 構文が未公開の段階でも、出力物だけでランタイムと設定を使えることを確かめる。
            let harness = format!(
                r#"#define main unused_suz_main
{generated}
#undef main
#include "suz_arena.h"
int main(void) {{ suz_arena_init(SUZ_ARENA_LIMIT, "test", 1, 1); return suz_arena.capacity == 2048 ? 0 : 1; }}
"#
            );
            fs::write(&program, harness).unwrap();
            let executable = directory.0.join("exported");
            let built = Command::new("bash")
                .arg(output_dir.join("scripts/build-c.sh"))
                .arg(&program)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(built.status.success(), "{built:?}");
            assert!(Command::new(executable).status().unwrap().success());
        }
    }
    // 未使用のarenaが大容量を確保しないことを、確保を一切許さない環境層で検査する。
    let environment = directory.0.join("no_alloc.c");
    fs::write(
        &environment,
        r#"#include "suz_env.h"
#include <stdlib.h>
void *suz_env_alloc(size_t n) { (void)n; _Exit(90); }
void suz_env_free(void *p) { (void)p; _Exit(91); }
void suz_env_log(suz_log_level l, const char *s, size_t n) { (void)l; (void)s; (void)n; }
_Noreturn void suz_env_panic(void) { _Exit(101); }
"#,
    )
    .unwrap();
    for verb in ["build", "run"] {
        let executable = directory.0.join("app");
        let mut command = Command::new(binary);
        command
            .arg(verb)
            .arg(&source)
            .args(["--arena-limit", "18446744073709551615"])
            .arg("--env")
            .arg(&environment);
        if verb == "build" {
            command.arg("-o").arg(&executable);
        }
        let result = command.output().unwrap();
        assert!(result.status.success(), "{verb}: {result:?}");
        if verb == "build" {
            assert!(Command::new(executable).status().unwrap().success());
        }
    }
}

#[test]
fn arena_capacity_rejects_invalid_and_duplicate_values() {
    let binary = env!("CARGO_BIN_EXE_suz");
    for verb in ["build", "run", "emit-c"] {
        for value in [
            "",
            "0",
            "0K",
            "-1",
            "+1",
            "1k",
            "1.5M",
            "unlimited",
            "1Gx",
            "18446744073709551616",
            "17179869184G",
        ] {
            let result = Command::new(binary)
                .args([verb, "missing.suz", "--arena-limit", value])
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(2), "{verb} {value}: {result:?}");
            assert!(String::from_utf8_lossy(&result.stderr).contains("[tool.arena-capacity]"));
        }
        for tail in [
            vec!["--arena-limit"],
            vec!["--arena-limit", "1", "--arena-limit", "2"],
        ] {
            let result = Command::new(binary)
                .args([verb, "missing.suz"])
                .args(tail)
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(2));
            assert!(String::from_utf8_lossy(&result.stderr).contains("[tool.arena-capacity]"));
        }
    }
    let result = Command::new(binary)
        .args(["check", "missing.suz", "--arena-limit", "1"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("[tool.arguments]"));
}

#[test]
fn arena_pointer_updates_cycles_snapshots_and_nested_lifetimes() {
    let result = crate::test_util::compile_and_run_c23(
        r#"
struct Node { value: i32, next: ptr Node }
struct Holder { node: ptr Node }
fn main() -> u8 {
    arena {
        fix ptr a = new Node { value: 1, next: null }
        fix ptr b = new Node { value: 2, next: a }
        a.next = b
        b.value += 3
        if let next = a.next {
            if next.value != 5 { return 1 }
            next.value = 6
        } else { return 2 }
        var ptr selected: ptr Node = a
        if let snapshot = selected {
            selected = b
            snapshot.value = 7
        }
        if a.value != 7 || b.value != 6 { return 3 }
        fix ptr count = new (3 as i32)
        *count += 4
        if *count != 7 { return 4 }
        fix ptr indirect = new count
        if let inner = *indirect { *inner = 9 }
        if *count != 9 { return 5 }
        var holder = Holder { node: null }
        arena {
            fix ptr alias = a
            holder = Holder { node: alias }
            fix ptr box = new Holder { node: a }
            if let copied = box.node { holder.node = copied }
        }
        if let saved = holder.node { if saved.value != 7 { return 6 } }
        else { return 7 }
    }
    return 0
}
"#,
        "arena.suz",
    );
    assert_eq!(result.status.code(), Some(0), "{result:?}");
}

#[test]
fn arena_rejects_escaping_references_through_aliases_and_copies() {
    for body in [
        "var ptr saved: ptr Node = null\narena { saved = new Node { next: null } }",
        "arena { fix ptr outer = new Node { next: null }\narena { fix ptr alias = outer\nalias.next = new Node { next: null } } }",
        "arena { fix ptr outer = new Node { next: null }\narena { fix ptr inner = new Node { next: null }\n*outer = Node { next: inner } } }",
        "arena { var box = Box { node: null }\narena { fix ptr inner = new Node { next: null }\nfix copy = Box { node: inner }\nbox = copy } }",
        "var ptr saved: ptr Node = null\nfor i in 0..<3 { arena { saved = new Node { next: null } } }",
    ] {
        let source = format!(
            "struct Node {{ next: ptr Node }}\nstruct Box {{ node: ptr Node }}\nfn main() -> u8 {{ {body}\nreturn 0 }}"
        );
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("[arena.store]"),
            "{body}: {error}"
        );
        assert_eq!(error.related.len(), 2, "{error}");
    }
}

#[test]
fn arena_rejects_invalid_pointer_operations_and_function_boundaries() {
    for (body, rule) in [
        ("fix ptr p = new 1", "arena.new"),
        ("fix ptr p = null", "arena.null"),
        ("arena { fix p = new 1 }", "arena.ptr-decl"),
        ("fix ptr p = 1", "arena.ptr-decl"),
        (
            "arena { var ptr p = new 1\nfix n = *p }",
            "arena.null-deref",
        ),
        (
            "arena { fix ptr p: ptr i32 = null\nfix n = *p }",
            "arena.null-deref",
        ),
        ("arena { fix ptr p = new 1\np = new 2 }", "var.fix-assign"),
    ] {
        let source = format!("fn main() -> u8 {{ {body}\nreturn 0 }}");
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains(&format!("[{rule}]")),
            "{body}: {error}"
        );
    }
    for source in [
        "fn read(p: ptr i32) -> i32 { return 0 }\nfn main() -> u8 { return 0 }",
        "fn make() -> ptr i32 { arena { return new 1 } }\nfn main() -> u8 { return 0 }",
        "struct Box { p: ptr i32 }\nfn read(b: Box) -> i32 { return 0 }\nfn main() -> u8 { return 0 }",
    ] {
        let error = suz::compile(source.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("[arena.parameter]"), "{error}");
    }
}

#[test]
fn arena_rewinds_on_return_failure_recovery_and_loop_exits() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("exits.suz");
    fs::write(
        &source,
        r#"
error Err { Bad }
fn returned() -> i64 {
    arena { fix ptr p = new (13 as i64)
        arena { fix ptr q = new (17 as i64)
            return *p + *q
        }
    }
}
fn failed() -> i64 or Err {
    arena { fix ptr p = new (11 as i64)
        fail Err.Bad
    }
}
fn forwarded() -> i64 or Err {
    arena { fix ptr p = new (19 as i64)
        fix n = failed() or fail
        return n
    }
}
fn early() -> i64 {
    arena { fix ptr p = new (23 as i64)
        fix n = failed() or return *p
        return n
    }
}
fn main() -> u8 {
    arena {
        fix ptr sentinel = new (99 as i64)
        for i in 0..<1000 {
            if returned() != 30 { return 1 }
            if early() != 23 { return 2 }
            fix result = forwarded() or error {
                _ => { arena {
                    fix ptr recovered = new (31 as i64)
                    recover *recovered
                } }
            }
            if result != 31 { return 3 }
            loop { arena {
                fix ptr p = new (37 as i64)
                break
            } }
            arena { fix ptr p = new (41 as i64)
                if i < 999 { continue }
            }
        }
        if *sentinel != 99 { return 4 }
        // このループからの脱出では、外側のarenaを回収してはならない。
        loop { break }
        fix ptr second = new (43 as i64)
        if *sentinel != 99 { return 5 }
    }
    return 0
}
"#,
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_suz"))
        .arg("run")
        .arg(&source)
        .args(["--arena-limit", "64"])
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn arena_evaluates_initializers_once_and_supports_value_blocks() {
    let result = crate::test_util::compile_and_run_c23(
        r#"
struct Pair { value: i32, link: ptr i32 }
fn generic(value: int) -> int { arena { fix ptr p = new value
    return *p } }
fn main() -> u8 {
    arena {
        var calls: i32 = 0
        fix ptr pair = new Pair {
            value: expr { calls += 1
                calls },
            link: new expr { calls += 1
                calls },
        }
        if calls != 2 || pair.value != 1 { return 1 }
        if let p = pair.link { if *p != 2 { return 2 } }
        fix ptr outer = expr { fix ptr inner = new (7 as i32)
            inner }
        if let p = outer { *p += expr { calls += 1
            3 } }
        if calls != 3 { return 3 }
        if let p = outer { if *p != 10 { return 4 } }
        if generic(5 as i16) != 5 { return 5 }
    }
    return 0
}
"#,
        "initializers.suz",
    );
    assert_eq!(result.status.code(), Some(0), "{result:?}");
}

#[test]
fn arena_rejects_pointer_conversions_captures_and_aggregate_operations() {
    for statement in [
        "fix n = p + 1",
        "fix n = p == p",
        "fix n = p as i64",
        "fix ptr q = 1 as ptr i32",
        "fix n = p[0]",
        "log info `${p}`",
        "fix n = Box { p: p } == Box { p: p }",
        "fix box = Box { p: p }\nlog info `${box}`",
        "fix box = `p: 0` as Box or Box { p: p }",
        "invoke(fn [p]() -> i32 { return 0 })",
        "fix box = Box { p: p }\ninvoke(fn [box]() -> i32 { return 0 })",
        "if let q = p { q = p }",
        "if let q = p {}\nfix n = *q",
        "if let q = 1 {}",
        "fix n = new nothing()",
    ] {
        let source = format!(
            "struct Box {{ p: ptr i32 }}\nfnshape Read() -> i32\nfn invoke(f: fn Read) -> i32 {{ return f() }}\nfn nothing() -> void {{ return }}\nfn main() -> u8 {{ arena {{ fix ptr p = new (1 as i32)\n{statement}\n}}\nreturn 0 }}"
        );
        assert!(suz::compile(source.as_bytes()).is_err(), "{statement}");
    }
    for source in [
        "struct Node { next: Node }\nfn main() -> u8 { return 0 }",
        "struct Node { next: ptr int }\nfn main() -> u8 { return 0 }",
        "fn main() -> u8 { fix ptr p: ptr int = null\nreturn 0 }",
        "fn main() -> u8 { fix ptr p: ptr void = null\nreturn 0 }",
        "fix Ptr: ptr i32 = null\nfn main() -> u8 { return 0 }",
        "extern \"x.h\" { fn take(p: ptr i32) -> void }\nfn main() -> u8 { return 0 }",
        "fnshape Read() -> i32\nfn invoke(f: fn Read) -> i32 { return f() }\nfn main() -> u8 { arena { invoke(fn []() -> i32 { fix ptr p = new 1\nreturn 0 }) }\nreturn 0 }",
    ] {
        assert!(suz::compile(source.as_bytes()).is_err(), "{source}");
    }
}

#[test]
fn arena_tracks_branch_union_field_loads_and_member_diagnostics() {
    let source = r#"struct Node { next: ptr Node }
struct Box { head: ptr Node }
fn main() -> u8 {
    arena {
        fix ptr outer = new Node { next: null }
        arena {
            fix ptr inner = new Node { next: null }
            fix ptr head = new Box { head: inner }
            fix copy = (*head) with { head: head.head }
            fix ptr selected = ifexpr true { outer } else { inner }
            if let p = selected {
                p.next = copy.head
            }
        }
    }
    return 0
}"#;
    let error = suz::compile(source.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("[arena.store]"), "{error}");
    assert_eq!(error.line, 12);
    assert!(error.message().contains("'next'"), "{error}");
    assert_eq!(error.related[0].line, 7);
    assert_eq!(error.related[1].line, 14);
}

#[test]
fn arena_pointer_compound_assignment_reads_before_its_rhs() {
    let result = crate::test_util::compile_and_run_c23(
        r#"
struct First { other: ptr Second }
struct Second { other: ptr First }
fn generic(value: int) -> int {
    arena { fix ptr p: ptr int = new value
        return *p }
}
fn main() -> u8 {
    arena {
        fix ptr first = new First { other: null }
        fix ptr second = new Second { other: first }
        first.other = second
        fix ptr n = new (7 as i32)
        *n += expr { *n = 20
            1 }
        if *n != 8 { return 1 }
        fix ptr flag = new true
        if not *flag { return 2 }
        fix ptr word = new `hello`
        if *word != `hello` { return 3 }
        if generic(5 as u16) != 5 { return 4 }
    }
    return 0
}
"#,
        "compound.suz",
    );
    assert_eq!(result.status.code(), Some(0), "{result:?}");
}
