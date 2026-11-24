"""Read-only product projection; no learning payloads are returned."""
import os
from pathlib import Path
import shutil
import socket
from .core import Engine, SafetyError, verify, parse, read_file
from .transport import health


def status(workspace=None, key_file=None, port=None):
    from .platforms import learnharness_path
    path = learnharness_path()
    result = {'mode': 'monitor', 'state': 'Setup required', 'device': socket.gethostname(),
              'data_path': str(path), 'detected': path.is_dir(), 'head': None, 'snapshots': [],
              'backups': [], 'events': [], 'conflicts': [], 'automatic': False,
              'integration': 'Deferred — writer exclusion is not proven',
              'transport_installed': bool(shutil.which('syncthing')),
              'transport': health(key_file or os.environ.get('HARNESS_SYNC_KEY_FILE'), port or os.environ.get('HARNESS_SYNC_ST_PORT', '8384')),
              'limitations': ['Production import and export are disabled.',
                              'Use isolated synthetic workspaces to validate the protocol.',
                              'LearnHarness semantic compatibility and writer supervision require further integration.']}
    if workspace:
        engine = Engine(workspace)
        state = engine.state()
        snapshots = engine.snapshots()
        result.update(mode='synthetic', device=state['device'], data_path=str(engine.data), head=state['head'],
                      state='Review required' if (engine.root / 'journal.json').exists() else 'Synthetic workspace ready',
                      events=state['events'])
        for sid, (manifest, _, _) in snapshots.items():
            result['snapshots'].append({'id': sid, 'device': manifest['device_id'], 'time': manifest['timestamp'],
                                        'files': len(manifest['files']), 'bytes': sum(f['bytes'] for f in manifest['files']),
                                        'format': manifest['format'], 'verified': True, 'applied': sid == state['head']})
        for sid in snapshots:
            if state['head'] and sid != state['head']:
                try:
                    if not engine.ancestor(state['head'], sid, snapshots) and not engine.ancestor(sid, state['head'], snapshots):
                        result['conflicts'].append(sid)
                except SafetyError:
                    result['conflicts'].append(sid)
        for p in sorted((engine.root / 'backups').glob('*.json')):
            backup = parse(read_file(p))['snapshot_id']
            manifest, _ = verify(engine.root / 'backups' / 'store' / 'devices' / state['device'] / 'snapshots' / backup)
            result['backups'].append({'id': backup, 'device': manifest['device_id'], 'time': manifest['timestamp'],
                                      'files': len(manifest['files']), 'bytes': sum(f['bytes'] for f in manifest['files']),
                                      'format': manifest['format'], 'verified': True, 'applied': False})
        if result['conflicts']:
            result['state'] = 'Divergent histories preserved'
    return result
