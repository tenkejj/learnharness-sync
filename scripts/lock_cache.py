"""Attach verified cached npm tarball integrity to the locally resolved lockfile."""
import base64
import hashlib
import json
from pathlib import Path
cache = Path.home() / '.npm/_cacache'
entries = []
for p in (cache / 'index-v5').rglob('*'):
    if not p.is_file(): continue
    for line in p.read_text().splitlines():
        try: entries.append(json.loads(line.split('\t',1)[1]))
        except (ValueError, IndexError): pass
lock = json.loads(Path('package-lock.json').read_text())
for path, value in lock['packages'].items():
    if not path: continue
    name = path.removeprefix('node_modules/')
    suffix = '/' + name + '/-/' + name.split('/')[-1] + '-' + value['version'] + '.tgz'
    entry = next(e for e in reversed(entries) if e['key'].endswith(suffix) and e.get('integrity'))
    algorithm, encoded = entry['integrity'].split()[0].split('-',1)
    checksum = base64.b64decode(encoded).hex()
    payload = (cache / 'content-v2' / algorithm / checksum[:2] / checksum[2:4] / checksum[4:]).read_bytes()
    assert hashlib.new(algorithm, payload).hexdigest() == checksum
    value['resolved'] = entry['key'].removeprefix('make-fetch-happen:request-cache:')
    value['integrity'] = entry['integrity']
Path('package-lock.json').write_text(json.dumps(lock,indent=2)+'\n')
print('Locked integrity of', len(lock['packages']) - 1, 'locally verified tarballs')
