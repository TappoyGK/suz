#include "suz_parse.h"
#include "suz_unicode_cat.h"

#include <string.h>

static bool digit(unsigned char byte) {
    return byte >= '0' && byte <= '9';
}

bool suz_parse_integer(const char *bytes, uint64_t length, unsigned bits,
                       bool signed_target, bool wrap, uint64_t *output) {
    if (length == 0) return false;
    const bool negative = bytes[0] == '-';
    uint64_t index = negative ? 1 : 0;
    if (index == length || (!wrap && !signed_target && negative)) return false;
    const uint64_t mask = UINT64_MAX >> (64 - bits);
    const uint64_t limit = signed_target ? (mask >> 1) + (negative ? 1 : 0) : mask;
    uint64_t value = 0;
    for (; index < length; ++index) {
        const unsigned char byte = (unsigned char)bytes[index];
        if (byte == '_') {
            if (index == 0 || index + 1 == length ||
                !digit((unsigned char)bytes[index - 1]) ||
                !digit((unsigned char)bytes[index + 1])) return false;
            continue;
        }
        if (!digit(byte)) return false;
        const unsigned next = byte - '0';
        if (!wrap && (value > limit / 10 ||
                      (value == limit / 10 && next > limit % 10))) return false;
        /* 符号なしのオーバーフローにより、as%に必要な下位ビットが正確に残る。
           累積値の型の大きさによって入力桁数を制限しない。 */
        value = value * 10 + next;
    }
    *output = (negative ? UINT64_C(0) - value : value) & mask;
    return true;
}

bool suz_parse_integer_sat(const char *bytes, uint64_t length, unsigned bits,
                           bool signed_target, uint64_t *output) {
    if (length == 0) return false;
    const bool negative = bytes[0] == '-';
    uint64_t index = negative ? 1 : 0;
    if (index == length) return false;
    const uint64_t mask = UINT64_MAX >> (64 - bits);
    if (!signed_target && negative) {
        for (; index < length; ++index) {
            const unsigned char byte = (unsigned char)bytes[index];
            if (byte == '_') {
                if (index == 0 || index + 1 == length ||
                    !digit((unsigned char)bytes[index - 1]) ||
                    !digit((unsigned char)bytes[index + 1])) return false;
                continue;
            }
            if (!digit(byte)) return false;
        }
        *output = 0;
        return true;
    }
    const uint64_t limit = signed_target ? (mask >> 1) + (negative ? 1 : 0) : mask;
    uint64_t value = 0;
    for (; index < length; ++index) {
        const unsigned char byte = (unsigned char)bytes[index];
        if (byte == '_') {
            if (index == 0 || index + 1 == length ||
                !digit((unsigned char)bytes[index - 1]) ||
                !digit((unsigned char)bytes[index + 1])) return false;
            continue;
        }
        if (!digit(byte)) return false;
        const unsigned next = byte - '0';
        if (value > limit / 10 || (value == limit / 10 && next > limit % 10)) {
            for (++index; index < length; ++index) {
                const unsigned char rest = (unsigned char)bytes[index];
                if (rest == '_') {
                    if (index == 0 || index + 1 == length ||
                        !digit((unsigned char)bytes[index - 1]) ||
                        !digit((unsigned char)bytes[index + 1])) return false;
                    continue;
                }
                if (!digit(rest)) return false;
            }
            *output = negative ? ((UINT64_C(0) - ((mask >> 1) + 1)) & mask)
                               : (signed_target ? (mask >> 1) : mask);
            return true;
        }
        value = value * 10 + next;
    }
    *output = (negative ? UINT64_C(0) - value : value) & mask;
    return true;
}

bool suz_parse_bool(const char *bytes, uint64_t length, bool *output) {
    if (length == 4 && memcmp(bytes, "true", 4) == 0) {
        *output = true;
        return true;
    }
    if (length == 5 && memcmp(bytes, "false", 5) == 0) {
        *output = false;
        return true;
    }
    return false;
}

/* `{H}`（Hは1〜6桁の16進数）を解析する。bytes[0]は'{'を想定する。 */
static uint32_t suz_parse_unicode_escape(const char *bytes, uint64_t length,
                                         uint32_t *output, uint64_t *consumed) {
    if (length == 0 || bytes[0] != '{') return SUZ_CHAR_LITERAL_SYNTAX;
    uint64_t index = 1;
    uint32_t value = 0;
    unsigned digits = 0;
    while (index < length && bytes[index] != '}') {
        unsigned char c = (unsigned char)bytes[index++];
        unsigned digit;
        if (c >= '0' && c <= '9') digit = c - '0';
        else if (c >= 'a' && c <= 'f') digit = c - 'a' + 10;
        else if (c >= 'A' && c <= 'F') digit = c - 'A' + 10;
        else return SUZ_CHAR_LITERAL_SYNTAX;
        if (++digits > 6) return SUZ_CHAR_LITERAL_TOO_MANY_DIGITS;
        value = (value << 4) | digit;
    }
    if (!digits) return SUZ_CHAR_LITERAL_EMPTY_ESCAPE;
    if (index >= length) return SUZ_CHAR_LITERAL_SYNTAX;
    if (value >= 0xd800 && value <= 0xdfff) return SUZ_CHAR_LITERAL_SURROGATE;
    if (value > 0x10ffff) return SUZ_CHAR_LITERAL_OUT_OF_RANGE;
    *output = value;
    *consumed = index + 1;
    return SUZ_CHAR_LITERAL_OK;
}

