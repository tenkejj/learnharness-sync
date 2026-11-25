"""The private physical runner only accepts reviewed build identities."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "physical_managed_acceptance", ROOT / "scripts/physical-managed-acceptance.py"
)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)
REGISTRY = ROOT / "support/learnharness-builds.json"


class PhysicalManagedAcceptanceRegistryTests(unittest.TestCase):
    def test_both_reviewed_versions_keep_distinct_verified_images(self):
        old = RUNNER.verified_build(REGISTRY, "0.6.1")[1]
        new = RUNNER.verified_build(REGISTRY, "0.7.0")[1]
        self.assertEqual(old["filename"], "LearnHarness-0.6.1-x86_64.AppImage")
        self.assertEqual(new["filename"], "LearnHarness-0.7.0-x86_64.AppImage")
        self.assertNotEqual(old["sha256"], new["sha256"])
        self.assertNotEqual(old["wire_format"], new["wire_format"])

    def test_unknown_and_malformed_builds_are_refused(self):
        with self.assertRaisesRegex(RuntimeError, "no unique verified build"):
            RUNNER.verified_build(REGISTRY, "0.8.0")
        with tempfile.TemporaryDirectory() as temporary:
            matrix = json.loads(REGISTRY.read_text())
            matrix["builds"][1]["sha256"] = "unverified"
            path = Path(temporary) / "learnharness-builds.json"
            path.write_text(json.dumps(matrix))
            with self.assertRaisesRegex(RuntimeError, "Malformed verified"):
                RUNNER.verified_build(path, "0.7.0")
            matrix["builds"][1] = matrix["builds"][0]
            path.write_text(json.dumps(matrix))
            with self.assertRaisesRegex(RuntimeError, "no unique verified build"):
                RUNNER.verified_build(path, "0.7.0")


if __name__ == "__main__":
    unittest.main()
