"""Safe synthetic vertical slice. Production mutation deliberately unavailable.

The sandbox capability is not proof of LearnHarness quiescence. It only enables
operations on a purpose-created, isolated fixture tree, never an imported path.
"""
from __future__ import annotations
import contextlib
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import time
import uuid
from .platforms import NOFOLLOW, NONBLOCK, exclusive, linked, sync_directory

MAX_FILE = 16 * 1024 * 1024
MAX_TOTAL = 128 * 1024 * 1024
MAX_FILES = 4096
HEX = re.compile(r'^[a-f0-9]{64}$')
DEVICE = re.compile(r'^[a-zA-Z0-9_-]{1,64}$')
FORMAT = 'harness-sync-fixture-v1'

class SafetyError(Exception):
    """Action refused without exposing payloads or credentials."""


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True).encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def fsync_dir(path):
    sync_directory(path)


def mkdir(path):
    path = Path(path).absolute()
    if any(linked(p) for p in [path, *path.parents]):
        raise SafetyError('Symlink directory ancestry refused')
    missing = []
    cursor = path
    while not cursor.exists():
        missing.append(cursor)
        cursor = cursor.parent
    for directory in reversed(missing):
        directory.mkdir(mode=0o700, exist_ok=True)
        fsync_dir(directory.parent)
    if linked(path) or not path.is_dir():
        raise SafetyError('Unsafe directory')


def write(path, data):
    """Exclusive file creation, restricted permissions, durable content."""
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | NOFOLLOW | getattr(os, 'O_BINARY', 0), 0o600)
    with os.fdopen(fd, 'wb') as out:
        out.write(data)
        out.flush()
        os.fsync(out.fileno())


def atomic_json(path, value):
    tmp = path.with_name('.' + path.name + '-' + uuid.uuid4().hex)
    write(tmp, canonical(value))
    os.replace(tmp, path)
    fsync_dir(path.parent)


