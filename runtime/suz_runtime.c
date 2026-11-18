#include "suz_runtime.h"
#include "suz_env.h"

#include <fenv.h>
#include <stdlib.h>
#include <limits.h>
#include <stdatomic.h>
#include <stdckdint.h>
#include <stdio.h>
#include <string.h>

_Static_assert(CHAR_BIT == 8, "Suz requires 8-bit bytes");
_Static_assert(UINT8_MAX == 255, "Suz requires uint8_t");

void suz_runtime_init(void) {
#pragma STDC FENV_ACCESS ON
    /* 丸めだけでなく、例外マスクとフラグも初期化する。
       fesetroundだけでは、継承した浮動小数点トラップが有効なまま残る。 */
    if (fesetenv(FE_DFL_ENV) != 0) abort();
}

bool suz_try_add_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result) {
    int64_t value;
    if (ckd_add(&value, a, b) || value < min || value > max) return false;
    *result = value;
    return true;
}

bool suz_try_sub_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result) {
    int64_t value;
    if (ckd_sub(&value, a, b) || value < min || value > max) return false;
    *result = value;
    return true;
}

bool suz_try_mul_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result) {
    int64_t value;
    if (ckd_mul(&value, a, b) || value < min || value > max) return false;
    *result = value;
    return true;
}

bool suz_try_div_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result) {
    (void)max; /* maxを明示的に捨てて未使用引数の警告を抑える。値の変更や検査は行わない。 */
    if (b == 0 || (a == min && b == -1)) return false;
    *result = a / b;
    return true;
}

bool suz_try_rem_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result) {
    (void)max;
    if (b == 0 || (a == min && b == -1)) return false;
    *result = a % b;
    return true;
}

bool suz_try_add_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result) {
    uint64_t value;
    if (ckd_add(&value, a, b) || value > max) return false;
    *result = value;
    return true;
}

bool suz_try_sub_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result) {
    uint64_t value;
    if (ckd_sub(&value, a, b) || value > max) return false;
    *result = value;
    return true;
}

bool suz_try_mul_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result) {
    uint64_t value;
    if (ckd_mul(&value, a, b) || value > max) return false;
    *result = value;
    return true;
}

bool suz_try_div_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result) {
    (void)max;
    if (b == 0) return false;
    *result = a / b;
    return true;
}

bool suz_try_rem_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result) {
    (void)max;
    if (b == 0) return false;
    *result = a % b;
    return true;
}

bool suz_try_shl_i64(int64_t a, uint64_t count, unsigned width, int64_t *result) {
    if (count >= width) return false;
    /* 負の符号付き値をシフトせず、乗算で処理する。
       64ビットの全オペランドと有効なシフト量は、この中間表現に正確に収まる。 */
    _BitInt(128) value = (_BitInt(128))a * ((_BitInt(128))1 << count);
    _BitInt(128) limit = (_BitInt(128))1 << (width - 1);
    if (value < -limit || value >= limit) return false;
    *result = (int64_t)value;
    return true;
}

bool suz_try_shr_i64(int64_t a, uint64_t count, unsigned width, int64_t *result) {
    if (count >= width) return false;
    /* 非負の値をシフトしてから符号を戻す。
       Cで処理系定義の負数シフトに頼らず、算術右シフトを実現する。 */
    *result = a < 0 ? -1 - (int64_t)((uint64_t)(-(a + 1)) >> count)
                    : (int64_t)((uint64_t)a >> count);
    return true;
}

bool suz_try_shl_u64(uint64_t a, uint64_t count, unsigned width, uint64_t *result) {
    if (count >= width) return false;
    uint64_t max = UINT64_MAX >> (64 - width);
    if (a > (max >> count)) return false;
    *result = a << count;
    return true;
}

bool suz_try_shr_u64(uint64_t a, uint64_t count, unsigned width, uint64_t *result) {
    if (count >= width) return false;
    *result = a >> count;
    return true;
}

/* output_lockを保持している間だけアクセスする。 */
static suz_log_level output_level;

static void write_text(const char *text) {
    size_t len = strlen(text);
    if (len != 0) suz_env_log(output_level, text, len);
}

