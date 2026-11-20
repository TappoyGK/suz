use crate::{
    support::{batch::Batch, runner::worker_count},
    test_util::TemporaryDirectory,
};
use std::{fmt::Write as _, fs, path::PathBuf, process::Command};

const DEFAULT_SEED: u64 = 0x5EED_A12E_4A00_0001;
const DEFAULT_COUNT: usize = 200;
const ASAN_OPTIONS: &str = "detect_leaks=0:abort_on_error=0:exitcode=86";

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn pick(&mut self, limit: usize) -> usize {
        ((self.next_u64() >> 32) as usize) % limit
    }

    fn chance(&mut self, numerator: usize, denominator: usize) -> bool {
        self.pick(denominator) < numerator
    }
}

fn pick_opt_ptr(rng: &mut Rng, has_inner: bool) -> &'static str {
    if has_inner && rng.chance(1, 5) {
        let inner = ["inner_a", "inner_opt", "inner_holder.p"];
        return inner[rng.pick(inner.len())];
    }
    let outer = [
        "null",
        "outer_a",
        "outer_opt",
        "outer_mut",
        "outer_holder.p",
    ];
    outer[rng.pick(outer.len())]
}

fn pick_nonnull_ptr(rng: &mut Rng, has_inner: bool) -> &'static str {
    if has_inner && rng.chance(1, 5) {
        let inner = ["inner_a", "inner_holder.q"];
        return inner[rng.pick(inner.len())];
    }
    let outer = ["outer_a", "outer_mut", "outer_holder.q"];
    outer[rng.pick(outer.len())]
}

