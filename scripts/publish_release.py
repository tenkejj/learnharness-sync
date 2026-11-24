"""Publish only separately authorized, verified platform artifacts. No shell use."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from check_release import VERSION

AUTHORIZATION = 'I AUTHORIZE HARNESSYNC v0.2.0-alpha.1 PUBLIC RELEASE'
EXTENSIONS = {'.appimage': 'linux', '.deb': 'linux', '.rpm': 'linux', '.exe': 'windows', '.dmg': 'macos'}


def collect_assets(root, readiness):
    if not isinstance(readiness, dict) or set(readiness) != {'linux', 'windows', 'macos'} or any(type(x) is not bool for x in readiness.values()):
        raise ValueError('Invalid platform readiness schema')
    if readiness['linux'] is not True:
        raise ValueError('Linux native installation is not verified')
    root = Path(root)
    if root.is_symlink() or not root.is_dir():
        raise ValueError('Artifact root must be an existing regular directory')
    files, names = [], set()
    for file in sorted(root.rglob('*')):
        if file.is_symlink():
            raise ValueError('Artifact symlinks refused')
        if not file.is_file():
            continue
        platform = EXTENSIONS.get(file.suffix.lower())
        if not platform or readiness[platform] is not True:
            continue
        if file.name in names:
            raise ValueError('Duplicate release asset filenames')
        names.add(file.name)
        files.append(file)
    if not any(EXTENSIONS[file.suffix.lower()] == 'linux' for file in files):
        raise ValueError('No verified Linux installer; cannot publish a source-only beta')
    return files


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--authorization', required=True)
    parser.add_argument('--artifacts', type=Path, required=True)
    args = parser.parse_args()
    if args.authorization != AUTHORIZATION:
        raise SystemExit('Explicit beta publication authorization required')
    root = Path(__file__).resolve().parent.parent
    subprocess.run([sys.executable, str(root / 'scripts/check_release.py')], check=True)
    readiness = json.loads((root / '.github/platform-readiness.json').read_text())
    files = collect_assets(args.artifacts, readiness)
    checksums = args.artifacts / 'SHA256SUMS'
    checksums.write_text(''.join(hashlib.sha256(file.read_bytes()).hexdigest()+'  '+file.name+'\n' for file in files))
    subprocess.run(['gh','release','create','v'+VERSION,'--repo','tenkejj/learnharness-sync','--prerelease','--verify-tag',
                    '--title','LearnHarness Sync v'+VERSION,'--notes-file',str(root/'docs/RELEASE_NOTES.md'),
                    *map(str,files),str(checksums)],check=True)

if __name__ == '__main__':
    main()
