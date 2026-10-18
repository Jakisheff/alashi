"""Read-only public release checks, plus unauthenticated requests that must fail."""
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import urllib.error
import urllib.request

BASE = 'https://alashi.network'
SHA = '523e14ac058a58ff25bc4786407fbb8bd60105fe'
RELEASE = 'main-523e14a-20261018'
DIST = Path(__file__).resolve().parents[3] / 'frontend/dist'
results = []

def request(path, *, method='GET', body=None, origin=None):
    headers = {'Accept': '*/*'}
    if body is not None:
        body = json.dumps(body).encode()
        headers['Content-Type'] = 'application/json'
    if origin:
        headers['Origin'] = origin
    req = urllib.request.Request(BASE + path, data=body, method=method, headers=headers)
    try:
        response = urllib.request.urlopen(req, timeout=20)
    except urllib.error.HTTPError as error:
        response = error
    data = response.read()
    results.append({'path': path, 'method': method, 'status': response.status, 'bytes': len(data), 'content_type': response.headers.get('Content-Type'), 'cache_control': response.headers.get('Cache-Control'), 'sha256': hashlib.sha256(data).hexdigest()})
    return response.status, data, response.headers

class Assets(HTMLParser):
    def __init__(self):
        super().__init__()
        self.paths = []
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'script' and attrs.get('src'):
            self.paths.append(attrs['src'])
        if tag == 'link' and 'stylesheet' in attrs.get('rel', '').split():
            self.paths.append(attrs['href'])

for path in ['/', '/strategies', '/arena', '/devnet']:
    status, data, _ = request(path)
    assert status == 200 and data == (DIST / 'index.html').read_bytes(), path
status, data, _ = request('/version.json')
version = json.loads(data)
assert status == 200 and version['frontend_commit'] == SHA and version['backend_commit'] == SHA and version['release'] == RELEASE
assets = Assets()
assets.feed((DIST / 'index.html').read_text())
for path in assets.paths:
    assert path.startswith('/releases/' + RELEASE + '/')
    status, data, _ = request(path)
    assert status == 200 and data == (DIST / path.split('/' + RELEASE + '/', 1)[1]).read_bytes(), path
for suffix in ['strategies/map-layout.json', 'models/desk-genie.glb']:
    status, data, headers = request('/releases/' + RELEASE + '/' + suffix)
    assert status == 200 and data == (DIST / suffix).read_bytes(), suffix
status, data, headers = request('/strategy-library')
assert status == 200 and json.loads(data)['ok'] is True and headers.get('Cache-Control') == 'no-store'
for method, suffix, origin in [('GET', 'strategies', None), ('POST', 'strategy-runs', BASE), ('POST', 'strategy-runs', 'https://invalid.example')]:
    status, data, _ = request('/agents/' + 'a' * 64 + '/owner/' + suffix, method=method, body={} if method == 'POST' else None, origin=origin)
    assert status in (400, 401, 403) and json.loads(data)['ok'] is False
status, data, _ = request('/deck/')
assert status == 200 and b'<html' in data.lower()
status, data, _ = request('/chain/devnet/games/GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b')
assert status == 200 and json.loads(data)['ok'] is True
output = Path(__file__).with_name('public-result.json')
output.write_text(json.dumps({'release': RELEASE, 'source_commit': SHA, 'passed': True, 'checks': results}, indent=2) + '\n')
print(json.dumps({'passed': True, 'checks': len(results), 'release': RELEASE}))
