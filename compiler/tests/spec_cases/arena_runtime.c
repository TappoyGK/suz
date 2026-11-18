#include "suz_arena.h"
#include "suz_env.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static size_t allocations;
static size_t expected_used;
static bool fail_allocation;
static bool panic_expected;

void *suz_env_alloc(size_t size) {
    ++allocations;
    if (fail_allocation) return NULL;
    return malloc(size);
}
void suz_env_free(void *pointer) { free(pointer); }
void suz_env_log(suz_log_level level, const char *text, size_t size) {
    assert(level == SUZ_LOG_PANIC);
    fwrite(text, 1, size, stderr);
}
_Noreturn void suz_env_panic(void) {
    assert(panic_expected);
    /* エラー診断で追加確保せず、失敗前の使用位置と実体を保つことを検査する。 */
    assert(allocations == 1 && suz_arena.used == expected_used);
    if (suz_arena.bytes) assert(suz_arena.bytes[0] == 42);
    _Exit(101);
}

int main(int argc, char **argv) {
    assert(argc == 2);
    const char *mode = argv[1];
    panic_expected = strcmp(mode, "success") != 0;
    fail_allocation = strcmp(mode, "init-failure") == 0;
    size_t capacity = strcmp(mode, "capacity") == 0 ? 9 : 128;
    suz_arena_init(capacity, "arena.suz", 4, 2);
    assert(allocations == 1 && suz_arena.used == 0);
    unsigned char *first = suz_arena_alloc(1, 1, "arena.suz", 6, 3);
    *first = 42;
    expected_used = 1;
    if (strcmp(mode, "capacity") == 0) {
        suz_arena_alloc(2, 8, "arena.suz", 8, 5);
    } else if (strcmp(mode, "padding-overflow") == 0) {
        suz_arena_alloc(SIZE_MAX, 8, "arena.suz", 8, 5);
    } else if (strcmp(mode, "end-overflow") == 0) {
        suz_arena_alloc(SIZE_MAX, 1, "arena.suz", 8, 5);
    } else {
        assert(!panic_expected);
        size_t outer = suz_arena.used;
        void *previous = NULL;
        for (int i = 0; i < 1000; ++i) {
            /* 先頭の1バイトに続く余白も回収し、毎回同じ位置を使う。 */
            size_t inner = suz_arena.used;
            long double *value = suz_arena_alloc(sizeof(*value), alignof(long double), "arena.suz", 10, 5);
            assert((uintptr_t)value % alignof(long double) == 0);
            if (previous) assert(value == previous);
            *value = 3.5L;
            previous = value;
            size_t nested = suz_arena.used;
            unsigned char *tail = suz_arena_alloc(capacity - nested, 1, "arena.suz", 11, 5);
            memset(tail, 7, capacity - nested);
            assert(suz_arena.used == capacity); /* 上限ちょうどは成功する。 */
            suz_arena.used = nested;
            assert(*value == 3.5L && *first == 42);
            suz_arena.used = inner;
            assert(suz_arena.used == outer && allocations == 1);
        }
        suz_arena.used = 0;
        assert(suz_arena_alloc(capacity, 1, "arena.suz", 15, 5) == first);
        assert(suz_arena.used == capacity && allocations == 1);
        suz_env_free(suz_arena.bytes);
        return 0;
    }
    return 99;
}