fn emit_inner_ops(out: &mut String, rng: &mut Rng, phase: usize, in_loop: bool, indent: &str) {
    let op_count = 1 + rng.pick(3);
    let loop_var = if in_loop {
        format!("i_{phase}")
    } else {
        format!("idx_{phase}")
    };
    let flag_var = format!("flag_{phase}");
    for op_idx in 0..op_count {
        let kind = rng.pick(15);
        match kind {
            0 => {
                let src = pick_opt_ptr(rng, true);
                let _ = writeln!(out, "{indent}outer_opt = {src}");
            }
            1 => {
                let src = pick_nonnull_ptr(rng, true);
                let _ = writeln!(out, "{indent}outer_mut = {src}");
            }
            2 => {
                let src = pick_opt_ptr(rng, true);
                let _ = writeln!(out, "{indent}inner_opt = {src}");
            }
            3 => {
                let src = pick_opt_ptr(rng, true);
                match rng.pick(4) {
                    0 => {
                        let _ = writeln!(out, "{indent}outer_holder#p = {src}");
                    }
                    1 => {
                        let _ = writeln!(out, "{indent}outer_pair#b = {src}");
                    }
                    2 => {
                        let _ = writeln!(out, "{indent}outer_mut#next = {src}");
                    }
                    _ => {
                        let _ = writeln!(out, "{indent}inner_a#next = {src}");
                    }
                }
            }
            4 => {
                let p = pick_opt_ptr(rng, true);
                let q = pick_nonnull_ptr(rng, true);
                if rng.chance(1, 2) {
                    let _ = writeln!(out, "{indent}#outer_mut = $Node {{ value: 42, next: {p} }}");
                } else {
                    let _ = writeln!(out, "{indent}outer_holder = $Holder {{ p: {p}, q: {q} }}");
                }
            }
            5 => {
                let p = pick_opt_ptr(rng, true);
                if rng.chance(1, 2) {
                    let _ = writeln!(out, "{indent}outer_holder = outer_holder with {{ p: {p} }}");
                } else {
                    let _ = writeln!(
                        out,
                        "{indent}#outer_mut = (.outer_mut) with {{ next: {p} }}"
                    );
                }
            }
            6 => {
                let p = pick_opt_ptr(rng, true);
                let copy_var = format!("copy_h_{phase}_{op_idx}");
                let _ = writeln!(out, "{indent}var {copy_var} = outer_holder");
                let _ = writeln!(out, "{indent}{copy_var}#p = {p}");
                if rng.chance(1, 2) {
                    let _ = writeln!(out, "{indent}outer_holder = {copy_var}");
                } else {
                    let _ = writeln!(out, "{indent}outer_pair#a = {copy_var}");
                }
            }
            7 => {
                let src = pick_opt_ptr(rng, true);
                let let_var = format!("narrowed_{phase}_{op_idx}");
                let _ = writeln!(
                    out,
                    "{indent}let {let_var} = {src} {{\n{indent}    outer_opt = {let_var}\n{indent}    {let_var}#value += 1\n{indent}}} else {{\n{indent}    outer_opt = null\n{indent}}}"
                );
            }
            8 => {
                let cond = if rng.chance(1, 2) {
                    flag_var.as_str()
                } else {
                    "outer_a.value == 10"
                };
                let a = pick_opt_ptr(rng, true);
                let b = pick_opt_ptr(rng, false);
                if rng.chance(1, 2) {
                    let _ = writeln!(
                        out,
                        "{indent}outer_opt = ifexpr {cond} {{ {a} }} else {{ {b} }}"
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "{indent}outer_opt = matchexpr ({loop_var} % 2) {{\n{indent}    0 => {a}\n{indent}    _ => {b}\n{indent}}}"
                    );
                }
            }
            9 => {
                let arg = pick_opt_ptr(rng, true);
                if rng.chance(1, 2) {
                    let _ = writeln!(
                        out,
                        "{indent}sink = apply_action(fn [inner_a](x: ptr? $Node) -> i32 {{ return borrow_read(x) + inner_a.value }}, {arg})"
                    );
                } else {
                    let _ = writeln!(
                        out,
                        "{indent}sink = apply_action(fn [var outer_opt](x: ptr? $Node) -> i32 {{\n{indent}    outer_opt = x\n{indent}    return borrow_read(outer_opt)\n{indent}}}, {arg})"
                    );
                }
            }
            10 => {
                let target = pick_nonnull_ptr(rng, true);
                let opt = pick_opt_ptr(rng, true);
                let _ = writeln!(out, "{indent}borrow_mutate({target})");
                let _ = writeln!(out, "{indent}sink = borrow_read({opt})");
                let _ = writeln!(out, "{indent}sink = borrow_holder(outer_holder)");
            }
            11 => {
                let tail = pick_opt_ptr(rng, true);
                if rng.chance(1, 2) {
                    let _ = writeln!(out, "{indent}outer_opt = build_chain(5, {tail}, 1)");
                } else {
                    let _ = writeln!(out, "{indent}outer_opt = maybe_alloc({flag_var}, {tail})");
                }
            }
            12 => {
                let q = pick_nonnull_ptr(rng, true);
                let h_var = format!("h_arg_{phase}_{op_idx}");
                let _ = writeln!(
                    out,
                    "{indent}fix ptr {h_var} = new $Holder {{ p: null, q: {q} }}"
                );
                let _ = writeln!(out, "{indent}attach({h_var})");
            }
            13 => {
                let _ = writeln!(out, "{indent}outer_holder#bump_q(1)");
                let _ = writeln!(out, "{indent}outer_mut#bump(1)");
                let _ = writeln!(out, "{indent}sink = outer_holder.read_p()");
            }
            _ => {
                let q = pick_nonnull_ptr(rng, true);
                let _ = writeln!(out, "{indent}#pp_outer = {q}");
            }
        }
    }
    if in_loop && rng.chance(1, 3) {
        if rng.chance(1, 2) {
            let _ = writeln!(out, "{indent}if {flag_var} {{ break }}");
        } else {
            let _ = writeln!(out, "{indent}if not {flag_var} {{ continue }}");
        }
    }
    let _ = writeln!(out, "{indent}inner_opt = inner_opt");
    let _ = writeln!(out, "{indent}inner_holder#p = inner_holder.p");
}

fn emit_post_rewind_access(out: &mut String, indent: &str) {
    let _ = writeln!(
        out,
        "{indent}let p_opt = outer_opt {{\n{indent}    fix v = p_opt.value\n{indent}    p_opt#value = v + 1\n{indent}}}"
    );
    let _ = writeln!(
        out,
        "{indent}let p_hp = outer_holder.p {{\n{indent}    fix v = p_hp.value\n{indent}    p_hp#value = v + 1\n{indent}}}"
    );
    let _ = writeln!(
        out,
        "{indent}let p_pa = outer_pair.a.p {{\n{indent}    fix v = p_pa.value\n{indent}    p_pa#value = v + 1\n{indent}}}"
    );
    let _ = writeln!(
        out,
        "{indent}let p_pb = outer_pair.b {{\n{indent}    fix v = p_pb.value\n{indent}    p_pb#value = v + 1\n{indent}}}"
    );
    let _ = writeln!(out, "{indent}sink = outer_mut.value");
    let _ = writeln!(out, "{indent}sink = (.pp_outer).value");
    let _ = writeln!(out, "{indent}sink = outer_holder.q.value");
    let _ = writeln!(
        out,
        "{indent}let p_next = outer_a.next {{ sink = p_next.value }}"
    );
}

