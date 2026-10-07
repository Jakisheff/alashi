"""Offline private harness guard and response checks; no model or live arena."""
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import subprocess
from pathlib import Path
import tempfile
import threading
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import private_match as match


class Checks(unittest.TestCase):
    def test_profile_and_private_server_match_or_fail_closed(self):
        with tempfile.TemporaryDirectory() as base:
            home = Path(base) / 'agent'
            home.mkdir(mode=0o700)
            recovery = 'ab' * 32
            record = 'cd' * 32
            identity = {'schema': 'alashi.bootstrap.v1', 'agent_record_id': record,
                        'recovery_secret': recovery, 'wallet': 'public-wallet', 'signature': 'public-signature',
                        'registration': {'mode': 'agent_lifecycle_v2', 'network': 'devnet',
                                         'wallet': 'public-wallet', 'signature': 'public-signature',
                                         'commitment': 'confirmed'}}
            path = home / 'agent.json'
            path.write_text(json.dumps(identity))
            path.chmod(0o600)
            local = match.profile(home)
            self.assertEqual(local['agent_record_id'], record)
            self.assertEqual(local['character_id'], match.frame_hash('alashi-character-v2',
                [match.frame_hash('alashi-owner-v1', [recovery]), record]))
            class Handler(BaseHTTPRequestHandler):
                character = local['character_id']
                def do_GET(self):
                    self.send_response(200)
                    self.end_headers()
                    self.wfile.write(json.dumps({'ok': True, 'registered': True,
                        'agent_record_id': record, 'character_id': self.character}).encode())
                def log_message(self, *args):
                    pass
            server = HTTPServer(('127.0.0.1', 0), Handler)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                url = f'http://127.0.0.1:{server.server_port}'
                with patch.object(match, 'loopback', return_value=('127.0.0.1', server.server_port)):
                    match.server_profile(url, local)
                    Handler.character = 'wrong-character'
                    with self.assertRaises(match.Stop):
                        match.server_profile(url, local)
            finally:
                server.shutdown()
                thread.join()
                server.server_close()
            identity['registration'] = None
            path.write_text(json.dumps(identity))
            with self.assertRaises(match.Stop):
                match.profile(home)
            path.chmod(0o644)
            with self.assertRaises(match.Stop):
                match.profile(home)
            with self.assertRaises(match.Stop):
                match.profile(home / 'missing')
            with self.assertRaises(match.Stop):
                match.loopback('https://alashi.network')
            with self.assertRaises(match.Stop):
                match.loopback('http://127.0.0.1:8095')

    def test_numeric_view_choice_and_terminal(self):
        faction = {'cash': 2_000_000, 'goods': 1, 'influence': 1, 'vote_weight': 1,
                   'acted': False, 'voted': False, 'is_president': False, 'alive': True,
                   'name': 'untrusted name', 'token': 'private'}
        state = {'game_id': 3, 'execution_mode': 'http_simulated', 'phase': 'market',
                 'epoch': 'classic', 'round': 1, 'now': 1, 'grace_until': 100,
                 'price_now': 1, 'factions': [faction, faction], 'message': 'private'}
        safe = match.view({'ok': True, 'game_id': 3, 'your_faction_idx': 0, 'state': state}, 3)
        self.assertNotIn('untrusted name', json.dumps(safe))
        self.assertNotIn('private', json.dumps(safe))
        options = match.candidates(safe)
        self.assertEqual(match.choice('{"choice":0}', options),
                         {'action': 'sell', 'params': {'units': 1}, 'by': 'llm'})
        with self.assertRaises(match.Stop):
            match.choice('{"choice":-1}', options)
        with self.assertRaises(match.Stop):
            match.choice('{"choice":false}', options)
        ended = match.terminal({'ok': True, 'finished': True, 'game_id': 3,
                                'result': {'game_id': 3, 'party_no': 19, 'final_cash': [1, 2]}},
                               3, [{'agent_record_id': 'a'}, {'agent_record_id': 'b'}])
        self.assertEqual(ended['status'], 'finished')
        self.assertEqual(ended['final_cash'], [1, 2])
        with self.assertRaises(match.Stop):
            match.terminal({'ok': True, 'finished': True, 'game_id': 3,
                            'result': {'game_id': 4, 'party_no': 19, 'final_cash': [1, 2]}}, 3, [])

    def test_model_timeout_is_bounded_by_live_phase(self):
        import time
        deadline = time.monotonic() + 1800
        self.assertEqual(match.decision_timeout({'now': 100, 'grace_until': 130}, deadline), 20)
        self.assertEqual(match.decision_timeout({'now': 128, 'grace_until': 130}, deadline), 0)

    def test_provider_failures_keep_only_safe_diagnostics(self):
        cases = [
            (subprocess.TimeoutExpired(['provider'], 7, output='secret-output'), 'timeout', None),
            (OSError('secret-path'), 'os_error', None),
            (subprocess.CompletedProcess(['provider'], 17, stdout='secret-output'), 'nonzero_exit', 17),
        ]
        for outcome, failure, exit_code in cases:
            with self.subTest(failure=failure), patch.object(match.subprocess, 'run',
                    side_effect=outcome if isinstance(outcome, BaseException) else None,
                    return_value=outcome if not isinstance(outcome, BaseException) else None):
                with self.assertRaises(match.ProviderFailure) as caught:
                    match.provider_run('test-model', ['provider'], 7)
            diagnostic = caught.exception.diagnostic
            self.assertEqual(diagnostic['failure'], failure)
            self.assertEqual(diagnostic['model'], 'test-model')
            self.assertEqual(diagnostic['timeout_s'], 7)
            self.assertEqual(diagnostic['exit_code'], exit_code)
            self.assertGreaterEqual(diagnostic['elapsed_ms'], 0)
            self.assertNotIn('secret', str(caught.exception) + json.dumps(diagnostic))

    def test_oversized_codex_answer_is_classified_without_content(self):
        def oversized(command, **kwargs):
            Path(command[command.index('-o') + 1]).write_text('private-content' * 400)
            return subprocess.CompletedProcess(command, 0)
        with patch.object(match.subprocess, 'run', side_effect=oversized):
            with self.assertRaises(match.ProviderFailure) as caught:
                match.model_decision('codex', {'phase': 'market'}, [{'action': 'sell'}], 8)
        self.assertEqual(caught.exception.diagnostic['failure'], 'oversized_output')
        self.assertEqual(caught.exception.diagnostic['model'], 'gpt-6-sol')
        self.assertNotIn('private-content', str(caught.exception) + json.dumps(caught.exception.diagnostic))

    def test_opencode_config_denies_all_tools_for_selected_agent(self):
        with tempfile.TemporaryDirectory() as d:
            config = match.write_opencode_config(Path(d))
            self.assertEqual(config['permission'], {'*': 'deny'})
            self.assertEqual(config['agent']['alashi-choice']['permission'], {'*': 'deny'})
            self.assertEqual(json.loads((Path(d) / 'opencode.json').read_text()), config)

    def test_opencode_text_requires_no_tools_and_strict_choice(self):
        text = json.dumps({'type': 'text', 'part': {'type': 'text', 'text': '{"choice":1}'}})
        self.assertEqual(match.opencode_text(text), '{"choice":1}')
        with self.assertRaises(match.Stop):
            match.opencode_text(json.dumps({'type': 'tool', 'part': {'type': 'tool'}}))
        with self.assertRaises(match.Stop):
            match.opencode_text(json.dumps({'type': 'step_finish'}))


    def test_orchestrator_uses_two_existing_profiles_then_terminal(self):
        faction = {'cash': 2_000_000, 'goods': 1, 'influence': 1, 'vote_weight': 1,
                   'acted': False, 'voted': False, 'is_president': False, 'alive': True}
        live = {'ok': True, 'game_id': 3, 'your_faction_idx': 0,
                'state': {'game_id': 3, 'execution_mode': 'http_simulated', 'phase': 'market',
                          'epoch': 'classic', 'round': 1, 'now': 1, 'grace_until': 100,
                          'price_now': 1, 'factions': [faction, faction]}}
        finished = {'ok': True, 'game_id': 3, 'finished': True,
                    'result': {'game_id': 3, 'party_no': 19, 'final_cash': [1, 2]}}
        calls = []
        states = 0
        def fake_profile(home):
            return {'agent_record_id': 'a' if str(home).endswith('codex') else 'b'}
        def fake_node(home, url, command, *options):
            nonlocal states
            calls.append((str(home), command, options))
            if command == 'start':
                self.assertIn('--existing-only', options)
                return {'ok': True, 'status': 'joined', 'game_id': 3,
                        'faction_idx': 0 if str(home).endswith('codex') else 1}
            if command == 'state':
                states += 1
                return live if states <= 4 else finished
            if command == 'act':
                return {'ok': True, 'op_id': 1, 'op_consumed': True}
            self.fail('unexpected command')
        args = SimpleNamespace(url='http://127.0.0.1:18094',
                               codex_home='/private/codex', opencode_home='/private/opencode')
        with patch.object(match, 'profile', side_effect=fake_profile), \
             patch.object(match, 'server_profile'), \
             patch.object(match, 'session', return_value={'schema': 'alashi.game.v2', 'game_id': 3,
                                                         'pending_act': None}), \
             patch.object(match, 'node', side_effect=fake_node), \
             patch.object(match, 'model_decision', return_value={'action': 'sell',
                                                                 'params': {'units': 1}, 'by': 'llm'}):
            outcome = match.run(args)
        self.assertEqual(outcome['status'], 'finished')
        self.assertEqual(outcome['model_decisions'], [1, 1])
        self.assertEqual(outcome['accepted_actions'], [1, 1])
        self.assertTrue(outcome['both_models_acted'])
        self.assertEqual([c[1] for c in calls], ['start', 'start', 'state', 'state',
                                                 'state', 'act', 'state', 'act', 'state'])


    def test_consumed_rejected_retry_clears_pending_and_continues(self):
        seen = []
        pending_checks = 0
        def fake_session(home, game):
            nonlocal pending_checks
            if str(home).endswith('codex'):
                pending_checks += 1
                return {'schema': 'alashi.game.v2', 'game_id': game,
                        'pending_act': {'op_id': 1} if pending_checks == 1 else None}
            return {'schema': 'alashi.game.v2', 'game_id': game, 'pending_act': None}
        def fake_node(home, url, command, *options):
            seen.append(command)
            if command == 'start':
                return {'ok': True, 'status': 'joined', 'game_id': 3,
                        'faction_idx': 0 if str(home).endswith('codex') else 1}
            if command == 'retry':
                return {'ok': False, 'error': 'WrongPhase', 'op_id': 1, 'op_consumed': True}
            if command == 'state':
                return {'ok': True, 'game_id': 3, 'finished': True,
                        'result': {'game_id': 3, 'party_no': 19, 'final_cash': [1, 2]}}
            self.fail('unexpected command')
        args = SimpleNamespace(url='http://127.0.0.1:18094',
                               codex_home='/private/codex', opencode_home='/private/opencode')
        with patch.object(match, 'profile', side_effect=lambda home: {'agent_record_id': str(home)}), \
             patch.object(match, 'server_profile'), \
             patch.object(match, 'session', side_effect=fake_session), \
             patch.object(match, 'node', side_effect=fake_node):
            outcome = match.run(args)
        self.assertEqual(outcome['status'], 'finished')
        self.assertIn('retry', seen)

    def test_consumed_rejection_does_not_ask_model_twice_in_same_phase(self):
        faction = {'cash': 2_000_000, 'goods': 1, 'influence': 1, 'vote_weight': 1,
                   'acted': False, 'voted': False, 'is_president': False, 'alive': True}
        state_calls = 0
        model_calls = []
        def fake_node(home, url, command, *options):
            nonlocal state_calls
            idx = 0 if str(home).endswith('codex') else 1
            if command == 'start':
                return {'ok': True, 'status': 'joined', 'game_id': 3, 'faction_idx': idx}
            if command == 'act':
                return {'ok': idx == 1, 'op_id': 1, 'op_consumed': True}
            if command == 'state':
                state_calls += 1
                if state_calls > 6:
                    return {'ok': True, 'game_id': 3, 'finished': True,
                            'result': {'game_id': 3, 'party_no': 19, 'final_cash': [1, 2]}}
                factions = [dict(faction), dict(faction)]
                if state_calls > 4:
                    factions[1]['acted'] = True
                return {'ok': True, 'game_id': 3, 'your_faction_idx': idx,
                        'state': {'game_id': 3, 'execution_mode': 'http_simulated', 'phase': 'market',
                                  'epoch': 'classic', 'round': 1, 'now': 1, 'grace_until': 100,
                                  'price_now': 1, 'factions': factions}}
            self.fail('unexpected command')
        def choose(kind, safe, actions, deadline):
            model_calls.append(kind)
            return {'action': 'sell', 'params': {'units': 1}, 'by': 'llm'}
        args = SimpleNamespace(url='http://127.0.0.1:18094',
                               codex_home='/private/codex', opencode_home='/private/opencode')
        with patch.object(match, 'profile', side_effect=lambda home: {'agent_record_id': str(home)}), \
             patch.object(match, 'server_profile'), \
             patch.object(match, 'session', return_value={'schema': 'alashi.game.v2', 'game_id': 3,
                                                         'pending_act': None}), \
             patch.object(match, 'node', side_effect=fake_node), \
             patch.object(match, 'model_decision', side_effect=choose), \
             patch.object(match.time, 'sleep'):
            match.run(args)
        self.assertEqual(model_calls, ['codex', 'opencode'])

    def test_one_provider_timeout_skips_phase_but_other_and_later_phase_continue(self):
        faction = {'cash': 2_000_000, 'goods': 1, 'influence': 1, 'vote_weight': 1,
                   'acted': False, 'voted': False, 'is_president': False, 'alive': True}
        state_calls = 0
        calls = []
        acts = []
        def fake_node(home, url, command, *options):
            nonlocal state_calls
            idx = 0 if str(home).endswith('codex') else 1
            if command == 'start':
                return {'ok': True, 'status': 'joined', 'game_id': 3, 'faction_idx': idx}
            if command == 'act':
                acts.append((idx, json.loads(options[-1])['action']))
                return {'ok': True, 'op_id': len(acts), 'op_consumed': True}
            if command == 'state':
                state_calls += 1
                if state_calls > 9:
                    return {'ok': True, 'game_id': 3, 'finished': True,
                            'result': {'game_id': 3, 'party_no': 19, 'final_cash': [1, 2]}}
                phase = 'market' if state_calls <= 5 else 'action'
                factions = [dict(faction), dict(faction)]
                if phase == 'market' and state_calls >= 4:
                    factions[0]['acted'] = True
                return {'ok': True, 'game_id': 3, 'your_faction_idx': idx,
                        'state': {'game_id': 3, 'execution_mode': 'http_simulated', 'phase': phase,
                                  'epoch': 'classic', 'round': 1, 'now': 1, 'grace_until': 100,
                                  'price_now': 1, 'factions': factions}}
            self.fail('unexpected command')
        def choose(kind, safe, actions, timeout_s):
            calls.append((kind, safe['phase']))
            if kind == 'opencode' and safe['phase'] == 'market':
                raise match.ProviderFailure('zai-coding-plan/glm-5.3-flash', 'timeout', 20000, 20)
            return {**actions[0], 'by': 'llm'}
        args = SimpleNamespace(url='http://127.0.0.1:18094',
                               codex_home='/private/codex', opencode_home='/private/opencode')
        with patch.object(match, 'profile', side_effect=lambda home: {'agent_record_id': str(home)}), \
             patch.object(match, 'server_profile'), \
             patch.object(match, 'session', return_value={'schema': 'alashi.game.v2', 'game_id': 3,
                                                         'pending_act': None}), \
             patch.object(match, 'node', side_effect=fake_node), \
             patch.object(match, 'model_decision', side_effect=choose), \
             patch.object(match.time, 'sleep'):
            outcome = match.run(args)
        self.assertEqual(calls, [('codex', 'market'), ('opencode', 'market'),
                                 ('codex', 'action'), ('opencode', 'action')])
        self.assertEqual(acts, [(0, 'sell'), (0, 'produce'), (1, 'produce')])
        self.assertEqual(outcome['model_decisions'], [2, 2])
        self.assertEqual(outcome['accepted_actions'], [2, 1])
        self.assertEqual(outcome['model_timeouts'], [0, 1])

    def test_stale_model_choice_is_not_submitted_after_phase_change(self):
        faction = {'cash': 2_000_000, 'goods': 1, 'influence': 1, 'vote_weight': 1,
                   'acted': False, 'voted': False, 'is_president': False, 'alive': True}
        state_calls = 0
        acts = []
        def fake_node(home, url, command, *options):
            nonlocal state_calls
            idx = 0 if str(home).endswith('codex') else 1
            if command == 'start':
                return {'ok': True, 'status': 'joined', 'game_id': 3, 'faction_idx': idx}
            if command == 'act':
                acts.append(idx)
                return {'ok': True, 'op_id': 1, 'op_consumed': True}
            if command == 'state':
                state_calls += 1
                if state_calls > 4:
                    return {'ok': True, 'game_id': 3, 'finished': True,
                            'result': {'game_id': 3, 'party_no': 19, 'final_cash': [1, 2]}}
                phase = 'market' if state_calls <= 2 else 'lobby'
                return {'ok': True, 'game_id': 3, 'your_faction_idx': idx,
                        'state': {'game_id': 3, 'execution_mode': 'http_simulated', 'phase': phase,
                                  'epoch': 'classic', 'round': 1, 'now': 1, 'grace_until': 100,
                                  'price_now': 1, 'factions': [faction, faction]}}
            self.fail('unexpected command')
        args = SimpleNamespace(url='http://127.0.0.1:18094',
                               codex_home='/private/codex', opencode_home='/private/opencode')
        with patch.object(match, 'profile', side_effect=lambda home: {'agent_record_id': str(home)}), \
             patch.object(match, 'server_profile'), \
             patch.object(match, 'session', return_value={'schema': 'alashi.game.v2', 'game_id': 3,
                                                         'pending_act': None}), \
             patch.object(match, 'node', side_effect=fake_node), \
             patch.object(match, 'model_decision', return_value={'action': 'sell',
                                                                 'params': {'units': 1}, 'by': 'llm'}), \
             patch.object(match.time, 'sleep'):
            match.run(args)
        self.assertEqual(acts, [])


if __name__ == '__main__':
    unittest.main()
