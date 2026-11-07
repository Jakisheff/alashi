#!/usr/bin/env python3
"""Development feedback and frozen heldout seed-cluster analysis."""
import collections,hashlib,html,itertools,json,math,random,statistics,sys
from pathlib import Path
import common as c
ROOT=Path(__file__).resolve().parent

def load(p):return json.loads(p.read_text())
def mean(xs):return statistics.mean(xs) if xs else None
def percentile(xs,q):
 values=sorted(xs);position=(len(values)-1)*q;low=math.floor(position);high=math.ceil(position);return values[low]+(values[high]-values[low])*(position-low)
def exact(groups):
 observed=mean(g[2]-(g[0]+g[1])/2 for g in groups);contrasts=[[1.5*v-.5*sum(g) for v in g] for g in groups];extreme=0;total=0
 for values in itertools.product(*contrasts):
  extreme+=abs(sum(values)/len(groups))>=abs(observed)-1e-8;total+=1
 return {'difference':observed,'p_two_sided':extreme/total,'assignments':total}
def get_data(stage):
 p=load(ROOT/'protocol.json');jobs=p[stage+'_jobs'];results={};games={};traces={}
 for job in jobs:
  folder=ROOT/'runs'/stage/job['id'];result=load(folder/'result.json');assert result['id']==job['id'];results[job['id']]=result
  if result['status']=='completed':
   game=load(folder/'game.json');assert game['payouts'][job['seat']]==result['payout'];games[job['id']]=game;traces[job['id']]=[json.loads(s) for s in (folder/'trace.jsonl').read_text().splitlines()]
 return p,jobs,results,games,traces

def usage_by_arm(stage):
 result={}
 for arm in ['A0','A1','H']:
  paths=list((ROOT/'game-calls').glob(f'{stage}-*-{arm}-s*.response.json'));usages=[load(p)['raw'].get('usage',{}) for p in paths]
  result[arm]={'saved_responses':len(paths),'prompt_tokens':sum(u.get('prompt_tokens',0) for u in usages),'completion_tokens':sum(u.get('completion_tokens',0) for u in usages)}
 return result

def development():
 p,jobs,results,games,traces=get_data('dev');groups={a:[r for r in results.values() if r['status']=='completed' and r['arm']==a] for a in ['A0','H']};feedback={'status':'development_only_not_heldout','response_budget':5,'final_action_required':'Submit the final harness by the fifth response; no new heldout data are available','initial_harness':{n:(ROOT/'candidate-initial'/n).read_text() for n in ['memory.md','helper.py']},'initial_claim':load(ROOT/'candidate-initial/claim.json'),'groups':{a:{'completed':len(rs),'mean_payout':mean([r['payout'] for r in rs]),'mean_rank':mean([r['rank'] for r in rs])} for a,rs in groups.items()},'pairs':[],'failed':[r for r in results.values() if r['status']!='completed'],'game_summaries':[],'usage':usage_by_arm('dev')}
 for i in range(3):
  for seat in range(3):
   ids={a:f'dev-s{i+1}-seat{seat}-{a}' for a in ['A0','H']}
   if all(results[x]['status']=='completed' for x in ids.values()):feedback['pairs'].append({'seed_index':i,'seat':seat,'baseline_payout':results[ids['A0']]['payout'],'candidate_payout':results[ids['H']]['payout'],'difference':results[ids['H']]['payout']-results[ids['A0']]['payout']})
 for job in jobs:
  key=job['id']
  if key not in games:continue
  feedback['game_summaries'].append({'id':key,'arm':job['arm'],'subject_idx':job['seat'],'outcome':{k:games[key][k] for k in ['payouts','ranks','bank','license','final_cash','final_influence']},'steps':[{'round':t['state']['round'],'phase':t['state']['phase'],'stage':t['state']['stage'],'own':t['state']['participants'][job['seat']],'law':t['state']['law_id'],'holder':t['state']['role']['holder_idx'],'action':t['action']} for t in traces[key]]})
 feedback['mean_paired_difference']=mean([x['difference'] for x in feedback['pairs']]);c.write_json(ROOT/'dev-feedback.json',feedback);print('Development feedback saved;',len(games),'completed games; difference',feedback['mean_paired_difference'])

def metrics(trace,game,seat):
 actions=collections.Counter(t['state']['phase']+'/'+t['action']['action'] for t in trace)
 return {'license_won':bool(game['license'] and game['license']['holder_idx']==seat),'wealth_first':game['ranks'][0]==seat,'bribe_spend':sum(t['action'].get('amount',0) for t in trace if t['action']['action']=='bribe'),'bid_committed':sum(t['action'].get('amount',0) for t in trace if t['action']['action']=='bid'),'role_rounds':sum(t['state']['phase']=='law' and t['state']['stage']=='primary' and t['state']['role']['holder_idx']==seat for t in trace),'action_counts':dict(actions)}
