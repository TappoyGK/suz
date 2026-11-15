#ifndef SUZ_RUNTIME_H
#define SUZ_RUNTIME_H

#include "suz_config.h"
#include "suz_char.h"

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Establish the default non-trapping, round-to-nearest floating-point environment
   before calling any Suz function. Abort if the host cannot establish it. */
void suz_runtime_init(void);

/* Supply the target Suz integer type's min/max bounds (unsigned min is zero).
   Both operands must already fit those bounds; result must be writable.
   Return true and write *result on success. Return false and leave *result
   unchanged on overflow or division by zero. Division and remainder both fail
   for signed min / -1. Arithmetic is performed only after these checks. */
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
void suz_log_begin(const char *source, size_t line);
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
