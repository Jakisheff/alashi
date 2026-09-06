#!/usr/bin/env python3
"""Execute an isolated LOCAL Solana game, preserving public RPC receipts only."""
import json,time,hashlib,struct,base64,urllib.request
from pathlib import Path
from solders.keypair import Keypair
from solders.pubkey import Pubkey
from solders.hash import Hash
from solders.instruction import Instruction,AccountMeta
from solders.message import Message
from solders.transaction import Transaction
URL='http://127.0.0.1:28999'
PROGRAM=Pubkey.from_string('8EikcWzM7d3EjttApmymo2maWp5A3NtMpKoYdWUzzzL')
SYS=Pubkey.default();SLOTS=Pubkey.from_string('SysvarS1otHashes111111111111111111111111111')
def rpc(method,params=[]):
 req=urllib.request.Request(URL,json.dumps({'jsonrpc':'2.0','id':1,'method':method,'params':params}).encode(),{'Content-Type':'application/json'})
 with urllib.request.urlopen(req,timeout=12) as r:data=json.load(r)
 if 'error' in data:raise RuntimeError(str(data['error']))
 return data['result']
def confirm(sig):
 for _ in range(50):
  v=rpc('getSignatureStatuses',[[sig],{'searchTransactionHistory':True}])['value'][0]
  if v:
   if v['err']:raise RuntimeError(str(v))
   if v['confirmationStatus'] in ['confirmed','finalized']:return v
  time.sleep(.15)
 raise RuntimeError('Confirmation timed out')
def meta(pub,sign=False,write=True):return AccountMeta(pub,sign,write)
names=['Aitore','Aikorkem','Aisultan','Botagul','Aibot','Zhambyl'];players=[Keypair() for _ in names];admin=players[0];gid=int(time.time())
game=Pubkey.find_program_address([b'game',struct.pack('<Q',gid)],PROGRAM)[0]
factions=[Pubkey.find_program_address([b'faction',bytes(game),bytes(p.pubkey())],PROGRAM)[0] for p in players]
receipts=[]
def send(name,signer,accounts,data=b''):
 ix=Instruction(PROGRAM,hashlib.sha256(('global:'+name).encode()).digest()[:8]+data,accounts)
 bh=Hash.from_string(rpc('getLatestBlockhash',[{'commitment':'confirmed'}])['value']['blockhash'])
 tx=Transaction([signer],Message([ix],signer.pubkey()),bh)
 sig=rpc('sendTransaction',[base64.b64encode(bytes(tx)).decode(),{'encoding':'base64','preflightCommitment':'confirmed'}]);status=confirm(sig)
 for _ in range(30):
  receipt=rpc('getTransaction',[sig,{'encoding':'json','commitment':'confirmed','maxSupportedTransactionVersion':0}])
  if receipt:break
  time.sleep(.1)
 assert receipt and receipt['meta']['err'] is None
 receipts.append({'instruction':name,'signature':sig,'status':status,'receipt':receipt})
 print(name+' confirmed slot '+str(receipt['slot']),flush=True)
 return receipts[-1]
for p in players:confirm(rpc('requestAirdrop',[str(p.pubkey()),2_000_000_000]))
send('initialize',admin,[meta(admin.pubkey(),True),meta(game),meta(SYS,write=False)],struct.pack('<QQqBB',gid,10_000_000,0,0,0))
for i,p in enumerate(players):send('join',p,[meta(p.pubkey(),True),meta(game),meta(factions[i]),meta(SYS,write=False)],struct.pack('<I',len(names[i]))+names[i].encode())
def advance():send('advance',admin,[meta(admin.pubkey(),True,False),meta(game),meta(SLOTS,write=False)]+[meta(f) for f in factions])
advance()
for round in range(1,7):
 if round>1:
  for i,p in enumerate(players):send('sell',p,[meta(p.pubkey(),True,False),meta(game),meta(factions[i])],struct.pack('<H',2))
 advance()
 for i,p in enumerate(players):send('produce',p,[meta(p.pubkey(),True,False),meta(game,write=False),meta(factions[i])])
 advance()
 for i,p in enumerate(players):send('vote',p,[meta(p.pubkey(),True,False),meta(game,write=False),meta(factions[i])],bytes([1]))
 advance()
settle=send('settle',admin,[meta(admin.pubkey(),True,False),meta(game)]+[meta(f,write=False) for f in factions]+[meta(p.pubkey()) for p in players]+[meta(admin.pubkey())])
payouts=[];settled=None
for log in settle['receipt']['meta']['logMessages']:
 if not log.startswith('Program data: '):continue
 b=base64.b64decode(log.split(': ',1)[1])
 if b[:8]==hashlib.sha256(b'event:Payout').digest()[:8]:payouts.append({'wallet':str(Pubkey.from_bytes(b[40:72])),'rank':b[72],'amount':struct.unpack('<Q',b[73:81])[0]})
 if b[:8]==hashlib.sha256(b'event:Settled').digest()[:8]:settled=dict(zip(['pot','rake','paid'],struct.unpack('<QQQ',b[40:64])))
assert len(payouts)==4 and settled and settled['paid']==57_000_000 and settled['rake']==3_000_000
output={'network':'solana-local-validator','rpc':URL,'program':str(PROGRAM),'program_sha256':hashlib.sha256(Path('target/deploy/alashi.so').read_bytes()).hexdigest(),'game':str(game),'game_id':gid,'players':[{'name':n,'wallet':str(p.pubkey())} for p,n in zip(players,names)],'entry_fee':10_000_000,'payouts':payouts,'settled':settled,'transactions':receipts}
Path('docs/ops/CYBER_LOCAL_PROOF_20260906.json').write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps({'ok':True,'transactions':len(receipts),'payouts':payouts,'settled':settled}))
