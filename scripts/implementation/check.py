"""[impl.file-length] 行数検査と [impl.exceptions] の例外の照合を行う。"""

from pathlib import PurePosixPath
import re
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


def repository_files(root):
    """Git管理対象と無視されていない未追跡ファイルを、Git上の表記で返す。"""
    output = subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root,
    )
    names = set(output.decode('utf-8').split('\0')) - {''}
    return {name for name in names if (root / name).is_file()}


def source_counts(root, files=None):
    """Gitのファイル一覧から対象ソースのコード行数を数える。"""
    if files is None:
        files = repository_files(root)
    counts = {}
    for name in sorted(files):
        language = source_language(name)
        path = root / name
        if language:
            try:
                counts[name] = code_lines(path.read_text(encoding='utf-8'), language)
            except ValueError as error:
                raise ValueError(f'{name}: {error}') from error
    return counts


def read_exceptions(text, known_rules=None, *, files=None):
    """共通の形式を検査し、行数検査が扱う例外だけを返す。"""
    document = tomllib.loads(text)
    if not document:
        return {}
    if set(document) != {'exceptions'} or not isinstance(document['exceptions'], list):
        raise ValueError('Expected an exceptions array')
    result = {}
    seen = set()
    for entry in document['exceptions']:
        if not isinstance(entry, dict):
            raise ValueError('Each exception must be a table')
        required = {'rule', 'path', 'reason'}
        if not required <= set(entry) or set(entry) - required - {'item', 'max_code_lines'}:
            raise ValueError(f'Invalid exception fields: {entry}')
        rule = entry['rule']
        if (not isinstance(rule, str) or not re.fullmatch(r'impl\.[a-z][a-z-]*', rule)
                or (known_rules is not None and rule not in known_rules)):
            raise ValueError(f'Unknown or invalid exception rule: {rule}')
        name = entry['path']
        if not isinstance(name, str) or not name or '\\' in name:
            raise ValueError('Expected a repository-relative source path')
        path = PurePosixPath(name)
        if path.is_absolute() or '..' in path.parts or str(path) != name or name == '.':
            raise ValueError(f'Invalid exception path: {name}')
        if files is not None and name not in files:
            raise ValueError(f'{name}: exception source is missing [impl.exceptions]')
        if not isinstance(entry['reason'], str) or not entry['reason'].strip():
            raise ValueError(f'Missing exception reason: {name}')
        item = entry.get('item')
        if 'item' in entry and (not isinstance(item, str) or not item.strip()):
            raise ValueError(f'Invalid exception item: {name}')
        key = (rule, name, item)
        if key in seen:
            raise ValueError(f'Duplicate exception: {name}')
        seen.add(key)
        if rule != 'impl.file-length':
            if 'max_code_lines' in entry:
                raise ValueError(f'Unexpected code line limit: {name}')
            continue
        if not source_language(name):
            raise ValueError(f'Invalid file-length exception path: {name}')
        if 'max_code_lines' not in entry:
            raise ValueError(f'Missing exception limit: {name}')
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
    rules = set(re.findall(r'\*\*\[(impl\.[a-z-]+)\]\*\*',
                           (root / 'docs/implementation.md').read_text(encoding='utf-8')))
    files = repository_files(root)
    exceptions = read_exceptions((root / EXCEPTIONS).read_text(encoding='utf-8'), rules, files=files)
    counts = source_counts(root, files)
    return counts, exceptions, check_counts(counts, exceptions)
