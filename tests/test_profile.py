from pathlib import Path
import json
import plistlib
import tempfile
import unittest
from harness_sync.core import Engine, SafetyError, collect, inventory
from harness_sync.profile import Profile
from harness_sync.controller import dispatch
from harness_sync.automation import tick
from harness_sync.integrations import startup_plan
from harness_sync.platforms import app_data, lifecycle_capabilities
import shutil

class ProfileTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name).resolve()
        self.profile=Profile(self.root/'profile')
    def tearDown(self):self.tmp.cleanup()
    def test_secret_is_separate_and_not_returned(self):
        result=self.profile.save({'port':9999}, 'SYNTHETICKEY123')
        self.assertNotIn('SYNTHETICKEY123',json.dumps(result))
        self.assertNotIn('SYNTHETICKEY123',self.profile.path.read_text())
        self.assertTrue(result['api_key_configured'])
    def test_automation_without_workspace_refused(self):
        with self.assertRaises(SafetyError):self.profile.save({'automatic':True})
    def test_production_paths_cannot_be_configured(self):
        for key in ('workspace','data_path','production_mutation','version'):
            with self.assertRaises(SafetyError):self.profile.save({key:'/production'})
    def test_init_needs_confirmation_and_never_adopts_data(self):
        with self.assertRaises(SafetyError):dispatch(self.profile,'initialize_synthetic')
        target=self.profile.root/'synthetic-workspace';target.mkdir()
        with self.assertRaises(SafetyError):dispatch(self.profile,'initialize_synthetic',{'confirmed':True})
    def test_dispatch_rejects_unknown_and_no_production_apply(self):
        with self.assertRaises(SafetyError):dispatch(self.profile,'apply',{'confirmed':True,'snapshot':'f'*64})
        with self.assertRaises(SafetyError):dispatch(self.profile,'shell',{'command':'rm'})
    def test_explicit_synthetic_restore_preserves_causality_and_current_backup(self):
        workspace=dispatch(self.profile,'initialize_synthetic',{'confirmed':True})['workspace']
        engine=Engine(workspace);(engine.data/'settings.json').write_text('{"version":1,"synthetic":"first"}')
        first=engine.export();(engine.data/'settings.json').write_text('{"version":1,"synthetic":"second"}')
        second=engine.export()
        engine.restore(first,'RESTORE SYNTHETIC BACKUP')
        head=engine.state()['head']
        self.assertTrue(engine.ancestor(second,head,engine.snapshots()))
        self.assertIn(b'first',(engine.data/'settings.json').read_bytes())
        self.assertTrue(list((engine.root/'backups').glob('*.json')))
    def test_empty_automation_needs_authority(self):
        workspace=self.profile.initialize_workspace()
        self.assertEqual(tick(workspace)['state'],'awaiting_authority')
    def test_synthetic_cycle_applies_only_unique_causal_successor(self):
        a=Engine.initialize(self.root/'a','a'); b=Engine.initialize(self.root/'b','b')
        (a.data/'settings.json').write_text('{"version":1}')
        sid=a.export();shutil.copytree(a.store,b.store,dirs_exist_ok=True);b.apply(sid,True)
        (a.data/'settings.json').write_text('{"version":1,"synthetic":2}');new=a.export()
        shutil.copytree(a.store,b.store,dirs_exist_ok=True)
        self.assertEqual(tick(b.root)['state'],'applied');self.assertEqual(b.state()['head'],new)
        self.assertEqual(tick(b.root)['state'],'waiting')
    def test_native_startup_plans_are_data_not_installation(self):
        mac=startup_plan('/tmp/Harness Sync','/tmp/Profile A','darwin')
        self.assertEqual(plistlib.loads(mac['content'].encode())['ProgramArguments'][0],'/tmp/Harness Sync')
        win=startup_plan('/tmp/a&b.exe','/tmp/Profile A','win32')
        self.assertIn('&amp;',win['content']);self.assertIn('LeastPrivilege',win['content'])
        linux=startup_plan('/tmp/50% app','/tmp/profile','linux')
        self.assertIn('50%% app',linux['content'])
    def test_paths_and_production_gate(self):
        self.assertEqual(app_data('linux','/tmp/home',{'XDG_CONFIG_HOME':'/tmp/xdg'}),Path('/tmp/xdg'))
        self.assertEqual(app_data('darwin','/tmp/home',{}),Path('/tmp/home/Library/Application Support'))
        self.assertFalse(lifecycle_capabilities()['production_mutation'])
