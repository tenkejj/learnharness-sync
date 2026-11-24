"""Real kernel/process tests, not a process-name or mocked writer test."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from harness_sync.core import Engine, SafetyError

@unittest.skipUnless(sys.platform.startswith('linux'), 'Linux child-subreaper contract')
class SupervisorTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.root=Path(self.tmp.name).resolve()
        self.engine=Engine.initialize(self.root/'workspace','supervised-test')
    def tearDown(self):self.tmp.cleanup()
    def launch(self, source, timeout=3):
        script=self.root/'writer.py';script.write_text(source)
        return subprocess.Popen([sys.executable,'-m','harness_sync.supervisor','--workspace',str(self.engine.root),
                                 '--timeout',str(timeout),'--invented-data-only','--',sys.executable,str(script)],
                                stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    def wait_marker(self,process):
        deadline=time.monotonic()+2
        while time.monotonic()<deadline:
            if (self.engine.root/'WRITER_ACTIVE').exists():return
            if process.poll() is not None:
                self.fail('Subreaper launch failed: '+process.communicate()[0])
            time.sleep(.01)
        self.fail('No active marker')
    def test_detached_double_fork_writer_is_reaped_before_export(self):
        source='''import os,time
from pathlib import Path
if os.fork():os._exit(0)
os.setsid()
if os.fork():os._exit(0)
time.sleep(.25)
(Path(os.environ['LH_DATA_DIR'])/'settings.json').write_text('{"version":1,"invented":true}')
os._exit(0)
'''
        process=self.launch(source)
        self.wait_marker(process)
        with self.assertRaises(SafetyError):self.engine.export()
        out,err=process.communicate(timeout=5)
        self.assertEqual(process.returncode,0,(out,err))
        self.assertTrue(json.loads(out)['family_reaped'])
        self.assertFalse(json.loads(out)['production_exclusion'])
        self.assertFalse((self.engine.root/'WRITER_ACTIVE').exists())
        self.assertIn('invented',(self.engine.data/'settings.json').read_text())
        self.engine.export()
    def test_failed_writer_leaves_quarantine(self):
        process=self.launch('raise SystemExit(7)')
        out,err=process.communicate(timeout=5)
        self.assertEqual(process.returncode,1,(out,err))
        self.assertTrue((self.engine.root/'WRITER_ACTIVE').exists())
        with self.assertRaises(SafetyError):self.engine.export()
    def test_killed_supervisor_never_releases_durable_safety_marker(self):
        process=self.launch("import os,time;from pathlib import Path;root=Path(os.environ['LH_DATA_DIR']);(root.parent/'worker-started').touch();time.sleep(.3);(root/'settings.json').write_text('{\"version\":1}')")
        self.wait_marker(process)
        deadline=time.monotonic()+2
        while not (self.engine.root/'worker-started').exists() and time.monotonic()<deadline:
            time.sleep(.01)
        self.assertTrue((self.engine.root/'worker-started').exists())
        process.kill();process.communicate(timeout=5)
        self.assertTrue((self.engine.root/'WRITER_ACTIVE').exists())
        with self.assertRaises(SafetyError):self.engine.export()
        # Allow the isolated child to finish; no production process is signaled.
        time.sleep(.5)
        self.assertTrue((self.engine.data/'settings.json').exists())
        with self.assertRaises(SafetyError):self.engine.export()
    def test_home_and_config_are_redirected_for_isolated_application(self):
        source='''import os
from pathlib import Path
home=Path(os.environ['HOME']);config=Path(os.environ['XDG_CONFIG_HOME'])
assert home.parent==config.parent
assert home.name=='home' and config.name=='config'
(config/'isolation-proof').write_text('invented')
'''
        process=self.launch(source);out,err=process.communicate(timeout=5)
        self.assertEqual(process.returncode,0,(out,err))
        self.assertTrue((self.engine.root/'application/config/isolation-proof').exists())
