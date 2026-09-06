#!/usr/bin/env python3
import unittest
from board_agents import parse_decision, available_actions
class Decisions(unittest.TestCase):
 def test_accepts_llm_json(self):
  d=parse_decision('```json\n{"action":"sell","params":{"units":2}}\n```','market',0,0)
  self.assertEqual(d['params']['units'],2)
 def test_rejects_wrong_phase(self):
  with self.assertRaises(ValueError):parse_decision('{"action":"produce"}','law',0,0)
 def test_rejects_bad_numbers(self):
  for n in ('0','-1','true','1.2','65536'):
   with self.assertRaises(ValueError):parse_decision('{"action":"buy","params":{"units":'+n+'}}','market',0,0)
 def test_president_only_veto(self):
  with self.assertRaises(ValueError):parse_decision('{"action":"vote","params":{"choice":"yes"},"veto":true}','law',1,0)
 def test_bribe_to_other(self):
  with self.assertRaises(ValueError):parse_decision('{"action":"bribe","params":{"to":0,"amount":5000000}}','action',0,0)
 def test_pass_is_deliberate(self):
  self.assertEqual(parse_decision('{"action":"pass","params":{}}','action',0,0)['action'],'pass')
 def test_empty_market_only_allows_pass(self):
  state={'phase':'market','price_now':12,'factions':[{'idx':0,'cash':0,'goods':0}]}
  self.assertEqual(available_actions(state,0),['pass'])
 def test_buy_price_uses_millions(self):
  state={'phase':'market','price_now':12,'factions':[{'idx':0,'cash':11999999,'goods':2}]}
  self.assertEqual(available_actions(state,0),['pass','sell'])
  state['factions'][0]['cash']=12000000
  self.assertIn('buy',available_actions(state,0))
if __name__=='__main__':unittest.main()
