"""Offline authenticity checks for the local-validator demo receipts (requires solders)."""
import json
from pathlib import Path
from solders.message import Message
from solders.signature import Signature
from solders.pubkey import Pubkey
from solders.hash import Hash
from solders.instruction import CompiledInstruction
alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def decode(text):
 n=0
 for c in text:n=n*58+alphabet.index(c)
 return b'\0'*(len(text)-len(text.lstrip('1')))+(n.to_bytes((n.bit_length()+7)//8,'big') if n else b'')
p=json.loads((Path(__file__).resolve().parents[1]/'docs/ops/CYBER_LOCAL_PROOF_20260906.json').read_text())
assert p['network']=='solana-local-validator'
for tx in p['transactions']:
 m=tx['receipt']['transaction']['message'];h=m['header']
 message=Message.new_with_compiled_instructions(h['numRequiredSignatures'],h['numReadonlySignedAccounts'],h['numReadonlyUnsignedAccounts'],[Pubkey.from_string(k) for k in m['accountKeys']],Hash.from_string(m['recentBlockhash']),[CompiledInstruction(i['programIdIndex'],decode(i['data']),bytes(i['accounts'])) for i in m['instructions']])
 assert Signature.from_string(tx['signature']).verify(Pubkey.from_string(m['accountKeys'][0]),bytes(message))
 assert tx['receipt']['meta']['err'] is None
 assert p['program'] in m['accountKeys']
joins=[t for t in p['transactions'] if t['instruction']=='join']
assert len(joins)==len(p['players'])==6
assert len({player['wallet'] for player in p['players']})==6
for player,join in zip(p['players'],joins):
 m=join['receipt']['transaction']['message']
 assert m['header']['numRequiredSignatures']==1 and m['accountKeys'][0]==player['wallet']
 raw=decode(m['instructions'][0]['data'])
 assert raw[12:].decode()==player['name']
assert sum(x['amount'] for x in p['payouts'])==57_000_000
assert p['settled']=={'pot':60_000_000,'rake':3_000_000,'paid':57_000_000}
assert sorted(x['amount'] for x in p['payouts'])==[2_850_000,8_550_000,17_100_000,28_500_000]
print(f"PASS: {len(p['transactions'])} valid signatures; all 6 joins have their own sole signer and correct name; payout and rake match")
