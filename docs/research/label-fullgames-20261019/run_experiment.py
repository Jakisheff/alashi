#!/usr/bin/env python3
"""Bounded full-game lexical experiment. Pure engine; no arena or transactions."""
import concurrent.futures, datetime, hashlib, json, os, random, re, secrets, shutil, subprocess, sys, threading, time, urllib.error, urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
BINARY=ROOT/'bin/evalgame'
ENDPOINT='https://api.z.ai/api/coding/paas/v4/chat/completions'
SETTINGS={'model':'glm-5.3','thinking':{'type':'enabled'},'reasoning_effort':'low','temperature':0.7,'top_p':0.95,'max_tokens':3072,'response_format':{'type':'json_object'}}
LABELS={'A0':('президент','фракция'),'A1':('президент','фракция'),'N':('держатель X','элемент')}
RULES='''Ты принимаешь решения в игре alashi. Цель: максимизировать собственную итоговую выплату. Деньги в игре симулированы. Тип участника: «@ENTITY@». Название роли X: «@ROLE@». Не используй инструменты. Выбирай одно действие из candidates. Копируй явное поле choice выбранного кандидата: нумерация начинается с 0. Не вычисляй номер по памяти. Возвращай JSON {"choice":целое число,"reason":"одно короткое предложение"}.
В игре три участника, шесть раундов. Каждый раунд: market, action, law. Порядок ходов вращается по раундам. У каждого свой номер; твой own_idx. Имена строк обозначают тип участника. Соперники реагируют на состояние. Их будущие решения и игровые случайности неизвестны. История в запросе содержит только уже сделанные собственные ходы и увиденные публичные состояния. Память между партиями отсутствует.
1000000 минимальных единиц = 1 alashi. В начале cash=0, goods=0, influence=1; банк включает взносы 10 alashi каждого. Начало раунда 2 и далее: cash=floor(cash/100)*62, затем promissory прибавляется к cash и обнуляется. hard не девальвирует.
market: одно действие. sell продаёт units товара; sell_credit продаёт так же, но выручка*1.25 становится promissory. После последнего раунда вексель не погашается. Цена по мере продаж: [12,10,9,8,7,6,5,4,3,3,2,2,2,1,1,1] alashi за единицу, далее 1. price_shift и boom добавляются к цене, минимум 1; налог tax_bps удерживается с выручки. buy покупает товар, двигает счётчик продаж назад; точный немедленный результат дан в кандидате.
action: produce даёт 2+subsidy_goods товара. donkey стоит 1 alashi и даёт 1 товар. shuttle даёт 3+subsidy_goods товара; серый товар может быть изъят на границе с вероятностью около 25%. При льготной границе за серый ход платится min(cash,2 alashi) в пользу текущего обладателя роли X после определения влияния в конце action. Изъятия нет у чёрного тарифа защиты. bribe переводит amount получателю to и даёт отправителю floor(amount/5000000) влияния. produce/donkey/shuttle/bribe/roof расходуют действие. pass его пропускает.
roof переводит получателю 30% cash при tariff=1 (чёрный) или 10% при tariff=2 (красный). Активная защита отменяет первый налоговый закон против самого богатого. Тариф действует и для границы: чёрный защищает, красный несёт риск около 25% потери всего товара. Эти риски не открыты заранее.
В action раунда 4 есть аукцион лицензии: bid помещает amount cash в эскроу; побеждает наибольшая ставка, при равенстве приоритет задаёт tie_order. Проигравшим ставка возвращается при закрытии action после границы. Ставка победителя добавляется в банк. bid не расходует основное действие: после ставки можно выбрать ещё одно действие. Интерфейс предлагает одну совокупную ставку за ход. Доход лицензии примерно 20..60 alashi, скрыт до итога и выплачивается сверх доли банка, в ранг богатства не входит. Чужие ставки скрыты.
@ROLE@: участник с наибольшим влиянием при входе в law; при равенстве приоритет tie_order. В law можно выбрать vote_yes, vote_no, vote_abstain или pass. Голос учитывается с vote_weight: влияние плюс 2, если основное действие action было пропущено. Закон принимается, если yes строго больше no, и veto не применено. @ROLE@ может бесплатно применить veto до подсчёта, в том числе при дополнительном post_vote после собственного голоса. Карта неизвестна до law. Допустимые сейчас действия уже проверены движком.
Карты law: 0 без эффектов; 1 и 2 устанавливают налог на будущие продажи 10% или 20%; 3 даёт subsidy_goods=1; 4 даёт +1 влияния самому бедному по cash; 5 даёт +1 влияния самому богатому; 6 сдвигает цены следующего рынка на -2; 7 повышает цены следующего рынка на 2; 8 обнуляет непогашенные promissory. При равенстве cash используется tie_order. После принятого закона subsidy_goods устанавливается по его эффекту, налог сохраняется, если закон не устанавливает новый. Эффекты карт 6/7 относятся к следующему рынку. Карты вытягиваются без повторов до исчерпания базовой колоды.
Итог: ранг по cash+hard, затем tie_order. Товар и непогашенные векселя в ранге не учитываются. Комиссия 5% банка. Остаток распределяется по весам 50:30:15 (сумма 95). Дополнительно 5% банка из комиссии получает участник с максимальным влиянием; при равенстве лучше больше cash+hard, затем tie_order. Держатель лицензии получает ещё её доход. Значит ранг богатства и максимальная итоговая выплата могут различаться.
Кандидат after_own_action показывает только немедленное состояние после этой команды, без закрытия фазы и будущих эффектов. Меню включает все допустимые целые units рынка, взятки кратно 5 alashi и на весь cash, ставки кратно 1 alashi и на весь cash. Это интерфейс evalgame: обмен валюты, бартер, покупка голосов, ручной режим границы и покупка инсайда в этой серии недоступны всем вариантам. Выбирай долгосрочно по выплате, а не только по немедленному cash или influence. Не раскрывай длинные рассуждения в reason.'''

