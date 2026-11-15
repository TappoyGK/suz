#ifndef SUZ_ENV_H
#define SUZ_ENV_H

#include "suz_config.h"
#include <stddef.h>

void *suz_env_alloc(size_t size);
void suz_env_free(void *ptr);
void suz_env_log(const char *text, size_t len);
_Noreturn void suz_env_panic(void);

#endif
