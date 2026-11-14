#ifndef SUZ_TEXT_H
#define SUZ_TEXT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct suz_text_entry const *suz_text;

typedef struct {
    const char *source;
    size_t line;
    size_t column;
} suz_text_location;

/* Call once before Suz main, before any text operation. Zero means unlimited.
   Initialization is allocation-free; entries are registered lazily. */
void suz_text_pool_init(uint64_t limit);

/* Input is valid UTF-8, including embedded NUL. Constant input must remain
   valid until process exit; dynamic input is copied only when absent.
   Equal contents always return the same pointer, regardless of input kind. */
suz_text suz_text_intern(const char *bytes, uint64_t length, bool constant,
                         suz_text_location location);
const char *suz_text_bytes(suz_text value);
uint64_t suz_text_length(suz_text value);
int suz_text_compare(suz_text left, suz_text right);

/* Runtime accounting, not a Suz language API. */
uint64_t suz_text_pool_used(void);

typedef struct {
    char *bytes;
    uint64_t length;
    uint64_t capacity;
} suz_text_buffer;

/* Start with {0}; append valid UTF-8 fragments that do not alias the buffer,
   then dispose on every path.
   Temporary storage does not contribute to pool accounting. */
void suz_text_buffer_append(suz_text_buffer *buffer, const char *bytes,
                            uint64_t length, suz_text_location location);
void suz_text_buffer_u64(suz_text_buffer *buffer, uint64_t value, suz_text_location location);
void suz_text_buffer_i64(suz_text_buffer *buffer, int64_t value, suz_text_location location);
void suz_text_buffer_dispose(suz_text_buffer *buffer);

/* Materialize an already validated quoted text span after the entire struct
   input has succeeded. Allocation failures are attributed to the cast. */
suz_text suz_text_from_parsed(const char *bytes, uint64_t length, suz_text_location location);

#endif
