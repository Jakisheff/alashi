#!/usr/bin/env python3
"""Exercise HTTP resource limits on a disposable arena; never uses the live port."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def main():
    with tempfile.TemporaryDirectory(prefix="alashi-security-") as scratch:
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        env = dict(os.environ, ALASHI_STATE_FILE=str(Path(scratch) / "state.json"),
                   ALASHI_SEQ_FILE=str(Path(scratch) / "seq"))
        connections = []
        with open(Path(scratch) / "server.log", "w") as log:
            process = subprocess.Popen([str(ROOT / "arena/target/release/arenad"), "--port", str(port)],
                                       cwd=ROOT, env=env, stdout=log, stderr=log)
            try:
                def get(path, body=None):
                    request = urllib.request.Request(f"http://127.0.0.1:{port}{path}",
                        data=None if body is None else json.dumps(body).encode(),
                        headers={"Content-Type": "application/json"})
                    with urllib.request.urlopen(request, timeout=3) as response:
                        return json.load(response)

                for _ in range(50):
                    assert process.poll() is None, "test server exited"
                    try:
                        get("/games")
                        break
                    except OSError:
                        time.sleep(0.1)
                else:
                    raise AssertionError("test server did not become ready")

                # Occupy all 64 workers with incomplete requests, below the read deadline.
                for _ in range(64):
                    conn = socket.create_connection(("127.0.0.1", port), timeout=3)
                    conn.settimeout(3)
                    conn.sendall(b"GET /games HTTP/1.1\r\nHost: localhost\r\n")
                    connections.append(conn)
                with socket.create_connection(("127.0.0.1", port), timeout=3) as excess:
                    excess.settimeout(3)
                    response = excess.recv(4096)
                    assert response.startswith(b"HTTP/1.1 503"), response
                for conn in connections:
                    conn.close()
                connections.clear()
                for _ in range(30):
                    try:
                        assert get("/games")["ok"]
                        break
                    except OSError:
                        time.sleep(0.1)
                else:
                    raise AssertionError("worker permits did not release")
                print("PASS: 65th connection rejected; service recovered after clients closed", flush=True)

                created = get("/game/new", {"lobby_duration": 600})
                gid = created["game_id"]
                assert (Path(scratch) / "state.json").stat().st_mode & 0o777 == 0o600
                for _ in range(16):
                    conn = socket.create_connection(("127.0.0.1", port), timeout=3)
                    conn.settimeout(3)
                    conn.sendall(f"GET /game/{gid}/wait?r=0&p=lobby&t=60 HTTP/1.1\r\nHost: localhost\r\n\r\n".encode())
                    connections.append(conn)
                time.sleep(0.2)
                with socket.create_connection(("127.0.0.1", port), timeout=3) as excess:
                    excess.settimeout(3)
                    excess.sendall(f"GET /game/{gid}/wait?r=0&p=lobby&t=60 HTTP/1.1\r\nHost: localhost\r\n\r\n".encode())
                    response = excess.recv(4096)
                    assert response.startswith(b"HTTP/1.1 503"), response
                assert get("/games")["ok"]
                print("PASS: long-poll quota leaves ordinary API available; snapshot mode is 0600", flush=True)
            finally:
                for conn in connections:
                    conn.close()
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


if __name__ == "__main__":
    main()
