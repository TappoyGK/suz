#include "suz_parse.h"
#include "suz_char.h"
#include "suz_text.h"
#include "suz_runtime.h"
#include "suz_env.h"

#include <string.h>
#include <stdlib.h>

struct suz_text_entry {
    const char *bytes;
    uint64_t length;
    uint32_t hash;
    uint64_t scalars;
};

static struct {
    suz_text *slots;
    uint64_t capacity;
    uint64_t count;
    uint64_t used;
    uint64_t limit;
} pool;

static char *decimal(char *out, uint64_t value) {
    char digits[20];
    size_t count = 0;
    do {
        digits[count++] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0);
    while (count != 0) *out++ = digits[--count];
    return out;
}

static char *literal(char *out, const char *text) {
    while (*text != '\0') *out++ = *text++;
    return out;
}

static _Noreturn void length_failure(suz_text_location location) {
    suz_panic(location.source, location.line, location.column,
              "text length exceeds the supported range [rt.symbol-allocation]");
}

static uint64_t add(uint64_t left, uint64_t right, suz_text_location location) {
    if (right > UINT64_MAX - left) length_failure(location);
    return left + right;
}

static size_t allocation_size(uint64_t size, suz_text_location location) {
    if (size > SIZE_MAX) length_failure(location);
    return (size_t)size;
}

static void *allocate(uint64_t size, bool retained, suz_text_location location) {
    size_t request = allocation_size(size, location);
    uint64_t total = retained ? add(pool.used, size, location) : pool.used;
    if (retained && pool.limit != 0 && total > pool.limit) {
        /* Fixed storage suffices for three u64 values and these literals.
           Reporting exhaustion must not ask the allocator for more memory. */
        char reason[192];
        char *out = literal(reason, "text pool limit exceeded: limit=");
        out = decimal(out, pool.limit);
        out = literal(out, ", used=");
        out = decimal(out, pool.used);
        out = literal(out, ", required=");
        out = decimal(out, size);
        out = literal(out, " [rt.symbol-allocation]");
        *out = '\0';
        suz_panic(location.source, location.line, location.column, reason);
    }
    void *memory = suz_env_alloc(request);
    if (memory == NULL) {
        suz_panic(location.source, location.line, location.column,
                  "text memory allocation failed [rt.symbol-allocation]");
    }
    if (retained) pool.used = total;
    return memory;
}

void suz_text_pool_init(uint64_t limit) {
    pool.limit = limit;
}

uint64_t suz_text_pool_used(void) {
    return pool.used;
}

static uint32_t hash_bytes(const char *bytes, uint64_t length) {
    uint32_t hash = UINT32_C(2166136261);
    for (uint64_t i = 0; i < length; ++i) {
        hash ^= (unsigned char)bytes[i];
        hash *= UINT32_C(16777619);
    }
    return hash;
}

static uint64_t slot_for(suz_text *slots, uint64_t capacity, uint32_t hash,
                         const char *bytes, uint64_t length) {
    uint64_t index = hash & (capacity - 1);
    while (slots[index] != NULL) {
        suz_text entry = slots[index];
        if (entry->hash == hash && entry->length == length &&
            (length == 0 || memcmp(entry->bytes, bytes, (size_t)length) == 0)) break;
        index = (index + 1) & (capacity - 1);
    }
    return index;
}

static void grow(suz_text_location location) {
    uint64_t capacity = pool.capacity == 0 ? 16 : add(pool.capacity, pool.capacity, location);
    if (capacity > UINT64_MAX / sizeof(suz_text)) length_failure(location);
    uint64_t size = capacity * sizeof(suz_text);
    suz_text *slots = allocate(size, true, location);
    /* Assign NULL rather than assuming its representation is all zero bits. */
    for (uint64_t i = 0; i < capacity; ++i) slots[i] = NULL;
    for (uint64_t i = 0; i < pool.capacity; ++i) {
        suz_text entry = pool.slots[i];
        if (entry != NULL) {
            slots[slot_for(slots, capacity, entry->hash, entry->bytes, entry->length)] = entry;
        }
    }
    suz_env_free(pool.slots);
    pool.used -= pool.capacity * sizeof(suz_text);
    pool.slots = slots;
    pool.capacity = capacity;
}

suz_text suz_text_intern(const char *bytes, uint64_t length, bool constant,
                         suz_text_location location) {
    allocation_size(length, location);
    uint32_t hash = hash_bytes(bytes, length);
    if (pool.capacity != 0) {
        uint64_t index = slot_for(pool.slots, pool.capacity, hash, bytes, length);
        if (pool.slots[index] != NULL) return pool.slots[index];
    }
    if (pool.count >= pool.capacity / 2) grow(location);
    uint64_t size = constant ? sizeof(struct suz_text_entry)
                            : add(sizeof(struct suz_text_entry), length, location);
    struct suz_text_entry *entry = allocate(size, true, location);
    entry->length = length;
    entry->scalars = 0;
    for (uint64_t i = 0; i < length; ++i) {
        if (((unsigned char)bytes[i] & 0xc0) != 0x80) ++entry->scalars;
    }
    entry->hash = hash;
    if (constant) {
        entry->bytes = bytes;
    } else {
        char *body = (char *)(entry + 1);
        if (length != 0) memcpy(body, bytes, (size_t)length);
        entry->bytes = body;
    }
    pool.slots[slot_for(pool.slots, pool.capacity, hash, bytes, length)] = entry;
    ++pool.count;
    return entry;
}

