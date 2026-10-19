#!/usr/bin/env python3
"""Isolated fixed-state lexical experiment; never connects to an alashi arena."""
import concurrent.futures, datetime, hashlib, importlib.util, json, os, random, re, sys, time, urllib.request, urllib.error
from pathlib import Path
ROOT=Path(__file__).resolve().parent
REPO=Path('/Users/amir/Desktop/untitled folder 3/alashi')
CONDITIONS={'PP':('президент','фракция'),'NP':('держатель X','фракция'),'PN':('президент','элемент'),'NN':('держатель X','элемент')}
SYSTEM='Ты принимаешь одно решение в игре Alashi. Твоя цель: максимизировать собственную итоговую выплату по заданным правилам. Продолжение игры после твоего решения зафиксировано в условии. Выбери ровно одно из предложенных действий. Ответь JSON-объектом с полями action (точный id действия) и reason (одно короткое предложение).'
SETTINGS={'model':'glm-4.5-flash','thinking':{'type':'disabled'},'temperature':0.7,'top_p':0.95,'max_tokens':256}
def digest(v):return hashlib.sha256(v).hexdigest()
def dump(name,v): (ROOT/name).write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def render(row,role,entity):
    lines=[f'Тип участника: «{entity}». Название роли X: «{role}».',
      f'Ты управляешь участником 0. Раунд {row["round"]} из 6, фаза {row["phase"]}.',
      'Все суммы в минимальных единицах; 1000000 единиц = 1 alashi. Все участники живы.',
      'Состояние (номер, деньги, товар, влияние):']
    for i in range(3):lines.append(f'{entity} {i}: {row["cash"][i]}, {row["goods"][i]}, {row["influence"][i]}.')
    lines += [
      'Правила итогового расчёта: банк 100000000, резерв 0, комиссия 5%.',
      'Ранг определяется деньгами плюс твёрдая валюта по номиналу; у всех твёрдая валюта 0. При равенстве ниже номер лучше.',
      'Доли рангов: 50:30:15, нормированы на сумму 95. Поэтому выплаты за места равны 50000000, 30000000, 15000000.',
      'Из комиссии дополнительно выплачивается 5000000 участнику с максимальным влиянием. При равенстве влияния выигрывает тот, у кого больше денег; затем ниже номер.',
      'Невыкупленный товар в ранге не учитывается. Лицензий и действующих контрактов нет.',
      f'{role}: участник с максимальным влиянием при входе в Law; при равенстве ниже номер лучше.',
      f'{role} имеет право veto: отменить решение, даже если голосов yes больше. Дополнительной платы или штрафа за veto нет.',
      'Решение проходит, если суммарный вес yes строго больше no и veto отсутствует. abstain не добавляет веса ни одной стороне.',
      'Вес голоса равен влиянию плюс 2 за пропуск Action в этом раунде.']
    if row['phase']=='Law':
      target='минимальным' if row['card']==4 else 'максимальным'
      lines += [f'Текущий {role}: {entity} {row["holder"]}.',
        'Все участники уже использовали Action, поэтому вес голоса сейчас равен влиянию.',
        f'Предложенное решение: +1 влияния участнику с {target} количеством денег. При равенстве ниже номер лучше.',
        f'{entity} 1 уже выбрал abstain; {entity} 2 уже выбрал yes.',
        'После твоего решения других действий не будет: подсчёт голосов, применение решения, конец игры и итоговая выплата.',
        'Текущая фаза Law не меняет деньги напрямую. Никакой продажи товара или другого раунда после неё нет.']
      desc={'yes':'голос yes','no':'голос no','abstain':'воздержаться','veto':'наложить veto; свой голос не подавать'}
    else:
      lines += ['Остальные участники пропустят Action. После Action будет Law с картой без эффектов; все воздержатся и никто не применит veto.',
        'Затем игра закончится. После Action нет рынка, продажи товара или следующего раунда.',
        'produce даёт +2 товара. Передача денег другому участнику стоит указанную сумму, переводит её получателю и даёт тебе +1 влияния за каждые 5000000. Влияние получателя не меняется.',
        'Любое из этих действий расходует единственное действие Action, кроме pass.']
      desc={'pass':'пропустить Action','produce':'произвести +2 товара'}
      for o in row['options']:
        if 'to' in o:desc[o['id']]=f'передать {o["amount"]} участнику {o["to"]}; тебе +{o["amount"]//5000000} влияния'
    lines.append('Допустимые действия (id: описание):')
    for o in row['options']:lines.append(f'{o["id"]}: {desc[o["id"]]}.')
    lines.append('Выбери действие, которое максимизирует твою итоговую выплату. При равной выплате можно выбрать любое из равных действий.')
    return '\n'.join(lines)
