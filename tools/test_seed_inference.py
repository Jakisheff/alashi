"""Documented seed-inference channel of the local evaluation protocol.

The whole match randomness is derived from one seed through public formulas
(arena/src/runner.rs phase seeding, rules/src/transitions.rs law draw). This
test keeps the formula replication pinned to recorded engine output and
demonstrates why operator comparisons must not use low-entropy seeds.
"""
import json
import unittest

import compare_agents as ev

MASK64 = (1 << 64) - 1
RECORDED_RUN = ev.ROOT / 'data/eval/runs/external-20260907/evidence.json'
RECORDED_7777 = (2, 5, 8, 4, 6, 3)


def splitmix64(x):
    z = (x + 0x9E3779B97F4A7C15) & MASK64
    z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK64
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK64
    return z ^ (z >> 31)


def round_card(seed, round_, mask, amnesty):
    """Law card of one round: Action-phase advance seed, deck probing, 90s amnesty bit."""
    rs = splitmix64(seed ^ ((round_ << 8) | 2))
    m = 0 if mask == 0xFF else mask
    idx = rs % 8
    while m & (1 << idx):
        idx = (idx + 1) % 8
    m |= 1 << idx
    if not amnesty and idx != 8 and ((rs >> 16) & 1) == 1:
        return 8, m, True
    return idx, m, amnesty


def law_cards(seed):
    mask, amnesty, cards = 0, False, []
    for round_ in range(1, 7):
        card, mask, amnesty = round_card(seed, round_, mask, amnesty)
        cards.append(card)
    return tuple(cards)


def matches(sequence, limit):
    found = []
    for seed in range(limit):
        mask, amnesty, ok = 0, False, True
        for round_, want in enumerate(sequence, 1):
            card, mask, amnesty = round_card(seed, round_, mask, amnesty)
            if card != want:
                ok = False
                break
        if ok:
            found.append(seed)
    return found


class SeedInferenceTests(unittest.TestCase):
    def test_formula_replication_matches_recorded_engine_output(self):
        if not RECORDED_RUN.is_file():
            self.skipTest('recorded run external-20260907 is absent')
        episodes = json.loads(RECORDED_RUN.read_text())
        checked = 0
        for episode in episodes:
            if episode['side'] != 'baseline' or episode['seat'] != 0:
                continue
            cards = tuple(e['law_card'] for e in episode['events'] if e['kind'] == 'after_passed_law')
            self.assertEqual(law_cards(episode['seed']), cards)
            checked += 1
        self.assertGreaterEqual(checked, 3)

    def test_low_entropy_seed_is_recoverable_from_law_cards_alone(self):
        found = matches(RECORDED_7777, 100_000)
        self.assertIn(7777, found)
        self.assertLessEqual(len(found), 25)


if __name__ == '__main__':
    unittest.main()
