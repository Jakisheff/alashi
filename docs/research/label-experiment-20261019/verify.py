"""Independent terminal arithmetic check, without importing or calling the game engine."""
import json
from pathlib import Path
root=Path(__file__).resolve().parent
rows=json.loads((root/'fixtures.json').read_text());checked=0
for row in rows:
 for option in row['options']:
  cash=list(row['cash']);influence=list(row['influence']);action=option['id']
  if row['phase']=='Action' and 'to' in option:
   cash[0]-=option['amount'];cash[option['to']]+=option['amount'];influence[0]+=option['amount']//5000000
  if row['phase']=='Law':
   yes=1+(influence[0] if action=='yes' else 0);no=influence[0] if action=='no' else 0
   if yes>no and action!='veto':
    beneficiary=min(range(3),key=lambda i:(cash[i],i)) if row['card']==4 else min(range(3),key=lambda i:(-cash[i],i))
    influence[beneficiary]+=1
  order=sorted(range(3),key=lambda i:(-cash[i],i));rank=order.index(0)
  factory=min(range(3),key=lambda i:(-influence[i],-cash[i],i))
  expected={'cash':cash,'influence':influence,'rank':rank,'factory_bonus':5000000 if factory==0 else 0,'payout':[50000000,30000000,15000000][rank]+(5000000 if factory==0 else 0)}
  assert expected==option['outcome'],(row['id'],action,expected,option['outcome']);checked+=1
validation=json.loads((root/'validation.json').read_text());validation['independent_terminal_arithmetic']={'branches_checked':checked,'passed':True};(root/'validation.json').write_text(json.dumps(validation,ensure_ascii=False,indent=2)+'\n')
print('Independent arithmetic agrees with engine on every field of',checked,'branches')
