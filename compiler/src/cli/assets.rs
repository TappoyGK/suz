//! 生成Cと一緒に出力するランタイムのソース・ヘッダーをバイナリへ埋め込む。
//!
//! 利用元: [`crate::cli::emit`]。

macro_rules! bundled {
    ($($path:literal),* $(,)?) => {
        pub const FILES: &[(&str, &[u8])] = &[
            $(($path, include_bytes!(concat!("../../../", $path)))),*
        ];
    };
}

bundled!(
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "scripts/build-c.sh",
    "scripts/cflags.txt",
    "runtime/suz_config.h",
    "runtime/suz_env.h",
    "runtime/suz_env.c",
    "runtime/suz_runtime.h",
    "runtime/suz_runtime.c",
    "runtime/suz_text.h",
    "runtime/suz_parse.h",
    "runtime/suz_char.h",
    "runtime/suz_parse_struct.h",
    "runtime/suz_text.c",
    "runtime/suz_parse.c",
    "runtime/suz_parse_float.c",
    "runtime/suz_parse_struct.c",
    "runtime/suz_float.c",
    "runtime/suz_format_float.c",
    "runtime/third_party/ryu/README.md",
    "runtime/third_party/ryu/LICENSE-Boost",
    "runtime/third_party/ryu/LICENSE-Apache2",
    "runtime/third_party/ryu/ryu/common.h",
    "runtime/third_party/ryu/ryu/digit_table.h",
    "runtime/third_party/ryu/ryu/f2s_full_table.h",
    "runtime/third_party/ryu/ryu/d2s_intrinsics.h",
    "runtime/third_party/ryu/ryu/d2s.c",
    "runtime/third_party/ryu/ryu/f2s_intrinsics.h",
    "runtime/third_party/ryu/ryu/d2s_small_table.h",
    "runtime/third_party/ryu/ryu/ryu.h",
    "runtime/third_party/ryu/ryu/f2s.c",
    "runtime/third_party/ryu/ryu/d2s_full_table.h",
);
