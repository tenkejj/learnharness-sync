#!/usr/bin/env python3
"""Plan by default. Install isolated prefix only; never enable services or override LearnHarness."""
import argparse
from pathlib import Path
import shutil
import sys

parser = argparse.ArgumentParser()
parser.add_argument('--prefix', type=Path, required=True, help='New isolated installation directory')
parser.add_argument('--apply', action='store_true')
parser.add_argument('--uninstall', action='store_true')
args = parser.parse_args()
prefix = args.prefix.absolute()
if any(p.is_symlink() for p in [prefix, *prefix.parents]):
    raise SystemExit('Symlink installation path refused')
root = Path(__file__).resolve().parent.parent
print(f'Plan: {"remove owned installation" if args.uninstall else "copy standalone runtime, dashboard, own launcher and service template"} at {prefix}')
print('No system services, production data or existing LearnHarness launchers will be modified.')
if not args.apply:
    raise SystemExit(0)
if any(c.isspace() or c in '"%\\' for c in str(prefix)):
    raise SystemExit('Templates require a prefix without whitespace or expansion characters')
if args.uninstall:
    marker = prefix / '.harness-sync-install'
    if not marker.is_file() or marker.read_text() != 'HarnessSync 0.1.0\n':
        raise SystemExit('Refusing to uninstall unowned directory')
    shutil.rmtree(prefix)
else:
    if prefix.exists():
        raise SystemExit('Install requires a new empty prefix')
    if not (root / 'dist/index.html').is_file():
        raise SystemExit('Build frontend first')
    prefix.mkdir(mode=0o700, parents=True)
    for item in ('harness_sync', 'dist', 'assets'):
        shutil.copytree(root / item, prefix / item, ignore=shutil.ignore_patterns('__pycache__'))
    (prefix / '.harness-sync-install').write_text('HarnessSync 0.1.0\n')
    (prefix / 'harness-sync.desktop').write_text(f'[Desktop Entry]\nType=Application\nName=HarnessSync\nComment=Unofficial local-first LearnHarness companion\nExec=python -m harness_sync desktop\nPath={prefix}\nIcon={prefix}/assets/brand/learnharness-sync-app-icon-512.png\nTerminal=false\nCategories=Utility;\n')
    (prefix / 'harness-sync-observer.service').write_text(f'[Unit]\nDescription=HarnessSync read-only observer\nAfter=graphical-session.target\n\n[Service]\nType=simple\nWorkingDirectory={prefix}\nExecStart=/usr/bin/python -m harness_sync watch\nRestart=on-failure\nUMask=0077\nNoNewPrivileges=true\nProtectSystem=strict\nProtectHome=read-only\n\n[Install]\nWantedBy=default.target\n')
    print('Copied. Nothing has been activated. Review templates before installing into XDG locations.')
