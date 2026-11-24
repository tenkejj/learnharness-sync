"""Inventory native code in a built AppImage on its Arch Linux build host."""
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from collections import defaultdict
from pathlib import Path


def fields(path):
    result = {}
    for section in path.read_text().strip().split("\n\n"):
        if not section.strip():
            continue
        key, *values = section.splitlines()
        result[key.strip("%")] = values
    return result


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def build_id(path):
    with path.open("rb") as stream:
        if stream.read(4) != b"\x7fELF":
            return None
    notes = subprocess.run(["readelf", "-n", path], check=True,
                           capture_output=True, text=True).stdout
    match = re.search(r"Build ID: ([0-9a-f]+)", notes)
    return match.group(1) if match else None


def inventory(image):
    packages = defaultdict(list)
    for listing in Path("/var/lib/pacman/local").glob("*/files"):
        meta = fields(listing.with_name("desc"))
        for name in fields(listing).get("FILES", []):
            if name.startswith(("usr/bin/", "usr/lib/")) and not name.endswith("/"):
                packages[Path(name).name].append((Path("/") / name, meta))
    records = []
    with tempfile.TemporaryDirectory() as scratch:
        subprocess.run([image, "--appimage-extract"], cwd=scratch, check=True,
                       stdout=subprocess.DEVNULL)
        root = Path(scratch) / "squashfs-root"
        for entry in sorted(root.rglob("*")):
            if entry.is_symlink() or not entry.is_file():
                continue
            with entry.open("rb") as stream:
                if stream.read(4) != b"\x7fELF":
                    continue
            rec = {"path": str(entry.relative_to(root)), "sha256": digest(entry),
                   "build_id": build_id(entry), "host_matches": []}
            for candidate, package in packages.get(entry.name, []):
                if not candidate.is_file():
                    continue
                same_bytes = digest(candidate) == rec["sha256"]
                if not same_bytes and (not rec["build_id"] or build_id(candidate) != rec["build_id"]):
                    continue
                rec["host_matches"].append({
                    "file": str(candidate), "package": package["NAME"][0],
                    "version": package["VERSION"][0], "licenses": package.get("LICENSE", []),
                    "match": "sha256" if same_bytes else "elf_build_id",
                })
            records.append(rec)
    return {"appimage_sha256": digest(image), "elf_files": records}


if __name__ == "__main__":
    print(json.dumps(inventory(Path(sys.argv[1]).resolve(strict=True)), indent=2))
