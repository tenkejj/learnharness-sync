"""Same-origin, loopback-only, read-only dashboard API."""
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import secrets
from pathlib import Path
from urllib.parse import urlsplit
from .status import status


class Handler(SimpleHTTPRequestHandler):
    workspace = None
    profile = None
    csrf = None

    def log_message(self, *_args):
        pass  # no URL, payload or credentials in logs

    def end_headers(self):
        self.send_header('X-Content-Type-Options', 'nosniff')
        self.send_header('Referrer-Policy', 'no-referrer')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Security-Policy', "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; connect-src 'self'; frame-ancestors 'none'")
        super().end_headers()

    def do_GET(self):
        host = self.headers.get('Host', '')
        if host not in (f'127.0.0.1:{self.server.server_port}', f'localhost:{self.server.server_port}'):
            self.send_error(403)
            return
        if self.headers.get('Sec-Fetch-Site', 'none') not in ('none', 'same-origin'):
            self.send_error(403)
            return
        path = urlsplit(self.path).path
        if path == '/api/session' and self.profile is not None:
            self.json_response(200, {'token': self.csrf})
            return
        if path == '/api/settings' and self.profile is not None:
            self.json_response(200, self.profile.public())
            return
        if path == '/api/status':
            try:
                if self.profile is not None:
                    from .controller import dispatch
                    value = dispatch(self.profile, 'status')
                else:
                    value = status(self.workspace)
                body = json.dumps(value).encode()
                self.send_response(200)
            except Exception:
                body = json.dumps({'error': 'Engine inspection failed. Run diagnostics against the synthetic workspace; data was not changed.'}).encode()
                self.send_response(503)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        # Serve build assets only. Never expose repository, workspace or directory listings.
        if path == '/':
            self.path = '/index.html'
        elif path != '/index.html' and not path.startswith(('/assets/', '/brand/')):
            self.send_error(404)
            return
        return super().do_GET()

    def json_response(self, code, value):
        body = json.dumps(value).encode()
        self.send_response(code)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if self.profile is None:
            self.send_error(501)
            return
        host = f'127.0.0.1:{self.server.server_port}'
        if (self.headers.get('Host') != host or self.headers.get('Origin') != 'http://' + host
            or self.headers.get('Sec-Fetch-Site', 'none') not in ('same-origin', 'none')
            or not self.csrf or not secrets.compare_digest(self.headers.get('X-HarnessSync-Token', ''), self.csrf)):
            self.json_response(403, {'error': 'Same-origin authorization required'})
            return
        if self.path != '/api/command' or self.headers.get('Content-Type') != 'application/json':
            self.json_response(400, {'error': 'Unsupported command request'})
            return
        try:
            length = int(self.headers.get('Content-Length', '0'))
            if not 0 < length <= 32768 or self.headers.get('Transfer-Encoding'):
                raise ValueError('length')
            from .core import parse, SafetyError
            request = parse(self.rfile.read(length))
            if not isinstance(request, dict) or set(request) != {'command', 'args'}:
                raise ValueError('shape')
            from .controller import dispatch
            result = dispatch(self.profile, request['command'], request['args'])
            self.json_response(200, {'result': result})
        except (ValueError, TypeError, KeyError):
            self.json_response(400, {'error': 'Invalid command arguments'})
        except Exception as exc:
            from .core import SafetyError
            self.json_response(409, {'error': str(exc) if isinstance(exc, SafetyError) else 'Operation failed; no success is claimed. Inspect local diagnostics.'})

    def list_directory(self, _path):
        self.send_error(404)


def create_server(workspace=None, port=8743, profile=None):
    root = Path(__file__).resolve().parent.parent / 'dist'
    if not (root / 'index.html').is_file():
        raise RuntimeError('Build frontend first: npm run build')
    class BoundHandler(Handler):
        pass
    BoundHandler.workspace = workspace
    BoundHandler.profile = profile
    BoundHandler.csrf = secrets.token_urlsafe(32)
    return ThreadingHTTPServer(('127.0.0.1', port), partial(BoundHandler, directory=str(root)))
