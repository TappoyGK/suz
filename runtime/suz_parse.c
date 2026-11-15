#include "suz_parse.h"

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
        /* Unsigned overflow retains exactly the low bits required by as%.
           No accumulator size limits the number of input digits. */
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

bool suz_parse_string(const char *bytes, uint64_t length, uint64_t *consumed,
                       char *output, uint64_t *decoded_length) {
    if (!length || bytes[0] != '`') return false;
    uint64_t written = 0;
    for (uint64_t index = 1; index < length; ++index) {
        char c = bytes[index];
        if (c == '`') {
            *consumed = index + 1;
            *decoded_length = written;
            return true;
        }
        if (c == '\r' || c == '\n') return false;
        if (c == '$' && index + 1 < length && bytes[index + 1] == '{') return false;
        if (c == '\\') {
            if (++index == length) return false;
            switch (bytes[index]) {
                case '0': c = '\0'; break;
                case 'n': c = '\n'; break;
                case 'r': c = '\r'; break;
                case 't': c = '\t'; break;
                case '\\': case '`': case '$': c = bytes[index]; break;
                default: return false;
            }
        }
        if (output) output[written] = c;
        ++written;
    }
    return false;
}

bool suz_parse_char(const char *bytes, uint64_t length, uint32_t *output) {
    uint32_t value;
    uint64_t consumed;
    if (!suz_char_decode(bytes, length, &value, &consumed) || consumed != length) return false;
    *output = value;
    return true;
}

bool suz_parse_char_literal(const char *bytes, uint64_t length,
                             uint32_t *output, uint64_t *consumed) {
    if (length < 3 || bytes[0] != '\'') return false;
    uint64_t index = 1;
    uint32_t value;
    if (bytes[index] == '\\') {
        ++index;
        switch (bytes[index++]) {
            case '0': value = 0; break;
            case 'n': value = '\n'; break;
            case 'r': value = '\r'; break;
            case 't': value = '\t'; break;
            case '\\': value = '\\'; break;
            case '\'': value = '\''; break;
            case 'u': {
                if (index >= length || bytes[index++] != '{') return false;
                value = 0;
                unsigned digits = 0;
                while (index < length && bytes[index] != '}') {
                    unsigned char c = (unsigned char)bytes[index++];
                    unsigned digit;
                    if (c >= '0' && c <= '9') digit = c - '0';
                    else if (c >= 'a' && c <= 'f') digit = c - 'a' + 10;
                    else if (c >= 'A' && c <= 'F') digit = c - 'A' + 10;
                    else return false;
                    if (++digits > 6) return false;
                    value = (value << 4) | digit;
                }
                if (!digits || index >= length || !suz_char_valid(value)) return false;
                ++index;
                break;
            }
            default: return false;
        }
    } else {
        if (bytes[index] == '\'' || bytes[index] == '\n' || bytes[index] == '\r') return false;
        uint64_t count;
        if (!suz_char_decode(bytes + index, length - index, &value, &count)) return false;
        index += count;
    }
    if (index >= length || bytes[index++] != '\'') return false;
    *output = value;
    *consumed = index;
    return true;
}