def heldout():
 p,jobs,results,games,traces=get_data('test');s={'planned_games':len(jobs),'completed_games':len(games),'failed_games':len(jobs)-len(games),'groups':{},'seed_clusters':[],'blocks':[],'usage':usage_by_arm('test'),'preregistration':load(ROOT/'registration.json'),'freeze_registration':load(ROOT/'freeze-registration.json'),'protocol_sha256':c.sha(ROOT/'protocol.json'),'harness_freeze':load(ROOT/'harness-freeze.json'),'discovery_claim':load(ROOT/'candidate-final/claim.json')}
 detail={k:metrics(traces[k],game,results[k]['seat']) for k,game in games.items()}
 for arm in ['A0','A1','H']:
  rows=[r for r in results.values() if r['arm']==arm and r['status']=='completed'];ids=[r['id'] for r in rows]
  s['groups'][arm]={'n':len(rows),'mean_payout':mean([r['payout'] for r in rows]),'mean_net_profit':mean([r['net_profit'] for r in rows]),'mean_wealth_rank':mean([r['rank'] for r in rows]),'license_wins':sum(detail[k]['license_won'] for k in ids),'wealth_first':sum(detail[k]['wealth_first'] for k in ids),'mean_bid_committed':mean([detail[k]['bid_committed'] for k in ids]),'mean_bribe_spend':mean([detail[k]['bribe_spend'] for k in ids]),'mean_role_rounds':mean([detail[k]['role_rounds'] for k in ids])}
 cluster_means=[];deltas=[]
 for i,seed in enumerate(p['test_seeds']):
  ids=[f'test-s{i+1}-seat{seat}-{arm}' for seat in range(3) for arm in ['A0','A1','H']];complete=all(k in games for k in ids);row={'seed_index':i,'seed':seed,'opponents':['greedy','tactical'] if i<6 else ['random','genome'],'complete':complete}
  if complete:
   values=[mean([results[f'test-s{i+1}-seat{seat}-{arm}']['payout'] for seat in range(3)]) for arm in ['A0','A1','H']];delta=values[2]-(values[0]+values[1])/2;row.update(arm_means=dict(zip(['A0','A1','H'],values)),difference=delta);cluster_means.append(values);deltas.append(delta)
  s['seed_clusters'].append(row)
  for seat in range(3):
   keys=[f'test-s{i+1}-seat{seat}-{arm}' for arm in ['A0','A1','H']]
   if all(k in games for k in keys):
    base0,base1,h=[results[k]['payout'] for k in keys];s['blocks'].append({'seed_index':i,'seat':seat,'repeat_absolute_pay_gap':abs(base0-base1),'harness_absolute_pay_gap':(abs(h-base0)+abs(h-base1))/2,'difference':h-(base0+base1)/2})
 if cluster_means:
  primary=exact(cluster_means);rng=random.Random(2026102003);boot=[mean([rng.choice(deltas) for _ in deltas]) for _ in range(10000)];primary.update(ci95=[percentile(boot,.025),percentile(boot,.975)],complete_seed_clusters=len(deltas),conclusion_eligible=len(deltas)>=8,inferential_unit='seed');s['primary']=primary
 else:s['primary']={'complete_seed_clusters':0,'conclusion_eligible':False}
 s['opponent_subgroups']={kind:{'complete_clusters':len(rows),'mean_difference':mean([r['difference'] for r in rows])} for kind,rows in [('familiar',[r for r in s['seed_clusters'][:6] if r['complete']]),('novel',[r for r in s['seed_clusters'][6:] if r['complete']])]}
 s['repeat_absolute_pay_gap']=mean([r['repeat_absolute_pay_gap'] for r in s['blocks']]);s['harness_absolute_pay_gap']=mean([r['harness_absolute_pay_gap'] for r in s['blocks']]);s['collection']=load(ROOT/'test-finished.json');c.write_json(ROOT/'results.json',s);c.write_json(ROOT/'game-metrics.json',detail);report(s,jobs,results,traces)
 print(json.dumps({k:s[k] for k in ['completed_games','failed_games','groups','primary','opponent_subgroups','usage']},ensure_ascii=False,indent=2))

