"""Exercise actual HTTP handler with in-memory IO; no sandbox socket privileges needed."""
from http import HTTPStatus
import io
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
import urllib.request
from unittest.mock import patch
from harness_sync.server import Handler
from harness_sync.status import status
from harness_sync.core import SafetyError
from harness_sync.transport import NoRedirect, health

class FakeRequest:
    def __init__(self, data):
        self.data = io.BytesIO(data)
    def makefile(self, *_): return self.data
    def sendall(self, value): pass

class TestHandler(Handler):
    def setup(self):
        self.rfile = self.request.data
        self.wfile = io.BytesIO()
    def finish(self): pass

class ApiTests(unittest.TestCase):
    def request(self, path, host='127.0.0.1:8743', site='same-origin', method='GET'):
        raw = f'{method} {path} HTTP/1.0\r\nHost: {host}\r\nSec-Fetch-Site: {site}\r\n\r\n'.encode()
        handler = TestHandler(FakeRequest(raw), ('127.0.0.1', 12345), SimpleNamespace(server_port=8743), directory=str(Path('dist').resolve()))
        return handler.wfile.getvalue()

    def test_status_actual_engine_projection(self):
        with patch('harness_sync.status.health', return_value={'state':'not_configured','peers':[],'message':'No key'}):
            response = self.request('/api/status')
        headers, body = response.split(b'\r\n\r\n', 1)
        self.assertIn(b'200 OK', headers)
        result = json.loads(body)
        self.assertFalse(result['automatic']); self.assertEqual(result['mode'], 'monitor')
        self.assertEqual(result['snapshots'], [])
        self.assertNotIn('Access-Control-Allow-Origin', headers.decode())
        self.assertIn(b"frame-ancestors 'none'", headers)

    def test_dns_rebinding_cross_site_and_mutation_denied(self):
        self.assertIn(b'403', self.request('/api/status', host='attacker.example'))
        self.assertIn(b'403', self.request('/api/status', site='cross-site'))
        self.assertIn(b'501', self.request('/api/status', method='POST'))

    def test_source_and_state_are_not_served(self):
        for path in ('/harness_sync/core.py', '/ledger.json', '/assets/', '/../PROMPT_FOR_CODEX.md'):
            self.assertIn(b'404', self.request(path))

    def test_engine_error_is_truthful_and_redacted(self):
        with patch('harness_sync.server.status', side_effect=ValueError('PRIVATE_TOKEN')):
            response = self.request('/api/status')
        self.assertIn(b'503', response); self.assertNotIn(b'PRIVATE_TOKEN', response)

    def test_transport_never_redirects_api_key(self):
        request = urllib.request.Request('http://127.0.0.1:8384/rest/system/status', headers={'X-API-Key':'SYNTHETIC_SECRET'})
        with self.assertRaises((ValueError, SafetyError)):
            NoRedirect().redirect_request(request, None, 302, 'redirect', {}, 'https://external.example')
        self.assertEqual(health()['state'], 'not_configured')

if __name__ == '__main__': unittest.main()

class CommandApiTests(unittest.TestCase):
    def setUp(self):
        import tempfile
        from harness_sync.profile import Profile
        self.tmp=tempfile.TemporaryDirectory()
        self.profile=Profile(Path(self.tmp.name).resolve()/'profile')
    def tearDown(self):self.tmp.cleanup()
    def request(self, value, token='session-token', origin='http://127.0.0.1:8743', host='127.0.0.1:8743', length=None):
        body=json.dumps(value).encode()
        raw=(f'POST /api/command HTTP/1.0\r\nHost: {host}\r\nOrigin: {origin}\r\nX-HarnessSync-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {len(body) if length is None else length}\r\n\r\n').encode()+body
        class BoundHandler(TestHandler):pass
        BoundHandler.profile=self.profile;BoundHandler.csrf='session-token'
        handler=BoundHandler(FakeRequest(raw),('127.0.0.1',9),SimpleNamespace(server_port=8743),directory=str(Path('dist').resolve()))
        return handler.wfile.getvalue()
    def test_authenticated_local_save_and_secret_redaction(self):
        response=self.request({'command':'save_settings','args':{'changes':{'port':9999},'api_key':'SYNTHETICKEY123'}})
        self.assertIn(b'200 OK',response);self.assertNotIn(b'SYNTHETICKEY123',response)
        self.assertEqual(self.profile.load()['port'],9999)
    def test_token_origin_and_host_each_required(self):
        for kwargs in ({'token':''},{'origin':'http://evil.example'},{'host':'evil.example'}):
            response=self.request({'command':'save_settings','args':{'changes':{'port':1234}}},**kwargs)
            self.assertIn(b'403',response);self.assertFalse(self.profile.path.exists())
    def test_oversized_command_is_rejected(self):
        response=self.request({'command':'settings','args':{}},length=32769)
        self.assertIn(b'400',response)
    def test_production_paths_remain_inaccessible_through_api(self):
        response=self.request({'command':'save_settings','args':{'changes':{'workspace':'/production'}}})
        self.assertIn(b'409',response);self.assertFalse(self.profile.path.exists())
