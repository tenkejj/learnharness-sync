from pathlib import Path
import os
import json
import tempfile
import unittest
from harness_sync.core import SafetyError
from harness_sync.learnharness_format import inspect_isolated

class LearnHarnessFormatTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name).resolve()
        self.stamp='2026-01-01T00:00:00.000Z'
        self.put('chats/chat_fixture.json',{'id':'chat_fixture','title':'Invented session','createdAt':self.stamp,'updatedAt':self.stamp,'messages':[],'version':1})
    def tearDown(self):self.tmp.cleanup()
    def put(self,path,value):
        p=self.root/path;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(value)+'\n')
    def test_real_record_shape_is_distinct_from_old_fixture_shape(self):
        result=inspect_isolated(self.root)
        self.assertTrue(result['structural_checks_passed']);self.assertFalse(result['production_compatible'])
        self.assertEqual(result['chats'],1)
    def test_legacy_lazy_stamp_and_newer_version(self):
        p=self.root/'chats/chat_fixture.json';v=json.loads(p.read_text());v.pop('version');p.write_text(json.dumps(v))
        inspect_isolated(self.root);v['version']=2;p.write_text(json.dumps(v))
        with self.assertRaises(SafetyError):inspect_isolated(self.root)
    def test_unsupported_release_and_unknown_plugin_state_refused(self):
        with self.assertRaises(SafetyError):inspect_isolated(self.root,'0.6.2')
        self.put('plugins/plugin-state.json',{})
        with self.assertRaises(SafetyError):inspect_isolated(self.root)
    def test_graph_relationships_and_provenance(self):
        self.put('graph/nodes.jsonl',{'id':'n_a','name':'A','statement':'Invented statement','aliases':[],'createdAt':self.stamp,'provenance':[{'chatId':'chat_fixture','messageId':''}]})
        self.put('graph/chains.jsonl',{'id':'arg_a','premises':['n_a'],'conclusion':'n_a','createdAt':self.stamp,'provenance':[]})
        self.assertEqual(inspect_isolated(self.root)['nodes'],1)
        self.put('graph/chains.jsonl',{'id':'arg_a','premises':['missing'],'conclusion':'n_a','createdAt':self.stamp,'provenance':[]})
        with self.assertRaises(SafetyError):inspect_isolated(self.root)
    def test_unfinished_job_is_not_quiescent(self):
        job={'id':'job_fixture','kind':'research','chatId':'chat_fixture','toolCallId':'t','status':'running','activity':'Invented','createdAt':self.stamp,'updatedAt':self.stamp}
        self.put('jobs/job_fixture.json',job)
        with self.assertRaises(SafetyError):inspect_isolated(self.root)
        job['status']='done';self.put('jobs/job_fixture.json',job);inspect_isolated(self.root)
    def test_secret_field_and_external_attachment_never_read(self):
        self.put('settings.json',{'version':1,'api_key':'SYNTHETIC_SECRET'})
        with self.assertRaises(SafetyError) as ctx:inspect_isolated(self.root)
        self.assertNotIn('SYNTHETIC_SECRET',str(ctx.exception))
        self.put('settings.json',{'version':1})
        self.put('graph/sources.json',{'src_fixture':{'id':'src_fixture','filePath':'/unreadable/never-open','importedAt':self.stamp}})
        self.assertEqual(inspect_isolated(self.root)['external_path_references'],1)
    def test_fixture_simplification_is_not_accepted_as_real_chat(self):
        self.put('chats/chat_fixture.json',{'version':1,'id':'chat_fixture','title':'Invented'})
        with self.assertRaises(SafetyError):inspect_isolated(self.root)

    @unittest.skipUnless(os.name == 'posix' and getattr(os, 'geteuid', lambda: 0)() != 0, 'Needs unprivileged POSIX permissions')
    def test_unreadable_records_cannot_silently_disappear(self):
        directory=self.root/'chats';directory.chmod(0)
        try:
            with self.assertRaises(SafetyError):inspect_isolated(self.root)
            from harness_sync.core import collect
            with self.assertRaises(SafetyError):collect(self.root)
        finally:directory.chmod(0o700)