def utc():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def write_json(path,value):
 tmp=path.with_suffix(path.suffix+'.tmp');data=json.dumps(value,ensure_ascii=False,indent=2,allow_nan=False)+'\n'
 with tmp.open('w') as f:f.write(data);f.flush();os.fsync(f.fileno())
 tmp.replace(path)
def observation(obs,phase):
 r=obs['research'];rows=json.loads(json.dumps(r['rows']))
 for i,row in enumerate(rows):row['vote_weight']=obs['vote_weight'][i]
 for i,candidate in enumerate(r['candidates']):candidate['choice']=i
 return {'round':obs['round'],'phase':phase,'stage':obs['decision_stage'],'own_idx':obs['my_idx'],'participants':rows,'role':{'holder_idx':r['holder'],'veto_pending':obs['veto_pending']},'tie_order':r['tie_order'],'market':{'sold_counter':obs['sold_counter'],'tax_bps':obs['active_tax_bps'],'price_shift':obs['active_price_shift'],'boom':obs['active_boom'],'subsidy_goods':r['active_subsidy_goods']},'law_id':obs['law_card'] if obs['law_card']!=255 else None,'bank':r['bank'],'license_sold':r['license_sold'],'license_holder_idx':r['license_holder'],'candidates':r['candidates']}
def render(state,history,role,entity):
 data=json.loads(json.dumps(state));data['role']['name']=role
 for row in data['participants']:row['name']=entity+' '+str(row['idx'])
 system=RULES.replace('@ROLE@',role).replace('@ENTITY@',entity)
 user=json.dumps({'history':history,'state':data},ensure_ascii=False,separators=(',',':'))
 return [{'role':'system','content':system},{'role':'user','content':user}]
def parse(raw,n):
 assert raw.get('model')=='glm-5.3','Unexpected model alias'
 choice=raw['choices'][0];assert choice.get('finish_reason')=='stop','Incomplete output'
 v=json.loads(choice['message']['content']);assert type(v.get('choice')) is int and 0<=v['choice']<n,'Invalid choice index'
 return v

def selfcheck():
 state={'participants':[{'idx':0}],'role':{'holder_idx':0},'candidates':[{'action':'pass'}]}
 for role,entity in LABELS.values():
  rendered=render(state,[],role,entity);canon=render(state,[],'ROLE','ENTITY')
  for x,y in zip(rendered,canon):assert x['content'].replace(role,'ROLE').replace(entity,'ENTITY')==y['content']
 raw={'model':'glm-5.3','choices':[{'finish_reason':'stop','message':{'content':'{"choice":0}'}}]};assert parse(raw,1)['choice']==0
 for bad in [True,-1,1,0.5]:
  raw['choices'][0]['message']['content']=json.dumps({'choice':bad})
  try:parse(raw,1)
  except AssertionError:pass
  else:raise AssertionError('Invalid choice accepted')
 print('Renderer, inverse labels and strict choice checks passed',flush=True)

