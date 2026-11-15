import json, subprocess, sys

proc = subprocess.Popen(
    ['bin/evalgame'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
    stderr=subprocess.PIPE, text=True, bufsize=1,
)
proc.stdin.write(json.dumps({'seed': 7, 'seat': 0, 'opponents': ['greedy', 'tactical'],
                             'builtin': None, 'epoch': 1, 'vote_weight_mode': 1}) + '\n')
proc.stdin.flush()

scripted = {
    0: {'action': 'sell', 'units': 99},                     # wellformed, illegal amount
    1: {'action': 'produce'},
    2: {'action': 'bribe', 'to': 1, 'amount': 1234567},     # not a multiple of 5M
    3: {'action': 'vote_yes'},
}
count, decisions, result = 0, [], None
for _ in range(500):
    line = proc.stdout.readline()
    if not line:
        break
    msg = json.loads(line)
    if msg['type'] == 'request':
        reply = scripted.get(count, {'action': 'pass'})
        count += 1
        proc.stdin.write(json.dumps(reply) + '\n')
        proc.stdin.flush()
    elif msg['type'] == 'decision':
        decisions.append((msg['phase'], msg['decision'], msg['parse_error']))
    elif msg['type'] == 'result':
        result = msg['game']
        break
try:
    proc.stdin.close()
except Exception:
    pass
try:
    proc.wait(timeout=5)
except Exception:
    proc.kill()
stderr = proc.stderr.read()

print('turns replied:', count, 'decisions:', len(decisions), 'result:', bool(result))
for d in decisions[:6]:
    print(' decision:', d)
if result:
    own = [a for ph in result['phases'] for a in ph['actions'] if a['actor'] == 0]
    print('own actions:', len(own), 'ok count:', sum(a['ok'] for a in own))
    bad = [a for a in own if not a['ok']][:4]
    for a in bad:
        print(' not-ok:', json.dumps(a, ensure_ascii=False)[:160])
    print('payouts:', result['payouts'])
print('stderr tail:', stderr[-300:])
