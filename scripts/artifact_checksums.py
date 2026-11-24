import hashlib
from pathlib import Path
import sys
root=Path(sys.argv[1])
files=sorted(p for p in root.rglob('*') if p.is_file() and p.name!='SHA256SUMS')
if not files:
    raise SystemExit('No native artifacts were built; refusing an empty checksums file')
(root/'SHA256SUMS').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.relative_to(root).as_posix()+'\n' for p in files))
