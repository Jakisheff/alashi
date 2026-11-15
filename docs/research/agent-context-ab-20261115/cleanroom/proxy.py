#!/usr/bin/env python3
"""Logging reverse proxy: logs {method, path, status} per request to jsonl."""

import json
import sys
import threading
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LISTEN_PORT = int(sys.argv[1])
TARGET = f'http://127.0.0.1:{sys.argv[2]}'
LOG = sys.argv[3]


class Proxy(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def _relay(self, method):
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length) if length else None
        request = urllib.request.Request(TARGET + self.path, data=body, method=method)
        for header in ('Content-Type',):
            if self.headers.get(header):
                request.add_header(header, self.headers[header])
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                payload = response.read()
                status = response.status
                ctype = response.headers.get('Content-Type', 'application/json')
        except urllib.error.HTTPError as error:
            payload = error.read()
            status = error.code
            ctype = error.headers.get('Content-Type', 'application/json')
        with open(LOG, 'a') as handle:
            entry = {'method': method, 'path': self.path, 'status': status}
            try:
                parsed = json.loads(payload)
                if isinstance(parsed, dict) and parsed.get('ok') is False:
                    entry['arena_error'] = parsed.get('error')
                    entry['message'] = (parsed.get('message') or '')[:120]
            except Exception:
                pass
            handle.write(json.dumps(entry) + '\n')
        self.send_response(status)
        self.send_header('Content-Type', ctype)
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_GET(self):
        self._relay('GET')

    def do_POST(self):
        self._relay('POST')

    def log_message(self, *args):
        pass


if __name__ == '__main__':
    server = ThreadingHTTPServer(('127.0.0.1', LISTEN_PORT), Proxy)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    print(f'proxy {LISTEN_PORT} -> {TARGET}, log {LOG}', flush=True)
    threading.Event().wait()
