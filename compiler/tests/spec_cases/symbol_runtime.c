#include "suz_text.h"
/* 本番の個数加算を境界値から検証するため、内部関数も同じ翻訳単位に置く。 */
#include "suz_text.c"
#include "suz_parse.h"
#include "suz_runtime.h"
#include "suz_env.h"

#include <assert.h>
#include <fenv.h>
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const suz_text_location location = {"pool.suz", 4, 17};
static struct { void *ptr; size_t size; } allocations[4096];
static size_t calls, fail_call, frees;
static uint64_t live, peak, last_size, used_before_failure;
static bool panic_expected, limit_failure, temporary_allocations;
static char diagnostic[512];
static size_t diagnostic_length;

void *suz_env_alloc(size_t size) {
    assert(size != 0);
    if (!temporary_allocations) assert(suz_text_pool_used() == live);
    ++calls;
    assert(calls < sizeof(allocations) / sizeof(allocations[0]));
    if (calls == fail_call) {
        used_before_failure = suz_text_pool_used();
        return NULL;
    }
    void *ptr = malloc(size);
    assert(ptr != NULL);
    allocations[calls].ptr = ptr;
    allocations[calls].size = size;
    live += size;
    last_size = size;
    if (live > peak) peak = live;
    return ptr;
}

void suz_env_free(void *ptr) {
    if (ptr == NULL) return;
    if (!temporary_allocations) assert(suz_text_pool_used() == live);
    for (size_t i = 1; i <= calls; ++i) {
        if (allocations[i].ptr == ptr) {
            live -= allocations[i].size;
            allocations[i].ptr = NULL;
            ++frees;
            free(ptr);
            return;
        }
    }
    assert(!"free of an unknown allocation");
}

void suz_env_log(suz_log_level level, const char *bytes, size_t length) {
    (void)level;
    assert(length > 0 && length < sizeof(diagnostic) - diagnostic_length);
    memcpy(diagnostic + diagnostic_length, bytes, length);
    diagnostic_length += length;
}

_Noreturn void suz_env_panic(void) {
    assert(panic_expected);
    assert(strstr(diagnostic, "panic pool.suz:4:17: ") == diagnostic);
    assert(strstr(diagnostic, "[rt.symbol-allocation]") != NULL);
    if (fail_call != 0) {
        assert(calls == fail_call);
        assert(suz_text_pool_used() == used_before_failure);
        assert(strstr(diagnostic, "memory allocation failed") != NULL);
    }
    if (limit_failure) {
        assert(calls == 0);
        assert(suz_text_pool_used() == 0);
        assert(strstr(diagnostic, "limit=1, used=0, required=") != NULL);
    }
    if (!temporary_allocations) assert(suz_text_pool_used() == live);
    fwrite(diagnostic, 1, diagnostic_length, stderr);
    exit(101);
}

static suz_text intern(const char *bytes, bool constant) {
    return suz_text_intern(bytes, strlen(bytes), constant, location);
}

static void identity(void) {
    char input[] = "dynamic first";
    suz_text a = intern(input, false);
    memset(input, '?', sizeof(input) - 1);
    assert(a == intern("dynamic first", true));
    assert(suz_text_length(a) == 13);
    assert(memcmp(suz_text_bytes(a), "dynamic first", 13) == 0);
    const char *literal = "constant first";
    suz_text b = intern(literal, true);
    assert(suz_text_bytes(b) == literal);
    assert(b == intern("constant first", false));
    assert(a != b);
    suz_text empty = suz_text_intern(NULL, 0, false, location);
    assert(empty == intern("", true));
    assert(suz_text_length(empty) == 0);
    /* 索引を拡張しても、以前返した参照を無効にしてはならない。 */
    for (unsigned i = 0; i < 1000; ++i) {
        char text[32];
        int length = snprintf(text, sizeof(text), "entry %u", i);
        suz_text_intern(text, (uint64_t)length, false, location);
    }
    assert(a == intern("dynamic first", true));
    assert(b == intern("constant first", false));
    assert(empty == intern("", false));
    assert(live == suz_text_pool_used());
    assert(frees > 0);
}

static void collision(void) {
    /* この異なる文字列は、32ビットFNV-1aで同じハッシュ値を持つ。 */
    suz_text a = intern("lrgmpljekmot", false);
    suz_text b = intern("ukvnvtragfjx", true);
    assert(a != b);
    assert(a == intern("lrgmpljekmot", true));
    assert(b == intern("ukvnvtragfjx", false));
    assert(suz_text_length(a) == 12 && suz_text_length(b) == 12);
}

