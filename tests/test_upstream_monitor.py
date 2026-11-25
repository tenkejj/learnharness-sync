"""Offline tests for the read-only upstream release classifier."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/monitor-learnharness-release.py"


class UpstreamMonitorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "source"
        self.source.mkdir()
        (self.source / "contract.ts").write_text("known contract")
        self.contract_hash = hashlib.sha256(b"known contract").hexdigest()
        self.asset_hash = "a" * 64
        self.matrix = self.root / "matrix.json"
        self.release = self.root / "release.json"
        self.report = self.root / "report.json"
        self.matrix.write_text(json.dumps({
            "schema_sources": {"contract.ts": self.contract_hash},
            "builds": [{"version": "0.7.0", "size": 200, "sha256": self.asset_hash}],
        }))

    def run_monitor(self, version):
        tag = "v" + version
        self.release.write_text(json.dumps({
            "tag_name": tag,
            "assets": [{
                "name": f"LearnHarness-{version}-x86_64.AppImage",
                "size": 200,
                "digest": "sha256:" + self.asset_hash,
                "browser_download_url": f"https://github.com/amosblomqvist/learnharness/releases/download/{tag}/LearnHarness-{version}-x86_64.AppImage",
            }],
        }))
        result = subprocess.run([
            "python", str(SCRIPT), "--matrix", str(self.matrix),
            "--release-json", str(self.release), "--source-root", str(self.source),
            "--report", str(self.report),
        ], capture_output=True, text=True)
        return result, json.loads(self.report.read_text())

    def test_known_identity_and_schema_remain_supported(self):
        result, report = self.run_monitor("0.7.0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["status"], "verified-build-unchanged")
        self.assertTrue(report["support_enabled"])

    def test_new_release_cannot_enable_itself_even_with_matching_schema(self):
        result, report = self.run_monitor("0.8.0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["status"], "candidate-schema-unchanged")
        self.assertFalse(report["support_enabled"])
        (self.source / "contract.ts").write_text("new schema")
        result, report = self.run_monitor("0.8.0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["status"], "candidate-schema-changed")
        self.assertFalse(report["support_enabled"])

    def test_modified_known_asset_fails_closed(self):
        matrix = json.loads(self.matrix.read_text())
        matrix["builds"][0]["sha256"] = "b" * 64
        self.matrix.write_text(json.dumps(matrix))
        result, report = self.run_monitor("0.7.0")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(report["status"], "reviewed-build-changed")
        self.assertFalse(report["support_enabled"])


if __name__ == "__main__":
    unittest.main()
