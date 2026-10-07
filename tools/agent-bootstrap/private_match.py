#!/usr/bin/env python3
"""Private, bounded two-harness match using saved Node bootstrap identities."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit

HERE = Path(__file__).resolve().parent
BOOTSTRAP = HERE / 'alashi.mjs'
HEX = re.compile(r'[0-9a-f]{64}\Z')
MAX_OUTPUT = 1_000_000
LAWS = {'status_quo', 'tax_10', 'tax_20', 'subsidy_produce', 'subsidy_poor',
        'subsidy_rich', 'embargo', 'boom', 'vzaimozachet', 'no_law'}


class Stop(Exception):
    pass


def frame_hash(prefix, parts):
    h = hashlib.sha256(prefix.encode())
    for part in parts:
        raw = part.encode()
        h.update(len(raw).to_bytes(8, 'little'))
        h.update(raw)
    return h.hexdigest()


def private_file(path):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o077 or info.st_size > MAX_OUTPUT:
        raise Stop('private file permissions or size invalid')
    return json.loads(path.read_text())


def profile(home):
    try:
        info = home.lstat()
    except FileNotFoundError:
        raise Stop('saved private profile missing') from None
    if not stat.S_ISDIR(info.st_mode) or info.st_mode & 0o077:
        raise Stop('private directory must be mode 700')
    value = private_file(home / 'agent.json')  # Fail before Node can create a wallet.
    receipt = value.get('registration') or {}
    if value.get('schema') != 'alashi.bootstrap.v1' or not HEX.fullmatch(value.get('agent_record_id', '')) \
            or not HEX.fullmatch(value.get('recovery_secret', '')) \
            or receipt.get('mode') != 'agent_lifecycle_v2' or receipt.get('network') != 'devnet' \
            or receipt.get('wallet') != value.get('wallet') or receipt.get('signature') != value.get('signature') \
            or receipt.get('commitment') != 'confirmed':
        raise Stop('existing confirmed devnet registration required')
    owner = frame_hash('alashi-owner-v1', [value['recovery_secret']])
    character = frame_hash('alashi-character-v2', [owner, value['agent_record_id']])
    return {'agent_record_id': value['agent_record_id'], 'character_id': character,
            'wallet': value['wallet'], 'signature': value['signature']}


def loopback(url):
    parsed = urlsplit(url)
    if parsed.scheme != 'http' or parsed.hostname != '127.0.0.1' or parsed.port != 18094 \
            or parsed.path not in ('', '/') or parsed.query or parsed.fragment \
            or parsed.username or parsed.password:
        raise Stop('private staging tunnel http://127.0.0.1:18094 required')
    return parsed.hostname, parsed.port


def server_profile(url, identity):
    host, port = loopback(url)
    conn = http.client.HTTPConnection(host, port, timeout=8)
    try:
        conn.request('GET', '/agents/' + identity['agent_record_id'])
        response = conn.getresponse()
        raw = response.read(MAX_OUTPUT + 1)
        if response.status != 200 or len(raw) > MAX_OUTPUT:
            raise Stop('private server profile unavailable')
        value = json.loads(raw)
    finally:
        conn.close()
    if value.get('ok') is not True or value.get('registered') is not True \
            or value.get('agent_record_id') != identity['agent_record_id'] \
            or value.get('character_id') != identity['character_id']:
        raise Stop('saved identity does not match staging server')


def node(home, url, command, *options):
    args = ['node', str(BOOTSTRAP), command, *options, '--url', url]
    env = {**os.environ, 'ALASHI_AGENT_HOME': str(home)}
    try:
        result = subprocess.run(args, env=env, capture_output=True, text=True, timeout=25)
        if len(result.stdout) > MAX_OUTPUT:
            raise Stop('Node response too large')
        value = json.loads(result.stdout)
    except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError):
        raise Stop('Node bootstrap response unavailable') from None
    if not isinstance(value, dict) or type(value.get('ok')) is not bool:
        raise Stop('invalid Node bootstrap response')
    return value


def session(home, game):
    value = private_file(home / f'game-{game}.json')
    if value.get('schema') != 'alashi.game.v2' or value.get('game_id') != game:
        raise Stop('wrong private game session')
    return value


def numeric(value):
    return type(value) is int and 0 <= value <= 10**15


def view(reply, game):
    if reply.get('ok') is not True or reply.get('game_id') != game:
        raise Stop('untrusted game response')
    state = reply.get('state')
    idx = reply.get('your_faction_idx')
    if not isinstance(state, dict) or state.get('game_id') != game \
            or state.get('execution_mode') != 'http_simulated' or not numeric(idx):
        raise Stop('invalid public state')
    phase = state.get('phase')
    epoch = state.get('epoch')
    factions = state.get('factions')
    if phase not in ('lobby', 'market', 'action', 'law', 'finished', 'aborted') \
            or epoch not in ('classic', '90s') or not isinstance(factions, list) \
            or not 0 <= idx < len(factions) <= 6:
        raise Stop('invalid phase or faction')
    safe = {'phase': phase, 'epoch': epoch, 'own_idx': idx, 'factions': []}
    for key in ('round', 'now', 'grace_until', 'price_now', 'yes_influence', 'no_influence'):
        item = state.get(key)
        if item is not None and not numeric(item):
            raise Stop('invalid game number')
        safe[key] = item
    law = state.get('law_card_name')
    safe['law_card_name'] = law if law in LAWS else None
    for faction in factions:
        if not isinstance(faction, dict):
            raise Stop('invalid faction')
        row = {}
        for key in ('cash', 'goods', 'influence', 'vote_weight'):
            if not numeric(faction.get(key)):
                raise Stop('invalid faction number')
            row[key] = faction[key]
        for key in ('acted', 'voted', 'is_president', 'alive'):
            if type(faction.get(key)) is not bool:
                raise Stop('invalid faction flag')
            row[key] = faction[key]
        safe['factions'].append(row)
    return safe


def candidates(safe):
    own = safe['factions'][safe['own_idx']]
    if not own['alive']:
        return []
    phase = safe['phase']
    if phase == 'market' and not own['acted']:
        actions = [{'action': 'sell', 'params': {'units': n}} for n in range(1, min(own['goods'], 3) + 1)]
        price = safe['price_now']
        if numeric(price) and price > 0:
            actions += [{'action': 'buy', 'params': {'units': n}}
                        for n in range(1, min(3, own['cash'] // (price * 1_000_000)) + 1)]
        return actions
    if phase == 'action' and not own['acted']:
        actions = [{'action': 'produce'}]
        if safe['epoch'] == '90s':
            actions.append({'action': 'shuttle'})
        if own['cash'] >= 1_000_000:
            actions.append({'action': 'donkey'})
        return actions
    if phase == 'law' and not own['voted']:
        actions = [{'action': 'vote', 'params': {'choice': x}} for x in ('yes', 'no', 'abstain')]
        if own['is_president']:
            actions.append({'action': 'veto'})
        return actions
    return []


def choice(raw, actions):
    value = json.loads(raw)
    if not isinstance(value, dict) or set(value) != {'choice'} or type(value['choice']) is not int \
            or not 0 <= value['choice'] < len(actions):
        raise Stop('model choice invalid')
    return {**actions[value['choice']], 'by': 'llm'}


def opencode_text(output):
    texts = []
    for line in output.splitlines():
        event = json.loads(line)
        part = event.get('part') or {}
        if part.get('type') in ('tool', 'tool-call') or event.get('type') in ('tool', 'tool-call'):
            raise Stop('OpenCode attempted a tool')
        if part.get('type') == 'text' and isinstance(part.get('text'), str):
            texts.append(part['text'])
    if not texts:
        raise Stop('OpenCode returned no text')
    return texts[-1].strip()


def decision_timeout(safe, deadline):
    if safe['now'] is None or safe['grace_until'] is None:
        return 0
    return max(0, min(20, safe['grace_until'] - safe['now'] - 2,
                      deadline - time.monotonic()))


def write_opencode_config(root):
    config = {'permission': {'*': 'deny'}, 'agent': {'alashi-choice': {
        'mode': 'primary', 'model': 'zai-coding-plan/glm-5.3-flash',
        'permission': {'*': 'deny'}, 'prompt': 'Return only the requested JSON choice; do not use tools.'}}}
    (root / 'opencode.json').write_text(json.dumps(config))
    return config


def model_decision(kind, safe, actions, timeout_s):
    prompt = ('Choose one strategically best action for your Alashi faction. Game money is simulated. '
              'Return only JSON {"choice":integer}, the zero-based index into candidates. '
              'No tools, files, shell, web, wallet, or other commands. '
              + json.dumps({'state': safe, 'candidates': actions}, separators=(',', ':')))
    with tempfile.TemporaryDirectory(prefix='alashi-model-') as temporary:
        root = Path(temporary)
        timeout = max(1, min(20, int(timeout_s)))
        if kind == 'codex':
            schema = root / 'schema.json'
            schema.write_text(json.dumps({'type': 'object', 'additionalProperties': False,
                                          'required': ['choice'], 'properties': {'choice': {'type': 'integer'}}}))
            answer = root / 'answer.json'
            command = ['codex', 'exec', '--ignore-user-config', '--sandbox', 'read-only',
                       '--disable', 'shell_tool', '--disable', 'apps', '--disable', 'multi_agent',
                       '--disable', 'browser_use', '--disable', 'computer_use',
                       '--disable', 'image_generation', '--disable', 'in_app_browser',
                       '-c', 'web_search="disabled"', '--model', 'gpt-6-sol',
                       '--skip-git-repo-check', '--ephemeral', '-C', temporary,
                       '--output-schema', str(schema), '-o', str(answer), '-']
            try:
                result = subprocess.run(command, input=prompt, text=True, stdout=subprocess.DEVNULL,
                                        stderr=subprocess.DEVNULL, timeout=timeout)
            except (OSError, subprocess.TimeoutExpired):
                raise Stop('Codex decision unavailable') from None
            if result.returncode or not answer.is_file() or answer.stat().st_size > 4096:
                raise Stop('Codex decision unavailable')
            raw = answer.read_text().strip()
        else:
            write_opencode_config(root)
            try:
                resolved = subprocess.run(['opencode', 'debug', 'config', '--pure'], cwd=temporary,
                                          text=True, capture_output=True, timeout=15)
                effective = json.loads(resolved.stdout)
            except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError):
                raise Stop('OpenCode permission check unavailable') from None
            agent = (effective.get('agent') or {}).get('alashi-choice') or {}
            if resolved.returncode or effective.get('permission') != {'*': 'deny'} \
                    or agent.get('permission') != {'*': 'deny'} \
                    or agent.get('model') != 'zai-coding-plan/glm-5.3-flash':
                raise Stop('OpenCode tools are not denied')
            command = ['opencode', 'run', '--pure', '--agent', 'alashi-choice',
                       '--model', 'zai-coding-plan/glm-5.3-flash', '--format', 'json',
                       '--dir', temporary, prompt]
            try:
                result = subprocess.run(command, cwd=temporary, text=True,
                                        capture_output=True, timeout=timeout)
            except (OSError, subprocess.TimeoutExpired):
                raise Stop('OpenCode decision unavailable') from None
            if result.returncode or len(result.stdout) > MAX_OUTPUT:
                raise Stop('OpenCode decision unavailable')
            raw = opencode_text(result.stdout)
    return choice(raw, actions)


def terminal(reply, game, identities):
    if reply.get('finished') is not True:
        return None
    result = reply.get('result')
    if reply.get('ok') is not True or reply.get('game_id') != game or not isinstance(result, dict) \
            or result.get('game_id') != game or not numeric(result.get('party_no')):
        raise Stop('invalid terminal response')
    cash = result.get('final_cash')
    if not isinstance(cash, list) or not all(numeric(v) for v in cash):
        raise Stop('invalid terminal cash')
    return {'ok': True, 'status': 'finished', 'game_id': game, 'party_no': result['party_no'],
            'final_cash': cash, 'agents': [i['agent_record_id'] for i in identities]}


def run(args):
    loopback(args.url)
    homes = [Path(args.codex_home), Path(args.opencode_home)]
    identities = [profile(home) for home in homes]
    if identities[0]['agent_record_id'] == identities[1]['agent_record_id']:
        raise Stop('two distinct saved identities required')
    for identity in identities:
        server_profile(args.url, identity)
    names = ['NodeProbe', 'NodePeer']
    models = ['gpt-6-sol', 'zai-coding-plan/glm-5.3-flash']
    joined = []
    for home, name, model in zip(homes, names, models):
        reply = node(home, args.url, 'start', '--name', name, '--model', model, '--existing-only', 'true')
        if reply.get('ok') is not True or reply.get('status') != 'joined':
            raise Stop('managed match did not join both saved identities')
        joined.append(reply)
    game = joined[0].get('game_id')
    if not numeric(game) or game == 0 or joined[1].get('game_id') != game:
        raise Stop('agents were assigned different games')
    if joined[0].get('faction_idx') == joined[1].get('faction_idx') \
            or not all(numeric(x.get('faction_idx')) for x in joined):
        raise Stop('agents were assigned the same or invalid faction')
    deadline = time.monotonic() + 1800
    used = [0, 0]
    accepted = [0, 0]
    attempted = set()
    while time.monotonic() < deadline:
        replies = []
        for i, (home, name, model) in enumerate(zip(homes, names, models)):
            retry_rejected = False
            private = session(home, game)
            if private.get('pending_act') is not None:
                retry = node(home, args.url, 'retry', '--game', str(game))
                error = retry.get('error')
                code = error.get('code') if isinstance(error, dict) else error
                if code == 'bad_token':
                    refresh = node(home, args.url, 'start', '--name', name, '--model', model, '--game', str(game), '--existing-only', 'true')
                    if refresh.get('ok') is not True:
                        raise Stop('token recovery failed')
                    retry = node(home, args.url, 'retry', '--game', str(game))
                if retry.get('op_consumed') is not True \
                        or session(home, game).get('pending_act') is not None:
                    raise Stop('pending operation unresolved')
                retry_rejected = retry.get('ok') is False
            reply = node(home, args.url, 'state', '--game', str(game))
            if reply.get('ok') is not True:
                raise Stop('game state unavailable')
            ended = terminal(reply, game, identities)
            if ended:
                ended['model_decisions'] = used
                ended['accepted_actions'] = accepted
                ended['e2e_verified'] = all(count > 0 for count in accepted)
                return ended
            if retry_rejected:
                state = view(reply, game)
                attempted.add((i, state['round'], state['phase']))
            replies.append(reply)
        options = []
        for reply in replies:
            safe = view(reply, game)
            actions = candidates(safe)
            if safe['grace_until'] is not None and safe['now'] is not None \
                    and safe['now'] >= safe['grace_until']:
                actions = []
            options.append((safe, actions))
        actionable = [i for i, (safe, actions) in enumerate(options)
                      if actions and (i, safe['round'], safe['phase']) not in attempted
                      and decision_timeout(safe, deadline) >= 5]
        if actionable:
            if any(used[i] >= 20 for i in actionable):
                raise Stop('per-model decision cap reached')
            for i in actionable:
                safe = options[i][0]
                attempted.add((i, safe['round'], safe['phase']))
            with ThreadPoolExecutor(max_workers=2) as pool:
                future = {i: pool.submit(model_decision, ['codex', 'opencode'][i],
                                         *options[i], decision_timeout(options[i][0], deadline))
                          for i in actionable}
                decisions = {i: task.result() for i, task in future.items()}
            for i, decision in decisions.items():
                used[i] += 1
                fresh = node(homes[i], args.url, 'state', '--game', str(game))
                if fresh.get('finished') is True:
                    continue
                current = view(fresh, game)
                previous = options[i][0]
                if current['round'] != previous['round'] or current['phase'] != previous['phase'] \
                        or current['own_idx'] != previous['own_idx'] \
                        or decision_timeout(current, deadline) < 2 \
                        or {key: value for key, value in decision.items() if key != 'by'} not in candidates(current):
                    print(json.dumps({'status': 'stale_choice', 'game_id': game, 'player': i,
                                      'decision_count': used[i]}), flush=True)
                    continue
                reply = node(homes[i], args.url, 'act', '--game', str(game),
                             '--json', json.dumps(decision, separators=(',', ':')))
                if reply.get('op_consumed') is not True:
                    raise Stop('action outcome unresolved; saved operation requires review')
                if reply.get('ok') is True:
                    accepted[i] += 1
                print(json.dumps({'status': 'action', 'game_id': game, 'player': i,
                                  'accepted': reply.get('ok') is True, 'decision_count': used[i]}), flush=True)
        else:
            time.sleep(2)
    raise Stop('30-minute match deadline reached')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', required=True)
    parser.add_argument('--codex-home', required=True)
    parser.add_argument('--opencode-home', required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(run(args)))
    except (Stop, OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(json.dumps({'ok': False, 'status': 'stopped', 'reason': str(error)
                          if isinstance(error, Stop) else 'invalid private input'}))
        sys.exit(1)


if __name__ == '__main__':
    main()
