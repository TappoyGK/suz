#ifndef SUZ_ARENA_H
#define SUZ_ARENA_H

#include "suz_config.h"
#include <stddef.h>
#include <stdint.h>

/* コンパイラ内部のAPI。使用位置の保存・復帰は生成Cでusedを読み書きする。
   復帰先は現在の使用位置以下の、対応する領域進入時の値でなければならない。
   生存する参照の検査はコンパイラが行い、ランタイムは参照をたどらない。 */
typedef struct {
    unsigned char *bytes;
    size_t capacity;
    size_t used;
} suz_arena_state;
extern suz_arena_state suz_arena;

/* arenaを使うプログラムだけが、Suzのmainより前に1回呼ぶ。
   capacityは1以上。環境層から確保した領域はプロセス終了まで保持する。 */
void suz_arena_init(uint64_t capacity, const char *source, size_t line, size_t column);

/* 初期化後に呼ぶ。sizeは1以上、alignmentは対象C型のalignofであり、
   max_align_t以下とする。初期値は呼び出し前に評価し、成功後に格納する。 */
void *suz_arena_alloc(size_t size, size_t alignment, const char *source, size_t line, size_t column);

#endif