fn generate_program(seed: u64, index: usize) -> String {
    let generic = !index.is_multiple_of(2);
    let mut rng = Rng(seed
        ^ ((index as u64)
            .wrapping_add(1)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)));
    // 共通の操作生成で、通常構造体と汎用構造体の両方を検査する。
    // 型引数を経由しても、別名・フィールド更新・回収後のアクセスの判定を変えない。
    let mut out = String::from(if generic {
        r#"struct Node[T] { value: T, next: ptr? Node[T] }
struct Holder[T] { p: ptr? Node[T], q: ptr Node[T] }
struct Pair[T] { a: Holder[T], b: ptr? Node[T] }
error ArenaErr { Abort }
fnshape NodeAction[T](ptr? Node[T]) -> i32
fn apply_generic[T](f: fn NodeAction[T], arg: ptr? Node[T]) -> i32 { return f(arg) }

fn Holder[T].read_p() -> T {
    let n = self.p { return n.value } else { return self.q.value }
}

@needs T: +(T) -> T
fn Holder[T]#bump_q(delta: T) -> void {
    self.q#value += delta
    return
}

@needs T: +(T) -> T
fn Node[T]#bump(delta: T) -> void {
    self#value += delta
    return
}

arena fn make_node[T](v: T, tail: ptr? Node[T]) -> ptr Node[T] {
    return new Node[T] { value: v, next: tail }
}

"#
    } else {
        r#"struct $Node { value: i32, next: ptr? $Node }
struct $Holder { p: ptr? $Node, q: ptr $Node }
struct $Pair { a: $Holder, b: ptr? $Node }
error ArenaErr { Abort }
fnshape NodeAction(ptr? $Node) -> i32

fn $Holder.read_p() -> i32 {
    let n = self.p { return n.value } else { return self.q.value }
}

fn $Holder#bump_q(delta: i32) -> void {
    self.q#value += delta
    return
}

fn $Node#bump(delta: i32) -> void {
    self#value += delta
    return
}

"#
    });
    out.push_str(
        r#"fn borrow_read(p: ptr? $Node) -> i32 {
    let n = p { return n.value } else { return 0 }
}

fn borrow_mutate(p: ptr $Node) -> void {
    p#value += 1
    return
}

fn borrow_holder(h: $Holder) -> i32 {
    let n = h.p { return n.value } else { return h.q.value }
}

fn apply_action(f: fn NodeAction, arg: ptr? $Node) -> i32 {
    return f(arg)
}

arena fn attach(h: ptr $Holder) -> void {
    h.q#next = new $Node { value: 7, next: null }
    return
}

arena fn build_chain(v: i32, tail: ptr? $Node, depth: i32) -> ptr $Node {
    if depth <= 0 {
        return new $Node { value: v, next: tail }
    }
    fix ptr next = build_chain(v + 1, tail, depth - 1)
    return new $Node { value: v, next: next }
}

arena fn maybe_alloc(flag: bool, fallback: ptr? $Node) -> ptr? $Node {
    if flag {
        return new $Node { value: 7, next: fallback }
    }
    return fallback
}

fn helper_return_inside_arena(outer_p: ptr $Node, flag: bool) -> i32 {
    arena {
        fix ptr local_n = new $Node { value: 99, next: outer_p }
        if flag {
            return borrow_read(local_n)
        }
    }
    return borrow_read(outer_p)
}

fn may_fail_inside_arena(outer_p: ptr $Node, do_fail: bool) -> i32 or ArenaErr {
    arena {
        fix ptr local_n = new $Node { value: 88, next: outer_p }
        if do_fail {
            fail ArenaErr.Abort
        }
        return borrow_read(local_n)
    }
}

