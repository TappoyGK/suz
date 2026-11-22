#include "suz_arena_report.h"
#include "suz_arena.h"
#include "suz_runtime.h"
#include <assert.h>
#include <string.h>

/* 行数はCLIでは1..65535。公開Rust APIの値も同じ範囲で、計算量と容量に上限がある。 */
void suz_arena_tree_init(suz_arena_tree *tree, size_t capacity, const char *source, size_t line, size_t column) {
    assert(capacity > 0 && capacity <= UINT16_MAX);
    size_t lookup_capacity = 2;
    while (lookup_capacity < capacity * 2) lookup_capacity *= 2;
    size_t bytes = capacity * (sizeof(suz_arena_report_node) + sizeof(size_t)) + lookup_capacity * sizeof(size_t);
    tree->nodes = suz_env_alloc(bytes);
    if (!tree->nodes) suz_panic(source, line, column, "arena report allocation failed [rt.arena-report-storage]");
    tree->path = (size_t *)(tree->nodes + capacity);
    tree->lookup = tree->path + capacity;
    tree->lookup_capacity = lookup_capacity;
    memset(tree->lookup, 0, lookup_capacity * sizeof(size_t));
    tree->capacity = capacity;
    tree->count = 1;
    tree->active = NULL;
    tree->nodes[0] = (suz_arena_report_node){
        .source = source, .function = "main", .line = line, .column = column,
        .parent = SIZE_MAX, .first_child = SIZE_MAX, .last_child = SIZE_MAX, .next_sibling = SIZE_MAX
    };
}

/* 使用率を半分以下に保つオープンアドレス法。兄弟の列や進入履歴を走査しない。
   0は空欄、登録値はノード番号+1。表示順は別の兄弟リストで保持する。 */
size_t suz_arena_tree_find(suz_arena_tree *tree, const suz_arena_report_site *site, size_t parent) {
    uint64_t hash = (uint64_t)parent * UINT64_C(0x9e3779b97f4a7c15) + site->id;
    hash = (hash ^ (hash >> 30)) * UINT64_C(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27)) * UINT64_C(0x94d049bb133111eb);
    hash ^= hash >> 31;
    size_t slot = (size_t)hash & (tree->lookup_capacity - 1);
    while (tree->lookup[slot]) {
        size_t id = tree->lookup[slot] - 1;
        if (tree->nodes[id].parent == parent && tree->nodes[id].site_id == site->id) return id;
        slot = (slot + 1) & (tree->lookup_capacity - 1);
    }
    if (tree->count == tree->capacity) {
        tree->nodes[parent].omitted = true;
        return SIZE_MAX;
    }
    size_t id = tree->count++;
    tree->nodes[id] = (suz_arena_report_node){
        .site_id = site->id, .source = site->source, .function = site->function,
        .line = site->line, .column = site->column,
        .parent = parent, .first_child = SIZE_MAX, .last_child = SIZE_MAX, .next_sibling = SIZE_MAX,
        .depth = tree->nodes[parent].depth + 1
    };
    if (tree->nodes[parent].last_child != SIZE_MAX) tree->nodes[tree->nodes[parent].last_child].next_sibling = id;
    else tree->nodes[parent].first_child = id;
    tree->nodes[parent].last_child = id;
    tree->lookup[slot] = id + 1;
    return id;
}

void suz_arena_tree_destroy(suz_arena_tree *tree) {
    suz_env_free(tree->nodes);
    *tree = (suz_arena_tree){0};
}

static void text(const char *value) { suz_log_text(value, strlen(value)); }
static size_t digits(size_t value) {
    size_t length = 1;
    while (value >= 10) { value /= 10; ++length; }
    return length;
}

/* テキスト表では制御文字・非ASCIIバイトをエスケープし、パスによる改行や桁ずれを防ぐ。
   CSVは元のUTF-8を保持し、引用符を二重にする。 */
static size_t escaped(const char *value, bool write) {
    size_t length = 0;
    for (const unsigned char *p = (const unsigned char *)value; *p; ++p) {
        if (*p < 32 || *p >= 127 || *p == '\\') {
            if (write) {
                const char hex[] = "0123456789abcdef";
                char escape[] = { '\\', 'x', hex[*p >> 4], hex[*p & 15] };
                suz_log_text(escape, sizeof(escape));
            }
            length += 4;
        } else {
            if (write) suz_log_text((const char *)p, 1);
            ++length;
        }
    }
    return length;
}

static void csv_string(const char *value) {
    text("\"");
    for (const char *p = value; *p; ++p) {
        if (*p == '"') text("\"");
        suz_log_text(p, 1);
    }
    text("\"");
}

static void spaces(size_t count) {
    while (count--) text(" ");
}

static size_t site(const suz_arena_report_node *node, size_t id, bool write) {
    if (id == 0) { if (write) text("main"); return 4; }
    size_t length = escaped(node->source, write) + 2 + digits(node->line) + digits(node->column);
    if (write) { text(":"); suz_log_u64(node->line); text(":"); suz_log_u64(node->column); }
    return length;
}

/* 整数の書式化は固定のローカル配列だけで行い、ロケールやstdioへ依存しない。 */
static char *decimal(char *out, size_t value) {
    char reversed[20];
    size_t count = 0;
    do { reversed[count++] = (char)('0' + value % 10); value /= 10; } while (value);
    while (count) *out++ = reversed[--count];
    *out = 0;
    return out;
}

