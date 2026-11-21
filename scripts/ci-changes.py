#!/usr/bin/env python3
"""変更ファイルからCIの検査を省略できるか判断し、GitHub Actionsへ結果を渡す。"""

import json
import os
from pathlib import Path
import subprocess


def requires_checks(paths):
    """コードや検証用の文書を含む場合はTrue。説明文だけならFalse。"""
    for path in paths:
        if (
            not path.endswith('.md')
            or path in {
                'README.md',
                'docs/spec.md',
                'docs/spec-cases.md',
                'runtime/third_party/ryu/README.md',
            }
            or path.startswith('docs/spec-cases/')
        ):
            return True
    return False


def changed_paths(event_name, event):
    """変更ファイルの一覧を返す。Noneは絞り込めないため検査が必要、空リストは変更なし。"""
    if event_name == 'pull_request':
        # PRでは分岐点からの変更、pushでは更新前と更新後の変更を調べる。
        pr = event['pull_request']
        revision = f"{pr['base']['sha']}...{pr['head']['sha']}"
    elif event_name == 'push' and event.get('before', '').strip('0'):
        # rev-parseで比較元がコミットとして存在するか確認する。見つからない場合は終了値1。
        base = subprocess.run(
            ['git', 'rev-parse', '--verify', '--quiet', f"{event['before']}^{{commit}}"],
            capture_output=True,
        )
        if base.returncode == 1:
            # 履歴を戻したpushでは、比較元が取得した履歴に含まれないことがある。
            # 現在のコミットが有効な場合だけ、変更の絞り込みをやめて全検査する。
            subprocess.run(
                ['git', 'rev-parse', '--verify', f"{event['after']}^{{commit}}"],
                check=True, capture_output=True,
            )
            print('Push base commit is unavailable; running full checks.')
            return None
        # 比較元の不在以外のGitエラーは、検査の実行で代用せず失敗として報告する。
        base.check_returncode()
        revision = f"{event['before']}..{event['after']}"
    else:
        # 手動実行と新規ブランチでは、ファイルによる検査省略を行わない。
        return None
    # 改名は旧ファイルの削除と新ファイルの追加として扱い、コード削除を見落とさない。
    # -zはファイル名の区切りをNULにして、名前に空白や改行があっても区別する。
    result = subprocess.run(
        ['git', 'diff', '--name-only', '--no-renames', '-z', revision, '--'],
        check=True, capture_output=True,
    )
    return result.stdout.decode('utf-8').rstrip('\0').split('\0') if result.stdout else []


def main():
    # GitHubが用意するイベントJSONを読み、後続ジョブが使うrequiredを出力する。
    event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
    paths = changed_paths(os.environ['GITHUB_EVENT_NAME'], event)
    required = paths is None or requires_checks(paths)
    with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
        output.write(f"required={str(required).lower()}\n")
    print('Build and test checks required.' if required else 'Documentation-only change; build and tests skipped.')


if __name__ == '__main__':
    main()
