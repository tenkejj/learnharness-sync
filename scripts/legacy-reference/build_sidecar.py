"""Freeze Python into Tauri's architecture-specific sidecar (no global install)."""
from pathlib import Path
import platform
import shutil
import subprocess
import sys
root = Path(__file__).resolve().parent.parent
system = sys.platform
arch = platform.machine().lower()
targets = {('linux','x86_64'): 'x86_64-unknown-linux-gnu', ('win32','amd64'): 'x86_64-pc-windows-msvc',
           ('darwin','arm64'): 'aarch64-apple-darwin', ('darwin','x86_64'): 'x86_64-apple-darwin'}
triple = targets.get((system, arch))
if not triple:
    raise SystemExit('Unsupported build host; no production claim')
output = root / 'build' / 'sidecar'
subprocess.run([sys.executable, '-m', 'PyInstaller', '--noconfirm', '--clean', '--onefile',
                '--exclude-module', 'gi', '--name', 'harness-sync-engine', '--paths', str(root), '--distpath', str(output),
                '--workpath', str(root / 'build' / 'pyinstaller'), '--specpath', str(root / 'build'),
                str(root / 'scripts' / 'sidecar_entry.py')], check=True)
ext = '.exe' if system == 'win32' else ''
source = output / ('harness-sync-engine' + ext)
folder = root / 'src-tauri' / 'binaries'
folder.mkdir(exist_ok=True)
shutil.copy2(source, folder / ('harness-sync-engine-' + triple + ext))
# Execute the produced native sidecar. A frozen binary is not accepted merely because it exists.
import json
import tempfile
with tempfile.TemporaryDirectory(prefix='harness-sidecar-smoke-') as temporary:
    profile = str(Path(temporary).resolve() / 'profile')
    probe = subprocess.run([str(source), 'rpc', '--profile', profile],
                           input=b'{"command":"settings","args":{}}', capture_output=True, timeout=30)
    if probe.returncode != 0 or json.loads(probe.stdout)['result']['production_mutation'] is not False:
        raise SystemExit('Frozen engine RPC smoke check failed')
print('Built sidecar and passed local RPC smoke for', triple, '— native GUI/install/live tests are still required')
