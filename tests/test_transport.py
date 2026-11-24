"""REST contract tests, explicitly NOT a live P2P test."""
import copy
from pathlib import Path
import tempfile
import unittest
from harness_sync.core import Engine, SafetyError
from harness_sync.transport import Client

OWN = '-'.join(['AAAAAAA']*8)
PEER = '-'.join(['BBBBBBB']*8)

class ContractClient(Client):
    def __init__(self):
        self.calls = []
        self.devices = [{'deviceID': OWN}, {'deviceID': PEER, 'name': 'existing', 'paused': False}]
        self.folders = []
    def request(self, method, route, value=None):
        self.calls.append((method, route, copy.deepcopy(value)))
        if method == 'GET':
            if route == '/rest/system/status': return {'myID': OWN}
            if route == '/rest/config/devices': return copy.deepcopy(self.devices)
            if route == '/rest/config/folders': return copy.deepcopy(self.folders)
            if route == '/rest/config/defaults/device': return {'addresses': ['dynamic']}
            if route == '/rest/config/defaults/folder': return {'devices': []}
            if route == '/rest/system/connections': return {'connections': {}}
            if route.startswith('/rest/db/completion?'): return {'completion': 42.5, 'needBytes': 21}
            if route.startswith('/rest/db/status?'): return {'state': 'syncing'}
            if route.startswith('/rest/folder/errors?'): return {'errors': []}
        if method == 'POST' and route == '/rest/config/devices': self.devices.append(copy.deepcopy(value)); return None
        if method == 'POST' and route == '/rest/config/folders':
            self.folders = [f for f in self.folders if f['id'] != value['id']] + [copy.deepcopy(value)]; return None
        if method == 'DELETE': self.folders = [f for f in self.folders if not route.endswith(f['id'])]; return None
        if method == 'PATCH':
            folder = next(f for f in self.folders if route.endswith(f['id'])); folder.update(copy.deepcopy(value)); return None
        raise AssertionError((method, route))

class TransportContracts(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.engine = Engine.initialize(Path(self.tmp.name).resolve()/'peer','local')
        self.client = ContractClient()
    def tearDown(self): self.tmp.cleanup()
    def pair(self, **kw): return self.client.pair(self.engine, PEER, 'remote', 'test', confirmed=True, **kw)
    def test_pairing_is_idempotent_and_isolates_namespaces(self):
        self.pair(); self.pair()
        self.assertEqual(len(self.client.folders), 2)
        own = next(f for f in self.client.folders if f['type']=='sendonly')
        remote = next(f for f in self.client.folders if f['type']=='receiveonly')
        self.assertEqual(Path(own['path']), self.engine.store/'devices/local')
        self.assertEqual(Path(remote['path']), self.engine.store/'devices/remote')
        self.assertEqual({d['deviceID'] for d in remote['devices']},{OWN,PEER})
        self.assertFalse(any(Path(f['path'])==self.engine.data for f in self.client.folders))
    def test_no_consent_no_mutation(self):
        with self.assertRaises(SafetyError): self.client.pair(self.engine,PEER,'remote','test')
        self.assertEqual(self.client.calls,[])
    def test_collision_is_detected_before_mutation(self):
        self.client.folders=[{'id':'harnesssync-test-local','path':'/another/path','type':'sendreceive'}]
        with self.assertRaises(SafetyError): self.pair()
        self.assertTrue(all(method=='GET' for method,_,_ in self.client.calls))
    def test_overlap_is_refused(self):
        self.client.folders=[{'id':'unrelated','path':str(self.engine.root),'type':'sendreceive'}]
        with self.assertRaises(SafetyError): self.pair()
        self.assertTrue(all(method=='GET' for method,_,_ in self.client.calls))
    def test_offline_configured_peer_is_not_hidden(self):
        status=self.client.inspect()
        self.assertEqual(status['peers'][0]['id'],PEER)
        self.assertFalse(status['peers'][0]['connected'])
    def test_real_reported_progress_is_projected(self):
        self.pair();status=self.client.inspect()
        self.assertEqual(status['folders'][0]['completion'],42.5)
        self.assertEqual(status['folders'][0]['need_bytes'],21)
    def test_stop_sharing_preserves_payloads_and_other_devices(self):
        self.pair(); (self.engine.store/'devices/remote/preserved').write_text('synthetic')
        self.client.stop_sharing(self.engine,PEER,'remote','test',True)
        self.assertTrue((self.engine.store/'devices/remote/preserved').is_file())
        self.assertEqual(len(self.client.devices),2)
        self.assertEqual(len(self.client.folders),1)
        self.assertNotIn(PEER,[d['deviceID'] for d in self.client.folders[0]['devices']])
    def test_arbitrary_network_addresses_rejected(self):
        with self.assertRaises(SafetyError): self.pair(address='http://external.example')

    def test_default_template_cannot_share_with_unselected_device(self):
        original=self.client.request
        def default_with_extra(method, route, value=None):
            if route=='/rest/config/defaults/folder':return {'devices':[{'deviceID':'UNSELECTED'}]}
            return original(method,route,value)
        self.client.request=default_with_extra
        self.pair()
        for folder in self.client.folders:
            self.assertNotIn('UNSELECTED',[d['deviceID'] for d in folder['devices']])

    def test_stop_sharing_cannot_delete_own_namespace(self):
        self.pair();before=copy.deepcopy(self.client.folders)
        with self.assertRaises(SafetyError):self.client.stop_sharing(self.engine,PEER,'local','test',True)
        self.assertEqual(before,self.client.folders)
