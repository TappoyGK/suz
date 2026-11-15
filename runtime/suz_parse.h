#ifndef SUZ_PARSE_H
#define SUZ_PARSE_H

#include "suz_char.h"
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

/* Parse one scalar from the entire text, or one quoted literal prefix.
   Ignore outputs on failure. Neither function allocates. */
bool suz_parse_char(const char *bytes, uint64_t length, uint32_t *output);
enum suz_char_literal_status {
    SUZ_CHAR_LITERAL_OK, SUZ_CHAR_LITERAL_SYNTAX, SUZ_CHAR_LITERAL_EMPTY_ESCAPE,
    SUZ_CHAR_LITERAL_TOO_MANY_DIGITS, SUZ_CHAR_LITERAL_SURROGATE,
    SUZ_CHAR_LITERAL_OUT_OF_RANGE
};
/* Same parser, with a status code for compiler diagnostics. */
uint32_t suz_parse_char_literal_status(const char *bytes, uint64_t length,
                                       uint32_t *output, uint64_t *consumed);
bool suz_parse_char_literal(const char *bytes, uint64_t length,
                             uint32_t *output, uint64_t *consumed);
#endif
