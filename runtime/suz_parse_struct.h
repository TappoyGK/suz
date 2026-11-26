#ifndef SUZ_PARSE_STRUCT_H
#define SUZ_PARSE_STRUCT_H
#include "suz_parse.h"
#include <stddef.h>

enum suz_parse_kind { SUZ_PARSE_INTEGER, SUZ_PARSE_FLOAT, SUZ_PARSE_BOOL,
                      SUZ_PARSE_TEXT, SUZ_PARSE_ENUM, SUZ_PARSE_STRUCT, SUZ_PARSE_CHAR };

typedef struct suz_parse_type suz_parse_type;
typedef struct {
    const char *name;
    const suz_parse_type *type;
    size_t slot;
} suz_parse_field;

struct suz_parse_type {
    unsigned kind;
    unsigned bits;
    bool signed_integer;
    const char *name;
    const suz_parse_field *fields;
    size_t count;
    size_t slots;
};

typedef struct {
    uint64_t bits;
    uint64_t start;
    uint64_t length;
    uint64_t decoded_length;
    uint64_t scalars;
    bool seen;
} suz_parsed_value;

/* スキーマは空でない非循環のSuz構造体。出力は宣言順のスカラーの葉ごとに一項目、schema->slots項目を持つ。
   メモリ確保とインターンは行わない。失敗時の出力は使わない。文字列の範囲には二重引用符を含む。 */
bool suz_parse_struct(const char *bytes, uint64_t length, const suz_parse_type *schema,
                       suz_parsed_value *output);
float suz_parsed_f32(uint64_t bits);
double suz_parsed_f64(uint64_t bits);
#endif
