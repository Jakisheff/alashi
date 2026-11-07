import json,unittest
from discover import execute
from safe_runtime import validate
class RuntimeTests(unittest.TestCase):
 def test_pure_code_and_features(self):
  result=execute('print(sum(len(g["observations"]) for g in games))','analysis',games=[{'observations':[1,2]}]);self.assertEqual(result,{'ok':True,'output':'2\n'})
  result=execute('def augment(state, history):\n return {"cash": state["cash"] * 2}','helper',state={'cash':4},history=[]);self.assertEqual(result['value'],{'cash':8})
 def test_no_host_capabilities(self):
  for code in ['import os','open("secret")','state.__class__','getattr(state,"x")','class X: pass']:
   with self.subTest(code=code),self.assertRaises(ValueError):validate(code)
 def test_invalid_outputs_and_timeout(self):
  self.assertFalse(execute('def augment(state, history):\n return []','helper',state={},history=[])['ok'])
  self.assertFalse(execute('while True:\n pass','analysis',games=[])['ok'])
if __name__=='__main__':unittest.main()