fn main() -> u8 {
    arena {
        fix ptr outer_a = new $Node { value: 10, next: null }
        fix ptr pp_outer: ptr ptr $Node = new outer_a
        var ptr outer_opt: ptr? $Node = null
        var ptr outer_mut: ptr $Node = outer_a
        var outer_holder = $Holder { p: null, q: outer_a }
        var outer_pair = $Pair { a: outer_holder, b: null }
        var sink: i32 = 0
"#,
    );

    let phases = 1 + rng.pick(2);
    for phase in 0..phases {
        let flag = if rng.chance(1, 2) { "true" } else { "false" };
        let use_loop = rng.chance(1, 2);
        let _ = writeln!(out, "        fix flag_{phase} = {flag}");
        if use_loop {
            let _ = writeln!(out, "        for i_{phase} in 0..<2 {{");
            let _ = writeln!(out, "            arena {{");
            let _ = writeln!(
                out,
                "                fix ptr inner_a = new $Node {{ value: 20, next: null }}"
            );
            let _ = writeln!(
                out,
                "                var ptr inner_opt: ptr? $Node = new $Node {{ value: 30, next: null }}"
            );
            let _ = writeln!(
                out,
                "                var inner_holder = $Holder {{ p: inner_a, q: inner_a }}"
            );
            emit_inner_ops(&mut out, &mut rng, phase, true, "                ");
            let _ = writeln!(out, "            }}");
            emit_post_rewind_access(&mut out, "            ");
            let _ = writeln!(out, "        }}");
        } else {
            let _ = writeln!(out, "        fix idx_{phase}: i32 = {phase}");
            let _ = writeln!(out, "        arena {{");
            let _ = writeln!(
                out,
                "            fix ptr inner_a = new $Node {{ value: 20, next: null }}"
            );
            let _ = writeln!(
                out,
                "            var ptr inner_opt: ptr? $Node = new $Node {{ value: 30, next: null }}"
            );
            let _ = writeln!(
                out,
                "            var inner_holder = $Holder {{ p: inner_a, q: inner_a }}"
            );
            if rng.chance(1, 2) {
                let _ = writeln!(out, "            arena {{");
                emit_inner_ops(&mut out, &mut rng, phase, false, "                ");
                let _ = writeln!(out, "            }}");
            } else {
                emit_inner_ops(&mut out, &mut rng, phase, false, "            ");
            }
            let _ = writeln!(out, "        }}");
            emit_post_rewind_access(&mut out, "        ");
        }
        let _ = writeln!(
            out,
            "        sink = helper_return_inside_arena(outer_mut, flag_{phase})"
        );
        let _ = writeln!(
            out,
            "        sink = may_fail_inside_arena(outer_mut, flag_{phase}) or error {{ ArenaErr.Abort => {{ recover 0 }} }}"
        );
        if rng.chance(1, 2) {
            // 回収後の再利用も通常どおり動くことを確かめる。
            let _ = writeln!(out, "        arena {{");
            let _ = writeln!(
                out,
                "            fix ptr reused = new $Node {{ value: 55, next: outer_a }}"
            );
            let _ = writeln!(out, "            sink = borrow_read(reused)");
            let _ = writeln!(out, "        }}");
        }
    }

    out.push_str("        outer_opt = outer_opt\n        outer_mut = outer_mut\n        outer_holder = outer_holder\n        outer_pair = outer_pair\n        sink = sink\n    }\n    return 0\n}\n");
    if generic {
        // arena fnの具体化も通す。通常版と同じ値と参照を返す。
        // コールバックは型引数付きfnshapeと具体化した関数値を経由して転送する。
        out = out
            .replace("fn NodeAction,", "fn NodeAction[i32],")
            .replace(
                "    return f(arg)\n",
                "    return (fn apply_generic[i32])(f, arg)\n",
            )
            .replace(
                "new $Node { value: v, next: tail }",
                "make_node[i32](v, tail)",
            )
            .replace(
                "new $Node { value: v, next: next }",
                "make_node[i32](v, next)",
            );
    }
    // 汎用版の半数は型引数を推論し、明示した場合と同じ寿命検査・実行を通す。
    if generic && index % 4 == 3 {
        out = out.replace("make_node[i32]", "make_node");
    }
    for (placeholder, concrete, applied) in [
        ("$Node", "Node", "Node[i32]"),
        ("$Holder", "Holder", "Holder[i32]"),
        ("$Pair", "Pair", "Pair[i32]"),
    ] {
        out = out.replace(placeholder, if generic { applied } else { concrete });
    }
    out
}

fn save_failure(seed: u64, index: usize, source: &str, details: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join("suz-arena-fuzz-failures");
    let _ = fs::create_dir_all(&dir);
    let source_path = dir.join(format!("seed-{seed}-case-{index}.suz"));
    let report_path = dir.join(format!("seed-{seed}-case-{index}.txt"));
    let _ = fs::write(&source_path, source);
    let _ = fs::write(&report_path, details);
    (source_path, report_path)
}

fn env_u64(name: &str, default: u64) -> u64 {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<u64>()
            .unwrap_or_else(|e| panic!("invalid {name}={value:?}: {e}")),
        Err(_) => default,
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<usize>()
            .unwrap_or_else(|e| panic!("invalid {name}={value:?}: {e}")),
        Err(_) => default,
    }
}

