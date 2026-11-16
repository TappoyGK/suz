"""[impl.file-length] RustとCのコメントを除いたコード行を数える。"""

import re


# Rustのライフタイムを文字リテラルと誤認しないため、文字1個とエスケープだけを認める。
RUST_CHAR = re.compile(r"'(?:[^'\\\r\n]|\\(?:[nrt0\\'\"]|x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}))'")
# C23の数値内の桁区切りを、文字リテラルの開始と混同しない。
C_NUMBER = re.compile(r"(?:[0-9]|\.[0-9])(?:[eEpP][+-]|[a-zA-Z0-9_.]|'[a-zA-Z0-9_])*")
RAW_STRING = re.compile(r'(?:br|cr|r)(#*)"')


def code_lines(source, language):
    """文字列内のコメント記号を保ち、空行・コメントだけの行を除く。"""
    if language not in {'rust', 'c'}:
        raise ValueError(f'Unsupported source language: {language}')
    if source.startswith('\ufeff'):
        source = source[1:]
    # Cの行継続はコメントの解釈より先に処理する。各文字の元の行番号を保持する。
    chars, lines = [], []
    line, i = 1, 0
    while i < len(source):
        if language == 'c' and source[i] == '\\':
            end = i + 1
            if source[end:end + 2] == '\r\n':
                end += 1
            if source[end:end + 1] == '\n':
                i = end + 1
                line += 1
                continue
        char = source[i]
        chars.append(char)
        lines.append(line)
        line += char == '\n'
        i += 1
    text = ''.join(chars)
    counted = set()

    def mark(start, end):
        for index in range(start, end):
            if not text[index].isspace():
                counted.add(lines[index])

    i = 0
    while i < len(text):
        if text.startswith('//', i):
            end = text.find('\n', i + 2)
            i = len(text) if end < 0 else end
        elif text.startswith('/*', i):
            start = i
            depth = 1
            i += 2
            while i < len(text) and depth:
                if language == 'rust' and text.startswith('/*', i):
                    depth += 1
                    i += 2
                elif text.startswith('*/', i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            if depth:
                raise ValueError(f'line {lines[start]}: Unterminated block comment')
        else:
            start = i
            boundary = i == 0 or not (text[i - 1].isalnum() or text[i - 1] == '_')
            raw = RAW_STRING.match(text, i) if language == 'rust' and boundary else None
            char = RUST_CHAR.match(text, i) if language == 'rust' else None
            number = C_NUMBER.match(text, i) if language == 'c' and boundary else None
            if number:
                i = number.end()
            elif raw:
                closing = '"' + raw[1]
                end = text.find(closing, raw.end())
                if end < 0:
                    raise ValueError(f'line {lines[start]}: Unterminated raw string')
                i = end + len(closing)
            elif char:
                i = char.end()
            elif text[i] == '"' or (language == 'c' and text[i] == "'"):
                quote = text[i]
                i += 1
                while i < len(text):
                    if text[i] == '\\':
                        i += 2
                    elif text[i] == quote:
                        i += 1
                        break
                    else:
                        i += 1
                else:
                    raise ValueError(f'line {lines[start]}: Unterminated quoted literal')
                if i > len(text):
                    raise ValueError(f'line {lines[start]}: Unterminated escape')
            else:
                i += 1
            mark(start, i)
    return len(counted)
