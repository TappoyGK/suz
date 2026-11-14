#include "suz_runtime.h"
#include "suz_env.h"

void suz_log_f32(float value) {
    char output[32];
    size_t length = suz_format_f32(value, output);
    suz_env_log(output, length);
}

void suz_log_f64(double value) {
    char output[32];
    size_t length = suz_format_f64(value, output);
    suz_env_log(output, length);
}
