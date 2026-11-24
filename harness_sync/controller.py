"""Shared desktop/CLI command boundary. Production filesystem mutations unavailable."""
from pathlib import Path
import time
import uuid
from .core import Engine, SafetyError, atomic_json
from .profile import Profile
from .status import status
from .transport import Client


def dispatch(profile, command, args=None):
    if not isinstance(command, str) or not isinstance(args or {}, dict):
        raise SafetyError('Invalid command request')
    args = args or {}
    if command == 'learnharness_audit':
        from .learnharness import audit
        return audit()
    if command == 'settings':
        return profile.public()
    if command == 'save_settings':
        return profile.save(args.get('changes', {}), args.get('api_key'))
    if command == 'initialize_synthetic':
        if args.get('confirmed') is not True:
            raise SafetyError('Confirm that only invented fixture records will be used')
        return {'workspace': profile.initialize_workspace()}
    settings = profile.load()
    workspace = settings['workspace']
    if command == 'status':
        result = status(workspace, key_file=profile.key_path if profile.key_path.exists() else None, port=settings['port'])
        result['settings'] = profile.public()
        result['automatic'] = settings['automatic']
        if workspace:
            result['publication_id'] = Engine(workspace).state()['device']
        return result
    if not workspace:
        raise SafetyError('Initialize an isolated synthetic workspace first. Production mutation remains disabled.')
    engine = Engine(workspace)
    if command in ('pair', 'stop_sharing'):
        if args.get('confirmed') is not True:
            raise SafetyError('Review and confirm local Syncthing configuration changes')
        client = Client(profile.key_path, settings['port'])
        with profile.locked():
            # Private rollback evidence, never returned/logged/synchronized.
            backup = {'time': int(time.time()), 'devices': client.request('GET', '/rest/config/devices'),
                      'folders': client.request('GET', '/rest/config/folders')}
            from .core import mkdir
            mkdir(profile.root / 'transport-config-backups')
            atomic_json(profile.root / 'transport-config-backups' / (uuid.uuid4().hex + '.json'), backup)
            if command == 'pair':
                result = client.pair(engine, args.get('device_id', ''), args.get('namespace', ''), settings['channel'],
                                     name=args.get('name', ''), confirmed=True)
            else:
                result = client.stop_sharing(engine, args.get('device_id', ''), args.get('namespace', ''), settings['channel'], confirmed=True)
        with engine.locked():
            engine.event(engine.state(), 'Dedicated Syncthing sharing configured' if command == 'pair' else 'Dedicated Syncthing sharing stopped')
        return result
    if command == 'export':
        return {'snapshot': engine.export()}
    if command == 'apply':
        if args.get('confirmed') is not True:
            raise SafetyError('Confirm the synthetic snapshot application')
        sid = args.get('snapshot', '')
        from .core import HEX
        if not isinstance(sid, str) or not HEX.fullmatch(sid):
            raise SafetyError('Select a valid snapshot ID')
        return {'message': engine.apply(sid, bootstrap=args.get('bootstrap') is True)}
    if command == 'restore':
        sid = args.get('snapshot', '')
        from .core import HEX
        if not isinstance(sid, str) or not HEX.fullmatch(sid):
            raise SafetyError('Select a valid backup ID')
        return {'message': engine.restore(sid, args.get('confirmation'))}
    if command == 'cycle':
        from .automation import tick
        return tick(workspace)
    raise SafetyError('Unsupported command')
