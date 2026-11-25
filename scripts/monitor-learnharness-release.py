#!/usr/bin/env python3
"""Read-only upstream release monitor. A report never enables compatibility."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile
import urllib.request

API = "https://api.github.com/repos/amosblomqvist/learnharness/releases/latest"
SOURCE = "https://codeload.github.com/amosblomqvist/learnharness/tar.gz/refs/tags/{}"
MAX_SOURCE_ARCHIVE = 50 * 1024 * 1024


def download(url, limit):
    request = urllib.request.Request(url, headers={"User-Agent": "learnharness-sync-compatibility-monitor", "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(request, timeout=45) as response:
        data = response.read(limit + 1)
    if len(data) > limit:
        raise ValueError("Upstream response exceeds the monitor limit")
    return data


def source_hashes(archive, paths):
    found = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as bundle:
        for member in bundle:
            if not member.isfile() or member.size > 10 * 1024 * 1024:
                continue
            for path in paths:
                if member.name.endswith("/" + path):
                    if path in found:
                        raise ValueError("Duplicate upstream source path")
                    found[path] = hashlib.sha256(bundle.extractfile(member).read()).hexdigest()
    if set(found) != set(paths):
        raise ValueError("Upstream source contracts are incomplete")
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", type=Path, default=Path("support/learnharness-builds.json"))
    parser.add_argument("--release-json", type=Path, help="Offline test input")
    parser.add_argument("--source-root", type=Path, help="Offline test input")
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--github-output", type=Path)
    args = parser.parse_args()
    matrix = json.loads(args.matrix.read_text())
    release = json.loads(args.release_json.read_text() if args.release_json else download(API, 2 * 1024 * 1024))
    tag = release.get("tag_name")
    if not isinstance(tag, str) or not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
        raise ValueError("Upstream release tag is not a supported semantic-version shape")
    version = tag[1:]
    name = f"LearnHarness-{version}-x86_64.AppImage"
    assets = [asset for asset in release.get("assets", []) if asset.get("name") == name]
    if len(assets) != 1:
        raise ValueError("Expected one Linux x86_64 AppImage asset")
    asset = assets[0]
    digest = asset.get("digest", "")
    url = asset.get("browser_download_url", "")
    if not re.fullmatch(r"sha256:[a-f0-9]{64}", digest) or not isinstance(asset.get("size"), int):
        raise ValueError("Upstream AppImage has no trustworthy SHA-256 metadata")
    if not url.startswith(f"https://github.com/amosblomqvist/learnharness/releases/download/{tag}/"):
        raise ValueError("Unexpected upstream asset URL")
    expected_sources = matrix["schema_sources"]
    if args.source_root:
        actual_sources = {path: hashlib.sha256((args.source_root / path).read_bytes()).hexdigest() for path in expected_sources}
    else:
        actual_sources = source_hashes(download(SOURCE.format(tag), MAX_SOURCE_ARCHIVE), expected_sources)
    schema_match = actual_sources == expected_sources
    known = next((row for row in matrix["builds"] if row["version"] == version), None)
    if known:
        identity_match = known["size"] == asset["size"] and known["sha256"] == digest[7:]
        status = "verified-build-unchanged" if identity_match and schema_match else "reviewed-build-changed"
    else:
        identity_match = False
        status = "candidate-schema-unchanged" if schema_match else "candidate-schema-changed"
    report = {
        "tag": tag, "status": status, "known_build": bool(known),
        "identity_match": identity_match, "schema_match": schema_match,
        "source_sha256": actual_sources,
        "asset": {"name": name, "size": asset["size"], "sha256": digest[7:], "url": url},
        "support_enabled": False if not known else identity_match and schema_match,
        "next_step": "Manual review, disposable managed application tests, packaged A-to-B-to-A, recovery and release gate approval are required before adding a new build.",
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if args.github_output:
        with args.github_output.open("a") as output:
            for key, value in {
                "candidate": str(not bool(known)).lower(),
                "asset_url": url,
                "asset_sha256": digest[7:],
                "asset_size": str(asset["size"]),
                "schema_match": str(schema_match).lower(),
            }.items():
                output.write(f"{key}={value}\n")
    print(json.dumps({"tag": tag, "status": status, "schema_match": schema_match}))
    if status == "reviewed-build-changed":
        raise SystemExit("Previously verified release identity or schema changed upstream")


if __name__ == "__main__":
    main()