const char *(suz_text_bytes)(suz_text value) {
    return value->bytes;
}

uint64_t (suz_text_length)(suz_text value) {
    return value->length;
}

int suz_text_compare(suz_text left, suz_text right) {
    if (left == right) return 0;
    uint64_t common = left->length < right->length ? left->length : right->length;
    int order = common == 0 ? 0 : memcmp(left->bytes, right->bytes, (size_t)common);
    if (order != 0) return order;
    return (left->length > right->length) - (left->length < right->length);
}

void suz_text_buffer_append(suz_text_buffer *buffer, const char *bytes,
                            uint64_t length, suz_text_location location) {
    uint64_t needed = add(buffer->length, length, location);
    allocation_size(needed, location);
    if (needed > buffer->capacity) {
        uint64_t capacity = buffer->capacity == 0 ? 64 : buffer->capacity;
        while (capacity < needed) {
            if (capacity > SIZE_MAX / 2 || capacity > UINT64_MAX / 2) {
                capacity = needed;
                break;
            }
            capacity *= 2;
        }
        char *memory = allocate(capacity, false, location);
        if (buffer->length != 0) memcpy(memory, buffer->bytes, (size_t)buffer->length);
        suz_env_free(buffer->bytes);
        buffer->bytes = memory;
        buffer->capacity = capacity;
    }
    if (length != 0) memcpy(buffer->bytes + buffer->length, bytes, (size_t)length);
    buffer->length = needed;
}

void suz_text_buffer_dispose(suz_text_buffer *buffer) {
    suz_env_free(buffer->bytes);
    *buffer = (suz_text_buffer){0};
}

void suz_text_buffer_u64(suz_text_buffer *buffer, uint64_t value, suz_text_location location) {
    char digits[20];
    size_t length = (size_t)(decimal(digits, value) - digits);
    suz_text_buffer_append(buffer, digits, length, location);
}

void suz_text_buffer_i64(suz_text_buffer *buffer, int64_t value, suz_text_location location) {
    if (value < 0) suz_text_buffer_append(buffer, "-", 1, location);
    suz_text_buffer_u64(buffer, value < 0 ? UINT64_C(0) - (uint64_t)value : (uint64_t)value, location);
}

suz_text suz_text_from_parsed(const char *bytes, uint64_t length, suz_text_location location) {
    if (memchr(bytes, '\\', (size_t)length) == NULL) {
        return suz_text_intern(bytes + 1, length - 2, false, location);
    }
    suz_text_buffer buffer = {0};
    suz_text_buffer_append(&buffer, bytes, length, location);
    uint64_t consumed, decoded;
    /* The validation pass has checked this exact immutable span. */
    bool valid = suz_parse_string(bytes, length, &consumed, buffer.bytes, &decoded);
    if (!valid || consumed != length) abort();
    suz_text result = suz_text_intern(buffer.bytes, decoded, false, location);
    suz_text_buffer_dispose(&buffer);
    return result;
}


suz_text_view suz_text_as_view(suz_text value) {
    return (suz_text_view){value->bytes, value->length, value->scalars};
}

bool suz_text_slice(suz_text_view value, uint64_t start, uint64_t end,
                    suz_text_view *output) {
    if (start > end || end > value.scalars) return false;
    uint64_t byte = 0;
    for (uint64_t scalar = 0; scalar < start; ++scalar) {
        do { ++byte; } while (byte < value.length &&
                              ((unsigned char)value.bytes[byte] & 0xc0) == 0x80);
    }
    uint64_t first = byte;
    if (end == value.scalars) {
        byte = value.length;
    } else {
        for (uint64_t scalar = start; scalar < end; ++scalar) {
            do { ++byte; } while (byte < value.length &&
                                  ((unsigned char)value.bytes[byte] & 0xc0) == 0x80);
        }
    }
    *output = (suz_text_view){first == 0 ? value.bytes : value.bytes + first,
                              byte - first, end - start};
    return true;
}

bool suz_text_index(suz_text_view value, uint64_t index, uint32_t *output) {
    if (index >= value.scalars) return false;
    suz_text_view scalar;
    if (!suz_text_slice(value, index, index + 1, &scalar)) return false;
    uint64_t width;
    return suz_char_decode(scalar.bytes, scalar.length, output, &width);
}

int suz_text_view_compare(suz_text_view left, suz_text_view right) {
    uint64_t common = left.length < right.length ? left.length : right.length;
    int order = common == 0 ? 0 : memcmp(left.bytes, right.bytes, (size_t)common);
    if (order != 0) return order;
    return (left.length > right.length) - (left.length < right.length);
}
