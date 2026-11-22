use super::symbol_runtime::build_with_environment;
use std::process::Command;

#[test]
fn text_buffers_omit_empty_literal_fragments() {
    // 実行結果では見えない、断片ビューの初期化と走査の余分な回数を検出する。
    for (template, fragments) in [("`a${n}`", 2), ("`${n}`", 1), ("``", 0)] {
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
fn next(p: ptr Counter) -> len_t {
    p#n = p.n + 1 or 0
    return p.n
}
fn copy(t: text) -> text { return textbuf t }
fn main() -> u8 {
    fix ptr p = new Counter { n: 0 }
    fix message = textbuf[next(p)] (`あ${next(p)}😀\0${next(p)}`)
    if message != `あ2😀\03` { return 1 }
    if p.n != 3 { return 2 }
    fix view = message[1:4] or ``
    fix copied = copy(view)
    if copied != `2😀\0` { return 3 }
    fix ptr descriptor = new message
    if .descriptor != message { return 4 }
    fix empty = textbuf[0] ``
    if len empty != 0 { return 5 }
    fix small: i64 = -9223372036854775808
    fix large: u64 = 18446744073709551615
    fix number = textbuf `${small} ${large} ${true} ${'😀'} ${1.25}`
    if number != `-9223372036854775808 18446744073709551615 true 😀 1.25` { return 6 }
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
fn parse(t: text) -> Item { return t as Item or Item { body: `fallback`, number: 0 } }
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
    const char invalid[] = "Item { body: `not saved`, number: 256 }";
    suz_text_view bad = {invalid, sizeof(invalid) - 1, sizeof(invalid) - 1, NULL};
    suz_struct_0 failed = suz_fn_0(bad);
    (void)failed;
    assert(suz_symbol_pool_used() == 0 && text_calls == before);
    size_t calls = text_calls;
    size_t used = suz_arena_position.used;
    const char valid[] = "Item { body: `a\\0b`, number: 1 }";
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