#[test]
#[ignore = "release check: run scripts/test-release.sh"]
fn arena_fuzz_accepted_programs_never_trigger_asan() {
    let seed = env_u64("SUZ_ARENA_FUZZ_SEED", DEFAULT_SEED);
    let count = env_usize("SUZ_ARENA_FUZZ_COUNT", DEFAULT_COUNT);
    assert!(count >= 10, "SUZ_ARENA_FUZZ_COUNT must be at least 10");

    let directory = TemporaryDirectory::new();
    let mut accepted: Vec<(usize, String, PathBuf)> = Vec::new();
    let mut accepted_by_kind = [0usize; 3];
    let mut rejected_by_kind = [0usize; 3];

    for index in 0..count {
        let source = generate_program(seed, index);
        let kind = if index.is_multiple_of(2) {
            0
        } else if index % 4 == 1 {
            1
        } else {
            2
        };
        match suz::compile(source.as_bytes()) {
            Ok(compiled) => {
                accepted_by_kind[kind] += 1;
                let c_path = directory.0.join(format!("case-{index}.c"));
                fs::write(&c_path, compiled).unwrap();
                accepted.push((index, source, c_path));
            }
            Err(error) => {
                assert!(
                    error.rule().starts_with("arena."),
                    "unexpected non-arena compile error at seed={seed}, case={index}: {error}\n{source}"
                );
                rejected_by_kind[kind] += 1;
            }
        }
    }

    for (kind, name) in ["ordinary", "generic-explicit", "generic-inferred"]
        .into_iter()
        .enumerate()
    {
        let total = accepted_by_kind[kind] + rejected_by_kind[kind];
        assert!(
            accepted_by_kind[kind] * 10 >= total && rejected_by_kind[kind] * 10 >= total,
            "arena fuzz must exercise accepted and rejected {name} programs: accepted={}, rejected={}, seed={seed}",
            accepted_by_kind[kind],
            rejected_by_kind[kind]
        );
    }

    let batch_inputs: Vec<(usize, PathBuf)> = accepted
        .iter()
        .map(|(index, _, path)| (*index, path.clone()))
        .collect();
    let workers = worker_count(std::env::var("SUZ_JOBS").ok().as_deref())
        .unwrap()
        .min(batch_inputs.len());
    let asan_flags = ["-fsanitize=address,undefined", "-fno-sanitize-recover=all"];
    let batch = Batch::build_with_flags(&directory.0, &batch_inputs, workers, &asan_flags)
        .unwrap_or_else(|e| panic!("ASan batch build failed (seed={seed}): {e}"));

    for (index, source, _) in &accepted {
        let output = batch
            .run_with_env(*index, &[("ASAN_OPTIONS", ASAN_OPTIONS)])
            .unwrap_or_else(|e| {
                let (src_path, rpt_path) = save_failure(seed, *index, source, &e);
                panic!(
                    "candidate compiler/runtime/lifetime bug at seed={seed}, case={index}: {e}\nsource: {}\nreport: {}",
                    src_path.display(),
                    rpt_path.display()
                );
            });
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let details = format!(
                "seed={seed}\ncase={index}\nflags={asan_flags:?}\nASAN_OPTIONS={ASAN_OPTIONS}\nstatus={:?}\nstdout:\n{stdout}\nstderr:\n{stderr}\n",
                output.status
            );
            let (src_path, rpt_path) = save_failure(seed, *index, source, &details);
            let reason = if stderr.contains("AddressSanitizer") {
                "AddressSanitizer detected an invalid memory access (candidate lifetime-check, compiler, or runtime bug)"
            } else {
                "accepted program exited abnormally (candidate compiler/runtime/budget bug)"
            };
            panic!(
                "{reason} at seed={seed}, case={index}, status={:?}\nsource: {}\nreport: {}\nstderr:\n{stderr}",
                output.status,
                src_path.display(),
                rpt_path.display()
            );
        }
    }

    // SUZ_CFLAGS=-fsanitize=address 経由の CLI ビルドと実行も 1 件確認する。
    if let Some((index, source, _)) = accepted.first() {
        let suz_path = directory.0.join("cli-check.suz");
        let bin_path = directory.0.join("cli-check");
        fs::write(&suz_path, source).unwrap();
        let build = Command::new(env!("CARGO_BIN_EXE_suz"))
            .env("SUZ_CFLAGS", "-fsanitize=address")
            .args([
                "build",
                suz_path.to_str().unwrap(),
                "-o",
                bin_path.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "suz build with SUZ_CFLAGS=-fsanitize=address failed (case={index}): {build:?}"
        );
        let run = Command::new(&bin_path)
            .env("ASAN_OPTIONS", ASAN_OPTIONS)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "cli-check with ASan failed (case={index}): {run:?}"
        );
    }
}
