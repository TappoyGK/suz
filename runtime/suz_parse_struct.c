#include "suz_parse_struct.h"
#include "suz_config.h"
#include <string.h>

typedef struct {
    const char *bytes;
    uint64_t length, index, line;
} reader;

static bool first(unsigned char c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
}
static bool rest(unsigned char c) { return first(c) || (c >= '0' && c <= '9'); }
static bool space(reader *r) {
    while (r->index < r->length) {
        char c = r->bytes[r->index];
        if (c == '\r') {
            if (r->index + 1 == r->length || r->bytes[r->index + 1] != '\n') return false;
            ++r->index;
            c = '\n';
        }
        if (c == '\n') ++r->line;
        else if (c != ' ' && c != '\t') break;
        ++r->index;
    }
    return true;
}
static bool symbol(reader *r, char expected) {
    if (!space(r) || r->index == r->length || r->bytes[r->index] != expected) return false;
    ++r->index;
    return true;
}
static bool name(reader *r, uint64_t *start, uint64_t *length) {
    if (!space(r) || r->index == r->length || !first((unsigned char)r->bytes[r->index])) return false;
    *start = r->index++;
    while (r->index < r->length && rest((unsigned char)r->bytes[r->index])) ++r->index;
    *length = r->index - *start;
    return true;
}
static bool equal(reader *r, uint64_t start, uint64_t length, const char *expected) {
    return length == strlen(expected) && memcmp(r->bytes + start, expected, (size_t)length) == 0;
}
static bool named(reader *r, const char *expected) {
    if (strchr(expected, '[') == NULL) {
        uint64_t start, length;
        return name(r, &start, &length) && equal(r, start, length, expected);
    }
    /* 型引数を含む名前もスキーマと照合する。字句間の空白は通常の構造体名と同じ扱い。 */
    while (*expected) {
        if (*expected == ' ') { ++expected; continue; }
        if (first((unsigned char)*expected)) {
            uint64_t start, length;
            size_t size = 1;
            while (rest((unsigned char)expected[size])) ++size;
            if (!name(r, &start, &length) || length != size ||
                memcmp(r->bytes + start, expected, size) != 0) return false;
            expected += size;
        } else {
            if (!symbol(r, *expected++)) return false;
        }
    }
    return true;
}
static bool value(reader *r, const suz_parse_type *type, suz_parsed_value *output);

static bool structure(reader *r, const suz_parse_type *type, suz_parsed_value *output) {
    if (!named(r, type->name) || !symbol(r, '{')) return false;
    uint64_t opening_line = r->line;
    size_t count = 0;
    for (;;) {
        uint64_t start, length;
        if (!name(r, &start, &length)) return false;
        size_t field = 0;
        while (field < type->count && !equal(r, start, length, type->fields[field].name)) ++field;
        if (field == type->count) return false;
        const suz_parse_field *entry = &type->fields[field];
        /* 構造体は空でないので、最初の葉を使って、そのフィールド全体が既に指定されたかも記録できる。 */
        if (output[entry->slot].seen || !symbol(r, ':') ||
            !value(r, entry->type, output + entry->slot)) return false;
        ++count;
        uint64_t end_line = r->line;
        if (!space(r) || r->index == r->length) return false;
        if (r->bytes[r->index] == '}') {
            ++r->index;
            return count == type->count && opening_line == r->line;
        }
        if (r->bytes[r->index] != ',' || end_line != r->line) return false;
        ++r->index;
        if (!space(r) || r->index == r->length) return false;
        if (r->bytes[r->index] == '}') {
            ++r->index;
            return count == type->count && opening_line != r->line;
        }
    }
}

static bool value(reader *r, const suz_parse_type *type, suz_parsed_value *output) {
    if (!space(r) || r->index == r->length) return false;
    if (type->kind == SUZ_PARSE_STRUCT) return structure(r, type, output);
    if (type->kind == SUZ_PARSE_CHAR) {
        uint32_t scalar;
        uint64_t consumed;
        if (!suz_parse_char_literal(r->bytes + r->index, r->length - r->index, &scalar, &consumed)) return false;
        r->index += consumed;
        output->bits = scalar;
    } else if (type->kind == SUZ_PARSE_TEXT) {
        uint64_t consumed, decoded;
        if (!suz_parse_string(r->bytes + r->index, r->length - r->index, &consumed, NULL, &decoded)) return false;
        output->start = r->index;
        output->length = consumed;
        r->index += consumed;
    } else if (type->kind == SUZ_PARSE_ENUM) {
        uint64_t start, length;
        if (!named(r, type->name) || !symbol(r, '.') || !name(r, &start, &length)) return false;
        size_t variant = 0;
        while (variant < type->count && !equal(r, start, length, type->fields[variant].name)) ++variant;
        if (variant == type->count) return false;
        output->bits = variant;
    } else {
        uint64_t start = r->index;
        while (r->index < r->length) {
            char c = r->bytes[r->index];
            if (c == ' ' || c == '\t' || c == '\r' || c == '\n' || c == ',' || c == '}') break;
            ++r->index;
        }
        const char *bytes = r->bytes + start;
        uint64_t length = r->index - start;
        bool truth;
        switch (type->kind) {
            case SUZ_PARSE_INTEGER:
                if (!suz_parse_integer(bytes, length, type->bits, type->signed_integer, false, &output->bits)) return false;
                break;
            case SUZ_PARSE_FLOAT:
                if (!suz_parse_float_bits(bytes, length, type->bits, &output->bits)) return false;
                break;
            case SUZ_PARSE_BOOL:
                if (!suz_parse_bool(bytes, length, &truth)) return false;
                output->bits = truth;
                break;
            default: return false;
        }
    }
    output->seen = true;
    return true;
}

bool suz_parse_struct(const char *bytes, uint64_t length, const suz_parse_type *schema,
                       suz_parsed_value *output) {
    if (!length || !first((unsigned char)bytes[0])) return false;
    memset(output, 0, schema->slots * sizeof(*output));
    reader input = {bytes, length, 0, 0};
    return structure(&input, schema, output) && input.index == length;
}
float suz_parsed_f32(uint64_t wide) {
    uint32_t bits = (uint32_t)wide;
    float value;
    memcpy(&value, &bits, sizeof(value));
    return value;
}
double suz_parsed_f64(uint64_t bits) {
    double value;
    memcpy(&value, &bits, sizeof(value));
    return value;
}
