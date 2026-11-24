import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS=Path(__file__).resolve().parent.parent/'scripts'
sys.path.insert(0,str(SCRIPTS))
from check_release import failures,REQUIRED,VERSION
from publish_release import collect_assets

class ReleasePolicyTests(unittest.TestCase):
    def test_missing_gate_schema_cannot_pass(self):
        for value in ({},{'version':VERSION},None,[]):self.assertTrue(failures(value))
        complete=dict.fromkeys(REQUIRED,True);complete['version']=VERSION
        self.assertEqual(failures(complete),[])
        complete['unexpected']=True;self.assertTrue(failures(complete))
    def test_only_verified_platform_installers_are_eligible(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for name in ('HarnessSync.AppImage','HarnessSync.rpm','HarnessSync.exe','HarnessSync.dmg'):(root/name).write_bytes(b'invented test artifact, not an installer')
            self.assertEqual([p.name for p in collect_assets(root,{'linux':True,'windows':False,'macos':False})],['HarnessSync.AppImage','HarnessSync.rpm'])
            with self.assertRaises(ValueError):collect_assets(root,{'linux':False,'windows':True,'macos':True})
    def test_source_bundle_is_not_a_linux_installer(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'source.tar.gz').write_bytes(b'invented source')
            with self.assertRaises(ValueError):collect_assets(root,{'linux':True,'windows':False,'macos':False})
