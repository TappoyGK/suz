#ifndef SUZ_ENV_H
#define SUZ_ENV_H

#include "suz_config.h"
#include <stddef.h>

typedef enum {
    SUZ_LOG_INFO,
    SUZ_LOG_WARN,
    SUZ_LOG_PANIC,
} suz_log_level;

void *suz_env_alloc(size_t size);
void suz_env_free(void *ptr);
void suz_env_log(suz_log_level level, const char *text, size_t len);
_Noreturn void suz_env_panic(void);

#endif
