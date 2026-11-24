"""Bounded loopback Syncthing REST client. No proxies, redirects or key logging."""
import copy
import json
import os
from pathlib import Path
import re
import shutil
import stat
import urllib.error
import urllib.parse
import urllib.request
from .core import SafetyError, DEVICE, canonical, mkdir, read_file

LIMIT = 2 * 1024 * 1024
ID = re.compile(r'^[A-Z2-7]{7}(?:-[A-Z2-7]{7}){7}$')


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise SafetyError('Redirects forbidden for credential-bearing requests')


class TransportError(SafetyError):
    pass


class Client:
    def __init__(self, key_file, port=8384, timeout=2):
        self.key_file = Path(key_file)
        if type(port) is not int or not 1 <= port <= 65535:
            raise TransportError('Invalid Syncthing API port')
        self.base = f'http://127.0.0.1:{port}'
        self.timeout = timeout
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())

    def request(self, method, route, value=None):
        if not route.startswith('/rest/') or '\r' in route or '\n' in route:
            raise TransportError('Invalid API route')
        try:
            key = read_file(self.key_file, 256).decode().strip()
            if os.name != 'nt' and self.key_file.stat().st_mode & 0o077:
                raise TransportError('Syncthing API key file must have mode 0600')
            if not re.fullmatch(r'[A-Za-z0-9_-]{8,256}', key):
                raise TransportError('Invalid protected API key')
            request = urllib.request.Request(self.base + route, data=canonical(value) if value is not None else None,
                                             headers={'X-API-Key': key, 'Content-Type': 'application/json'}, method=method)
            with self.opener.open(request, timeout=self.timeout) as response:
                body = response.read(LIMIT + 1)
                if len(body) > LIMIT:
                    raise TransportError('Syncthing response exceeds limit')
                return json.loads(body) if body else None
        except urllib.error.HTTPError as exc:
            raise TransportError(f'Syncthing API rejected request (HTTP {exc.code}); inspect its local GUI') from None
        except (OSError, ValueError, SafetyError) as exc:
            if isinstance(exc, TransportError):
                raise
            raise TransportError('Syncthing unavailable or its protected API settings are invalid') from None

    def inspect(self):
        system = self.request('GET', '/rest/system/status')
        configured = self.request('GET', '/rest/config/devices')
        connections = self.request('GET', '/rest/system/connections').get('connections', {})
        folders = self.request('GET', '/rest/config/folders')
        peers = []
        for device in configured:
            did = device['deviceID']
            if did == system['myID']:
                continue
            connection = connections.get(did, {})
            peers.append({'id': did, 'name': device.get('name', ''), 'connected': bool(connection.get('connected')),
                          'paused': bool(device.get('paused')), 'bytes': connection.get('inBytesTotal', 0)})
        transfer = []
        for folder in folders:
            if not folder['id'].startswith('harnesssync-'):
                continue
            query = urllib.parse.urlencode({'folder': folder['id']})
            completion = self.request('GET', '/rest/db/completion?' + query)
            state = self.request('GET', '/rest/db/status?' + query)
            errors = self.request('GET', '/rest/folder/errors?' + query)
            transfer.append({'id': folder['id'], 'type': folder['type'], 'state': state.get('state', 'unknown'),
                             'completion': completion.get('completion', 0), 'need_bytes': completion.get('needBytes', 0),
                             'errors': len(errors.get('errors') or []), 'error': bool(state.get('error'))})
        return {'state': 'available', 'device_id': system['myID'], 'peers': peers, 'folders': transfer,
                'message': 'Live Syncthing API state. Transfer completion is not snapshot application.'}

    @staticmethod
    def folder_id(channel, namespace):
        if not re.fullmatch(r'[A-Za-z0-9_-]{1,48}', channel) or not DEVICE.fullmatch(namespace):
            raise TransportError('Invalid transport channel or publication namespace')
        return f'harnesssync-{channel}-{namespace}'

    def pair(self, engine, peer_id, peer_namespace, channel, name='', address='dynamic', confirmed=False):
        if not confirmed:
            raise TransportError('Confirm the dedicated Syncthing configuration changes first')
        if not isinstance(peer_id, str) or not ID.fullmatch(peer_id) or not DEVICE.fullmatch(peer_namespace):
            raise TransportError('Invalid peer Device ID or publication namespace')
        if not isinstance(name, str) or len(name) > 80:
            raise TransportError('Device label exceeds limit')
        # UI accepts discovery only. Test runner may provide explicit loopback addresses.
        if address != 'dynamic' and not re.fullmatch(r'tcp://127\.0\.0\.1:[0-9]{1,5}', address):
            raise TransportError('Use dynamic discovery or an isolated loopback test address')
        own_id = self.request('GET', '/rest/system/status')['myID']
        namespace = engine.state()['device']
        if peer_id == own_id or peer_namespace == namespace:
            raise TransportError('Select a different device and publication namespace')
        configured = {d['deviceID']: d for d in self.request('GET', '/rest/config/devices')}
        folders = {f['id']: f for f in self.request('GET', '/rest/config/folders')}
        plans = []
        for owner, kind in ((namespace, 'sendonly'), (peer_namespace, 'receiveonly')):
            fid = self.folder_id(channel, owner)
            path = engine.store / 'devices' / owner
            # The only configurable paths are inside this isolated engine transport.
            existing = folders.get(fid)
            if existing and (Path(existing['path']).absolute() != path.absolute() or existing['type'] != kind):
                raise TransportError('Folder ID already belongs to another path or direction; no changes applied')
            for other in folders.values():
                other_path = Path(other['path']).absolute()
                if other['id'] != fid and (other_path == path or other_path in path.parents or path in other_path.parents):
                    raise TransportError('Another Syncthing folder overlaps the dedicated namespace')
            value = copy.deepcopy(existing or self.request('GET', '/rest/config/defaults/folder'))
            value.update(id=fid, label=f'LearnHarness Sync {channel}: {owner}', path=str(path), type=kind,
                         paused=False, rescanIntervalS=30, fsWatcherEnabled=True,
                         ignorePerms=True)
            devices = {d['deviceID']: d for d in value.get('devices', [])}
            # Receive folders accept exactly the owner, never transitively rebroadcast namespaces.
            if kind == 'receiveonly' or existing is None:
                devices = {}
            for did in (own_id, peer_id):
                devices.setdefault(did, {'deviceID': did})
            value['devices'] = list(devices.values())
            plans.append((fid, path, value))
        # Validate everything before any REST mutation. Changes are additive/idempotent,
        # not an atomic API transaction. Controller persists a private config backup.
        if peer_id not in configured:
            device = self.request('GET', '/rest/config/defaults/device')
            device.update(deviceID=peer_id, name=name, addresses=[address], introducer=False, autoAcceptFolders=False)
            self.request('POST', '/rest/config/devices', device)
        for _, path, value in plans:
            mkdir(path)
            self.request('POST', '/rest/config/folders', value)
        return {'paired': peer_id, 'folders': [p[0] for p in plans], 'message': 'Configured locally. Repeat on the peer; wait for actual connection.'}

    def stop_sharing(self, engine, peer_id, peer_namespace, channel, confirmed=False):
        if not confirmed or not ID.fullmatch(peer_id) or not DEVICE.fullmatch(peer_namespace):
            raise TransportError('Confirm the peer and stop-sharing operation')
        own = engine.state()['device']
        if peer_namespace == own:
            raise TransportError('Cannot stop sharing the local publication namespace')
        folders = self.request('GET', '/rest/config/folders')
        changes = []
        for owner in (own, peer_namespace):
            fid = self.folder_id(channel, owner)
            folder = next((f for f in folders if f['id'] == fid), None)
            if folder:
                if Path(folder['path']).absolute() != engine.store / 'devices' / owner or folder['type'] != ('sendonly' if owner == own else 'receiveonly'):
                    raise TransportError('Unexpected folder owner; configuration left untouched')
                changes.append((owner, fid, folder))
        for owner, fid, folder in changes:
            if owner == peer_namespace:
                self.request('DELETE', '/rest/config/folders/' + urllib.parse.quote(fid, safe=''))
            else:
                folder['devices'] = [d for d in folder['devices'] if d['deviceID'] != peer_id]
                self.request('PATCH', '/rest/config/folders/' + urllib.parse.quote(fid, safe=''), {'devices': folder['devices']})
        return {'message': 'Dedicated sharing removed locally. Existing payloads and unrelated device shares are retained.'}


def health(key_file=None, port=8384):
    if not key_file:
        return {'state': 'not_configured', 'peers': [], 'folders': [], 'message': 'Connect the local Syncthing API in Setup.'}
    try:
        return Client(key_file, int(port)).inspect()
    except Exception:
        return {'state': 'offline', 'peers': [], 'folders': [], 'message': 'Syncthing API unavailable or incompatible. Check its service, port and private key.'}