static void accounting(void) {
    static const char large_constant[4096] = "constant";
    suz_text_intern(large_constant, sizeof(large_constant), true, location);
    assert(live == suz_text_pool_used());
    assert(live < sizeof(large_constant));
    uint64_t before = live;
    intern("other constant", true);
    uint64_t metadata = live - before;
    assert(metadata > 0);
    before = live;
    intern("dynamic", false);
    assert(live - before == metadata + strlen("dynamic"));
    assert(live == suz_text_pool_used());
    before = live;
    size_t before_calls = calls;
    intern("dynamic", true);
    assert(live == before && calls == before_calls);
}

static void growth(void) {
    /* 索引を最初に置き換える時点で止める。確保中は新旧両方の索引サイズをプール使用量に含める。 */
    while (frees == 0) {
        char text[32];
        snprintf(text, sizeof(text), "entry %zu", calls);
        intern(text, false);
        assert(live == suz_text_pool_used());
    }
    assert(peak > live);
    assert(frees == 1);
    /* 最後の確保は、古い索引を解放した後のエントリーだった。 */
    assert(peak == live + allocations[1].size - last_size);
    printf("%llu\n", (unsigned long long)peak);
}

static void temporary(void) {
    intern("saved", true);
    uint64_t retained = live;
    temporary_allocations = true;
    for (unsigned i = 0; i < 1000; ++i) {
        suz_text_buffer buffer = {0};
        char fragment[128];
        memset(fragment, 'a' + i % 26, sizeof(fragment));
        suz_text_buffer_append(&buffer, fragment, 32, location);
        suz_text_buffer_append(&buffer, "\0", 1, location);
        suz_text_buffer_append(&buffer, fragment, sizeof(fragment), location);
        assert(buffer.length == 161 && buffer.bytes[32] == '\0');
        assert(memcmp(buffer.bytes + 33, fragment, sizeof(fragment)) == 0);
        assert(suz_text_pool_used() == retained);
        suz_text_buffer_dispose(&buffer);
        assert(buffer.bytes == NULL && buffer.length == 0 && buffer.capacity == 0);
        assert(live == retained);
    }
}

static void order(void) {
    suz_text a = suz_text_intern("a\0b", 3, true, location);
    assert(suz_text_length(a) == 3);
    assert(a == suz_text_intern("a\0b", 3, false, location));
    assert(a != intern("a", true));
    assert(suz_text_compare(intern("a", true), a) < 0);
    assert(suz_text_compare(a, intern("aa", true)) < 0);
    assert(suz_text_compare(intern("\xF0\x90\x80\x80", true),
                            intern("\xEE\x80\x80", true)) > 0);
    assert(intern("\xC3\xA9", true) != intern("e\xCC\x81", true));
    assert(suz_text_compare(a, a) == 0);
}

static void scalar_parsing(void) {
    /* 意図的に終端を付けないスライスで、誤ったC文字列としての読み取りを検出する。 */
    const char integer[] = {'2', '5', '5'};
    uint64_t value = 7;
    assert(suz_parse_integer(integer, sizeof(integer), 8, false, false, &value));
    assert(value == 255);
    const char boolean[] = {'f', 'a', 'l', 's', 'e'};
    bool truth = true;
    assert(suz_parse_bool(boolean, sizeof(boolean), &truth) && !truth);
    const char nul[] = {'1', '\0'};
    value = 7;
    assert(!suz_parse_integer(nul, sizeof(nul), 8, false, true, &value));
    assert(value == 7);
    assert(!suz_parse_integer("", 0, 64, true, false, &value));
    assert(value == 7);
    assert(!suz_parse_bool("false!", 6, &truth) && !truth);
    char long_integer[4097];
    memset(long_integer, '0', sizeof(long_integer));
    long_integer[0] = '1';
    long_integer[sizeof(long_integer) - 1] = '1';
    assert(suz_parse_integer(long_integer, sizeof(long_integer), 64, false, true, &value));
    assert(value == 1);
    assert(calls == 0 && live == 0 && suz_text_pool_used() == 0);
}

