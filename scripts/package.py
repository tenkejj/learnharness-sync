"""Deterministic DEVELOPMENT SOURCE bundle, not a native installer, explicit allowlist, no learning data."""
from pathlib import Path
import gzip
import hashlib
import io
import tarfile

root = Path(__file__).resolve().parent.parent
allowed = ['harness_sync', 'dist', 'assets', 'src', 'public', 'crates', 'src-tauri', 'scripts',
           'tests', 'docs', 'packaging', '.github', 'README.md', 'LICENSE', 'NOTICE',
           'CONTRIBUTING.md', 'SECURITY.md', 'CHANGELOG.md', 'Cargo.toml', 'package.json',
           'package-lock.json', 'tsconfig.json', 'index.html', '.gitignore', 'DESIGN.md', 'PRODUCT.md']
output = root / 'release'
output.mkdir(exist_ok=True)
archive = output / 'learnharness-sync-0.2.0-alpha.1-source.tar.gz'
with archive.open('wb') as raw:
    with gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w') as tar:
            for item in sorted(allowed):
                path = root / item
                candidates = sorted(path.rglob('*')) if path.is_dir() else [path]
                for file in candidates:
                    if file.is_symlink():
                        raise SystemExit('Refusing symlink in release allowlist')
                    if not file.is_file() or '__pycache__' in file.parts or 'binaries' in file.parts or file.suffix == '.pyc':
                        continue
                    data = file.read_bytes()
                    info = tarfile.TarInfo('learnharness-sync-0.2.0-alpha.1/' + file.relative_to(root).as_posix())
                    info.size = len(data); info.mtime = 0
                    info.mode = 0o755 if file.name == 'learnharness-preflight' else 0o644
                    tar.addfile(info, io.BytesIO(data))
checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
(output / (archive.name + '.sha256')).write_text(checksum + '  ' + archive.name + '\n')
print(archive)
print(checksum)
