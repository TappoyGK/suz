#ifndef SUZ_ENV_H
#define SUZ_ENV_H

#include <stddef.h>

void suz_env_log(const char *text, size_t len);
_Noreturn void suz_env_panic(void);

#endif
