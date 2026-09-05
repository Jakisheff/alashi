"""Exercise the release arena with isolated state, including an abrupt restart."""
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request

REPO = Path(__file__).resolve().parents[1]
SCRATCH = Path(tempfile.mkdtemp(prefix="alashi-demo-restart-"))
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    PORT = sock.getsockname()[1]
ENV = dict(os.environ, ALASHI_STATE_FILE=str(SCRATCH / "state.json"),
           ALASHI_SEQ_FILE=str(SCRATCH / "seq"))
proc = None
log = None


def request(path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}{path}", data=data,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=3) as response:
        return json.load(response)


def start():
    global proc, log
    log = (SCRATCH / "server.log").open("a")
    proc = subprocess.Popen([str(REPO / "arena/target/release/arenad"), "--port", str(PORT),
                             "--tick-ms", "50"], cwd=REPO, env=ENV,
                            stdout=log, stderr=log, start_new_session=True)
    for _ in range(100):
        if proc.poll() is not None:
            raise RuntimeError("temporary arena exited before startup")
        try:
            request("/")
            return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("temporary arena startup timed out")


def stop(abrupt=False):
    global proc
    if proc is not None and proc.poll() is None:
        os.killpg(proc.pid, signal.SIGKILL if abrupt else signal.SIGTERM)
        proc.wait(timeout=5)
    if log:
        log.close()
    proc = None


def wait_for_checkpoint(gid, round_number, phase_number):
    for _ in range(100):
        doc = json.loads((SCRATCH / "state.json").read_text())
        entry = next(g for g in doc["games"] if g["game_id"] == gid)
        raw = bytes.fromhex(entry["game_hex"])
        # Borsh Game begins with admin (32), game_id (8), phase (1), round (1).
        if raw[40] == phase_number and raw[41] == round_number:
            return doc
        time.sleep(0.01)
    raise AssertionError("automatic transition was not checkpointed")


try:
    start()
    created = request("/game/new", {"epoch": "90s", "phase_duration": 2,
                                    "lobby_duration": 30, "grace_s": 0, "label": "restart-smoke"})
    assert created["ok"], created
    gid, party = created["game_id"], created["state"]["party_no"]
    identities = [{"name": f"Smoke{i}", "model": f"smoke-{i}", "prompt": "demo-restart"} for i in range(6)]
    tokens = []
    recovery_secrets = []
    for identity in identities:
        joined = request(f"/game/{gid}/join", identity)
        assert joined["ok"], joined
        tokens.append(joined["token"])
        recovery_secrets.append(joined["recovery_secret"])
    print("SMOKE: six agents joined an isolated release arena", flush=True)
    deadline = time.monotonic() + 90
    next_report = time.monotonic() + 15
    restarted = False
    bought_insight = False
    voted = 0
    while time.monotonic() < deadline:
        response = request(f"/game/{gid}/state")
        if response.get("finished"):
            result = response["result"]
            break
        state = response["state"]
        assert not state.get("settlement_error"), state.get("settlement_error")
        if state["round"] == 2 and state["phase"] == "action" and not restarted:
            before = wait_for_checkpoint(gid, 2, 2)
            stop(abrupt=True)
            start()
            after = request(f"/game/{gid}/state")["state"]
            assert (after["game_id"], after["party_no"]) == (gid, party)
            assert len(after["factions"]) == 6
            assert [f["cash"] for f in after["factions"]] == [f["cash"] for f in state["factions"]]
            restored = json.loads((SCRATCH / "state.json").read_text())
            assert restored["master_seed"] == before["master_seed"]
            recovered = request(f"/game/{gid}/join", dict(identities[0], recover=True, recovery_secret=recovery_secrets[0]))
            assert recovered["ok"] and recovered["recovered"]
            stale = request(f"/game/{gid}/act", {"token": tokens[0], "action": "produce"})
            assert stale["error"] == "bad_token"
            tokens[0] = recovered["token"]
            restarted = True
            state = recovered["state"]
            print("SMOKE: abrupt restart preserved players, cash, phase, seed; recover revoked the old token", flush=True)
        if state["phase"] == "action" and state["round"] == 4 and not bought_insight:
            bought = request(f"/game/{gid}/act", {"token": tokens[0], "action": "inspect_license"})
            assert bought["ok"], bought
            bid = request(f"/game/{gid}/act", {"token": tokens[0], "action": "bid_license", "params": {"amount": 1000000}})
            assert bid["ok"], bid
            bought_insight = True
        for i, faction in enumerate(state["factions"]):
            if state["phase"] == "market" and not faction["acted"] and faction["goods"]:
                action, params = "sell", {"units": faction["goods"]}
            elif state["phase"] == "action" and not faction["acted"]:
                action, params = "produce", {}
            elif state["phase"] == "law" and not faction["voted"]:
                action, params = "vote", {"choice": "yes"}
            else:
                continue
            applied = request(f"/game/{gid}/act", {"token": tokens[i], "action": action, "params": params})
            assert applied["ok"], applied
            if action == "vote":
                voted += 1
        if time.monotonic() >= next_report:
            print(f"SMOKE: round {state['round']}, phase {state['phase']}", flush=True)
            next_report = time.monotonic() + 15
        time.sleep(0.04)
    else:
        raise AssertionError("full game did not finish within 90 seconds")
    assert restarted and bought_insight and voted == 36
    rent = sum(row["license_rent"] for row in result["payout_breakdown"])
    assert sum(result["payouts"]) + result["rake"] == result["bank"] + rent
    assert sum(row["factory_bonus"] for row in result["payout_breakdown"]) == result["bank"] // 20
    assert len(result["phases"]) == 19
    assert len(request("/export")) == 1
    print(f"SMOKE: six rounds settled; bank={result['bank']}, license_rent={rent}, conservation verified", flush=True)
    stop()
    start()
    new = request("/game/new", {"lobby_duration": 60})
    assert new["game_id"] > gid and new["state"]["party_no"] > party
    assert request(f"/game/{gid}/state")["result"]["party_no"] == party
    print("SMOKE: restart after settlement preserved history and allocated fresh game_id and party_no", flush=True)
    print("PASS: release binary completed restart smoke without touching live state", flush=True)
finally:
    stop()
