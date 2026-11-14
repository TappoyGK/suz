#include "suz_env.h"
#include "suz_text.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

static struct { void *ptr; size_t size; } text_allocations[64];
static size_t text_calls, text_live, text_peak;
static size_t text_fail_after = SIZE_MAX;
static size_t text_expected_panic_calls = SIZE_MAX;

void *suz_env_alloc(size_t size) {
    assert(size != 0);
    if (text_calls++ >= text_fail_after) return NULL;
    void *ptr = malloc(size);
    assert(ptr != NULL);
    for (size_t i = 0; i < 64; ++i) {
        if (text_allocations[i].ptr == NULL) {
            text_allocations[i].ptr = ptr;
            text_allocations[i].size = size;
            text_live += size;
            if (text_live > text_peak) text_peak = text_live;
            return ptr;
        }
    }
    abort();
}

void suz_env_free(void *ptr) {
    if (ptr == NULL) return;
    for (size_t i = 0; i < 64; ++i) {
        if (text_allocations[i].ptr == ptr) {
            text_live -= text_allocations[i].size;
            text_allocations[i].ptr = NULL;
            free(ptr);
            return;
        }
    }
    abort();
}

void suz_env_log(const char *bytes, size_t length) {
    fwrite(bytes, 1, length, stderr);
}

_Noreturn void suz_env_panic(void) {
    if (text_expected_panic_calls != SIZE_MAX) assert(text_calls == text_expected_panic_calls);
    exit(101);
}
