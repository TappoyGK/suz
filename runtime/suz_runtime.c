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
    /* Reset exception masks and flags as well as rounding; fesetround alone
       would leave inherited floating-point traps enabled. */
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
    (void)max; /* Explicitly discard max to suppress unused-parameter warnings; no value is changed or checked. */
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
    /* Multiplication avoids shifting a negative signed value. All 64-bit
       operands and valid shift counts fit exactly in this intermediate. */
    _BitInt(128) value = (_BitInt(128))a * ((_BitInt(128))1 << count);
    _BitInt(128) limit = (_BitInt(128))1 << (width - 1);
    if (value < -limit || value >= limit) return false;
    *result = (int64_t)value;
    return true;
}

bool suz_try_shr_i64(int64_t a, uint64_t count, unsigned width, int64_t *result) {
    if (count >= width) return false;
    /* Shift a nonnegative value, then restore the sign. This gives arithmetic
       right shift without relying on C's implementation-defined negative shift. */
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

static void write_text(const char *text) {
    size_t len = strlen(text);
    if (len != 0) suz_env_log(text, len);
}

static void write_position(size_t value) {
    /* At most 3 decimal digits per byte, plus a terminating NUL. */
    char text[3 * sizeof(size_t) + 1];
    int len = snprintf(text, sizeof(text), "%zu", value);
    if (len > 0) suz_env_log(text, (size_t)len);
}

static atomic_flag output_lock = ATOMIC_FLAG_INIT;

static void lock_output(void) {
    while (atomic_flag_test_and_set_explicit(&output_lock, memory_order_acquire)) {}
}

void suz_log_begin(const char *level, const char *source, size_t line) {
    lock_output();
    write_text(level);
    write_text(" ");
    write_text(source);
    write_text(":");
    write_position(line);
    write_text(": ");
}

void suz_log_text(const char *text, size_t len) {
    if (len != 0) suz_env_log(text, len);
}

void suz_log_u64(uint64_t value) {
    char text[20];
    size_t start = sizeof(text);
    do {
        text[--start] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0);
    suz_env_log(text + start, sizeof(text) - start);
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

_Noreturn void suz_panic(const char *source, size_t line, size_t column, const char *reason) {
    lock_output();
    write_text("panic ");
    write_text(source);
    write_text(":");
    write_position(line);
    write_text(":");
    write_position(column);
    write_text(": ");
    write_text(reason);
    write_text("\n");
    suz_env_panic();
}
