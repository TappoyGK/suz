#include "suz_runtime.h"

void suz_log_f32(float value) {
    char output[32];
    size_t length = suz_format_f32(value, output);
    suz_log_text(output, length);
}

void suz_log_f64(double value) {
    char output[32];
    size_t length = suz_format_f64(value, output);
    suz_log_text(output, length);
}
