#include "suz_arena.h"
#include "suz_runtime.h"
#include <assert.h>
#include <stdckdint.h>

suz_arena_state suz_arena;

void suz_arena_init(uint64_t capacity, const char *source, size_t line, size_t column) {
    assert(capacity > 0 && suz_arena.bytes == NULL);
    void *bytes = NULL;
    if (capacity <= SIZE_MAX) bytes = suz_env_alloc((size_t)capacity);
    if (bytes == NULL) {
        suz_panic_begin(source, line, column);
        suz_log_text("arena allocation failed: requested=", sizeof("arena allocation failed: requested=") - 1);
        suz_log_u64(capacity);
        suz_log_text(" [rt.arena-storage]", sizeof(" [rt.arena-storage]") - 1);
        suz_panic_end();
    }
    suz_arena = (suz_arena_state){bytes, (size_t)capacity, 0};
}

void *suz_arena_alloc(size_t size, size_t alignment, const char *source, size_t line, size_t column) {
    assert(suz_arena.bytes != NULL && size > 0);
    assert(alignment > 0 && alignment <= alignof(max_align_t));
    /* 基底アドレスは環境層の契約でmax_align_tに整列済み。
       アドレスを整数へ変換せず、使用位置に余白を足して型の整列を満たす。 */
    size_t padding = (alignment - suz_arena.used % alignment) % alignment;
    size_t additional, end;
    if (ckd_add(&additional, padding, size) || ckd_add(&end, suz_arena.used, additional)) {
        suz_panic(source, line, column, "arena size calculation overflow [rt.arena-allocation]");
    }
    if (end > suz_arena.capacity) {
        suz_panic_begin(source, line, column);
        suz_log_text("arena capacity exceeded: limit=", sizeof("arena capacity exceeded: limit=") - 1);
        suz_log_u64(suz_arena.capacity);
        suz_log_text(", used=", sizeof(", used=") - 1);
        suz_log_u64(suz_arena.used);
        suz_log_text(", additional=", sizeof(", additional=") - 1);
        suz_log_u64(additional);
        suz_log_text(" [rt.arena-allocation]", sizeof(" [rt.arena-allocation]") - 1);
        suz_panic_end();
    }
    /* 検査が完了するまで使用位置を変えず、範囲外のポインタも作らない。 */
    void *result = suz_arena.bytes + suz_arena.used + padding;
    suz_arena.used = end;
    return result;
}
