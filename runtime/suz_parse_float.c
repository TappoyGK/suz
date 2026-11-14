#include "suz_parse.h"
#include "suz_config.h"

#include <assert.h>
#include <string.h>

/* Every binary64 rounding boundary is m * 2^e with m < 2^54 and
   e >= -1075. Its terminating decimal has fewer than 770 significant
   digits (m * 5^1075 has fewer than 770 digits). Keeping 800 digits
   therefore distinguishes all boundaries; discarded nonzero digits only
   affect a tie. The same bound covers binary32.

   After the decimal-order checks, a denominator needs at most 10^1123,
   and a scaled numerator at most 800 decimal digits times 2^1074.
   Both fit in 4096 bits, including the rounding comparison. */
enum { DECIMAL_DIGITS = 800, WORDS = 128 };

typedef struct {
    uint32_t word[WORDS];
    unsigned used;
} big;

typedef struct {
    unsigned char digits[DECIMAL_DIGITS];
    unsigned used;
    bool sticky;
    bool nonzero;
    uint64_t count;
    uint64_t first;
} decimal;

static bool digit(unsigned char byte) { return byte >= '0' && byte <= '9'; }

static void collect(decimal *value, unsigned char byte) {
    if (!value->nonzero && byte != '0') {
        value->first = value->count;
        value->nonzero = true;
    }
    ++value->count;
    if (!value->nonzero) return;
    if (value->used < DECIMAL_DIGITS) value->digits[value->used++] = byte - '0';
    else if (byte != '0') value->sticky = true;
}

static bool digits(const char *bytes, uint64_t length, uint64_t *index, decimal *value) {
    if (*index == length || !digit((unsigned char)bytes[*index])) return false;
    while (*index < length) {
        unsigned char byte = (unsigned char)bytes[*index];
        if (digit(byte)) collect(value, byte);
        else if (byte == '_') {
            if (*index + 1 == length || !digit((unsigned char)bytes[*index + 1])) return false;
        } else break;
        ++*index;
    }
    return true;
}

/* One extra word permits cancellation with a mantissa's full uint64_t
   digit count, even when the written exponent exceeds UINT64_MAX.
   Values >= 2^65 can safely saturate: cancellation cannot bring them
   into the finite floating-point range. */
typedef struct { uint64_t low; unsigned high; } exponent;

static void exponent_digit(exponent *value, unsigned next) {
    if (value->high >= 2) return;
    uint64_t low = (value->low & UINT32_MAX) * 10 + next;
    uint64_t high = (value->low >> 32) * 10 + (low >> 32);
    value->low = (high << 32) | (low & UINT32_MAX);
    value->high = value->high * 10 + (unsigned)(high >> 32);
    if (value->high >= 2) value->high = 2;
}

static int order(exponent exp, bool exp_negative, uint64_t offset, bool offset_negative) {
    uint64_t magnitude;
    bool negative = exp_negative;
    if (exp.high >= 2) return negative ? -2000 : 2000;
    if (exp_negative == offset_negative) {
        if (exp.high || exp.low > 2000 || offset > 2000) return negative ? -2000 : 2000;
        magnitude = exp.low + offset;
    } else if (exp.high) {
        unsigned high = exp.high - (exp.low < offset ? 1 : 0);
        magnitude = exp.low - offset;
        if (high) return negative ? -2000 : 2000;
    } else if (exp.low >= offset) {
        magnitude = exp.low - offset;
    } else {
        magnitude = offset - exp.low;
        negative = offset_negative;
    }
    int result = magnitude > 2000 ? 2000 : (int)magnitude;
    return negative ? -result : result;
}

static void trim(big *value) {
    while (value->used && value->word[value->used - 1] == 0) --value->used;
}

static void multiply10(big *value, unsigned next) {
    uint64_t carry = next;
    for (unsigned i = 0; i < value->used; ++i) {
        uint64_t product = (uint64_t)value->word[i] * 10 + carry;
        value->word[i] = (uint32_t)product;
        carry = product >> 32;
    }
    if (carry) {
        assert(value->used < WORDS);
        value->word[value->used++] = (uint32_t)carry;
    }
}

static unsigned bit_length(const big *value) {
    if (!value->used) return 0;
    unsigned result = (value->used - 1) * 32;
    for (uint32_t top = value->word[value->used - 1]; top; top >>= 1) ++result;
    return result;
}

static int compare(const big *a, const big *b) {
    if (a->used != b->used) return a->used < b->used ? -1 : 1;
    for (unsigned i = a->used; i > 0; --i) {
        if (a->word[i - 1] != b->word[i - 1]) return a->word[i - 1] < b->word[i - 1] ? -1 : 1;
    }
    return 0;
}

static void shift(big *output, const big *input, unsigned bits) {
    assert(output != input);
    *output = (big){0};
    unsigned words = bits / 32;
    unsigned remainder = bits % 32;
    assert(input->used + words + (remainder != 0) <= WORDS);
    for (unsigned i = 0; i < input->used; ++i) {
        output->word[i + words] |= input->word[i] << remainder;
        if (remainder) output->word[i + words + 1] |= input->word[i] >> (32 - remainder);
    }
    output->used = input->used + words + (remainder != 0);
    trim(output);
}

static void subtract(big *a, const big *b) {
    uint64_t borrow = 0;
    for (unsigned i = 0; i < a->used; ++i) {
        uint64_t sub = (i < b->used ? b->word[i] : 0) + borrow;
        uint64_t original = a->word[i];
        a->word[i] = (uint32_t)(original - sub);
        borrow = original < sub;
    }
    assert(borrow == 0);
    trim(a);
}

