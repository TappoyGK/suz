//! 同じseedから配列の寸法・境界・別名経路を再生成し、期待値とASanの両方で検査する。
use crate::{
    support::{batch::Batch, runner::worker_count},
    test_util::TemporaryDirectory,
};
use std::fs;

fn next(state: &mut u64, width: u64) -> i64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*state >> 32) % width) as i64
}

fn source(state: &mut u64) -> String {
    let h = next(state, 5);
    let w = next(state, 5);
    let start = next(state, 7) - 1;
    let end = next(state, 7) - 1;
    let y = next(state, 7) - 1;
    let x = next(state, 7) - 1;
    let initial = next(state, 100);
    let expected = if start < 0 || start > end || end > h {
        -99
    } else if y < 0 || y >= end - start {
        -88
    } else if x < 0 || x >= w {
        -77
    } else {
        initial
    };
    let update = if h > 0 && w > 0 {
        format!("fix ptr row = a[0]\nfix ptr view = row[0:{w}]\nview#[0] += 1")
    } else {
        String::new()
    };
    let total = h * w * initial + i64::from(h > 0 && w > 0);
    format!(
        r#"
fn access(a: ptr [i64; _, _], start: i64, end: i64, y: i64, x: i64) -> i64 {{
    fix ptr view = a[start:end] or return -99
    fix ptr row = view[y] or return -88
    return row[x] or -77
}}
fn sum(a: ptr [i64; h, w]) -> i64 {{
    var total: i64 = 0
    for y in 0..<h {{ rev x in 0..<w {{ total += a[y][x] }} }}
    return total
}}
fn main() -> u8 {{
    fix ptr a = new [{initial}; {h}, {w}]
    if access(a, {start}, {end}, {y}, {x}) != {expected} {{ return 1 }}
    {update}
    if sum(a) != {total} {{ return 2 }}
    return 0
}}
"#
    )
}

fn lifetime(index: usize, invalid: bool) -> String {
    let pointer = if invalid { "inner" } else { "outer" };
    let update = match index % 4 {
        0 => format!("a[0]#[0] = {pointer}"),
        1 => format!("fix ptr row = a[0]\nrow#[0] = {pointer}"),
        2 => format!("fix ptr slice = a[0:1]\nslice[0]#[0] = {pointer}"),
        _ => format!("for row in a {{ row#[0] = {pointer} }}"),
    };
    format!(
        "struct Node {{ value: i64 }}\nfn main() -> u8 {{ fix ptr outer = new Node {{ value: 7 }}\nfix ptr a: ptr [ptr? Node; _, _] = new [null; 1, 1]\narena {{ fix ptr inner = new Node {{ value: 9 }}\n{update} }}\nlet p = a[0][0] {{ if p.value != 7 {{ return 1 }} }} else {{ return 2 }}\nreturn 0 }}"
    )
}

fn preserve(seed: u64, index: usize, source: &str, detail: &str) -> ! {
    let directory = std::env::temp_dir().join("suz-array-fuzz-failures");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{seed}-{index}.suz"));
    fs::write(&path, source).unwrap();
    panic!(
        "seed={seed}, case={index}, source={}: {detail}",
        path.display()
    );
}

#[test]
#[ignore = "release check: generated array programs under ASan"]
fn generated_array_programs_preserve_bounds_and_lifetimes_under_asan() {
    let seed = std::env::var("SUZ_ARRAY_FUZZ_SEED")
        .map(|s| s.parse().unwrap())
        .unwrap_or(0x5eed_a44a_u64);
    let count = std::env::var("SUZ_ARRAY_FUZZ_COUNT")
        .map(|s| s.parse().unwrap())
        .unwrap_or(64_usize);
    assert!(count >= 4);
    let mut state = seed;
    let directory = TemporaryDirectory::new();
    let mut sources = Vec::new();
    let mut inputs = Vec::new();
    for index in 0..count {
        let bad = lifetime(index, true);
        match suz::compile(bad.as_bytes()) {
            Err(error) if error.rule() == "arena.store" => {}
            result => preserve(
                seed,
                index,
                &bad,
                &format!("expected lifetime rejection: {result:?}"),
            ),
        }
        for source in [source(&mut state), lifetime(index, false)] {
            let id = sources.len();
            let c = suz::compile(source.as_bytes())
                .unwrap_or_else(|e| preserve(seed, id, &source, &e.to_string()));
            let path = directory.0.join(format!("{id}.c"));
            fs::write(&path, c).unwrap();
            inputs.push((id, path));
            sources.push(source);
        }
    }
    let batch = Batch::build_with_flags(
        &directory.0,
        &inputs,
        worker_count(None).unwrap(),
        &[
            "-O1",
            "-fsanitize=address,undefined",
            "-fno-sanitize-recover=all",
        ],
    )
    .unwrap();
    for (index, source) in sources.iter().enumerate() {
        let output = batch
            .run_with_env(
                index,
                &[(
                    "ASAN_OPTIONS",
                    "detect_leaks=0:abort_on_error=0:exitcode=86",
                )],
            )
            .unwrap_or_else(|e| preserve(seed, index, source, &e));
        if !output.status.success() {
            preserve(seed, index, source, &format!("{output:?}"));
        }
    }
}
