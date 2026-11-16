"""[impl.file-length] 行数検査と [impl.exceptions] の例外の照合を行う。"""

from pathlib import PurePosixPath
import subprocess
import tomllib

from .source import code_lines

LIMIT = 500
EXCEPTIONS = 'compiler/tests/implementation-exceptions.toml'


def source_language(path):
    """検査対象をリポジトリ内の位置と拡張子で判定する。"""
    parts = PurePosixPath(path).parts
    if len(parts) < 2:
        return None
    if parts[0] == 'compiler' and path.endswith('.rs'):
        return 'rust'
    if parts[0] == 'runtime' and parts[1] != 'third_party' and path.endswith(('.c', '.h')):
        return 'c'
    return None


def source_counts(root):
    """Git管理対象と未追跡のソースを調べる。生成物などの無視対象は除く。"""
    output = subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root,
    )
    counts = {}
    for name in sorted(set(output.decode('utf-8').split('\0')) - {''}):
        language = source_language(name)
        path = root / name
        if language and path.is_file():
            try:
                counts[name] = code_lines(path.read_text(encoding='utf-8'), language)
            except ValueError as error:
                raise ValueError(f'{name}: {error}') from error
    return counts


def read_exceptions(text):
    """未知の項目や重複を拒否し、ファイル単位の例外を返す。"""
    document = tomllib.loads(text)
    if set(document) != {'exceptions'} or not isinstance(document['exceptions'], list):
        raise ValueError('Expected an exceptions array')
    result = {}
    for entry in document['exceptions']:
        if not isinstance(entry, dict):
            raise ValueError('Each exception must be a table')
        if set(entry) != {'rule', 'path', 'reason', 'max_code_lines'}:
            raise ValueError(f'Invalid file-length exception fields: {entry}')
        if entry['rule'] != 'impl.file-length':
            raise ValueError(f"Unsupported exception rule: {entry['rule']}")
        name = entry['path']
        if not isinstance(name, str) or not name or '\\' in name:
            raise ValueError('Expected a repository-relative source path')
        path = PurePosixPath(name)
        if path.is_absolute() or '..' in path.parts or str(path) != name or not source_language(name):
            raise ValueError(f'Invalid exception path: {name}')
        if not isinstance(entry['reason'], str) or not entry['reason'].strip():
            raise ValueError(f'Missing exception reason: {name}')
        limit = entry['max_code_lines']
        if type(limit) is not int or limit <= LIMIT:
            raise ValueError(f'Exception limit must exceed {LIMIT}: {name}')
        if name in result:
            raise ValueError(f'Duplicate exception: {name}')
        result[name] = limit
    return result


def check_counts(counts, exceptions):
    """超過と不要になった例外を、例外の適用前後でそれぞれ検出する。"""
    errors = []
    for name in sorted(exceptions):
        if name not in counts:
            errors.append(f'{name}: exception source is missing [impl.exceptions]')
        elif counts[name] <= LIMIT:
            errors.append(f'{name}: obsolete exception ({counts[name]} code lines) [impl.exceptions]')
    for name, count in sorted(counts.items()):
        limit = exceptions.get(name, LIMIT)
        if count > limit:
            errors.append(f'{name}: {count} code lines exceed limit {limit} [impl.file-length]')
    return errors


def check_repository(root):
    """ソースの実測値と、レビュー対象となる例外一覧を照合する。"""
    exceptions = read_exceptions((root / EXCEPTIONS).read_text(encoding='utf-8'))
    counts = source_counts(root)
    return counts, exceptions, check_counts(counts, exceptions)