bool suz_parse_string_counts(const char *bytes, uint64_t length, uint64_t *consumed,
                       char *output, uint64_t *decoded_length, uint64_t *scalars) {
    if (!length || bytes[0] != '"') return false;
    uint64_t written = 0;
    *scalars = 0;
    for (uint64_t index = 1; index < length; ++index) {
        char c = bytes[index];
        if (c == '"') {
            *consumed = index + 1;
            *decoded_length = written;
            return true;
        }
        if (c == '\r' || c == '\n') return false;
        if (c == '$' && index + 1 < length && bytes[index + 1] == '{') return false;
        if (c == '\\') {
            if (++index == length) return false;
            if (bytes[index] == 'u') {
                uint32_t scalar;
                uint64_t esc_len;
                if (suz_parse_unicode_escape(bytes + index + 1, length - index - 1, &scalar, &esc_len) != SUZ_CHAR_LITERAL_OK) {
                    return false;
                }
                char encoded[4];
                size_t enc_len = suz_char_encode(scalar, encoded);
                if (output) memcpy(output + written, encoded, enc_len);
                written += enc_len;
                ++*scalars;
                index += esc_len;
                continue;
            }
            switch (bytes[index]) {
                case '0': c = '\0'; break;
                case 'n': c = '\n'; break;
                case 'r': c = '\r'; break;
                case 't': c = '\t'; break;
                case '\\': case '"': case '$': c = bytes[index]; break;
                default: return false;
            }
        }
        if (((unsigned char)c & 0xc0) != 0x80) ++*scalars;
        if (output) output[written] = c;
        ++written;
    }
    return false;
}

bool suz_parse_string(const char *bytes, uint64_t length, uint64_t *consumed,
                      char *output, uint64_t *decoded_length) {
    uint64_t scalars;
    return suz_parse_string_counts(bytes, length, consumed, output, decoded_length, &scalars);
}

bool suz_parse_char(const char *bytes, uint64_t length, uint32_t *output) {
    uint32_t value;
    uint64_t consumed;
    if (!suz_char_decode(bytes, length, &value, &consumed) || consumed != length) return false;
    *output = value;
    return true;
}

uint32_t suz_parse_char_literal_status(const char *bytes, uint64_t length,
                             uint32_t *output, uint64_t *consumed) {
    if (length < 2 || bytes[0] != '\\') return SUZ_CHAR_LITERAL_SYNTAX;
    uint32_t first;
    uint64_t first_len;
    if (!suz_char_decode(bytes + 1, length - 1, &first, &first_len)) return SUZ_CHAR_LITERAL_SYNTAX;
    if (first == 'u' && 1 + first_len < length && bytes[1 + first_len] == '{') {
        uint32_t value;
        uint64_t esc_len;
        uint32_t status = suz_parse_unicode_escape(
            bytes + 1 + first_len, length - 1 - first_len, &value, &esc_len);
        if (status != SUZ_CHAR_LITERAL_OK) return status;
        *output = value;
        *consumed = 1 + first_len + esc_len;
        return SUZ_CHAR_LITERAL_OK;
    }
    if (suz_unicode_is_disallowed_char_literal(first)) return SUZ_CHAR_LITERAL_DISALLOWED;
    if (suz_unicode_is_char_name_unit(first)) {
        uint64_t end = 1 + first_len;
        uint64_t count = 1;
        while (end < length) {
            uint32_t next;
            uint64_t next_len;
            if (!suz_char_decode(bytes + end, length - end, &next, &next_len) ||
                !suz_unicode_is_char_name_unit(next)) {
                break;
            }
            end += next_len;
            ++count;
        }
        if (count == 1) {
            *output = first;
            *consumed = end;
            return SUZ_CHAR_LITERAL_OK;
        }
        const char *name = bytes + 1;
        uint64_t name_len = end - 1;
        uint32_t value;
        if (name_len == 3 && memcmp(name, "nul", 3) == 0) value = 0x0000;
        else if (name_len == 3 && memcmp(name, "tab", 3) == 0) value = 0x0009;
        else if (name_len == 7 && memcmp(name, "newline", 7) == 0) value = 0x000A;
        else if (name_len == 6 && memcmp(name, "return", 6) == 0) value = 0x000D;
        else if (name_len == 5 && memcmp(name, "space", 5) == 0) value = 0x0020;
        else return SUZ_CHAR_LITERAL_UNKNOWN_NAME;
        *output = value;
        *consumed = end;
        return SUZ_CHAR_LITERAL_OK;
    }
    *output = first;
    *consumed = 1 + first_len;
    return SUZ_CHAR_LITERAL_OK;
}

bool suz_parse_char_literal(const char *bytes, uint64_t length,
                             uint32_t *output, uint64_t *consumed) {
    return suz_parse_char_literal_status(bytes, length, output, consumed) == SUZ_CHAR_LITERAL_OK;
}

bool suz_char_needs_unicode_escape(uint32_t scalar) {
    return suz_unicode_is_disallowed_char_literal(scalar) || suz_unicode_is_mark(scalar);
}
