#include "suz_arena.h"
#include "suz_runtime.h"
#include <assert.h>

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

_Noreturn void suz_arena_overflow(const char *source, size_t line, size_t column) {
    suz_panic(source, line, column, "arena size calculation overflow [rt.arena-allocation]");
}

_Noreturn void suz_arena_capacity_exceeded(size_t additional, const char *source, size_t line, size_t column) {
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