static void float_parsing(void) {
    const char slice[] = {'1', '.', '5'};
    double wide = 7.0;
    float narrow = 7.0f;
    assert(suz_parse_f64(slice, sizeof(slice), &wide) && wide == 1.5);
    assert(suz_parse_f32(slice, sizeof(slice), &narrow) && narrow == 1.5f);
    const char nul[] = {'1', '.', '5', '\0'};
    assert(!suz_parse_f64(nul, sizeof(nul), &wide) && wide == 1.5);
    assert(!suz_parse_f32("1.0e100", 7, &narrow) && narrow == 1.5f);
    uint64_t bits;
    assert(suz_parse_float_bits("-0", 2, 64, &bits) && bits == UINT64_C(0x8000000000000000));
    assert(suz_parse_float_bits("-1.0e-1000", 10, 32, &bits) && bits == UINT64_C(0x80000000));
    const char tie[] = "1.000000059604644775390625";
    const int modes[] = {FE_TONEAREST, FE_DOWNWARD, FE_UPWARD, FE_TOWARDZERO};
    for (unsigned i = 0; i < sizeof(modes) / sizeof(modes[0]); ++i) {
        assert(fesetround(modes[i]) == 0);
        assert(suz_parse_float_bits(tie, sizeof(tie) - 1, 32, &bits) && bits == UINT64_C(0x3f800000));
    }
    assert(fesetround(FE_TONEAREST) == 0);
    char long_input[2048];
    memset(long_input, '0', sizeof(long_input));
    memcpy(long_input, tie, sizeof(tie) - 1);
    long_input[sizeof(long_input) - 1] = '1';
    assert(suz_parse_float_bits(long_input, sizeof(long_input), 32, &bits) && bits == UINT64_C(0x3f800001));
    /* 2e-324に遠く離れた非ゼロの桁を加えると、分母10^1123が必要になる。 */
    memset(long_input, '0', sizeof(long_input));
    long_input[1] = '.';
    long_input[325] = '2';
    long_input[sizeof(long_input) - 1] = '1';
    assert(suz_parse_float_bits(long_input, sizeof(long_input), 64, &bits) && bits == 0);
    assert(calls == 0 && live == 0 && suz_text_pool_used() == 0);
}

int main(int argc, char **argv) {
    assert(argc >= 2);
    uint64_t limit = argc > 2 ? strtoull(argv[2], NULL, 10) : 0;
    suz_text_pool_init(strcmp(argv[1], "oom") == 0 ? 0 : limit);
    assert(calls == 0 && suz_text_pool_used() == 0);
    if (strcmp(argv[1], "scalar-limit") == 0) {
        /* プール無制限でも一つのsymbolの個数上限は変わらない。
           2GiB以上を確保せず、本番のUTF-8計数処理の最後の一文字を検証する。 */
        assert(limit == 0);
        uint64_t count = count_scalar(INT32_MAX - 1, 0xf0, location);
        assert(count == INT32_MAX);
        for (int i = 0; i < 3; ++i) count = count_scalar(count, 0x80, location);
        assert(count == INT32_MAX);
        panic_expected = true;
        count_scalar(count, 'x', location);
        return 2;
    }
    else if (strcmp(argv[1], "identity") == 0) identity();
    else if (strcmp(argv[1], "collision") == 0) collision();
    else if (strcmp(argv[1], "accounting") == 0) accounting();
    else if (strcmp(argv[1], "growth") == 0) {
        panic_expected = limit != 0;
        growth();
    }
    else if (strcmp(argv[1], "temporary") == 0) temporary();
    else if (strcmp(argv[1], "order") == 0) order();
    else if (strcmp(argv[1], "parse") == 0) scalar_parsing();
    else if (strcmp(argv[1], "parse-float") == 0) {
        if (argc > 3) assert(setlocale(LC_NUMERIC, argv[3]) != NULL);
        float_parsing();
    }
    else if (strcmp(argv[1], "empty") == 0) assert(live == 0);
    else if (strcmp(argv[1], "reuse") == 0) {
        suz_text value = intern("existing", false);
        uint64_t before = suz_text_pool_used();
        size_t before_calls = calls;
        assert(intern("existing", true) == value);
        assert(intern("existing", false) == value);
        assert(calls == before_calls && suz_text_pool_used() == before);
        printf("%llu\n", (unsigned long long)before);
    } else if (strcmp(argv[1], "limit") == 0) {
        panic_expected = limit_failure = true;
        intern("new", false);
        return 2;
    } else if (strcmp(argv[1], "oom") == 0) {
        panic_expected = true;
        fail_call = (size_t)limit;
        intern("new", false);
        temporary_allocations = true;
        suz_text_buffer buffer = {0};
        suz_text_buffer_append(&buffer, "temporary", 9, location);
        static const char large[128] = {0};
        suz_text_buffer_append(&buffer, large, sizeof(large), location);
        return 2;
    } else if (strcmp(argv[1], "overflow") == 0) {
        panic_expected = true;
        suz_text_buffer buffer = {NULL, UINT64_MAX, UINT64_MAX};
        suz_text_buffer_append(&buffer, "x", 1, location);
        return 2;
    } else return 3;
    return 0;
}
