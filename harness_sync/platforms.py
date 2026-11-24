"""Native paths and filesystem primitives; production lifecycle remains fail-closed."""
from contextlib import contextmanager
import os
from pathlib import Path
import sys

NOFOLLOW = getattr(os, 'O_NOFOLLOW', 0)
NONBLOCK = getattr(os, 'O_NONBLOCK', 0)


def linked(path):
    try:
        info = Path(path).lstat()
        return Path(path).is_symlink() or bool(getattr(info, 'st_file_attributes', 0) & 0x400)
    except FileNotFoundError:
        return False


def app_data(platform=None, home=None, env=None):
    platform = platform or sys.platform
    home = Path(home) if home else Path.home()
    env = os.environ if env is None else env
    if platform == 'win32':
        if sys.platform != 'win32':
            raise RuntimeError('Windows known-folder resolution requires Windows')
        import ctypes
        from ctypes import wintypes
        # SHGetFolderPathW CSIDL_APPDATA, backed by the Windows shell API.
        buffer = ctypes.create_unicode_buffer(32768)
        shell = ctypes.WinDLL('shell32', use_last_error=True)
        shell.SHGetFolderPathW.argtypes = [wintypes.HWND, ctypes.c_int, wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR]
        shell.SHGetFolderPathW.restype = ctypes.c_long
        if shell.SHGetFolderPathW(None, 26, None, 0, buffer) != 0:
            raise RuntimeError('Windows application data folder unavailable')
        return Path(buffer.value)
    if platform == 'darwin':
        return home / 'Library' / 'Application Support'
    return Path(env.get('XDG_CONFIG_HOME', str(home / '.config')))


def learnharness_path():
    return app_data() / 'LearnHarness' / 'data'


def profile_path():
    return app_data() / 'community.harnesssync.desktop'


def sync_directory(path):
    if sys.platform == 'win32':
        # No directory FlushFileBuffers guarantee. Never enable production mutation.
        return
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | NOFOLLOW)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


@contextmanager
def exclusive(fd):
    if sys.platform == 'win32':
        import msvcrt
        os.lseek(fd, 0, os.SEEK_SET)
        if os.fstat(fd).st_size == 0:
            os.write(fd, b'0')
        os.lseek(fd, 0, os.SEEK_SET)
        try:
            msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
        except OSError as exc:
            raise BlockingIOError('Another operation is active') from exc
        try:
            yield
        finally:
            os.lseek(fd, 0, os.SEEK_SET)
            msvcrt.locking(fd, msvcrt.LK_UNLCK, 1)
    else:
        import fcntl
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        try:
            yield
        finally:
            fcntl.flock(fd, fcntl.LOCK_UN)


def lifecycle_capabilities():
    return {'platform': sys.platform, 'production_mutation': False,
            'reason': 'No verified exclusive LearnHarness writer lifecycle adapter is installed.',
            'directory_flush': sys.platform != 'win32',
            'automation': 'systemd-user' if sys.platform.startswith('linux') else 'launchd' if sys.platform == 'darwin' else 'windows-task-scheduler'}
