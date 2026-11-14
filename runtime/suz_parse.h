#ifndef SUZ_PARSE_H
#define SUZ_PARSE_H

#include <stdbool.h>
#include <stdint.h>

/* Parse the complete byte slice, without allocation or locale dependence.
   bits is 8, 16, 32, or 64. The integer result is a two's-complement bit
   pattern; output is written only on success. */
bool suz_parse_integer(const char *bytes, uint64_t length, unsigned bits,
                       bool signed_target, bool wrap, uint64_t *output);
bool suz_parse_bool(const char *bytes, uint64_t length, bool *output);

/* Direct nearest-even conversion from Suz decimal syntax to IEEE bits.
   width is 32 or 64. Finite overflow fails; subnormal values and signed
   zero succeed. Parsing uses fixed scratch storage, without allocation. */
bool suz_parse_float_bits(const char *bytes, uint64_t length, unsigned width, uint64_t *output);
bool suz_parse_f32(const char *bytes, uint64_t length, float *output);
bool suz_parse_f64(const char *bytes, uint64_t length, double *output);

/* Read one quoted text value. Output may be NULL for validation; otherwise
   its capacity is at least length bytes. Consumed includes both backquotes.
   Decoded output may alias the beginning of input. Ignore outputs on failure. */
bool suz_parse_string(const char *bytes, uint64_t length, uint64_t *consumed,
                       char *output, uint64_t *decoded_length);

#endif
