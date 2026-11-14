#include "suz_env.h"

#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>

void *suz_env_alloc(size_t size) {
    return malloc(size);
}

void suz_env_free(void *ptr) {
    free(ptr);
}

void suz_env_log(const char *text, size_t len) {
#ifdef SIGPIPE
    /* A closed output pipe is an output failure, not a process failure. */
    (void)signal(SIGPIPE, SIG_IGN);
#endif
    while (len != 0) {
        errno = 0;
        size_t written = fwrite(text, 1, len, stderr);
        text += written;
        len -= written;
        if (ferror(stderr)) {
            int interrupted = errno == EINTR;
            clearerr(stderr);
            if (interrupted) continue;
            return;
        }
        if (written == 0) return;
    }
}

_Noreturn void suz_env_panic(void) {
    /* exit also attempts to flush buffered output. Flush failure does not
       change the requested exit status. */
    exit(101);
}
