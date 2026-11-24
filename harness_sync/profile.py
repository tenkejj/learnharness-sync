"""Local-only settings and secrets, never synchronized. No production data mutation."""
from contextlib import contextmanager
import os
from pathlib import Path
import re
from .core import SafetyError, atomic_json, mkdir, parse, read_file, write
from .platforms import NOFOLLOW, exclusive

DEFAULTS = {'version': 1, 'port': 8384, 'workspace': None, 'channel': 'harness-sync', 'automatic': False, 'bounded_wait': 2}


class Profile:
    def __init__(self, root):
        self.root = Path(root).absolute()
        mkdir(self.root)
        mkdir(self.root / 'secrets')
        if os.name != 'nt' and self.root.stat().st_mode & 0o077:
            raise SafetyError('LearnHarness Sync profile must be private (mode 0700)')
        self.path = self.root / 'settings.json'
        self.key_path = self.root / 'secrets' / 'syncthing-api.key'

    @contextmanager
    def locked(self):
        fd = os.open(self.root / 'profile.lock', os.O_RDWR | os.O_CREAT | NOFOLLOW, 0o600)
        try:
            with exclusive(fd):
                yield
        finally:
            os.close(fd)

    def load(self):
        if not self.path.exists():
            return dict(DEFAULTS)
        value = parse(read_file(self.path, 16384))
        self.validate(value)
        return value

    @staticmethod
    def validate(value):
        if not isinstance(value, dict) or set(value) != set(DEFAULTS) or value['version'] != 1:
            raise SafetyError('Unsupported settings format')
        if type(value['port']) is not int or not 1 <= value['port'] <= 65535:
            raise SafetyError('Use a valid loopback API port')
        if not isinstance(value['channel'], str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,48}', value['channel']):
            raise SafetyError('Use a short channel name with letters, digits, underscore or dash')
        if type(value['bounded_wait']) is not int or not 0 <= value['bounded_wait'] <= 10:
            raise SafetyError('Bounded wait must be between zero and ten seconds')
        if type(value['automatic']) is not bool or value['workspace'] is not None and not isinstance(value['workspace'], str):
            raise SafetyError('Invalid automation or workspace setting')

    def save(self, changes, api_key=None):
        if not isinstance(changes, dict) or set(changes) - {'port', 'channel', 'automatic', 'bounded_wait'}:
            raise SafetyError('Unsupported setting; production paths cannot be changed here')
        with self.locked():
            value = self.load()
            value.update(changes)
            self.validate(value)
            if value['automatic'] and not value['workspace']:
                raise SafetyError('Automation requires an isolated synthetic workspace')
            if api_key is not None:
                if not isinstance(api_key, str) or not re.fullmatch(r'[A-Za-z0-9_-]{8,256}', api_key):
                    raise SafetyError('Invalid API key format')
                # Dedicated file; atomic private replacement. Never echo it to UI.
                from .core import fsync_dir
                import uuid
                temp = self.key_path.with_name('.key-' + uuid.uuid4().hex)
                write(temp, api_key.encode())
                os.replace(temp, self.key_path)
                fsync_dir(self.key_path.parent)
            atomic_json(self.path, value)
            return self.public()

    def initialize_workspace(self):
        from .core import Engine
        import uuid
        with self.locked():
            value = self.load()
            if value['workspace']:
                Engine(value['workspace'])
                return value['workspace']
            target = self.root / 'synthetic-workspace'
            # Never adopt existing data or overwrite a previous partial setup.
            Engine.initialize(target, uuid.uuid4().hex)
            value['workspace'] = str(target)
            atomic_json(self.path, value)
            return str(target)

    def public(self):
        return dict(self.load(), api_key_configured=self.key_path.is_file(), production_mutation=False)