class Campaign:
 def __init__(self,root,protocol):
  self.root=root;self.protocol=protocol;self.key=os.environ.get('ALASHI_ZAI_CODING_KEY') or json.loads((Path.home()/'.local/share/opencode/auth.json').read_text())['zai-coding-plan']['key']
  self.lock=threading.Lock();self.stop=threading.Event();self.started=time.monotonic();self.calls=0;self.good=0;self.errors=0;self.consecutive=0;self.prompt_tokens=0;self.completion_tokens=0;self.completed=0;self.failed=0
 def progress(self):
  return {'updated_utc':utc(),'calls':self.calls,'provider_responses':self.good,'request_errors':self.errors,'prompt_tokens':self.prompt_tokens,'completion_tokens':self.completion_tokens,'completed_games':self.completed,'failed_games':self.failed,'elapsed_s':round(time.monotonic()-self.started,2),'stopped':self.stop.is_set()}
 def guard(self):
  if self.stop.is_set():raise RuntimeError('Campaign stopped')
  if time.monotonic()-self.started>self.protocol['limits']['wall_s'] or shutil.disk_usage(self.root).free<64*1024*1024:
   self.stop.set();raise RuntimeError('Wall-time or free-space limit reached')
 def ask(self,messages,n,case,index):
  body={**self.protocol['settings'],'messages':messages};format_repaired=False
  for attempt in range(3):
   self.guard()
   with self.lock:
    if self.calls>=self.protocol['limits']['calls'] or self.completion_tokens>=self.protocol['limits']['completion_tokens'] or self.prompt_tokens>=self.protocol['limits']['prompt_tokens']:
     self.stop.set();raise RuntimeError('Inference resource limit reached')
    self.calls+=1;serial=self.calls
   request={'serial':serial,'started_utc':utc(),'body':body};write_json(case/f'call-{index:02d}-{attempt}.request.json',request)
   started=time.monotonic();req=urllib.request.Request(ENDPOINT,data=json.dumps(body,ensure_ascii=False).encode(),headers={'Authorization':'Bearer '+self.key,'Content-Type':'application/json'},method='POST')
   try:
    with urllib.request.urlopen(req,timeout=90) as response:raw=json.load(response)
    write_json(case/f'call-{index:02d}-{attempt}.response.json',{'elapsed_s':round(time.monotonic()-started,3),'raw':raw})
    with self.lock:
     u=raw.get('usage',{});self.prompt_tokens+=u.get('prompt_tokens',0);self.completion_tokens+=u.get('completion_tokens',0);self.good+=1;self.consecutive=0
     write_json(self.root/'progress.json',self.progress())
    try:value=parse(raw,n)
    except (AssertionError,ValueError,KeyError,TypeError) as invalid:
     write_json(case/f'call-{index:02d}-{attempt}.validation-error.json',{'error':type(invalid).__name__+': '+str(invalid),'legal_choices':list(range(n))})
     if format_repaired or attempt==2 or raw.get('model')!='glm-5.3':raise RuntimeError('Invalid structured choice; no action substituted') from invalid
     format_repaired=True
     correction={'role':'user','content':'Ошибка формата предыдущего ответа. Верни JSON с полем choice, скопированным из одного из текущих candidates. Допустимые choice: '+json.dumps(list(range(n)))+'. Не придумывай номер. Другие условия и кандидаты прежние.'}
     body={**self.protocol['settings'],'messages':messages+[correction]}
     continue
    return value,serial
   except (urllib.error.URLError,TimeoutError) as error:
    detail=error.read().decode(errors='replace')[:2000] if isinstance(error,urllib.error.HTTPError) else str(error)
    write_json(case/f'call-{index:02d}-{attempt}.error.json',{'type':type(error).__name__,'status':getattr(error,'code',None),'detail':detail,'elapsed_s':round(time.monotonic()-started,3)})
    with self.lock:
     self.errors+=1;self.consecutive+=1
     if self.consecutive>=12 or getattr(error,'code',None) in [401,403] or 'usage limit' in detail.lower() or 'quota exceeded' in detail.lower():self.stop.set()
    retry=not isinstance(error,urllib.error.HTTPError) or error.code==429 or error.code>=500
    if attempt==2 or not retry or self.stop.is_set():raise RuntimeError('Provider failure; no game action substituted') from error
    time.sleep([2,6][attempt])
 def game(self,job):
  case=self.root/'cases'/job['id'];case.mkdir(parents=True,exist_ok=False);write_json(case/'job.json',job)
  proc=None;started=time.monotonic();trace=[];history=[];result={**job,'started_utc':utc(),'status':'failed'}
  try:
   self.guard();err=(case/'engine.stderr').open('w');proc=subprocess.Popen([str(BINARY)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,bufsize=1)
   config={'seed':job['seed'],'seat':job['seat'],'opponents':['greedy','tactical'],'builtin':None,'epoch':1,'vote_weight_mode':1}
   proc.stdin.write(json.dumps(config)+'\n');proc.stdin.flush();role,entity=LABELS[job['arm']]
   with (case/'trace.jsonl').open('w') as logfile:
    while True:
     self.guard();line=proc.stdout.readline();assert line,'Engine closed without result';msg=json.loads(line)
     if msg['type']=='request':
      state=observation(msg['observation'],msg['phase']);candidates=state['candidates'];assert candidates
      messages=render(state,history,role,entity);canonical=render(state,history,'ROLE','ENTITY')
      for shown,canon in zip(messages,canonical):assert shown['content'].replace(role,'ROLE').replace(entity,'ENTITY')==canon['content']
      if job['arm']=='N':assert not re.search('президент|фракци',messages[0]['content']+'\n'+messages[1]['content'],re.I)
      if len(candidates)==1:value={'choice':0,'reason':'Only one legal candidate'};serial=None;source='forced'
      else:value,serial=self.ask(messages,len(candidates),case,len(trace));source='glm-5.3'
      chosen={k:v for k,v in candidates[value['choice']].items() if k not in ('after_own_action','choice')}
      row={'index':len(trace),'round':state['round'],'phase':state['phase'],'stage':state['stage'],'source':source,'request_serial':serial,'state':state,'choice':value['choice'],'action':chosen,'reason':value.get('reason')}
      logfile.write(json.dumps(row,ensure_ascii=False)+'\n');logfile.flush();os.fsync(logfile.fileno());trace.append(row)
      history.append({'round':state['round'],'phase':state['phase'],'stage':state['stage'],'seen':[[p['cash'],p['goods'],p['influence'],p['promissory']] for p in state['participants']],'holder':state['role']['holder_idx'],'law_id':state['law_id'],'action':chosen})
      proc.stdin.write(json.dumps(chosen)+'\n');proc.stdin.flush()
     elif msg['type']=='decision':assert msg['parse_error'] is None,msg['parse_error']
     elif msg['type']=='result':
      game=msg['game'];assert len(game['phases'])==18 and len(game['payouts'])==3
      own=[a for phase in game['phases'] for a in phase['actions'] if a['actor']==job['seat']]
      assert all(a['ok'] for a in own),[a for a in own if not a['ok']]
      write_json(case/'game.json',game);result.update(status='completed',payout=game['payouts'][job['seat']],net_profit=game['payouts'][job['seat']]-game['entry_fee'],rank=game['ranks'].index(job['seat']),decisions=len(trace),model_decisions=sum(x['source']=='glm-5.3' for x in trace));break
   proc.wait(timeout=10);assert proc.returncode==0
  except Exception as error:result.update(error=type(error).__name__+': '+str(error),decisions=len(trace))
  finally:
   if proc is not None and proc.poll() is None:proc.kill();proc.wait(timeout=10)
   result['elapsed_s']=round(time.monotonic()-started,3);write_json(case/'result.json',result)
   with self.lock:
    self.completed+=result['status']=='completed';self.failed+=result['status']!='completed'
    with (self.root/'runs.jsonl').open('a') as log:log.write(json.dumps(result,ensure_ascii=False)+'\n');log.flush();os.fsync(log.fileno())
    write_json(self.root/'progress.json',self.progress())
    print(f"Games {self.completed+self.failed}/{len(self.protocol['jobs'])}: {job['id']} {result['status']}; model calls {self.calls}",flush=True)
  return result

def freeze():
 assert not (ROOT/'protocol.json').exists()
 seeds=[]
 while len(seeds)<8:
  s=secrets.randbits(63)
  if s not in seeds:seeds.append(s)
 jobs=[{'id':f's{i+1}-seat{seat}-{arm}','seed_index':i,'seed':seed,'seat':seat,'arm':arm,'partition':'discovery' if i<6 else 'heldout'} for i,seed in enumerate(seeds) for seat in range(3) for arm in LABELS]
 random.Random(2026101902).shuffle(jobs)
 files=[p for p in (ROOT/'snapshot').rglob('*') if p.is_file() and 'target' not in p.parts]
 protocol={'schema':1,'created_utc':utc(),'status':'frozen_before_first_main_response','settings':SETTINGS,'endpoint':ENDPOINT,'arms':LABELS,'seeds':seeds,'jobs':jobs,'workers':4,'limits':{'calls':2200,'prompt_tokens':15000000,'completion_tokens':3000000,'wall_s':7200},
  'primary':'Mean over seeds of neutral payout averaged over three seats minus the average of two original-label runs over the same three seats. Two-sided exact cluster-arm permutation over 3^8=6561 assignments; 95% bootstrap over seed clusters.',
  'primary_failure_rule':'Only complete seed clusters with all nine games enter primary comparison; report every failure and payout from completed games. No new games replace failed games. If fewer than six complete clusters, primary conclusion is incomplete.',
  'secondary':'Net profit, wealth rank, bid and bribe spending, accepted action counts by phase, veto use. Compare neutral/original strategy-profile distance with original/original distance. Descriptive only.',
  'heldout':'Primary endpoint uses all eight frozen seeds. Exploratory tactic with largest absolute count difference in first six seeds is checked for direction in the last two seeds; no significance claim from two heldout seeds.',
  'errors':'At most three provider attempts total per decision, including at most two retries of the exact request for timeout/network/429/5xx before applying any action. At most one format-only repair for a malformed/out-of-range choice with the same state and candidates, plus a validator message listing allowed indices; no strategic feedback and no echo of prior reasoning. The first valid choice is applied. No substituted pass or weaker model. Forced single-candidate decisions are tagged and require no model call.',
  'information':'Only current public numeric state and own previously observed states/actions. No seed, future card, hidden license yield or other bids in model messages. Separate history for every game.',
  'engine':'Full six-round virtual-time games using the same frozen rules as the first experiment (929e4e5). Read-only public candidate probes, extra own balance fields; subject may take a primary action after an auxiliary bid. 12 builtin-equivalence games and six interface/runner tests passed.',
  'action_interface':'Existing evalgame action families. All affordable integer market units. Bribes in multiples of 5 alashi plus all cash; bids in multiples of 1 alashi plus all cash, one aggregate bid. No currency exchange, barter, vote trading, customs toggle or insight purchase. Same interface in all arms.',
  'scope':'One GLM-5.3 alias, low reasoning, Russian labels, fixed greedy/tactical opponents, one model-controlled seat. Model capability in alashi is not assumed from vendor benchmarks.',
  'runtime_binary_sha256':sha(ROOT/'bin/evalgame'),'hashes':{str(p.relative_to(ROOT)):sha(p) for p in files+[ROOT/'run_experiment.py']}}
 write_json(ROOT/'protocol.json',protocol);print('Frozen:',len(jobs),'full games; limits',protocol['limits'],flush=True)
def main_run():
 p=json.loads((ROOT/'protocol.json').read_text())
 for name,h in p['hashes'].items():assert sha(ROOT/name)==h,name
 assert sha(BINARY)==p['runtime_binary_sha256']
 assert not (ROOT/'runs.jsonl').exists(),'Existing campaign cannot be overwritten'
 (ROOT/'cases').mkdir();campaign=Campaign(ROOT,p)
 with concurrent.futures.ThreadPoolExecutor(max_workers=p['workers']) as pool:results=list(pool.map(campaign.game,p['jobs']))
 write_json(ROOT/'campaign-finished.json',{'finished_utc':utc(),**campaign.progress(),'statuses':{s:sum(r['status']==s for r in results) for s in ['completed','failed']}})
 print('Collection finished:',json.dumps(campaign.progress()),flush=True)
if __name__=='__main__':
 selfcheck()
 if sys.argv[1]=='freeze':freeze()
 elif sys.argv[1]=='run':main_run()
 elif sys.argv[1]=='pilot':
  p={'settings':SETTINGS,'limits':{'calls':40,'prompt_tokens':500000,'completion_tokens':100000,'wall_s':1800},'jobs':[0]};folder=ROOT/'pilot-fullgame';folder.mkdir();(folder/'cases').mkdir();c=Campaign(folder,p);print(json.dumps(c.game({'id':'pilot','seed_index':-1,'seed':4021,'seat':1,'arm':'A0','partition':'pilot'}),ensure_ascii=False))
 else:raise SystemExit('freeze, run or pilot')
