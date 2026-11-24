import argparse
import json
from pathlib import Path
import signal
import sys
import threading
import time
from .core import Engine, SafetyError, atomic_json, mkdir
from .status import status


def main():
    parser = argparse.ArgumentParser(description='LearnHarness Sync — private, local-first synchronization for LearnHarness')
    sub = parser.add_subparsers(dest='command', required=True)
    init = sub.add_parser('init-sandbox', help='Create a NEW synthetic-only workspace')
    init.add_argument('workspace'); init.add_argument('--device', required=True)
    for name in ('status', 'export', 'recover', 'watch', 'serve', 'desktop', 'diagnostics'):
        p = sub.add_parser(name)
        p.add_argument('--workspace')
        p.add_argument('--profile')
        if name in ('serve', 'desktop'):
            p.add_argument('--port', type=int, default=8743 if name == 'serve' else 0)
    apply = sub.add_parser('apply')
    apply.add_argument('snapshot'); apply.add_argument('--workspace', required=True)
    apply.add_argument('--bootstrap', action='store_true')
    rpc = sub.add_parser('rpc', help='One bounded JSON request on stdin for the native sidecar')
    rpc.add_argument('--profile', required=True)
    args = parser.parse_args()
    if args.command == 'rpc':
        from .profile import Profile
        from .controller import dispatch
        from .core import parse
        raw = sys.stdin.buffer.read(32769)
        try:
            if len(raw) > 32768:
                raise SafetyError('Command exceeds size limit')
            request = parse(raw)
            if not isinstance(request, dict) or set(request) != {'command', 'args'}:
                raise SafetyError('Invalid command envelope')
            result = dispatch(Profile(args.profile), request['command'], request['args'])
            print(json.dumps({'result': result}))
        except Exception as exc:
            print(json.dumps({'error': str(exc) if isinstance(exc, SafetyError) else 'Engine operation failed; inspect local diagnostics.'}))
            sys.exit(1)
        return
    if args.command == 'init-sandbox':
        Engine.initialize(args.workspace, args.device)
        print('Created synthetic workspace. Never place production learning data here.')
    elif args.command in ('status', 'diagnostics'):
        print(json.dumps(status(args.workspace), indent=2))
    elif args.command in ('export', 'recover', 'apply'):
        if not args.workspace:
            raise SafetyError('Production mutations unavailable. Select a synthetic workspace.')
        engine = Engine(args.workspace)
        result = engine.export() if args.command == 'export' else engine.recover() if args.command == 'recover' else engine.apply(args.snapshot, args.bootstrap)
        print(result or 'Recovery complete')
    elif args.command == 'watch':
        # Independent daemon is a read-only observer. No process-name-based writes.
        while True:
            if args.profile:
                from .profile import Profile
                from .controller import dispatch
                profile = Profile(args.profile)
                if profile.load()['automatic']:
                    try:
                        dispatch(profile, 'cycle')
                    except SafetyError:
                        pass  # status reports safety state; never force an apply
                view = dispatch(profile, 'status')
            else:
                view = status(args.workspace)
            print(json.dumps({'state': view['state'], 'time': int(time.time())}), flush=True)
            time.sleep(10)
    else:
        from .server import create_server
        from .profile import Profile
        from .platforms import profile_path
        profile = None if args.workspace else Profile(args.profile or profile_path())
        server = create_server(args.workspace, args.port, profile)
        if args.command == 'serve':
            print(f'LearnHarness Sync: http://127.0.0.1:{server.server_port}', flush=True)
            server.serve_forever()
        else:
            import gi
            gi.require_version('Gtk', '3.0')
            gi.require_version('WebKit2', '4.1')
            from gi.repository import Gtk, WebKit2
            threading.Thread(target=server.serve_forever, daemon=True).start()
            window = Gtk.Window(title='LearnHarness Sync — experimental')
            window.set_default_size(1180, 800)
            window.set_icon_from_file(str(Path(__file__).resolve().parent.parent / 'assets/brand/learnharness-sync-app-icon-512.png'))
            webview = WebKit2.WebView()
            webview.get_settings().set_enable_developer_extras(False)
            webview.load_uri(f'http://127.0.0.1:{server.server_port}')
            window.add(webview)
            window.connect('destroy', Gtk.main_quit)
            window.show_all()
            try:
                Gtk.main()
            finally:
                server.shutdown()
                server.server_close()

if __name__ == '__main__':
    try:
        main()
    except (SafetyError, OSError, ValueError, RuntimeError) as exc:
        # Never print payloads or arbitrary OS error strings containing private paths.
        print(str(exc) if isinstance(exc, (SafetyError, RuntimeError)) else 'Operation failed; check paths and permissions. Data retained.', file=sys.stderr)
        sys.exit(1)
