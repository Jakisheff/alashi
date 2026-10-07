"""Offline private harness guard and response checks; no model or live arena."""
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
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
                return live if states <= 2 else finished
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
        self.assertTrue(outcome['e2e_verified'])
        self.assertEqual([c[1] for c in calls], ['start', 'start', 'state', 'state', 'act', 'act', 'state'])


if __name__ == '__main__':
    unittest.main()
