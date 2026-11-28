use super::symbol_runtime::build_with_environment;
use std::process::Command;

#[test]
fn text_buffers_omit_empty_literal_fragments() {
    // 実行結果では見えない、断片ビューの初期化と走査の余分な回数を検出する。
    for (template, fragments) in [("\"a${n}\"", 2), ("\"${n}\"", 1), ("\"\"", 0)] {
        let source = format!(
            "fn build(n: i32) -> text {{ return textbuf {template} }}\nfn main() -> u8 {{ return 0 }}"
        );
        let generated = suz::compile(source.as_bytes()).unwrap();
        let parts = generated
            .lines()
            .find(|line| line.contains("const suz_text_view parts[]"))
            .unwrap();
        assert_eq!(
            parts.matches("(suz_text_view)").count(),
            fragments.max(1),
            "{parts}"
        );
        if fragments != 0 {
            assert!(!parts.contains("\"\""), "{parts}");
        }
        let build = generated
            .lines()
            .find(|line| line.contains("return suz_textbuf_build("))
            .unwrap();
        assert!(build.contains(&format!(", parts, {fragments},")), "{build}");
    }
}

#[test]
fn text_buffers_build_directly_and_preserve_evaluation_order_and_views() {
    // 環境層の確保回数で、完成した一時文字列やインターンが混入しないことを検証する。
    let source = r#"struct Counter { n: len_t }
fn next(p: mut Counter) -> len_t {
    p#n = p.n + 1 or 0
    return p.n
}
fn copy(t: text) -> text { return textbuf t }
fn main() -> u8 {
    fix p = new Counter { n: 0 }
    fix message = textbuf[next(p)] ("あ${next(p)}😀\0${next(p)}")
    if message != "あ2😀\03" { return 1 }
    if p.n != 3 { return 2 }
    fix view = message.[1:4] or ""
    fix copied = copy(view)
    if copied != "2😀\0" { return 3 }
    fix descriptor = new message
    if .descriptor != message { return 4 }
    fix empty = textbuf[0] ""
    if len empty != 0 { return 5 }
    fix small: i64 = -9223372036854775808
    fix large: u64 = 18446744073709551615
    fix number = textbuf "${small} ${large} ${true} ${\😀} ${1.25}"
    if number != "-9223372036854775808 18446744073709551615 true 😀 1.25" { return 6 }
    return 0
}"#;
    let (_dir, path) = build_with_environment(
        source,
        r#"
int main(void) {
    int result = suz_program();
    assert(text_calls == 1 && text_live == 0 && suz_symbol_pool_used() == 0);
    return result;
}
"#,
    );
    let output = Command::new(path).output().unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn parsed_text_is_saved_only_after_validation_and_has_exact_capacity() {
    // C側から使用量とメタ情報を観測する。言語へ検査専用のAPIを追加しない。
    let source = r#"struct Item { body: text, number: u8 }
fn parse(t: text) -> Item { return t as Item or Item { body: "fallback", number: 0 } }
fn reserve(t: text, n: len_t) -> text { return textbuf[n] t }
fn main() -> u8 { return 0 }
"#;
    let (_dir, path) = build_with_environment(
        source,
        r#"
int main(void) {
    assert(suz_program() == 0);
    suz_arena_init(128, 128, 0, "driver", 1, 1);
    size_t before = text_calls;
    const char invalid[] = "Item { body: \"not saved\", number: 256 }";
    suz_text_view bad = {invalid, sizeof(invalid) - 1, sizeof(invalid) - 1, NULL};
    suz_struct_0 failed = suz_fn_0(bad);
    (void)failed;
    assert(suz_symbol_pool_used() == 0 && text_calls == before);
    size_t calls = text_calls;
    size_t used = suz_arena_position.used;
    const char valid[] = "Item { body: \"a\\0b\", number: 1 }";
    suz_text_view input = {valid, sizeof(valid) - 1, sizeof(valid) - 1, NULL};
    suz_struct_0 item = suz_fn_0(input);
    (void)item;
    assert(text_calls == calls && suz_symbol_pool_used() == 0);
    assert(used == 0);
    suz_text_storage *storage = (suz_text_storage *)suz_arena_position.bytes;
    assert(storage->capacity == 3 && storage->used == 3);
    assert(memcmp(storage->bytes, "a\0b", 3) == 0);
    suz_text_view copy = suz_fn_1(input, 100);
    assert(copy.storage->capacity == 100 && copy.storage->used == input.length);
    assert(memcmp(copy.bytes, input.bytes, input.length) == 0);
    suz_text_view slice;
    assert(suz_text_slice(copy, 1, 3, &slice));
    assert(slice.storage == copy.storage && slice.bytes == copy.bytes + 1);
    suz_text_view empty = suz_fn_1((suz_text_view){"", 0, 0, NULL}, 0);
    assert(empty.storage != NULL && empty.storage->capacity == 0 && empty.storage->used == 0);
    suz_arena_destroy();
    assert(text_live == 0);
    return 0;
}
"#,
    );
    let output = Command::new(path).output().unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn append_reuses_tail_capacity_and_copies_diverging_views_once() {
    // 同じ内容でも、余計な確保・コピーを行う実装はC側の実体と使用量で区別する。
    let source = r#"fn append(t: text, suffix: text) -> text {
    var result = t
    result <- suffix
    return result
}
fn formatted(t: text, n: i32) -> text {
    var result = t
    result <- "${n}"
    return result
}
fn main() -> u8 { return 0 }
"#;
    let (_dir, path) = build_with_environment(
        source,
        r#"
int main(void) {
    assert(suz_program() == 0);
    suz_arena_init(4096, 4096, 4096, "driver", 1, 1);
    suz_text_location loc = {"driver", 1, 1};
    const suz_text_view a = {"abc", 3, 3, NULL};
    suz_text_view old = suz_textbuf_build(8, &a, 1, loc);
    size_t used = suz_arena_position.used;
    size_t calls = text_calls;
    suz_text_view grown = suz_fn_0(old, old);
    assert(grown.storage == old.storage && grown.length == 6);
    assert(old.length == 3 && memcmp(old.bytes, "abc", 3) == 0);
    assert(suz_arena_position.used == used && text_calls == calls);
    suz_text_view empty = {"", 0, 0, NULL};
    suz_text_view forked = suz_fn_0(old, empty);
    assert(forked.storage != old.storage && forked.storage->capacity == 8);
    assert(forked.length == 3 && memcmp(grown.bytes, "abcabc", 6) == 0);
    // 末尾に達する切り出しなら、開始位置が途中でも書き足せる。
    suz_text_view tail;
    assert(suz_text_slice(grown, 3, 6, &tail));
    used = suz_arena_position.used;
    suz_text_view two = {"XY", 2, 2, NULL};
    suz_text_view extended = suz_fn_0(tail, two);
    assert(extended.storage == old.storage && extended.bytes == old.bytes + 3);
    assert(extended.length == 5 && memcmp(extended.bytes, "abcXY", 5) == 0);
    assert(extended.storage->used == 8 && suz_arena_position.used == used);
    // 8 -> 16 -> 32は容量の計算だけ。完成した実体を1回確保する。
    suz_text_view many = {"01234567890123456789", 20, 20, NULL};
    used = suz_arena_position.used;
    suz_text_view copied = suz_fn_0(extended, many);
    assert(copied.storage->capacity == 32 && copied.length == 25);
    assert(copied.storage != old.storage && old.storage->used == 8);
    assert(suz_arena_position.used - used == sizeof(suz_text_storage) + 32);
    assert(memcmp(copied.bytes, "abcXY01234567890123456789", 25) == 0);
    suz_text_view zero = suz_textbuf_build(0, NULL, 0, loc);
    suz_text_view first = suz_fn_0(zero, two);
    assert(first.storage->capacity == 16 && first.length == 2);
    used = suz_arena_position.used;
    suz_text_view formatted = suz_fn_1(first, 42);
    assert(formatted.storage == first.storage && formatted.length == 4);
    assert(memcmp(formatted.bytes, "XY42", 4) == 0 && suz_arena_position.used == used);
    assert(suz_symbol_pool_used() == 0 && text_calls == calls);
    suz_arena_destroy();
    assert(text_live == 0);
    return 0;
}
"#,
    );
    let output = Command::new(path).output().unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn append_checks_length_growth_and_storage_overflow_before_copying() {
    // 巨大な文字列を実際に確保せず、Cの長さ表現の境界を直接与える。
    // どの経路も内容へアクセスする前に失敗しなければならない。
    let (_dir, path) = build_with_environment(
        "fn main() -> u8 { fix t = textbuf \"\"\n return 0 }",
        r#"
int main(int argc, char **argv) {
    assert(argc == 2);
    suz_text_storage storage = {0};
    suz_text_view left = {storage.bytes, 0, 0, &storage};
    suz_text_view part = {"x", 1, 1, NULL};
    switch (argv[1][0]) {
    case 'b': left.length = UINT64_MAX; break;
    case 's': left.scalars = INT32_MAX; break;
    case 'g':
        storage.capacity = UINT64_MAX / 2 + 1;
        storage.used = storage.capacity;
        left.length = storage.capacity;
        break;
    case 'm': storage.capacity = UINT64_MAX; storage.used = 1; break;
    default: return 2;
    }
    (void)suz_textbuf_append(left, &part, 1, (suz_text_location){"append.suz", 7, 9});
    return 3;
}
"#,
    );
    for mode in ["b", "s", "g", "m"] {
        let output = Command::new(&path).arg(mode).output().unwrap();
        assert_eq!(output.status.code(), Some(101), "{mode}: {output:?}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("append.suz:7:9") && stderr.contains("[rt.arena-allocation]"),
            "{mode}: {stderr}"
        );
    }
}
