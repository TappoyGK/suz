#include "suz_runtime.h"
#include "suz_env.h"
#include <inttypes.h>
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void suz_log_text(const char *text, size_t len) {
    if (fwrite(text, 1, len, stdout) != len) abort();
}

int main(int argc, char **argv) {
    if (argc == 2 && setlocale(LC_NUMERIC, argv[1]) == NULL) return 2;
    unsigned width;
    uint64_t bits;
    while (scanf("%u %" SCNx64, &width, &bits) == 2) {
        if (width == 32) {
            uint32_t narrow = (uint32_t)bits;
            float value;
            memcpy(&value, &narrow, sizeof(value));
            suz_log_f32(value);
        } else {
            double value;
            memcpy(&value, &bits, sizeof(value));
            suz_log_f64(value);
        }
        putchar('\n');
    }
    return ferror(stdin) || ferror(stdout) ? 1 : 0;
}
