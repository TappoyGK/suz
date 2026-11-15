#!/usr/bin/env python3
"""Decide whether an event changes build or test inputs."""

import json
import os
from pathlib import Path
import subprocess


def requires_checks(paths):
    for path in paths:
        if (
            not path.endswith('.md')
            or path in {
                'docs/spec.md',
                'docs/spec-cases.md',
                'runtime/third_party/ryu/README.md',
            }
            or path.startswith('docs/spec-cases/')
        ):
            return True
    return False


def changed_paths(event_name, event):
    if event_name == 'pull_request':
        pr = event['pull_request']
        revision = f"{pr['base']['sha']}...{pr['head']['sha']}"
    elif event_name == 'push' and event.get('before', '').strip('0'):
        revision = f"{event['before']}..{event['after']}"
    else:
        # Manual runs and newly created branches always receive full checks.
        return None
    result = subprocess.run(
        ['git', 'diff', '--name-only', '--no-renames', '-z', revision, '--'],
        check=True, capture_output=True,
    )
    return result.stdout.decode('utf-8').rstrip('\0').split('\0') if result.stdout else []


def main():
    event = json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text())
    paths = changed_paths(os.environ['GITHUB_EVENT_NAME'], event)
    required = paths is None or requires_checks(paths)
    with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
        output.write(f"required={str(required).lower()}\n")
    print('Build and test checks required.' if required else 'Documentation-only change; build and tests skipped.')


if __name__ == '__main__':
    main()
