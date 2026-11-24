"""Exercise native profile preparation on a real 2 MiB private tmpfs."""
import errno
import hashlib
from pathlib import Path
import shutil
import subprocess


def transaction(action, *, failure=None):
    result = subprocess.run(["/peer", "--qa-transaction", action, "D"], capture_output=True)
    if failure:
        assert result.returncode != 0 and failure in result.stderr, result.stderr
    else:
        result.check_returncode()


def contents(root):
    return {p.relative_to(root): hashlib.sha256(p.read_bytes()).digest()
            for p in root.rglob("*") if p.is_file()}


root = Path("/lab")
data = root / "D/config/LearnHarness/data"
stage = root / "D/received"
(root / "active").write_text("transfer\n")
shutil.copytree("/existing", data)
shutil.copytree("/incoming", stage)
before = contents(data)
settings = stage / "settings.json"
settings.chmod(0)
transaction("prepare", failure=b"Permission denied")
assert contents(data) == before
settings.chmod(0o600)
with (root / "fill").open("wb", buffering=0) as sink:
    try:
        while True:
            sink.write(bytes(65536))
    except OSError as exc:
        assert exc.errno == errno.ENOSPC
transaction("prepare", failure=b"No space left on device")
assert contents(data) == before
(root / "fill").unlink()
transaction("recover")
transaction("prepare")
transaction("activate")
assert contents(data) == contents(stage)
transaction("recover")
assert contents(data) == before
print("EACCES and kernel ENOSPC preserved the destination; retry and backup restoration passed")