def read_file(path, limit=MAX_FILE):
    path = Path(path).absolute()
    if any(linked(parent) for parent in [path, *path.parents]):
        raise SafetyError('Symlink ancestor refused')
    try:
        fd = os.open(path, os.O_RDONLY | NOFOLLOW | NONBLOCK | getattr(os, 'O_BINARY', 0))
    except OSError as exc:
        raise SafetyError('Required file unavailable or unsafe') from exc
    with os.fdopen(fd, 'rb') as inp:
        info = os.fstat(inp.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_size > limit:
            raise SafetyError('Unsafe file type, link count or size')
        data = inp.read(limit + 1)
        if len(data) > limit:
            raise SafetyError('File exceeds size limit')
        return data


def parse(data):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise SafetyError('Duplicate JSON key')
            result[key] = value
        return result
    try:
        return json.loads(data, object_pairs_hook=unique,
                          parse_constant=lambda _: (_ for _ in ()).throw(SafetyError('Non-finite JSON')))
    except (ValueError, UnicodeError, RecursionError) as exc:
        raise SafetyError('Malformed JSON') from exc


def relative(name):
    if not isinstance(name, str) or not name or '\\' in name or '\x00' in name:
        raise SafetyError('Invalid relative path')
    p = PurePosixPath(name)
    if p.is_absolute() or any(x in ('', '.', '..') for x in name.split('/')):
        raise SafetyError('Path traversal refused')
    # Narrow audited fixture schema. Unknown/plugin/executable/secret files fail closed.
    if name != 'settings.json' and not (
        len(p.parts) == 2 and p.parts[0] in ('chats', 'graph', 'jobs')
        and re.fullmatch(r'[A-Za-z0-9_-]+\.(json|jsonl)', p.parts[1])):
        raise SafetyError('Unsupported or sensitive file path')
    if any(word in name.lower() for word in ('auth', 'token', 'credential', 'secret', 'trace', 'debug')):
        raise SafetyError('Sensitive file excluded')
    return p


def validate_payload(name, data):
    relative(name)
    values = [parse(line) for line in data.splitlines() if line.strip()] if name.endswith('.jsonl') else [parse(data)]
    if any(not isinstance(v, dict) for v in values):
        raise SafetyError('Expected JSON objects')
    if name == 'settings.json' and values[0].get('version') != 1:
        raise SafetyError('Unsupported settings schema')
    if name == 'graph/format.json' and values[0] != {'version': 1}:
        raise SafetyError('Unsupported graph format')
    if name.startswith('chats/'):
        if values[0].get('version') != 1 or values[0].get('id') != Path(name).stem:
            raise SafetyError('Unsupported chat schema or identity')


def collect(root):
    if linked(root) or not root.is_dir():
        raise SafetyError('Data root must be an existing directory')
    result, total = {}, 0
    def walk_error(_error):
        raise SafetyError('A data directory could not be inspected; no files may be omitted')
    for base, dirs, files in os.walk(root, followlinks=False, onerror=walk_error):
        for item in dirs:
            if linked(Path(base) / item):
                raise SafetyError('Symlink directory refused')
        for name in sorted(files):
            p = Path(base) / name
            rel = p.relative_to(root).as_posix()
            relative(rel)
            data = read_file(p)
            validate_payload(rel, data)
            result[rel] = data
            total += len(data)
            if total > MAX_TOTAL or len(result) > MAX_FILES:
                raise SafetyError('Snapshot exceeds limits')
    # Synthetic referential check, never claimed as upstream semantic validation.
    chats = {Path(p).stem for p in result if p.startswith('chats/')}
    for path, data in result.items():
        lines = data.splitlines() if path.endswith('.jsonl') else [data]
        for line in lines:
            if not line.strip():
                continue
            obj = parse(line)
            if 'chatId' in obj and obj['chatId'] not in chats:
                raise SafetyError('Dangling chat reference')
    return dict(sorted(result.items()))


def inventory(payload):
    return [{'path': p, 'sha256': digest(data), 'bytes': len(data)} for p, data in sorted(payload.items())]


def verify(folder):
    if linked(folder) or not folder.is_dir():
        raise SafetyError('Unsafe snapshot root')
    manifest = parse(read_file(folder / 'manifest.json', 2 * 1024 * 1024))
    if not isinstance(manifest, dict):
        raise SafetyError('Invalid manifest')
    sid = manifest.get('snapshot_id')
    body = {k: v for k, v in manifest.items() if k != 'snapshot_id'}
    if not isinstance(sid, str) or not HEX.fullmatch(sid) or digest(canonical(body)) != sid:
        raise SafetyError('Invalid manifest digest')
    if body.get('protocol_version') != 1 or body.get('format') != FORMAT or body.get('app_version') != 'synthetic-1':
        raise SafetyError('Incompatible protocol or application format')
    if not isinstance(body.get('device_id'), str) or not DEVICE.fullmatch(body['device_id']):
        raise SafetyError('Invalid device namespace')
    parents = body.get('parent_ids')
    if not isinstance(parents, list) or len(parents) > 1 or any(not isinstance(p, str) or not HEX.fullmatch(p) for p in parents):
        raise SafetyError('Invalid ancestry; resolutions not implemented')
    if read_file(folder / 'COMPLETE', 64) != sid.encode():
        raise SafetyError('Incomplete snapshot')
    if {p.name for p in folder.iterdir()} != {'manifest.json', 'COMPLETE', 'payload'}:
        raise SafetyError('Unexpected snapshot entries')
    payload = collect(folder / 'payload')
    if body.get('files') != inventory(payload):
        raise SafetyError('Snapshot integrity mismatch')
    return manifest, payload


class Engine:
    def __init__(self, root):
        raw = Path(root).absolute()
        # Reject links in the complete path before normalization.
        if any(linked(p) for p in [raw, *raw.parents]):
            raise SafetyError('Symlink workspace refused')
        self.root = raw.resolve()
        if parse(read_file(self.root / 'SANDBOX.json')) != {'format': FORMAT, 'purpose': 'synthetic-only'}:
            raise SafetyError('Only purpose-created synthetic workspaces are mutable')
        self.data = self.root / 'data'
        self.store = self.root / 'transport'
        self.state_file = self.root / 'ledger.json'

    @classmethod
    def initialize(cls, root, device):
        root = Path(root).absolute()
        if not DEVICE.fullmatch(device):
            raise SafetyError('Invalid device ID')
        if root.exists() or linked(root) or any(linked(p) for p in root.parents):
            raise SafetyError('Initialization requires a new isolated directory')
        mkdir(root)
        write(root / 'SANDBOX.json', canonical({'format': FORMAT, 'purpose': 'synthetic-only'}))
        for name in ('data', 'transport', 'backups'):
            mkdir(root / name)
        atomic_json(root / 'ledger.json', {'device': device, 'head': None, 'baseline': [], 'events': [], 'inspected': []})
        fsync_dir(root)
        return cls(root)

    @contextlib.contextmanager
    def locked(self):
        for protected in ('data', 'transport', 'backups', 'incoming', 'previous', 'ledger.json', 'journal.json', 'WRITER_ACTIVE'):
            if linked(self.root / protected):
                raise SafetyError('Symlink local engine path refused')
        fd = os.open(self.root / 'engine.lock', os.O_CREAT | os.O_RDWR | NOFOLLOW, 0o600)
        try:
            with exclusive(fd):
                if (self.root / 'WRITER_ACTIVE').exists():
                    raise SafetyError('Writer active; operation deferred')
                yield
        except BlockingIOError as exc:
            raise SafetyError('Another engine operation is active') from exc
        finally:
            os.close(fd)

    def state(self):
        return parse(read_file(self.state_file))

    def event(self, state, kind):
        state['events'].append({'time': int(time.time()), 'kind': kind})
        atomic_json(self.state_file, state)

    def _publish(self, payload, parents, device, local_backup=False):
        base = self.root / 'backups' / 'store' if local_backup else self.store
        folder = base / 'devices' / device / 'snapshots'
        mkdir(folder)
        body = {'protocol_version': 1, 'device_id': device, 'parent_ids': parents,
                'timestamp': int(time.time()), 'app_version': 'synthetic-1', 'format': FORMAT,
                'files': inventory(payload)}
        sid = digest(canonical(body))
        target = folder / sid
        if target.exists():
            verify(target)
            return sid
        stage = folder / ('.pending-' + uuid.uuid4().hex)
        mkdir(stage)
        mkdir(stage / 'payload')
        for name, data in payload.items():
            p = stage / 'payload' / name
            mkdir(p.parent)
            write(p, data)
        for base, _, _ in os.walk(stage / 'payload', topdown=False):
            fsync_dir(base)
        write(stage / 'manifest.json', canonical(dict(body, snapshot_id=sid)))
        write(stage / 'COMPLETE', sid.encode())
        fsync_dir(stage)
        verify(stage)
        os.rename(stage, target)
        fsync_dir(folder)
        return sid

    def export(self):
        with self.locked():
            self._recover()
            state = self.state()
            payload = collect(self.data)
            if state['head'] and inventory(payload) == state['baseline']:
                snapshots = self.snapshots()
                if state['head'] not in snapshots or snapshots[state['head']][0]['files'] != state['baseline']:
                    raise SafetyError('Current snapshot unavailable; publication cannot be confirmed')
                return state['head']
            sid = self._publish(payload, [state['head']] if state['head'] else [], state['device'])
            state.update(head=sid, baseline=inventory(payload))
            self.event(state, 'Snapshot published and verified')
            return sid

    def snapshots(self):
        if linked(self.store) or any(linked(p) for p in self.store.rglob('*')):
            raise SafetyError('Symlink transport entry refused')
        result = {}
        manifest_bytes = 0
        for folder in sorted(self.store.glob('devices/*/snapshots/*')):
            if folder.name.startswith('.'):
                continue
            manifest, payload = verify(folder)
            manifest_bytes += len(canonical(manifest))
            if manifest_bytes > 16 * 1024 * 1024:
                raise SafetyError('History metadata exceeds inspection limit; retain history and inspect offline')
            if folder.name != manifest['snapshot_id'] or folder.parent.parent.name != manifest['device_id']:
                raise SafetyError('Snapshot namespace mismatch')
            result[folder.name] = (manifest, None, folder)
            del payload
            if len(result) > 1024:
                raise SafetyError('History exceeds inspection limit')
        return result

    @staticmethod
    def ancestor(old, new, snapshots):
        pending, seen = [new], set()
        while pending:
            node = pending.pop()
            if node in seen:
                continue
            seen.add(node)
            if node not in snapshots:
                raise SafetyError('Unknown ancestry; await complete history')
            if node == old:
                return True
            pending.extend(snapshots[node][0]['parent_ids'])
        return False

    def apply(self, sid, bootstrap=False, failpoint=None):
        with self.locked():
            self._recover()
            state = self.state()
            snapshots = self.snapshots()
            if sid not in snapshots:
                raise SafetyError('Snapshot unavailable')
            current = collect(self.data)
            if state['head'] == sid:
                if inventory(current) != state['baseline']:
                    raise SafetyError('Local changes preserved; export before import')
                return 'Already applied'
            if state['head'] is None:
                if not bootstrap or current:
                    raise SafetyError('Explicit bootstrap requires an empty fixture peer')
            elif inventory(current) != state['baseline']:
                raise SafetyError('Local changes preserved; export before import')
            elif not self.ancestor(state['head'], sid, snapshots):
                raise SafetyError('Divergent or stale branch; both histories preserved')
            manifest, payload = verify(snapshots[sid][2])
            # Backup complete local tree first; retain indefinitely in MVP.
            backup_id = self._publish(current, [state['head']] if state['head'] else [], state['device'], local_backup=True)
            backup_folder = self.root / 'backups' / 'store' / 'devices' / state['device'] / 'snapshots' / backup_id
            verify(backup_folder)
            atomic_json(self.root / 'backups' / (uuid.uuid4().hex + '.json'), {'snapshot_id': backup_id})
            stage = self.root / 'incoming'
            if stage.exists():
                raise SafetyError('Unjournaled staging requires manual inspection')
            mkdir(stage)
            for name, data in payload.items():
                dest = stage / name
                mkdir(dest.parent)
                write(dest, data)
            for base, _, _ in os.walk(stage, topdown=False):
                fsync_dir(base)
            if inventory(collect(stage)) != manifest['files']:
                raise SafetyError('Staging integrity mismatch')
            journal = {'target': sid, 'old_state': state, 'phase': 'prepared'}
            atomic_json(self.root / 'journal.json', journal)
            self._interrupt(failpoint, 'prepared')
            os.rename(self.data, self.root / 'previous')
            fsync_dir(self.root)
            self._interrupt(failpoint, 'old-renamed')
            os.rename(stage, self.data)
            fsync_dir(self.root)
            self._interrupt(failpoint, 'new-renamed')
            state.update(head=sid, baseline=manifest['files'])
            self.event(state, 'Snapshot applied; previous state retained')
            self._interrupt(failpoint, 'ledger-written')
            # Mark durable commit before removal; previous retained under recovery name.
            journal['phase'] = 'committed'
            atomic_json(self.root / 'journal.json', journal)
            self._interrupt(failpoint, 'committed')
            self._recover()
            return 'Applied'

    @staticmethod
    def _interrupt(wanted, stage):
        if wanted == stage:
            raise SafetyError('Simulated interruption')

    def recover(self):
        with self.locked():
            self._recover()

    def _recover(self):
        journal_path = self.root / 'journal.json'
        if not journal_path.exists():
            return
        journal = parse(read_file(journal_path))
        previous, incoming = self.root / 'previous', self.root / 'incoming'
        if journal['phase'] != 'committed':
            if previous.exists():
                if self.data.exists():
                    os.rename(self.data, self.root / ('interrupted-' + uuid.uuid4().hex))
                os.rename(previous, self.data)
            if incoming.exists():
                os.rename(incoming, self.root / ('interrupted-' + uuid.uuid4().hex))
            atomic_json(self.state_file, journal['old_state'])
        elif previous.exists():
            os.rename(previous, self.root / ('recovery-' + uuid.uuid4().hex))
        fsync_dir(self.root)
        journal_path.unlink()
        fsync_dir(self.root)

    def restore(self, sid, confirmation):
        # An explicit fixture restore is a NEW causal revision, never a rewind.
        # A backup of the current state is made by the normal apply transaction.
        if not isinstance(sid, str) or not HEX.fullmatch(sid):
            raise SafetyError('Select a valid backup snapshot ID')
        if confirmation != 'RESTORE SYNTHETIC BACKUP':
            raise SafetyError('Type RESTORE SYNTHETIC BACKUP to confirm the isolated restore')
        with self.locked():
            self._recover()
            state = self.state()
            if not state['head'] or inventory(collect(self.data)) != state['baseline']:
                raise SafetyError('Publish local fixture changes before restoring')
            history = self.snapshots()
            if state['head'] not in history:
                raise SafetyError('Current history is unavailable')
            candidates = [self.root / 'backups' / 'store' / 'devices' / state['device'] / 'snapshots' / sid]
            if sid in history:
                candidates.insert(0, history[sid][2])
            manifest, payload = verify(candidates[0])
            if manifest['snapshot_id'] != sid:
                raise SafetyError('Backup identity mismatch')
            restored = self._publish(payload, [state['head']], state['device'])
        # apply rechecks baseline under its own lock if another local operation won.
        return self.apply(restored)
