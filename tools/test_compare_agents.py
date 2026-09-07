import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import compare_agents as ev
import pilot_budget

BINARY = ev.ROOT / 'arena/target/debug/evalgame'


class EvaluationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.config = ev.load_config(ev.ROOT/'data/eval/configs/builtin.json')
        self.config['seeds'] = [4242, 7777]
        self.config['candidate'] = dict(self.config['baseline'])

    def game(self, side='baseline', seed=4242, seat=0):
        folder = self.root / f'{side}-{seed}-{seat}-{len(list(self.root.iterdir()))}'
        folder.mkdir()
        return ev.run_game(BINARY, self.config, side, seed, seat, folder)

    def test_identical_versions_and_repeated_runs_are_identical(self):
        runs = []
        for seed in self.config['seeds']:
            for seat in range(3):
                a = self.game('baseline', seed, seat)
                b = self.game('candidate', seed, seat)
                self.assertEqual(a['status'], 'completed')
                self.assertEqual(a['game'], b['game'])
                self.assertEqual(a['game']['strategies'][seat], 'subject')
                runs.extend([a, b])
        self.assertEqual(runs[0]['game'], self.game()['game'])
        summary = ev.compare(runs, self.config)
        self.assertEqual(summary['mean_delta_complete_clusters'], 0)
        self.assertEqual(summary['cluster_bootstrap_95'], [0, 0])
        failed = copy.deepcopy(runs)
        failed[0]['status'] = 'failed'
        summary = ev.compare(failed, self.config)
        self.assertEqual(summary['completed_pairs'], 5)
        self.assertEqual(summary['complete_seed_clusters'], 1)
        self.assertIsNone(summary['cluster_bootstrap_95'])

    def test_external_protocol_matches_builtin_greedy(self):
        baseline = self.game()
        self.config['candidate'] = {'label':'external', 'command':[
            sys.executable, 'tools/eval_policy_example.py', '--bid-peso', '4']}
        candidate = self.game('candidate')
        self.assertEqual(candidate['status'], 'completed')
        self.assertEqual(baseline['game'], candidate['game'])
        self.assertEqual(candidate['diagnostics']['unknown_cost_calls'], 0)
        self.assertFalse(candidate['diagnostics']['call_errors'])

    def test_timeout_is_visible_and_does_not_become_a_clean_model_action(self):
        response, trace = ev.call_policy({'command':[sys.executable,'-c','import time; time.sleep(10)']}, {}, .05)
        self.assertEqual(response, {'action':'pass'})
        self.assertEqual(trace['source'], 'fallback')
        self.assertEqual(trace['error'], 'policy_timeout')
        self.assertIsNone(trace['cost_usd'])

    def test_external_requests_do_not_disclose_seed_in_either_epoch(self):
        self.config['candidate'] = {'label': 'probe', 'command': ['unused']}
        ids = set()

        def assert_no_seed(value):
            if isinstance(value, dict):
                self.assertFalse({'seed', 'game_seed'} & value.keys())
                for child in value.values():
                    assert_no_seed(child)
            elif isinstance(value, list):
                for child in value:
                    assert_no_seed(child)

        for epoch in (0, 1):
            self.config['epoch'] = epoch
            for _ in range(2):
                requests = []

                def probe(variant, request, timeout):
                    requests.append(copy.deepcopy(request))
                    response = {'action': 'pass'}
                    return response, {'source': 'external_unverified', 'error': None,
                                      'cost_usd': None, 'response': response}

                with patch.object(ev, 'call_policy', side_effect=probe):
                    result = self.game('candidate')
                self.assertEqual(result['status'], 'completed')
                self.assertTrue(requests)
                self.assertEqual(result['seed'], 4242)  # Harness retains replay evidence.
                self.assertNotIn(result['match_id'], ids)
                ids.add(result['match_id'])
                for request in requests:
                    assert_no_seed(request)
                    self.assertEqual(request['schema_version'], 2)
                    self.assertEqual(request['match_id'], result['match_id'])
                    self.assertEqual(len(request['match_id']), 32)

    def test_auto_seeds_resolve_to_unique_unpredictable_u64(self):
        cfg = dict(self.config)
        cfg['seeds'] = 'auto'
        cfg['auto_seed_count'] = 4
        with tempfile.NamedTemporaryFile('w', suffix='.json', delete=False) as handle:
            json.dump(cfg, handle)
            path = Path(handle.name)
        try:
            resolved = ev.load_config(path)
        finally:
            path.unlink()
        self.assertEqual(len(resolved['seeds']), 4)
        self.assertEqual(len(set(resolved['seeds'])), 4)
        self.assertTrue(all(type(s) is int and 0 <= s < 2**64 for s in resolved['seeds']))
        self.assertEqual(resolved['seeds_source'], 'auto_os_u64')
        cfg['auto_seed_count'] = 0
        with tempfile.NamedTemporaryFile('w', suffix='.json', delete=False) as handle:
            json.dump(cfg, handle)
            path = Path(handle.name)
        try:
            with self.assertRaises(ValueError):
                ev.load_config(path)
        finally:
            path.unlink()
        cfg.update(seeds=[4242, 7777], auto_seed_count=3)
        with tempfile.NamedTemporaryFile('w', suffix='.json', delete=False) as handle:
            json.dump(cfg, handle)
            path = Path(handle.name)
        try:
            with self.assertRaises(ValueError):
                ev.load_config(path)
        finally:
            path.unlink()

    def test_invalid_action_is_retained_as_fallback(self):
        self.config['candidate'] = {'label':'invalid', 'command':[
            sys.executable,'-c','print(\'{"action":"not-a-move"}\')']}
        result = self.game('candidate')
        self.assertEqual(result['status'], 'completed')
        self.assertEqual(result['diagnostics']['sources'], {'fallback':18})
        self.assertEqual(sum(result['diagnostics']['call_errors'].values()),18)
        self.assertEqual(result['diagnostics']['unknown_cost_calls'],18)

    def test_broken_engine_retains_failure(self):
        script=self.root/'broken'
        script.write_text('#!/bin/sh\nexit 2\n')
        script.chmod(0o700)
        folder=self.root/'failure'
        folder.mkdir()
        result=ev.run_game(script,self.config,'baseline',4242,0,folder)
        self.assertEqual(result['status'],'failed')
        self.assertTrue((folder/'result.json').exists())

    def test_cost_is_retained_when_engine_dies_after_request(self):
        script=self.root/'dies_after_request'
        script.write_text('#!'+sys.executable+'\nimport sys,json\nsys.stdin.readline()\n'
                          'print(json.dumps({"type":"request","phase":"market","observation":{}}),flush=True)\n')
        script.chmod(0o700)
        self.config['candidate']={'label':'cost','command':[
            sys.executable,'-c','print(\'{"action":"pass","cost_usd":0.75}\')']}
        folder=self.root/'pending'
        folder.mkdir()
        result=ev.run_game(script,self.config,'candidate',4242,0,folder)
        self.assertEqual(result['status'],'failed')
        self.assertEqual(result['diagnostics']['known_cost_usd'],.75)
        self.assertEqual(result['trace'][0]['type'],'unacknowledged')

    def test_settlement_and_subject_validation(self):
        result=self.game()
        broken=copy.deepcopy(result['game'])
        broken['payouts'][0]+=1
        with self.assertRaises(ValueError): ev.validate_game(broken,0)
        with self.assertRaises(ValueError): ev.validate_game(result['game'],1)

    def test_duplicate_seeds_rejected(self):
        self.config['seeds']=[7,7]
        path=self.root/'config.json'
        ev.write_json(path,self.config)
        with self.assertRaises(ValueError): ev.load_config(path)

    def test_pilot_budget_keeps_missing_costs_unknown(self):
        c=json.loads((ev.ROOT/'data/eval/configs/pilot_budget.json').read_text())
        result=pilot_budget.calculate(c)
        self.assertEqual(result['planned_hours'],11)
        self.assertIsNone(result['total_budget'])
        self.assertEqual(len(result['missing_inputs_before_invitations']),4)
        c.update(hourly_cost=10000,inference_budget_per_operator=1000,
                 other_costs=5000,offer_per_operator=30000)
        result=pilot_budget.calculate(c)
        self.assertEqual(result['total_budget'],120000)
        self.assertEqual(result['break_even_per_operator_if_all_pay'],24000)
        self.assertEqual(result['contribution_if_all_pay'],30000)


if __name__=='__main__':
    unittest.main()
