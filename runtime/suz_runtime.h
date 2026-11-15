#ifndef SUZ_RUNTIME_H
#define SUZ_RUNTIME_H

#include "suz_config.h"
#include "suz_env.h"
#include "suz_char.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Internal exact integers. Range analysis proves that every successful
   operation fits the permitted signed or unsigned 128-bit range. */
typedef struct {
    unsigned _BitInt(128) magnitude;
    bool negative;
    bool failed;
    size_t line;
    size_t column;
} suz_exact;
static inline suz_exact suz_exact_make(bool negative, unsigned _BitInt(128) magnitude) {
    return (suz_exact){magnitude, negative && magnitude != 0, false, 0, 0};
}
static inline suz_exact suz_exact_i64(int64_t n) {
    return suz_exact_make(n < 0, n < 0 ? UINT64_C(0) - (uint64_t)n : (uint64_t)n);
}
static inline suz_exact suz_exact_neg(suz_exact a) {
    a.negative = !a.negative && a.magnitude != 0;
    return a;
}
static inline int suz_exact_compare(suz_exact a, suz_exact b) {
    if (a.negative != b.negative) return a.negative ? -1 : 1;
    int order = (a.magnitude > b.magnitude) - (a.magnitude < b.magnitude);
    return a.negative ? -order : order;
}
static inline suz_exact suz_exact_add(suz_exact a, suz_exact b) {
    if (a.negative == b.negative) return suz_exact_make(a.negative, a.magnitude + b.magnitude);
    if (a.magnitude >= b.magnitude) return suz_exact_make(a.negative, a.magnitude - b.magnitude);
    return suz_exact_make(b.negative, b.magnitude - a.magnitude);
}
static inline suz_exact suz_exact_sub(suz_exact a, suz_exact b) {
    return suz_exact_add(a, suz_exact_neg(b));
}
static inline suz_exact suz_exact_mul(suz_exact a, suz_exact b) {
    return suz_exact_make(a.negative != b.negative, a.magnitude * b.magnitude);
}
static inline suz_exact suz_exact_divrem(suz_exact a, suz_exact b, bool remainder, size_t line, size_t column) {
    if (b.magnitude == 0) return (suz_exact){0, false, true, line, column};
    return suz_exact_make(remainder ? a.negative : a.negative != b.negative,
                         remainder ? a.magnitude % b.magnitude : a.magnitude / b.magnitude);
}
static inline bool suz_exact_fits(suz_exact a, bool is_signed, unsigned bits) {
    if (a.failed) return false;
    if (a.negative) return is_signed && a.magnitude <= ((unsigned _BitInt(128))1 << (bits - 1));
    return a.magnitude <= (((unsigned _BitInt(128))1 << (bits - (is_signed ? 1 : 0))) - 1);
}
static inline uint64_t suz_exact_bits(suz_exact a) {
    return a.negative ? UINT64_C(0) - (uint64_t)a.magnitude : (uint64_t)a.magnitude;
}
static inline int64_t suz_exact_signed(suz_exact a) {
    if (!a.negative) return (int64_t)a.magnitude;
    return -(int64_t)(a.magnitude - 1) - 1;
}
static inline double suz_exact_f64(suz_exact a) {
    double n = (double)a.magnitude;
    return a.negative ? -n : n;
}
static inline float suz_exact_f32(suz_exact a) {
    float n = (float)a.magnitude;
    return a.negative ? -n : n;
}
/* Compare the mathematical values without rounding the integer.
   Return -1, 0, 1, or 2 (unordered). */
int suz_exact_compare_float(suz_exact a, double b);
suz_exact suz_exact_require(suz_exact a, const char *source);
size_t suz_format_exact(suz_exact a, char output[40]);
void suz_log_exact(suz_exact a);

/* Establish the default non-trapping, round-to-nearest floating-point environment
   before calling any Suz function. Abort if the host cannot establish it. */
void suz_runtime_init(void);

/* Supply the target Suz integer type's min/max bounds (unsigned min is zero).
   Both operands must already fit those bounds; result must be writable.
   Return true and write *result on success. Return false and leave *result
   unchanged on overflow or division by zero. Division and remainder both fail
   for signed min / -1. Division and remainder are evaluated only after checking
   those failure conditions; addition, subtraction and multiplication use C23
   checked arithmetic before checking the target bounds. */
bool suz_try_add_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_sub_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_mul_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_div_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_rem_i64(int64_t a, int64_t b, int64_t min, int64_t max, int64_t *result);
bool suz_try_add_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);
bool suz_try_sub_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);
bool suz_try_mul_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);
bool suz_try_div_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);
bool suz_try_rem_u64(uint64_t a, uint64_t b, uint64_t max, uint64_t *result);

/* width is 8, 16, 32 or 64; a must fit that signed/unsigned type.
   Convert any Suz integer count to uint64_t before calling. Negative counts
   then exceed width and fail the same check as other out-of-range counts.
   Return false without changing *result for count >= width or left-shift
   overflow; otherwise write the result and return true. result must be writable. */
bool suz_try_shl_i64(int64_t a, uint64_t count, unsigned width, int64_t *result);
bool suz_try_shr_i64(int64_t a, uint64_t count, unsigned width, int64_t *result);
bool suz_try_shl_u64(uint64_t a, uint64_t count, unsigned width, uint64_t *result);
bool suz_try_shr_u64(uint64_t a, uint64_t count, unsigned width, uint64_t *result);

_Noreturn void suz_panic(const char *source, size_t line, size_t column, const char *reason);

/* Evaluate all interpolation values before begin. Keep begin/end paired;
   writers only emit already evaluated values. */
void suz_log_begin(suz_log_level level, const char *source, size_t line);
void suz_log_text(const char *text, size_t len);
void suz_log_i64(int64_t value);
void suz_log_u64(uint64_t value);
void suz_log_bool(bool value);
static inline void suz_log_char(uint32_t value) {
    char bytes[4];
    size_t length = suz_char_encode(value, bytes);
    suz_log_text(bytes, length);
}
/* Output needs 32 bytes; return the length without a terminating NUL. */
size_t suz_format_f32(float value, char output[32]);
size_t suz_format_f64(double value, char output[32]);
void suz_log_f32(float value);
void suz_log_f64(double value);
void suz_log_end(void);

#endif
