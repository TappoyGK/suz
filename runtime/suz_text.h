#ifndef SUZ_TEXT_H
#define SUZ_TEXT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* Interned Suz symbol. Views below share its immutable, process-lifetime bytes. */
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

typedef struct {
    const char *bytes;
    uint64_t length;
    uint64_t scalars;
} suz_text_view;

/* These operations do not allocate or access the intern table. Failed
   extraction leaves output untouched; positions count Unicode scalars. */
suz_text_view suz_text_as_view(suz_text value);
bool suz_text_slice(suz_text_view value, uint64_t start, uint64_t end,
                    suz_text_view *output);
bool suz_text_index(suz_text_view value, uint64_t index, uint32_t *output);
int suz_text_view_compare(suz_text_view left, suz_text_view right);

static inline const char *suz_view_bytes(suz_text_view value) { return value.bytes; }
static inline uint64_t suz_view_length(suz_text_view value) { return value.length; }
/* The generic selector is unevaluated; the selected call evaluates value once. */
#define suz_text_bytes(value) _Generic((value), suz_text_view: suz_view_bytes, default: suz_text_bytes)(value)
#define suz_text_length(value) _Generic((value), suz_text_view: suz_view_length, default: suz_text_length)(value)

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
