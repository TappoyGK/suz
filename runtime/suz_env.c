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

void suz_env_log(suz_log_level level, const char *text, size_t len) {
    (void)level; /* この引数が意図的に未使用であることをコンパイラへ示す。 */
#ifdef SIGPIPE
    /* 閉じた出力パイプは出力の失敗として扱い、プロセスの異常終了にしない。 */
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
    /* exitも出力バッファのフラッシュを試みる。フラッシュに失敗しても、指定した終了状態は変えない。 */
    exit(101);
}
