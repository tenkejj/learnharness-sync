"""Inspect packaged public application code, never user learning records."""
import json
from pathlib import Path
import struct
import sys
archive = Path(sys.argv[1])
with archive.open('rb') as f:
    prefix = f.read(16)
    header_size = struct.unpack('<I', prefix[4:8])[0]
    length = struct.unpack('<I', prefix[12:16])[0]
    header = json.loads(f.read(length))
    def entry(name):
        node = header
        for part in name.split('/'):
            node = node['files'][part]
        return node
    for name in ('package.json', 'electron/main.cjs', 'main.cjs'):
        try:
            meta = entry(name)
            f.seek(8 + header_size + int(meta['offset']))
            text = f.read(meta['size']).decode()
            target = Path('/tmp/harness-sync-appimage-audit') / name.replace('/', '-')
            target.write_text(text)
            print(name, '->', target)
        except KeyError:
            print(name, 'not in asar')
    print('Root packaged entries:', ', '.join(header.get('files', {})))