static bool rounded(const decimal *value, int decimal_order, unsigned width, uint64_t *output) {
    unsigned fraction = width == 32 ? 23 : 52;
    int minimum = width == 32 ? -126 : -1022;
    int maximum = width == 32 ? 127 : 1023;
    int bias = width == 32 ? 127 : 1023;
    big numerator = {0}, denominator = {{1}, 1}, work;
    for (unsigned i = 0; i < value->used; ++i) multiply10(&numerator, value->digits[i]);
    int power = decimal_order - (int)value->used + 1;
    for (int i = 0; i < power; ++i) multiply10(&numerator, 0);
    for (int i = 0; i > power; --i) multiply10(&denominator, 0);

    int binary_order = (int)bit_length(&numerator) - (int)bit_length(&denominator);
    int comparison;
    if (binary_order >= 0) {
        shift(&work, &denominator, (unsigned)binary_order);
        comparison = compare(&numerator, &work);
    } else {
        shift(&work, &numerator, (unsigned)-binary_order);
        comparison = compare(&work, &denominator);
    }
    if (comparison < 0) --binary_order;
    if (binary_order > maximum) return false;
    int normal_order = binary_order < minimum ? minimum : binary_order;
    int unit = normal_order - (int)fraction;
    if (unit < 0) {
        shift(&work, &numerator, (unsigned)-unit);
        numerator = work;
    } else {
        shift(&work, &denominator, (unsigned)unit);
        denominator = work;
    }
    uint64_t significand = 0;
    int top = (int)bit_length(&numerator) - (int)bit_length(&denominator);
    assert(top < 64);
    for (int bit = top; bit >= 0; --bit) {
        shift(&work, &denominator, (unsigned)bit);
        if (compare(&numerator, &work) >= 0) {
            subtract(&numerator, &work);
            significand |= UINT64_C(1) << bit;
        }
    }
    shift(&work, &numerator, 1);
    comparison = compare(&work, &denominator);
    if (comparison > 0 || (comparison == 0 && (value->sticky || (significand & 1)))) ++significand;
    if (significand == (UINT64_C(1) << (fraction + 1))) {
        significand >>= 1;
        ++normal_order;
    }
    if (normal_order > maximum) return false;
    *output = significand < (UINT64_C(1) << fraction) ? significand :
        ((uint64_t)(normal_order + bias) << fraction) | (significand - (UINT64_C(1) << fraction));
    return true;
}

bool suz_parse_float_bits(const char *bytes, uint64_t length, unsigned width, uint64_t *output) {
    assert(width == 32 || width == 64);
    if (!length) return false;
    bool negative = bytes[0] == '-';
    uint64_t index = negative ? 1 : 0;
    uint64_t sign = negative ? UINT64_C(1) << (width - 1) : 0;
    uint64_t infinity = width == 32 ? UINT64_C(0x7f800000) : UINT64_C(0x7ff0000000000000);
    if (length - index == 3) {
        if (memcmp(bytes + index, "inf", 3) == 0) { *output = sign | infinity; return true; }
        if (memcmp(bytes + index, "nan", 3) == 0) {
            *output = sign | infinity | (UINT64_C(1) << (width == 32 ? 22 : 51));
            return true;
        }
    }
    decimal value = {0};
    if (!digits(bytes, length, &index, &value)) return false;
    uint64_t integer_digits = value.count;
    bool point = index < length && bytes[index] == '.';
    if (point) {
        ++index;
        if (!digits(bytes, length, &index, &value)) return false;
    }
    exponent exp = {0};
    bool exp_negative = false;
    if (point && index < length && bytes[index] == 'e') {
        ++index;
        if (index < length && (bytes[index] == '-' || bytes[index] == '+')) {
            exp_negative = bytes[index++] == '-';
        }
        if (index == length || !digit((unsigned char)bytes[index])) return false;
        for (; index < length; ++index) {
            unsigned char byte = (unsigned char)bytes[index];
            if (digit(byte)) exponent_digit(&exp, byte - '0');
            else if (byte == '_') {
                if (index + 1 == length || !digit((unsigned char)bytes[index + 1])) return false;
            } else return false;
        }
    }
    if (index != length) return false;
    if (!value.nonzero) { *output = sign; return true; }
    bool offset_negative = integer_digits <= value.first;
    uint64_t offset = offset_negative ? value.first - integer_digits + 1 : integer_digits - value.first - 1;
    int decimal_order = order(exp, exp_negative, offset, offset_negative);
    if (decimal_order > (width == 32 ? 38 : 308)) return false;
    if (decimal_order < (width == 32 ? -46 : -324)) { *output = sign; return true; }
    uint64_t result;
    if (!rounded(&value, decimal_order, width, &result)) return false;
    *output = sign | result;
    return true;
}

bool suz_parse_f32(const char *bytes, uint64_t length, float *output) {
    uint64_t wide;
    if (!suz_parse_float_bits(bytes, length, 32, &wide)) return false;
    uint32_t bits = (uint32_t)wide;
    memcpy(output, &bits, sizeof(bits));
    return true;
}

bool suz_parse_f64(const char *bytes, uint64_t length, double *output) {
    uint64_t bits;
    if (!suz_parse_float_bits(bytes, length, 64, &bits)) return false;
    memcpy(output, &bits, sizeof(bits));
    return true;
}
