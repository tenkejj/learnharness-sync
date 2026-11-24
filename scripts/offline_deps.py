"""Local validation helper: extract only tarballs already present in npm cache.
Normal contributors should use npm install. No network or install scripts run.
"""
import base64
import hashlib
import json
from pathlib import Path
import re
import tarfile
import io

cache = Path.home() / '.npm/_cacache'
entries = {}
for index in (cache / 'index-v5').rglob('*'):
    if not index.is_file():
        continue
    for line in index.read_text().splitlines():
        try:
            value = json.loads(line.split('\t', 1)[1])
            key = value['key']
            if key.endswith('.tgz') and value.get('integrity'):
                entries[key] = value
        except (ValueError, IndexError, KeyError):
            pass
installed = {}

def unpack(name, version):
    if name in installed:
        return
    candidates = [e for k, e in entries.items() if '/' + name + '/-/' in k]
    match = next((e for e in candidates if e['key'].endswith('-' + version + '.tgz')), None)
    entry = match or (candidates[-1] if candidates else None)
    if not entry:
        raise SystemExit('Missing cached package: ' + name)
    integrity = entry['integrity'].split()[0]
    algorithm, encoded = integrity.split('-', 1)
    hexhash = base64.b64decode(encoded).hex()
    data = (cache / 'content-v2' / algorithm / hexhash[:2] / hexhash[2:4] / hexhash[4:]).read_bytes()
    assert hashlib.new(algorithm, data).hexdigest() == hexhash
    target = Path('node_modules') / name
    target.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for member in archive:
            member.name = member.name.split('/', 1)[1] if '/' in member.name else ''
            if member.name:
                archive.extract(member, target, filter='data')
    pkg = json.loads((target / 'package.json').read_text())
    installed[name] = pkg['version']
    for dep, ver in pkg.get('dependencies', {}).items():
        unpack(dep, ver.lstrip('^~'))
    for dep in pkg.get('optionalDependencies', {}):
        if ('linux-x64' in dep and 'musl' not in dep) or dep == '@esbuild/linux-x64':
            unpack(dep, pkg['optionalDependencies'][dep])
    bins = pkg.get('bin', {})
    if isinstance(bins, str):
        bins = {name: bins}
    for binary, filename in bins.items():
        link = Path('node_modules/.bin') / binary
        link.parent.mkdir(exist_ok=True)
        if not link.exists():
            link.symlink_to('../' + name + '/' + filename)
        (target / filename).chmod(0o755)

pkg = json.loads(Path('package.json').read_text())
for name, version in (pkg['dependencies'] | pkg['devDependencies']).items():
    unpack(name, version)
Path('docs/local-dependency-versions.json').write_text(json.dumps(installed, indent=2) + '\n')
print('Restored', len(installed), 'cached packages. See docs/local-dependency-versions.json for actual validation versions.')
