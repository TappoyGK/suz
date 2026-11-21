#include "suz_arena.h"
#include "suz_runtime.h"
#include <assert.h>

suz_arena_state suz_arena;

/* 添字の符号を別に渡し、u64の最大値と負のi64の双方を切り詰めず診断する。 */
_Noreturn void suz_array_bounds(const char *source, size_t line, size_t column, size_t dimension, uint64_t index, bool negative, int32_t length) {
    suz_panic_begin(source, line, column);
    suz_log_text("array index out of bounds: dimension=", sizeof("array index out of bounds: dimension=") - 1);
    suz_log_u64(dimension);
    suz_log_text(" index=", sizeof(" index=") - 1);
    if (negative) {
        suz_log_text("-", 1);
        index = UINT64_C(0) - index;
    }
    suz_log_u64(index);
    suz_log_text(" length=", sizeof(" length=") - 1);
    suz_log_u64((uint64_t)length);
    suz_log_text(" [rt.array-bounds]", sizeof(" [rt.array-bounds]") - 1);
    suz_panic_end();
}

/* 切り出しを含む書き込み経路では、失敗した範囲の両端を元の符号で報告する。 */
_Noreturn void suz_array_slice_bounds(const char *source, size_t line, size_t column, uint64_t start, bool start_negative, uint64_t end, bool end_negative, int32_t length) {
    suz_panic_begin(source, line, column);
    suz_log_text("array slice out of bounds: start=", sizeof("array slice out of bounds: start=") - 1);
    if (start_negative) {
        suz_log_text("-", 1);
        start = UINT64_C(0) - start;
    }
    suz_log_u64(start);
    suz_log_text(" end=", sizeof(" end=") - 1);
    if (end_negative) {
        suz_log_text("-", 1);
        end = UINT64_C(0) - end;
    }
    suz_log_u64(end);
    suz_log_text(" length=", sizeof(" length=") - 1);
    suz_log_u64((uint64_t)length);
    suz_log_text(" [rt.array-bounds]", sizeof(" [rt.array-bounds]") - 1);
    suz_panic_end();
}

/* mainが返ってから全ブロックを解放する。明示的なarenaの巻き戻しは再利用のため
   記憶領域を保持するが、プログラムの終了境界では保持する必要がない。 */
void suz_arena_destroy(void) {
    suz_arena_chunk *chunk = suz_arena.first;
    while (chunk != NULL) {
        suz_arena_chunk *next = chunk->next;
        suz_env_free(chunk);
        chunk = next;
    }
    suz_arena = (suz_arena_state){0};
}

_Noreturn void suz_arena_overflow(const char *source, size_t line, size_t column) {
    suz_panic(source, line, column, "arena size calculation overflow [rt.arena-allocation]");
}

static _Noreturn void allocation_failed(uint64_t capacity, const char *source, size_t line, size_t column) {
    suz_panic_begin(source, line, column);
    suz_log_text("arena allocation failed: requested=", sizeof("arena allocation failed: requested=") - 1);
    suz_log_u64(capacity);
    suz_log_text(" [rt.arena-storage]", sizeof(" [rt.arena-storage]") - 1);
    suz_panic_end();
}

static suz_arena_chunk *allocate(uint64_t capacity, const char *source, size_t line, size_t column) {
    size_t bytes;
    if (capacity > SIZE_MAX || ckd_add(&bytes, (size_t)capacity, offsetof(suz_arena_chunk, bytes))) {
        suz_arena_overflow(source, line, column);
    }
    suz_arena_chunk *chunk = suz_env_alloc(bytes);
    if (chunk == NULL) allocation_failed(capacity, source, line, column);
    chunk->next = NULL;
    chunk->capacity = (size_t)capacity;
#ifdef SUZ_ARENA_ASAN
    __asan_poison_memory_region(chunk->bytes, chunk->capacity);
#endif
    return chunk;
}

#ifdef SUZ_ARENA_ASAN
void suz_arena_poison_reclaimed(suz_arena_chunk *saved, size_t used, suz_arena_cursor current) {
    if (current.current == NULL) return;
    suz_arena_chunk *chunk = saved ? saved : suz_arena.first;
    /* 途中のブロックには使用位置を保存していない。既に毒の未使用部分も含めて
       末尾まで塗り直す。現在のブロックは使用位置まででよく、単一ブロック内の
       小さなarenaの回収で大容量の未使用領域を毎回塗り直すことを避ける。 */
    for (;;) {
        size_t end = chunk == current.current ? current.used : chunk->capacity;
        __asan_poison_memory_region(chunk->bytes + used, end - used);
        if (chunk == current.current) break;
        chunk = chunk->next;
        used = 0;
    }
}
#endif

suz_arena_cursor suz_arena_storage_init(uint64_t initial, uint64_t grow, uint64_t limit, const char *source, size_t line, size_t column) {
    assert(suz_arena.first == NULL);
    if (limit != 0 && initial > limit) initial = limit;
    suz_arena.grow = grow;
    suz_arena.limit = limit;
    if (initial == 0) return (suz_arena_cursor){0};
    suz_arena_chunk *chunk = allocate(initial, source, line, column);
    suz_arena.first = chunk;
    suz_arena.total = initial;
    return (suz_arena_cursor){chunk->bytes, chunk->capacity, 0, chunk};
}

_Noreturn void suz_arena_capacity_exceeded(size_t size, const char *source, size_t line, size_t column) {
    suz_panic_begin(source, line, column);
    suz_log_text("arena capacity exceeded: reserved=", sizeof("arena capacity exceeded: reserved=") - 1);
    suz_log_u64(suz_arena.total);
    suz_log_text(", limit=", sizeof(", limit=") - 1);
    if (suz_arena.limit) suz_log_u64(suz_arena.limit);
    else suz_log_text("unlimited", sizeof("unlimited") - 1);
    suz_log_text(", grow=", sizeof(", grow=") - 1);
    suz_log_u64(suz_arena.grow);
    suz_log_text(", requested=", sizeof(", requested=") - 1);
    suz_log_u64(size);
    suz_log_text(" [rt.arena-allocation]", sizeof(" [rt.arena-allocation]") - 1);
    suz_panic_end();
}

suz_arena_cursor suz_arena_expand(suz_arena_chunk *current, size_t size, const char *source, size_t line, size_t column) {
    if (size > SIZE_MAX - offsetof(suz_arena_chunk, bytes)) suz_arena_overflow(source, line, column);
    suz_arena_chunk **unused = current ? &current->next : &suz_arena.first;
    suz_arena_chunk **candidate = unused;
    while (*candidate && (*candidate)->capacity < size) candidate = &(*candidate)->next;
    suz_arena_chunk *chunk = *candidate;
    if (chunk) {
        /* 未使用の列から適合するものだけを取り出す。小さいブロックも後続で再利用できる。 */
        *candidate = chunk->next;
    } else {
        if (suz_arena.grow == 0) suz_arena_capacity_exceeded(size, source, line, column);
        uint64_t capacity = size > suz_arena.grow ? size : suz_arena.grow;
        uint64_t remaining = (suz_arena.limit ? suz_arena.limit : UINT64_MAX) - suz_arena.total;
        if (capacity > remaining) capacity = remaining;
        if (capacity < size) suz_arena_capacity_exceeded(size, source, line, column);
        chunk = allocate(capacity, source, line, column);
        suz_arena.total += capacity;
    }
    chunk->next = *unused;
    *unused = chunk;
    return (suz_arena_cursor){chunk->bytes, chunk->capacity, size, chunk};
}
