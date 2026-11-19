#include "suz_arena.h"
#include "suz_runtime.h"
#include <assert.h>

suz_arena_state suz_arena;

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
    return chunk;
}

static void select_chunk(suz_arena_chunk *chunk) {
    suz_arena.current = chunk;
    suz_arena.bytes = chunk->bytes;
    suz_arena.capacity = chunk->capacity;
    suz_arena.used = 0;
}

void suz_arena_init(uint64_t initial, uint64_t grow, uint64_t limit, const char *source, size_t line, size_t column) {
    assert(suz_arena.first == NULL);
    if (limit != 0 && initial > limit) initial = limit;
    suz_arena.grow = grow;
    suz_arena.limit = limit;
    if (initial == 0) return;
    suz_arena_chunk *chunk = allocate(initial, source, line, column);
    suz_arena.first = chunk;
    suz_arena.total = initial;
    select_chunk(chunk);
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

void *suz_arena_expand(size_t size, const char *source, size_t line, size_t column) {
    if (size > SIZE_MAX - offsetof(suz_arena_chunk, bytes)) suz_arena_overflow(source, line, column);
    suz_arena_chunk **unused = suz_arena.current ? &suz_arena.current->next : &suz_arena.first;
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
    select_chunk(chunk);
    suz_arena.used = size;
    return chunk->bytes;
}
