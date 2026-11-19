#include "suz_arena.h"
#include "suz_env.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static size_t allocations;
static size_t expected_used;
static size_t expected_allocations = 1;
static unsigned char *sentinel;
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
    /* 診断用に確保せず、失敗前の使用位置と実体を保つ。 */
    assert(allocations == expected_allocations && suz_arena_position.used == expected_used);
    if (sentinel) assert(*sentinel == 42);
    _Exit(101);
}

static void growth(uint64_t limit) {
    suz_arena_init(16, 32, limit, "arena.suz", 1, 1);
    unsigned char *first = suz_arena_alloc(1, 1, "arena.suz", 2, 1);
    *first = 42;
    suz_arena_mark outer = suz_arena_save();
    for (int i = 0; i < 1000; ++i) {
        /* 単一の実体はgrowより大きくても連続する。既存の実体は移動しない。 */
        unsigned char *large = suz_arena_alloc(64, alignof(max_align_t), "arena.suz", 3, 1);
        memset(large, 7, 64);
        assert((uintptr_t)large % alignof(max_align_t) == 0);
        suz_arena_mark inner = suz_arena_save();
        unsigned char *small = suz_arena_alloc(32, 1, "arena.suz", 4, 1);
        memset(small, 9, 32);
        assert(suz_arena.total == 112 && allocations == 3);
        suz_arena_restore(&inner);
        assert(large[0] == 7 && large[63] == 7 && *first == 42);
        assert(suz_arena_alloc(32, 1, "arena.suz", 5, 1) == small);
        suz_arena_restore(&outer);
        assert(*first == 42 && suz_arena_position.used == 1);
    }
    /* 大きな要求で小さい未使用ブロックを飛び越しても、小さい方を後で使える。 */
    suz_arena_mark empty = {NULL, 0};
    suz_arena_restore(&empty);
    suz_arena_alloc(64, 1, "arena.suz", 6, 1);
    suz_arena_alloc(16, 1, "arena.suz", 7, 1);
    suz_arena_alloc(32, 1, "arena.suz", 8, 1);
    assert(allocations == 3 && suz_arena.total == 112);
}

int main(int argc, char **argv) {
    assert(argc == 2);
    const char *mode = argv[1];
    if (strcmp(mode, "growth") == 0) { growth(112); return 0; }
    if (strcmp(mode, "growth-unlimited") == 0) { growth(0); return 0; }
    if (strcmp(mode, "lazy") == 0) {
        suz_arena_init(0, 64, 48, "arena.suz", 1, 1);
        assert(allocations == 0 && suz_arena.total == 0);
        suz_arena_mark empty = suz_arena_save();
        void *first = suz_arena_alloc(48, 1, "arena.suz", 2, 1);
        assert(allocations == 1 && suz_arena_position.capacity == 48);
        suz_arena_restore(&empty);
        assert(suz_arena_alloc(48, 1, "arena.suz", 3, 1) == first);
        assert(allocations == 1);
        return 0;
    }
    if (strcmp(mode, "zero-fixed") == 0) {
        panic_expected = true;
        expected_allocations = 0;
        suz_arena_init(0, 0, 0, "arena.suz", 1, 1);
        suz_arena_alloc(1, 1, "arena.suz", 2, 1);
        return 99;
    }
    panic_expected = strcmp(mode, "success") != 0;
    fail_allocation = strcmp(mode, "init-failure") == 0;
    size_t capacity = strcmp(mode, "capacity") == 0 ? 9 : 128;
    bool expanding = strcmp(mode, "grow-failure") == 0 || strcmp(mode, "limit") == 0;
    suz_arena_init(capacity, expanding ? 64 : 0, strcmp(mode, "limit") == 0 ? 140 : 0, "arena.suz", 4, 2);
    assert(allocations == 1 && suz_arena_position.used == 0);
    unsigned char *first = suz_arena_alloc(1, 1, "arena.suz", 6, 3);
    *first = 42;
    sentinel = first;
    expected_used = 1;
    if (strcmp(mode, "capacity") == 0) {
        suz_arena_alloc(2, 8, "arena.suz", 8, 5);
    } else if (strcmp(mode, "padding-overflow") == 0) {
        suz_arena_alloc(SIZE_MAX, 8, "arena.suz", 8, 5);
    } else if (strcmp(mode, "end-overflow") == 0) {
        suz_arena_alloc(SIZE_MAX, 1, "arena.suz", 8, 5);
    } else if (expanding) {
        if (strcmp(mode, "grow-failure") == 0) {
            fail_allocation = true;
            expected_allocations = 2;
        }
        suz_arena_alloc(128, 1, "arena.suz", 8, 5);
    } else {
        assert(!panic_expected);
        suz_arena_mark outer = suz_arena_save();
        void *previous = NULL;
        for (int i = 0; i < 1000; ++i) {
            long double *value = suz_arena_alloc(sizeof(*value), alignof(long double), "arena.suz", 10, 5);
            assert((uintptr_t)value % alignof(long double) == 0);
            if (previous) assert(value == previous);
            *value = 3.5L;
            previous = value;
            suz_arena_mark nested = suz_arena_save();
            unsigned char *tail = suz_arena_alloc(capacity - nested.used, 1, "arena.suz", 11, 5);
            memset(tail, 7, capacity - nested.used);
            assert(suz_arena_position.used == capacity);
            suz_arena_restore(&nested);
            assert(*value == 3.5L && *first == 42);
            suz_arena_restore(&outer);
            assert(suz_arena_position.used == outer.used && allocations == 1);
        }
        suz_arena_mark start = {suz_arena_position.current, 0};
        suz_arena_restore(&start);
        assert(suz_arena_alloc(capacity, 1, "arena.suz", 15, 5) == first);
        assert(suz_arena_position.used == capacity && allocations == 1);
        return 0;
    }
    return 99;
}
