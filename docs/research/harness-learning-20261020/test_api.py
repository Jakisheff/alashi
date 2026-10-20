import contextlib,io,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
from model_api import API
class APITests(unittest.TestCase):
 def test_saved_first_response_is_never_regenerated(self):
  with tempfile.TemporaryDirectory() as tmp,patch.dict('os.environ',{'ALASHI_ZAI_CODING_KEY':'unit-test-secret-value'}):
   api=API(Path(tmp),{'calls':3,'prompt_tokens':1000,'completion_tokens':1000,'wall_s':100});body={'messages':[{'role':'user','content':'mock public state'}]};raw={'model':'glm-5.3','usage':{'prompt_tokens':3,'completion_tokens':2}}
   with patch('urllib.request.urlopen',return_value=contextlib.closing(io.StringIO(json.dumps(raw)))) as network:
    response,serial=api.call('a0',body);again,same=api.call('a0',body);self.assertEqual(network.call_count,1);self.assertEqual((response,serial),(again,same));self.assertEqual(api.calls,1)
   self.assertNotIn('unit-test-secret-value',(Path(tmp)/'a0.request.json').read_text());restored=API(Path(tmp),api.limits);self.assertEqual((restored.calls,restored.responses,restored.prompt,restored.completion),(1,1,3,2))
 def test_unknown_attempt_is_consumed(self):
  with tempfile.TemporaryDirectory() as tmp,patch.dict('os.environ',{'ALASHI_ZAI_CODING_KEY':'unit-test-secret-value'}):
   root=Path(tmp);body={'messages':[]};(root/'lost.request.json').write_text(json.dumps({'serial':1,'body':body}));api=API(root,{'calls':3,'prompt_tokens':1000,'completion_tokens':1000,'wall_s':100})
   with patch('urllib.request.urlopen',side_effect=AssertionError('Unexpected model call')):
    with self.assertRaises(RuntimeError):api.call('lost',body)
   self.assertEqual(api.calls,1)
if __name__=='__main__':unittest.main()
