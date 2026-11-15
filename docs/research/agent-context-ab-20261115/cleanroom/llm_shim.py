#!/usr/bin/env python3
"""Loopback LLM shim for agent binaries hardcoding the plain PAAS path.

agent.rs calls {ALASHI_TEST_MOCK_LLM_BASE}/chat/completions on a loopback
base. This shim forwards to the coding-plan endpoint with the plan key, so
the stock binary stays unmodified. Usage: llm_shim.py <listen_port>
"""

import json
import os
import sys
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import threading

PORT = int(sys.argv[1])
UPSTREAM = 'https://api.z.ai/api/coding/paas/v4/chat/completions'
KEY = json.load(open(os.path.expanduser('~/.local/share/opencode/auth.json')))['zai-coding-plan']['key']
LOG = os.path.join(os.path.dirname(os.path.abspath(__file__)), f'llm-shim-{PORT}.jsonl')


class Shim(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def do_POST(self):
        length = int(self.headers.get('Content-Length') or 0)
        body = self.rfile.read(length)
        request = urllib.request.Request(UPSTREAM, data=body, method='POST')
        request.add_header('Content-Type', 'application/json')
        request.add_header('Authorization', 'Bearer ' + KEY)
        try:
            with urllib.request.urlopen(request, timeout=170) as response:
                payload = response.read()
                status = response.status
        except urllib.error.HTTPError as error:
            payload = error.read()
            status = error.code
        except Exception as error:
            payload = json.dumps({'shim_error': str(error)[:200]}).encode()
            status = 502
        usage = {}
        try:
            usage = json.loads(payload).get('usage', {})
        except Exception:
            pass
        with open(LOG, 'a') as handle:
            handle.write(json.dumps({'status': status, 'usage': usage}) + '\n')
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *args):
        pass


if __name__ == '__main__':
    server = ThreadingHTTPServer(('127.0.0.1', PORT), Shim)
    print(f'llm shim on {PORT} -> {UPSTREAM}', flush=True)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    threading.Event().wait()
