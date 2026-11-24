"""Run a COPY of real LearnHarness with invented records and private HOME/XDG.
Never uses actual sessions, replaces a launcher or touches a running service.
This is a manual GUI acceptance gate, not a declaration of production support.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from harness_sync.core import Engine, atomic_json, mkdir, canonical, write
from harness_sync.learnharness_format import inspect_isolated

parser = argparse.ArgumentParser()
parser.add_argument('--appimage', type=Path, required=True)
parser.add_argument('--report', type=Path, default=Path('/tmp/harness-sync-learnharness-isolated.json'))
parser.add_argument('--timeout', type=int, default=300)
args = parser.parse_args()
report = {'state': 'not_run', 'application_launch_verified': False, 'production_ready': False}
code = 1
try:
    if not args.appimage.is_file() or args.appimage.is_symlink():
        raise RuntimeError('A local verified AppImage is required')
    if args.appimage.name != 'LearnHarness-0.6.1-x86_64.AppImage':
        raise RuntimeError('Only the audited 0.6.1 release is eligible for isolated acceptance')
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
    root = Path(tempfile.mkdtemp(prefix='harness-sync-learnharness-')).resolve()
    engine = Engine.initialize(root / 'workspace', 'isolated-application')
    binary = root / 'LearnHarness-0.6.1-x86_64.AppImage'
    shutil.copyfile(args.appimage, binary); binary.chmod(0o700)
    checksum = hashlib.sha256(binary.read_bytes()).hexdigest()
    data = engine.root / 'application/config/LearnHarness/data'
    mkdir(data / 'chats')
    stamp = '2026-01-01T00:00:00.000Z'
    fixture = {'id': 'chat_harnesssync_isolated', 'title': 'HarnessSync invented acceptance session',
               'createdAt': stamp, 'updatedAt': stamp, 'messages': [], 'version': 1}
    write(data / 'chats/chat_harnesssync_isolated.json', canonical(fixture))
    before = inspect_isolated(data)
    # The operator uses the actual GUI, verifies the invented session, then exits.
    # All child workers must terminate before the subreaper returns success.
    result = subprocess.run([sys.executable, '-m', 'harness_sync.supervisor', '--workspace', str(engine.root),
                             '--timeout', str(args.timeout), '--invented-data-only', '--', str(binary)],
                            capture_output=True, text=True, timeout=args.timeout + 15)
    if result.returncode:
        raise RuntimeError('Isolated application did not exit cleanly; workspace quarantined')
    family = json.loads(result.stdout)
    after = inspect_isolated(data)
    report.update(state='passed_isolated_structure_and_family_only', application_launch_verified=True, appimage_sha256=checksum,
                  before=before, after=after, family_reaped=family['family_reaped'],
                  limitations=['GUI session verification is manual', 'Production import/export is NOT enabled'])
    code = 0
except PermissionError:
    report.update(state='blocked', reason='Local socket/desktop permission unavailable')
    code = 2
except Exception:
    report.update(state='failed', reason='Isolated application/schema/lifecycle acceptance failed; no production success claimed')
args.report.parent.mkdir(parents=True, exist_ok=True)
atomic_json(args.report, report)
print(json.dumps(report, indent=2))
sys.exit(code)
