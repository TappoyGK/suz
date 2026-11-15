#ifndef SUZ_CHAR_H
#define SUZ_CHAR_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

static inline bool suz_char_valid(uint32_t value) {
    return value <= 0x10ffff && (value < 0xd800 || value > 0xdfff);
}

/* Decode one scalar, accepting only canonical UTF-8. Outputs change on success. */
static inline bool suz_char_decode(const char *bytes, uint64_t length,
                                   uint32_t *output, uint64_t *consumed) {
    if (!length) return false;
    unsigned char first = (unsigned char)bytes[0];
    unsigned count;
    uint32_t value, minimum;
    if (first < 0x80) { count = 1; value = first; minimum = 0; }
    else if (first >= 0xc2 && first <= 0xdf) { count = 2; value = first & 0x1f; minimum = 0x80; }
    else if (first >= 0xe0 && first <= 0xef) { count = 3; value = first & 0xf; minimum = 0x800; }
    else if (first >= 0xf0 && first <= 0xf4) { count = 4; value = first & 7; minimum = 0x10000; }
    else return false;
    if (length < count) return false;
    for (unsigned i = 1; i < count; ++i) {
        unsigned char byte = (unsigned char)bytes[i];
        if ((byte & 0xc0) != 0x80) return false;
        value = (value << 6) | (byte & 0x3f);
    }
    if (value < minimum || !suz_char_valid(value)) return false;
    *output = value;
    *consumed = count;
    return true;
}

/* value is a Unicode scalar. Output has room for four bytes. */
static inline size_t suz_char_encode(uint32_t value, char output[4]) {
    if (value < 0x80) { output[0] = (char)value; return 1; }
    if (value < 0x800) {
        output[0] = (char)(0xc0 | (value >> 6));
        output[1] = (char)(0x80 | (value & 0x3f));
        return 2;
    }
    if (value < 0x10000) {
        output[0] = (char)(0xe0 | (value >> 12));
        output[1] = (char)(0x80 | ((value >> 6) & 0x3f));
        output[2] = (char)(0x80 | (value & 0x3f));
        return 3;
    }
    output[0] = (char)(0xf0 | (value >> 18));
    output[1] = (char)(0x80 | ((value >> 12) & 0x3f));
    output[2] = (char)(0x80 | ((value >> 6) & 0x3f));
    output[3] = (char)(0x80 | (value & 0x3f));
    return 4;
}
#endif