def report(s,jobs,results,traces):
 esc=html.escape;f=lambda x:'нет данных' if x is None else f'{x/1000000:.3f}';p=s['primary'];parts=['<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Alashi: самостоятельная находка</title><style>body{font:16px/1.5 system-ui;max-width:1200px;margin:32px auto;padding:0 18px;color:#172c2a;background:#fafcf9}h1{font-size:30px}.map{background:#e3f0e8;padding:18px;border-radius:8px}table{border-collapse:collapse;width:100%;display:block;overflow-x:auto;margin:20px 0}th,td{padding:9px;border-bottom:1px solid #c9d8d0;text-align:left;font-size:14px}th{background:#edf4ef}select{font:inherit;padding:8px}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#eff3ee;padding:16px}</style><h1>Самостоятельная находка и изменение обвязки alashi</h1><p class="map">48 старых партий → самостоятельный анализ → обвязка модели → 18 диагностических партий → фиксация → 108 новых партий. Все деньги симулированы.</p>',f'<p>Завершено {s["completed_games"]} из {s["planned_games"]} новых партий. Найденный агентом вывод:</p><pre>'+esc(s['discovery_claim']['hypothesis'])+'</pre><p>Проверяемый прогноз:</p><pre>'+esc(s['discovery_claim']['prediction'])+'</pre>']
 if 'difference' in p:parts.append(f'<p>Разница выплаты H минус среднее A0/A1: {f(p["difference"])} alashi. 95% bootstrap по seed: [{f(p["ci95"][0])}; {f(p["ci95"][1])}]. Двусторонний точный p={p["p_two_sided"]:.6f}; полных групп seed {p["complete_seed_clusters"]}. Единица вывода seed. Это результат данной выборки.</p>')
 parts.append('<table><tr><th>Вариант</th><th>Партий</th><th>Выплата, alashi</th><th>После взноса</th><th>Побед лицензии</th><th>Первое место по богатству</th></tr>')
 for arm,g in s['groups'].items():parts.append(f'<tr><td>{arm}</td><td>{g["n"]}</td><td>{f(g["mean_payout"])}</td><td>{f(g["mean_net_profit"])}</td><td>{g["license_wins"]}</td><td>{g["wealth_first"]}</td></tr>')
 parts.append('</table><p>A0 и A1: независимые повторы исходной обвязки. H: зафиксированная моделью память и helper. Модель GLM-5.3 low, одно обращение на невынужденный ход. Дополнительные признаки H увеличивают входной контекст; стоимость обучения и входные токены нужно учитывать отдельно.</p><h2>Все группы seed</h2><table><tr><th>Seed</th><th>Соперники</th><th>A0</th><th>A1</th><th>H</th><th>Разница H</th></tr>')
 for r in s['seed_clusters']:
  values=r.get('arm_means',{});parts.append('<tr><td>'+str(r['seed_index']+1)+'</td><td>'+esc('/'.join(r['opponents']))+'</td>'+''.join('<td>'+f(values.get(a))+'</td>' for a in ['A0','A1','H'])+'<td>'+f(r.get('difference'))+'</td></tr>')
 parts.append('</table><h2>Решения и добавленный контекст</h2><select id="game"></select><p id="summary"></p><table><thead><tr><th>Раунд</th><th>Фаза</th><th>Действие</th><th>Признаки обвязки</th><th>Объяснение модели</th></tr></thead><tbody id="steps"></tbody></table><h2>Границы вывода</h2><p>Одна модель и один найденный вариант обвязки. Первоначальный поиск исчерпал 12 ходов; дополнительный запрос только на оформление зафиксирован отдельно до его выполнения. Диагностические партии не входят в основной вывод. Подгруппы соперников описательны. Связь в старых данных и удачная новая партия по отдельности не доказывают причинный механизм. Сбор и проверка проходят в исследовательском Simulator, не в живой арене.</p>')
 records=[{'id':j['id'],'payout':results[j['id']]['payout'],'steps':[{'round':t['state']['round'],'phase':t['state']['phase']+' '+t['state']['stage'],'action':t['action'],'features':t['features'],'reason':t.get('reason') if t['source']!='forced' else 'Единственный допустимый кандидат'} for t in traces[j['id']]]} for j in jobs if j['id'] in traces]
 script='''const records=DATA;const select=document.getElementById('game');for(const r of records){const option=document.createElement('option');option.value=r.id;option.textContent=r.id;select.appendChild(option);}function show(){const r=records.find(x=>x.id===select.value);if(!r)return;document.getElementById('summary').textContent=`Выплата ${(r.payout/1000000).toFixed(3)} alashi`;const body=document.getElementById('steps');body.replaceChildren();for(const s of r.steps){const row=document.createElement('tr');for(const value of [s.round,s.phase,JSON.stringify(s.action),JSON.stringify(s.features),s.reason||'']){const cell=document.createElement('td');cell.textContent=value;row.appendChild(cell);}body.appendChild(row);}}select.addEventListener('change',show);show();'''.replace('DATA',json.dumps(records,ensure_ascii=False).replace('<','\\u003c'))
 parts.append('<script>'+script+'</script></html>');(ROOT/'report.html').write_text('\n'.join(parts))

if __name__=='__main__':
 assert exact([[1,1,1]]*3)=={'difference':0,'p_two_sided':1,'assignments':27}
 development() if sys.argv[1]=='dev' else heldout()