def prepare():
    assert not (ROOT/'protocol.json').exists(), 'Protocol is immutable after prepare'
    rows=json.loads((ROOT/'scenarios.json').read_text()); rng=random.Random(20261019)
    assert len(rows)==12 and len({r['id'] for r in rows})==12
    for row in rows:rng.shuffle(row['options'])
    prompts={}; schedule=[]
    for row in rows:
      for condition,(role,entity) in CONDITIONS.items():
        user=render(row,role,entity); canonical=render(row,'ROLE','ENTITY')
        # Inverse replacement must recover exactly the same prompt bytes, including every number and option order.
        assert user.replace(role,'ROLE').replace(entity,'ENTITY')==canonical
        if condition=='NN':assert not re.search('президент|фракци',SYSTEM+'\n'+user,re.I)
        key=row['id']+'_'+condition
        prompts[key]={'scenario':row['id'],'condition':condition,'messages':[{'role':'system','content':SYSTEM},{'role':'user','content':user}]}
        for repeat in range(6):schedule.append({'key':key,'repeat':repeat,'id':f'{key}_{repeat}'})
    rng.shuffle(schedule)
    dump('fixtures.json',rows);dump('prompts.json',prompts);dump('schedule.json',schedule)
    protocol={'created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'status':'frozen_before_first_experimental_response',
      'conditions':CONDITIONS,'settings':SETTINGS,'scenarios':12,'repeats_per_cell':6,'planned_requests':288,'workers':2,
      'selection':'Eight synthetic last-round Law states and four last-round Action states, validated with cloned current alashi-rules; no real match replay.',
      'agent_scope':'New common decision prompt; no Botagul persona, no persistent memory, no original agents history.',
      'primary':'PP versus NN excess disagreement: mean over scenarios of between-label action disagreement minus average within-label repeat disagreement.',
      'null_test':'10000 permutations of six PP and six NN responses within each scenario; one-sided test of excess disagreement. Fixed identical option order within scenario.',
      'secondary':'NP and PN separate lexical replacements; payout-optimal action rates and engine payoff regret. Descriptive only.',
      'failures':'Strip an optional complete JSON code fence, parse JSON strictly, validate action id. Invalid responses are failures, never pass. No retries. Stop after >=5 consecutive request errors.',
      'limits':['One model alias; backend version unspecified','Synthetic terminal states with restricted action menu and prescribed continuation','Neutral labels also have associations; no claim about archetypes or internal mechanism','Sampling independence and API alias stability cannot be established'],
      'hashes':{n:digest((ROOT/n).read_bytes()) for n in ['fixtures.json','prompts.json','schedule.json','scenarios.json','experiment.py','engine/src/main.rs']},
      'rules_hashes':{str(p.relative_to(ROOT)):digest(p.read_bytes()) for p in sorted((ROOT/'rules').rglob('*')) if p.is_file()},
      'api_reference':'https://docs.z.ai/api-reference/llm/chat-completion'}
    dump('protocol.json',protocol)
    print('Frozen:',len(schedule),'requests;',len(prompts),'prompts. All inverse-label and engine checks passed.')
def parse(content,allowed):
    content=content.strip()
    if content.startswith('```'):
      m=re.fullmatch(r'```(?:json)?\s*([\s\S]*?)\s*```',content);assert m,'Incomplete code fence';content=m.group(1)
    v=json.loads(content)
    assert isinstance(v,dict) and isinstance(v.get('action'),str) and v['action'] in allowed,'Unknown or missing action'
    assert isinstance(v.get('reason'),str),'Missing reason'
    return v

def run():
    protocol=json.loads((ROOT/'protocol.json').read_text())
    for name,h in protocol['hashes'].items():assert digest((ROOT/name).read_bytes())==h, 'Frozen input changed: '+name
    assert not (ROOT/'responses.jsonl').exists(),'Refusing to overwrite or silently resume a run'
    spec=importlib.util.spec_from_file_location('alashi_debrief',REPO/'tools/debrief.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
    key=m.llm_key();assert key,'Missing alashi API credential'
    prompts=json.loads((ROOT/'prompts.json').read_text());rows={r['id']:r for r in json.loads((ROOT/'fixtures.json').read_text())}
    jobs=json.loads((ROOT/'schedule.json').read_text())
    def request(job):
      p=prompts[job['key']];started=time.monotonic();result={**job,'scenario':p['scenario'],'condition':p['condition'],'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
      body={**protocol['settings'],'messages':p['messages']}
      req=urllib.request.Request('https://api.z.ai/api/paas/v4/chat/completions',data=json.dumps(body,ensure_ascii=False).encode(),headers={'Authorization':'Bearer '+key,'Content-Type':'application/json'},method='POST')
      try:
        with urllib.request.urlopen(req,timeout=50) as r:raw=json.load(r)
        result['raw']=raw
        try:
          result['parsed']=parse(raw['choices'][0]['message']['content'],[o['id'] for o in rows[p['scenario']]['options']]);result['status']='ok'
        except Exception as e:result.update(status='invalid',error=f'{type(e).__name__}: {e}')
      except Exception as e:result.update(status='request_error',error=f'{type(e).__name__}: {e}')
      result['elapsed_s']=round(time.monotonic()-started,3)
      return result
    # Bounded windows preserve randomized submission order and cap in-flight requests at two.
    with (ROOT/'responses.jsonl').open('x') as output, concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
      consecutive=0;done=0
      for offset in range(0,len(jobs),2):
        futures=[pool.submit(request,j) for j in jobs[offset:offset+2]]
        for f in futures:
          result=f.result();output.write(json.dumps(result,ensure_ascii=False)+'\n');output.flush();os.fsync(output.fileno());done+=1
          consecutive=consecutive+1 if result['status']=='request_error' else 0
          print(f'{done}/{len(jobs)} {result["id"]} {result["status"]} {result.get("parsed",{}).get("action","")} {result["elapsed_s"]}s',flush=True)
        if consecutive>=5:raise SystemExit('Stopped: consecutive API errors; partial data preserved')

def selfcheck():
    assert parse('```json\n{"action":"yes","reason":"x"}\n```',['yes'])['action']=='yes'
    for bad in ['{"action":"invented","reason":"x"}','prefix {"action":"yes","reason":"x"}','{"action":"yes"}']:
      try:parse(bad,['yes'])
      except (AssertionError,ValueError):pass
      else:raise AssertionError('Accepted invalid response')
    print('Parser self-check passed')
if __name__=='__main__':
    selfcheck()
    {'prepare':prepare,'run':run}[sys.argv[1]]()
