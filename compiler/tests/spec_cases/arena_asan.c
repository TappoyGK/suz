/* Suzが拒否する寿命違反をランタイムへ直接与え、ASanによる検出を検証する。 */
#include "suz_arena.h"
#include "suz_env.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static size_t allocations;
void *suz_env_alloc(size_t size) { ++allocations; return malloc(size); }
void suz_env_free(void *p) { free(p); }
void suz_env_log(suz_log_level level, const char *s, size_t n) { (void)level; fwrite(s, 1, n, stderr); }
_Noreturn void suz_env_panic(void) { _Exit(101); }

static unsigned char *allocate(size_t n, size_t alignment) {
    return suz_arena_alloc(n, alignment, "arena.suz", 1, 1);
}
static void poisoned(unsigned char *p, size_t size, bool expected) {
    for (size_t i = 0; i < size; ++i) assert(!!__asan_address_is_poisoned(p + i) == expected);
}

int main(int argc, char **argv) {
    assert(argc == 2);
    const char *mode = argv[1];
    bool growth = strcmp(mode, "growth") == 0 || strcmp(mode, "stale-growth") == 0;
    bool lazy = strcmp(mode, "lazy") == 0;
    // 端数の容量でも管理情報と隣のブロックを巻き込まない。
    suz_arena_init(lazy ? 0 : 73, 128, 0, "arena.suz", 1, 1);
    suz_arena_mark empty = suz_arena_save();
    unsigned char *live = allocate(1, 1);
    *live = 42;
    poisoned(live, 1, false);
    poisoned(live + 1, 7, true);
    suz_arena_mark outer = suz_arena_save();
    unsigned char *dead = allocate(3, 1);
    memset(dead, 7, 3);
    poisoned(dead, 3, false);
    poisoned(dead + 3, 5, true);
    unsigned char *last = dead;
    if (growth) {
        unsigned char *middle = allocate(128, alignof(max_align_t));
        memset(middle, 8, 128);
        last = allocate(256, alignof(max_align_t));
        memset(last, 9, 256);
        suz_arena_restore(&outer);
        poisoned(middle, 128, true);
        poisoned(last, 256, true);
    } else {
        suz_arena_restore(&outer);
    }
    assert(*live == 42);
    poisoned(dead, 8, true);
    if (strcmp(mode, "stale-read") == 0 || strcmp(mode, "stale-growth") == 0) {
        return *(volatile unsigned char *)last; // 最適化で検査対象の読み出しを消さない。
    }
    if (strcmp(mode, "stale-write") == 0) {
        *(volatile unsigned char *)dead = 1;
        return 0;
    }
    if (strcmp(mode, "padding") == 0) return *(volatile unsigned char *)(live + 1);
    if (strcmp(mode, "overflow") == 0) { allocate(SIZE_MAX, 1); return 99; }
    size_t count = allocations;
    for (int i = 0; i < 100; ++i) {
        unsigned char *again = allocate(3, 1);
        assert(again == dead);
        memset(again, 3, 3);
        poisoned(again + 3, 5, true);
        long double *wide = (long double *)allocate(sizeof(long double), alignof(long double));
        assert((uintptr_t)wide % alignof(long double) == 0);
        *wide = 3.5L;
        if (growth) {
            allocate(128, alignof(max_align_t));
            assert(allocate(256, alignof(max_align_t)) == last);
        }
        assert(*wide == 3.5L && *live == 42);
        suz_arena_restore(&outer);
        poisoned(again, 8, true);
        assert(allocations == count);
    }
    suz_arena_restore(&empty);
    poisoned(live, 8, true);
    assert(allocate(1, 1) == live);
    *live = 43;
    assert(allocations == count);
    return 0;
}
