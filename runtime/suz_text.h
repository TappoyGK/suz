#ifndef SUZ_TEXT_H
#define SUZ_TEXT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* インターン済みのSuzのsymbol。以下のビューは、不変でプロセス終了まで生存するバイト列を共有する。 */
struct suz_text_entry {
    const char *bytes;
    uint64_t length;
    uint32_t hash;
    uint64_t scalars;
};
typedef struct suz_text_entry const *suz_text;

/* 実体は不変。通常のビルドでもlenを外部関数呼び出しなしで取得する。 */
static inline int32_t suz_text_count(suz_text value) { return (int32_t)value->scalars; }

typedef struct {
    const char *source;
    size_t line;
    size_t column;
} suz_text_location;

/* Suzのmainより前、かつ文字列操作より前に一度呼ぶ。0は無制限を表す。
   初期化ではメモリを確保せず、エントリーは必要になってから登録する。 */
void suz_text_pool_init(uint64_t limit);

/* 入力はNULを含めてもよい有効なUTF-8。定数の入力はプロセス終了まで有効でなければならない。
   動的な入力は未登録の場合だけコピーする。入力の種類によらず、同じ内容なら常に同じポインターを返す。 */
suz_text suz_text_intern(const char *bytes, uint64_t length, bool constant,
                         suz_text_location location);
const char *suz_text_bytes(suz_text value);
uint64_t suz_text_length(suz_text value);
int suz_text_compare(suz_text left, suz_text right);

typedef struct {
    const char *bytes;
    uint64_t length;
    uint64_t scalars;
} suz_text_view;

/* これらの操作はメモリを確保せず、インターン表にもアクセスしない。
   切り出しに失敗した場合は出力を変えない。位置はUnicodeスカラー値の数で表す。 */
suz_text_view suz_text_as_view(suz_text value);
bool suz_text_slice(suz_text_view value, uint64_t start, uint64_t end,
                    suz_text_view *output);
bool suz_text_index(suz_text_view value, uint64_t index, uint32_t *output);
bool suz_text_view_equal(suz_text_view left, suz_text_view right);
int suz_text_view_compare(suz_text_view left, suz_text_view right);

static inline const char *suz_view_bytes(suz_text_view value) { return value.bytes; }
static inline uint64_t suz_view_length(suz_text_view value) { return value.length; }
/* ランタイムの使用量管理用であり、Suz言語のAPIではない。 */
uint64_t suz_text_pool_used(void);

typedef struct {
    char *bytes;
    uint64_t length;
    uint64_t capacity;
} suz_text_buffer;

/* {0}で初期化し、バッファと重ならない有効なUTF-8片を追加して、すべての経路で破棄する。
   一時領域はプールの使用量に含めない。 */
void suz_text_buffer_append(suz_text_buffer *buffer, const char *bytes,
                            uint64_t length, suz_text_location location);
void suz_text_buffer_u64(suz_text_buffer *buffer, uint64_t value, suz_text_location location);
void suz_text_buffer_i64(suz_text_buffer *buffer, int64_t value, suz_text_location location);
void suz_text_buffer_dispose(suz_text_buffer *buffer);

/* 構造体の入力全体の解析が成功した後、検証済みの引用付き文字列範囲を実体化する。
   メモリ確保の失敗は型変換の箇所に帰属させる。 */
suz_text suz_text_from_parsed(const char *bytes, uint64_t length, suz_text_location location);

#endif
