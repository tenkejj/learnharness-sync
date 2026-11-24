"""Linux process-family supervisor for a purpose-created isolated workspace.

This helper MUST run in its own process: it becomes a child subreaper. It never
asserts exclusive access against unmanaged launches and cannot unlock production.
A crash/timeout leaves WRITER_ACTIVE in place, blocking all fixture mutation.
"""
import argparse
import ctypes
import errno
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
from .core import Engine, SafetyError, canonical, fsync_dir, mkdir, write

PR_SET_CHILD_SUBREAPER = 36
PR_GET_CHILD_SUBREAPER = 37


def become_subreaper():
    if not sys.platform.startswith('linux'):
        raise SafetyError('This process-family adapter requires Linux')
    libc = ctypes.CDLL(None, use_errno=True)
    libc.prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
    libc.prctl.restype = ctypes.c_int
    if libc.prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) != 0:
        raise SafetyError('Kernel child supervision unavailable; launch refused')
    current = ctypes.c_int(0)
    if libc.prctl(PR_GET_CHILD_SUBREAPER, ctypes.addressof(current), 0, 0, 0) != 0 or current.value != 1:
        raise SafetyError('Child supervision could not be verified')


def run(workspace, argv, timeout=60):
    if not argv or not 0 < timeout <= 3600:
        raise SafetyError('A bounded isolated launch command is required')
    engine = Engine(workspace)  # refuses adoption of any ordinary existing data tree
    become_subreaper()
    isolated = engine.root / 'application'
    for part in ('home', 'config', 'cache', 'state', 'runtime'):
        mkdir(isolated / part)
    allowed = {'PATH', 'LANG', 'LC_ALL', 'LC_CTYPE', 'TZ', 'DISPLAY', 'WAYLAND_DISPLAY',
               'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'XAUTHORITY', 'GTK_THEME',
               'GDK_BACKEND', 'QT_QPA_PLATFORM'}
    env = {name: value for name, value in os.environ.items() if name in allowed}
    env.update(HOME=str(isolated / 'home'), XDG_CONFIG_HOME=str(isolated / 'config'),
               XDG_CACHE_HOME=str(isolated / 'cache'), XDG_STATE_HOME=str(isolated / 'state'),
               LH_DATA_DIR=str(engine.data))
    # Display/session transport is inherited for a real local GUI, but private
    # app data is redirected. This is not a security sandbox for malicious code.
    with engine.locked():
        marker = engine.root / 'WRITER_ACTIVE'
        write(marker, canonical({'supervisor': os.getpid(), 'state': 'active', 'scope': 'isolated-family-only'}))
        fsync_dir(engine.root)
        child = subprocess.Popen(argv, env=env, cwd=isolated, stdin=subprocess.DEVNULL,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        deadline = time.monotonic() + timeout
        root_status = None
        while True:
            try:
                pid, status = os.waitpid(-1, os.WNOHANG)
            except ChildProcessError:
                if root_status is None:
                    raise SafetyError('Root exit status unavailable; workspace remains quarantined')
                break  # ECHILD: every adopted/detached descendant has been reaped
            except InterruptedError:
                continue
            if pid:
                if pid == child.pid:
                    root_status = os.waitstatus_to_exitcode(status)
                    child.returncode = root_status
                continue
            if time.monotonic() >= deadline:
                # Do not clear the durable marker or infer quiescence on timeout.
                # Terminate the launched process group only; detached children can
                # remain alive, which is precisely why the marker remains active.
                try:
                    os.killpg(child.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                raise SafetyError('Isolated process family did not stop; workspace remains quarantined')
            time.sleep(0.02)
        # A failed launch/app exit still needs operator review; never publish it.
        if root_status != 0:
            raise SafetyError('Isolated application failed; workspace remains quarantined')
        marker.unlink()
        fsync_dir(engine.root)
        return {'root_exit': root_status, 'family_reaped': True, 'production_exclusion': False,
                'application_data': str(isolated / 'config' / 'LearnHarness' / 'data')}


def main():
    import json
    parser = argparse.ArgumentParser(description='Isolated Linux process-family integration test helper')
    parser.add_argument('--workspace', required=True)
    parser.add_argument('--timeout', type=float, default=60)
    parser.add_argument('--invented-data-only', action='store_true', required=True)
    parser.add_argument('argv', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    argv = args.argv[1:] if args.argv[:1] == ['--'] else args.argv
    try:
        print(json.dumps(run(args.workspace, argv, args.timeout)))
    except (SafetyError, OSError):
        print(json.dumps({'error': 'Isolated launch did not prove clean family shutdown; inspect the quarantined workspace.',
                          'production_exclusion': False}))
        sys.exit(1)

if __name__ == '__main__':
    main()