static void write_position(size_t value) {
    /* 1バイトにつき最大3桁の十進数字と、終端のNULが必要。 */
    char text[3 * sizeof(size_t) + 1];
    int len = snprintf(text, sizeof(text), "%zu", value);
    if (len > 0) suz_env_log(output_level, text, (size_t)len);
}

static atomic_flag output_lock = ATOMIC_FLAG_INIT;

static void lock_output(void) {
    while (atomic_flag_test_and_set_explicit(&output_lock, memory_order_acquire)) {}
}

void suz_log_begin(suz_log_level level, const char *source, size_t line) {
    lock_output();
    output_level = level;
    write_text(level == SUZ_LOG_WARN ? "warn" : "info");
    write_text(" ");
    write_text(source);
    write_text(":");
    write_position(line);
    write_text(": ");
}

void suz_log_text(const char *text, size_t len) {
    if (len != 0) suz_env_log(output_level, text, len);
}

void suz_log_u64(uint64_t value) {
    char text[20];
    size_t start = sizeof(text);
    do {
        text[--start] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0);
    suz_env_log(output_level, text + start, sizeof(text) - start);
}

void suz_log_i64(int64_t value) {
    if (value < 0) write_text("-");
    suz_log_u64(value < 0 ? UINT64_C(0) - (uint64_t)value : (uint64_t)value);
}

void suz_log_bool(bool value) {
    write_text(value ? "true" : "false");
}

void suz_log_end(void) {
    write_text("\n");
    atomic_flag_clear_explicit(&output_lock, memory_order_release);
}

void suz_panic_begin(const char *source, size_t line, size_t column) {
    lock_output();
    output_level = SUZ_LOG_PANIC;
    write_text("panic ");
    write_text(source);
    write_text(":");
    write_position(line);
    write_text(":");
    write_position(column);
    write_text(": ");
}

_Noreturn void suz_panic_end(void) {
    // 終了までロックを保持し、パニック診断の間に別の出力を挟まない。
    write_text("\n");
    suz_env_panic();
}

_Noreturn void suz_panic(const char *source, size_t line, size_t column, const char *reason) {
    suz_panic_begin(source, line, column);
    write_text(reason);
    suz_panic_end();
}

int suz_exact_compare_float(suz_exact a, double b) {
    uint64_t bits;
    memcpy(&bits, &b, sizeof(bits));
    unsigned exponent = (unsigned)((bits >> 52) & 2047);
    uint64_t mantissa = bits & ((UINT64_C(1) << 52) - 1);
    bool negative = (bits >> 63) != 0;
    if (exponent == 2047) return mantissa != 0 ? 2 : (negative ? 1 : -1);
    if (exponent != 0) mantissa |= UINT64_C(1) << 52;
    if (mantissa == 0) negative = false;
    if (a.negative != negative) return a.negative ? -1 : 1;
    int shift = exponent == 0 ? -1074 : (int)exponent - 1023 - 52;
    int order;
    if (shift >= 0) {
        if (shift > 75) order = -1;
        else {
            unsigned _BitInt(128) integer = (unsigned _BitInt(128))mantissa << shift;
            order = (a.magnitude > integer) - (a.magnitude < integer);
        }
    } else {
        unsigned right = (unsigned)-shift;
        uint64_t integer = right >= 64 ? 0 : mantissa >> right;
        bool fraction = right >= 64 ? mantissa != 0 : (mantissa & ((UINT64_C(1) << right) - 1)) != 0;
        order = (a.magnitude > integer) - (a.magnitude < integer);
        if (order == 0 && fraction) order = -1;
    }
    return negative ? -order : order;
}
suz_exact suz_exact_require(suz_exact a, const char *source) {
    if (a.failed) suz_panic(source, a.line, a.column, "integer division by zero [rt.div-zero]");
    return a;
}
size_t suz_format_exact(suz_exact a, char text[40]) {
    size_t start = 40;
    do {
        text[--start] = (char)('0' + a.magnitude % 10);
        a.magnitude /= 10;
    } while (a.magnitude != 0);
    if (a.negative) text[--start] = '-';
    size_t length = 40 - start;
    memmove(text, text + start, length);
    return length;
}
void suz_log_exact(suz_exact a) {
    char text[40];
    size_t length = suz_format_exact(a, text);
    suz_log_text(text, length);
}