static void amount(char buffer[64], size_t value, bool bytes) {
    if (bytes || value < 1024) {
        char *end = decimal(buffer, value);
        if (!bytes) strcpy(end, " B");
        return;
    }
    size_t unit = value < 1024 * 1024 ? 1024 : 1024 * 1024;
    size_t whole = value / unit;
    size_t tenth = ((value % unit) * 10 + unit / 2) / unit;
    if (tenth == 10) { ++whole; tenth = 0; }
    char *end = decimal(buffer, whole);
    *end++ = '.';
    *end++ = (char)('0' + tenth);
    strcpy(end, unit == 1024 ? " KiB" : " MiB");
}

static void csv_row(const suz_arena_report_node *node, size_t id, uint64_t reserved, size_t blocks, size_t expansions) {
    suz_log_u64(id); text(",");
    if (id) suz_log_u64(node->parent);
    text(","); suz_log_u64(node->depth); text(",");
    /* ソースと位置を別列にして、パス中のコロンと混同しない。 */
    csv_string(node->source); text(","); suz_log_u64(node->line); text(","); suz_log_u64(node->column); text(",");
    csv_string(node->function); text(","); suz_log_u64(node->peak);
    text(node->recursive ? ",true" : ",false");
    text(node->omitted ? ",true," : ",false,");
    if (id == 0) suz_log_u64(reserved);
    text(","); if (id == 0) suz_log_u64(blocks);
    text(","); if (id == 0) suz_log_u64(expansions);
}

void suz_arena_tree_output(suz_arena_tree *tree, size_t peak, bool initial_block, suz_arena_report_format format) {
    assert(tree->active == NULL);
    tree->nodes[0].peak = peak;
    size_t blocks = 0;
    for (suz_arena_chunk *chunk = suz_arena.first; chunk; chunk = chunk->next) ++blocks;
    size_t expansions = blocks - (initial_block ? 1 : 0);
    size_t site_width = 5, function_width = 8, amount_width = 4;
    for (size_t i = 0; i < tree->count; ++i) {
        const suz_arena_report_node *node = &tree->nodes[i];
        size_t width = node->depth * 4 + site(node, i, false);
        if (width > site_width) site_width = width;
        width = escaped(node->function, false);
        if (width > function_width) function_width = width;
        char buffer[64]; amount(buffer, node->peak, format == SUZ_ARENA_REPORT_TREE_BYTES);
        if (strlen(buffer) > amount_width) amount_width = strlen(buffer);
    }
    /* 全行を1つのロック内で出す。出力中に領域の確保も再帰呼出しも行わない。 */
    suz_log_begin(SUZ_LOG_INFO, NULL, 0);
    if (format == SUZ_ARENA_REPORT_CSV) text("id,parent,depth,source,line,column,function,peak,recursive,omitted,reserved,blocks,expansions");
    else {
        text("arena"); spaces(site_width - 5 + 2);
        text("function"); spaces(function_width - 8 + 2);
        spaces(amount_width - 4); text("peak");
    }
    size_t id = 0;
    for (;;) {
        text("\n");
        const suz_arena_report_node *node = &tree->nodes[id];
        if (format == SUZ_ARENA_REPORT_CSV) csv_row(node, id, suz_arena.total, blocks, expansions);
        else {
            size_t ancestor = id;
            for (size_t d = node->depth; d > 0; --d) {
                tree->path[d - 1] = ancestor;
                ancestor = tree->nodes[ancestor].parent;
            }
            for (size_t d = 0; d < node->depth; ++d) {
                bool last = tree->nodes[tree->path[d]].next_sibling == SIZE_MAX;
                text(d + 1 == node->depth ? (last ? "└── " : "├── ") : (last ? "    " : "│   "));
            }
            size_t width = node->depth * 4 + site(node, id, true);
            spaces(site_width - width + 2);
            width = escaped(node->function, true);
            spaces(function_width - width + 2);
            char buffer[64]; amount(buffer, node->peak, format == SUZ_ARENA_REPORT_TREE_BYTES);
            spaces(amount_width - strlen(buffer)); text(buffer);
            if (node->recursive) text(" [recursive]");
            if (node->omitted) text(" [omitted]");
        }
        if (node->first_child != SIZE_MAX) { id = node->first_child; continue; }
        while (id && tree->nodes[id].next_sibling == SIZE_MAX) id = tree->nodes[id].parent;
        if (!id) break;
        id = tree->nodes[id].next_sibling;
    }
    if (format != SUZ_ARENA_REPORT_CSV) {
        text("\nreserved="); suz_log_u64(suz_arena.total);
        text(" blocks="); suz_log_u64(blocks);
        text(" expansions="); suz_log_u64(expansions);
    }
    suz_log_end();
}

/* ブロックはmain終了まで解放されないため、終了時の合計が確保容量の最大値。
   再利用で数を増やさないよう、実際に残っているブロックを数える。 */
void suz_arena_report_summary(size_t peak, bool initial_block) {
    uint64_t blocks = 0;
    for (suz_arena_chunk *chunk = suz_arena.first; chunk; chunk = chunk->next) ++blocks;
    suz_log_begin(SUZ_LOG_INFO, NULL, 0);
    suz_log_text("arena peak=", sizeof("arena peak=") - 1);
    suz_log_u64(peak);
    suz_log_text(" reserved=", sizeof(" reserved=") - 1);
    suz_log_u64(suz_arena.total);
    suz_log_text(" blocks=", sizeof(" blocks=") - 1);
    suz_log_u64(blocks);
    suz_log_text(" expansions=", sizeof(" expansions=") - 1);
    suz_log_u64(blocks - (initial_block ? 1 : 0));
    suz_log_end();
}
