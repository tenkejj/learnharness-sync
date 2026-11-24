"""Read-only publication scan of worktree, INDEX bytes and all reachable trees.
Only locations are reported, never matched values. Not a complete secret detector.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

PATTERNS = [re.compile(p) for p in (
    rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
    rb'gh[pousr]_[A-Za-z0-9]{30,}', rb'github_pat_[A-Za-z0-9_]{40,}',
    rb'sk-[A-Za-z0-9_-]{32,}', rb'AKIA[0-9A-Z]{16}',
    rb'(?<![A-Za-z0-9_./])/home/(?!runner(?:/|\b))[A-Za-z0-9_.-]+/',
    rb'(?<![A-Za-z0-9_./])/Users/[A-Za-z0-9_.-]+/',
)]
FORBIDDEN = {'auth.json', 'config.xml', 'ledger.json', 'journal.json', 'SANDBOX.json', 'identity.json', 'devices.json', 'ENGINE_PROFILE.json', 'smoke-report.json'}
PRIVATE_PARTS = {'secrets', 'transport', 'backups', 'node_modules', '.aws', 'transport-config-backups', 'objects', 'manifests', 'staging'}
BINARY = {'.png', '.ico', '.icns'}


def private(path):
    path = Path(path)
    return path.name in FORBIDDEN or path.name.startswith('.env') or any(part in PRIVATE_PARTS for part in path.parts)


def audit(root):
    root = Path(root)
    def git(*args):
        result = subprocess.run(['git', '-C', str(root), *args], capture_output=True)
        if result.returncode:
            raise RuntimeError('Git audit could not complete')
        return result.stdout
    findings = []
    def report(scope, path, issue):
        findings.append({'scope': scope, 'file': path, 'issue': issue})
    def content(scope, path, data):
        if any(pattern.search(data) for pattern in PATTERNS):
            report(scope, path, 'credential/private-path pattern; manual review required')
    paths = {p for p in git('ls-files', '--cached', '--others', '--exclude-standard', '-z').split(b'\0') if p}
    for raw in sorted(paths):
        path = Path(raw.decode('utf-8', 'surrogateescape'))
        absolute = root / path
        if private(path):
            report('worktree', path.as_posix(), 'private/runtime path in publication set')
            continue
        if absolute.is_symlink():
            report('worktree', path.as_posix(), 'symlink in publication set')
        elif absolute.is_file() and path.suffix not in BINARY:
            content('worktree', path.as_posix(), absolute.read_bytes())
    blob_matches = {}
    def blob_check(scope, path, oid):
        if oid not in blob_matches:
            data = git('cat-file', 'blob', oid)
            blob_matches[oid] = any(pattern.search(data) for pattern in PATTERNS)
        if blob_matches[oid]:
            report(scope, path, 'credential/private-path pattern in Git object')
    indexed = 0
    for entry in git('ls-files', '--stage', '-z').split(b'\0'):
        if not entry:
            continue
        metadata, raw = entry.split(b'\t', 1)
        mode, oid, stage = metadata.decode().split()
        path = raw.decode('utf-8', 'surrogateescape')
        indexed += 1
        if private(path):
            report('index', path, 'private/runtime path staged for publication')
        if mode == '120000' or mode == '160000' or stage != '0':
            report('index', path, 'symlink, submodule or unresolved index entry requires review')
            continue
        blob_check('index', path, oid)
    commits = git('rev-list', '--all').splitlines()
    visited_paths = set()
    for commit in commits:
        for entry in git('ls-tree', '-r', '-z', commit.decode()).split(b'\0'):
            if not entry:
                continue
            metadata, raw = entry.split(b'\t', 1)
            mode, kind, oid = metadata.decode().split()
            path = raw.decode('utf-8', 'surrogateescape')
            key = (oid, path)
            if key in visited_paths:
                continue
            visited_paths.add(key)
            if private(path):
                report('history', path, 'private/runtime path in reachable history')
            if kind != 'blob' or mode == '120000':
                report('history', path, 'nonregular historical entry requires review')
            else:
                blob_check('history', path, oid)
    return {'targeted_scan': 'passed' if not findings else 'requires_review', 'worktree_files': len(paths),
            'index_entries': indexed, 'reachable_commits': len(commits), 'history_paths': len(visited_paths),
            'findings': findings, 'limitations': ['Unreachable objects and unknown secret formats are not certified',
                                               'Third-party licenses and advisories require separate review']}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    try:
        result = audit(args.root)
    except (OSError, RuntimeError, ValueError):
        print(json.dumps({'targeted_scan': 'failed', 'reason': 'Repository/index/history could not be fully inspected'}))
        return 1
    print(json.dumps(result, indent=2))
    return int(bool(result['findings']))

if __name__ == '__main__':
    sys.exit(main())
