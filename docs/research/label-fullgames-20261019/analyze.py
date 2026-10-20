#!/usr/bin/env python3
"""Analyze only the frozen completed campaign; no model or arena calls."""
import collections,hashlib,html,itertools,json,math,random,statistics,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent
ARMS=['A0','A1','N'];PESO=1000000

def load(path):return json.loads(path.read_text())
def dump(name,value):(ROOT/name).write_text(json.dumps(value,ensure_ascii=False,indent=2,allow_nan=False)+'\n')
def mean(values):return statistics.mean(values) if values else None
def percentile(xs,q):
 xs=sorted(xs);p=(len(xs)-1)*q;i=math.floor(p);j=math.ceil(p);return xs[i]+(xs[j]-xs[i])*(p-i)
def exact_test(arm_means):
 observed=mean(x[2]-(x[0]+x[1])/2 for x in arm_means)
 contrasts=[[1.5*v-0.5*sum(x) for v in x] for x in arm_means]
 permutations=[mean(values) for values in itertools.product(*contrasts)]
 return {'difference':observed,'p_two_sided':sum(abs(v)>=abs(observed)-1e-8 for v in permutations)/len(permutations),'assignments':len(permutations)}
assert exact_test([[1,1,1]]*3)=={'difference':0,'p_two_sided':1.0,'assignments':27}
assert exact_test([[1,1,4]]*4)['difference']==3

def profile(trace):
 counts=collections.Counter()
 for t in trace:
  action=t['action']['action'];phase=t['phase']
  if phase=='law' and t['stage']=='post_vote':continue
  if phase=='action' and action=='bid':continue
  counts[phase+'/'+action]+=1
 return {k:v/6 for k,v in counts.items()}
def distance(a,b):return sum(abs(a.get(k,0)-b.get(k,0)) for k in sorted(set(a)|set(b)))/6
assert distance({'market/sell':1,'action/produce':1,'law/vote_yes':1},{'market/buy':1,'action/shuttle':1,'law/vote_no':1})==1

def metrics(trace,game,seat):
 counts=collections.Counter(x['phase']+'/'+x['action']['action'] for x in trace)
 role_rounds=sum(t['phase']=='law' and t['stage']=='primary' and t['state']['role']['holder_idx']==seat for t in trace)
 return {'role_rounds':role_rounds,'accepted_choice_counts':dict(counts),'bribe_spend':sum(x['action'].get('amount',0) for x in trace if x['action']['action']=='bribe'),'bid_committed':sum(x['action'].get('amount',0) for x in trace if x['action']['action']=='bid'),'vetoes':sum(x['action']['action']=='veto' for x in trace),'license_won':bool(game.get('license') and game['license']['holder_idx']==seat),'wealth_first':game['ranks'][0]==seat,'payout_highest':game['payouts'][seat]==max(game['payouts'])}
def first_difference(a,b):
 for x,y in zip(a,b):
  if x['action']!=y['action']:
   assert x['state']==y['state'],'States diverged before the first chosen action difference'
   return {'round':x['round'],'phase':x['phase'],'stage':x['stage'],'left_action':x['action'],'right_action':y['action'],'identical_public_state':True,'left_reason':x.get('reason'),'right_reason':y.get('reason')}
 assert len(a)==len(b),'Identical prefix unexpectedly has different length'
 return None

def main():
 p=load(ROOT/'protocol.json');settings=load(ROOT/'analysis-settings.json')
 for name,h in p['hashes'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==h,name
 results={};traces={};games={}
 for job in p['jobs']:
  folder=ROOT/'cases'/job['id'];assert (folder/'result.json').exists(),'Collection incomplete: '+job['id']
  result=load(folder/'result.json');assert result['id']==job['id'];results[job['id']]=result
  if result['status']=='completed':
   traces[job['id']]=[json.loads(line) for line in (folder/'trace.jsonl').read_text().splitlines()];games[job['id']]=load(folder/'game.json')
   assert len(games[job['id']]['phases'])==18
   assert games[job['id']]['payouts'][job['seat']]==result['payout']
 profiles={k:profile(t) for k,t in traces.items()};detail={k:metrics(traces[k],games[k],results[k]['seat']) for k in traces}
 summary={'planned_games':len(p['jobs']),'completed_games':len(traces),'failed_games':sum(r['status']!='completed' for r in results.values()),'groups':{},'seed_clusters':[],'blocks':[],'preregistration':load(ROOT/'registration.json'),'protocol_sha256':hashlib.sha256((ROOT/'protocol.json').read_bytes()).hexdigest()}
 for arm in ARMS:
  xs=[r for r in results.values() if r['arm']==arm and r['status']=='completed'];summary['groups'][arm]={'n':len(xs),'mean_payout':mean([x['payout'] for x in xs]),'mean_net_profit':mean([x['net_profit'] for x in xs]),'mean_wealth_rank':mean([x['rank']+1 for x in xs]),'wealth_first_count':sum(detail[x['id']]['wealth_first'] for x in xs),'highest_payout_count':sum(detail[x['id']]['payout_highest'] for x in xs),'license_wins':sum(detail[x['id']]['license_won'] for x in xs),'mean_bribe_spend':mean([detail[x['id']]['bribe_spend'] for x in xs]),'mean_bid_committed':mean([detail[x['id']]['bid_committed'] for x in xs]),'mean_role_rounds':mean([detail[x['id']]['role_rounds'] for x in xs]),'mean_vetoes':mean([detail[x['id']]['vetoes'] for x in xs])}
 complete_means=[];complete_deltas=[]
 for seed_index,seed in enumerate(p['seeds']):
  ids=[f's{seed_index+1}-seat{seat}-{arm}' for seat in range(3) for arm in ARMS];complete=all(results[i]['status']=='completed' for i in ids)
  cluster={'seed_index':seed_index,'seed':seed,'complete':complete}
  if complete:
   values=[mean([results[f's{seed_index+1}-seat{seat}-{arm}']['payout'] for seat in range(3)]) for arm in ARMS]
   difference=values[2]-(values[0]+values[1])/2;cluster.update(arm_mean_payouts=dict(zip(ARMS,values)),difference=difference);complete_means.append(values);complete_deltas.append(difference)
  summary['seed_clusters'].append(cluster)
  for seat in range(3):
   keys=[f's{seed_index+1}-seat{seat}-{arm}' for arm in ARMS];block={'seed_index':seed_index,'seat':seat,'complete':all(k in traces for k in keys),'ids':dict(zip(ARMS,keys))}
   if block['complete']:
    a,b,n=keys;block.update(payouts={arm:results[k]['payout'] for arm,k in zip(ARMS,keys)},repeat_pay_gap=abs(results[a]['payout']-results[b]['payout']),label_pay_gap=(abs(results[n]['payout']-results[a]['payout'])+abs(results[n]['payout']-results[b]['payout']))/2,repeat_strategy_distance=distance(profiles[a],profiles[b]),label_strategy_distance=(distance(profiles[n],profiles[a])+distance(profiles[n],profiles[b]))/2,first_label_difference=first_difference(traces[a],traces[n]),first_repeat_difference=first_difference(traces[a],traces[b]))
    if block['first_label_difference'] is None:assert results[a]['payout']==results[n]['payout']
    if block['first_repeat_difference'] is None:assert results[a]['payout']==results[b]['payout']
   summary['blocks'].append(block)
 if complete_means:
  primary=exact_test(complete_means);rng=random.Random(settings['bootstrap_seed']);sampled=[mean([rng.choice(complete_deltas) for _ in complete_deltas]) for _ in range(settings['bootstrap_samples'])]
  primary.update(complete_seed_clusters=len(complete_deltas),ci95=[percentile(sampled,.025),percentile(sampled,.975)],conclusion_eligible=len(complete_deltas)>=settings['minimum_complete_seed_clusters'],sample_scope='Eight fixed seeds with all-seat paired games; inferential unit is seed, not individual moves.')
  summary['primary']=primary
 else:summary['primary']={'complete_seed_clusters':0,'conclusion_eligible':False}
 blocks=[x for x in summary['blocks'] if x['complete']]
 summary['secondary']={k:mean([x[k] for x in blocks]) for k in ['repeat_pay_gap','label_pay_gap','repeat_strategy_distance','label_strategy_distance']}
 # Frozen split: choose the largest action-count difference on discovery seeds only, then inspect its direction on heldout seeds.
 differences={}
 for seed_index in range(8):
  ids=[f's{seed_index+1}-seat{seat}-{a}' for seat in range(3) for a in ARMS]
  if not all(i in detail for i in ids):continue
  keys=set().union(*(detail[i]['accepted_choice_counts'] for i in ids));d={}
  for metric in sorted(keys):
   arm_means={a:mean([detail[f's{seed_index+1}-seat{seat}-{a}']['accepted_choice_counts'].get(metric,0) for seat in range(3)]) for a in ARMS}
   d[metric]=arm_means['N']-(arm_means['A0']+arm_means['A1'])/2
  differences[seed_index]=d
 discovery_keys=set().union(*(d.keys() for i,d in differences.items() if i<6)) if any(i<6 for i in differences) else set()
 if discovery_keys:
  ds={k:mean([d.get(k,0) for i,d in differences.items() if i<6]) for k in sorted(discovery_keys)};chosen=sorted(ds,key=lambda k:(-abs(ds[k]),k))[0]
  held=[d.get(chosen,0) for i,d in differences.items() if i>=6];hm=mean(held)
  summary['heldout_tactic']={'action_metric':chosen,'discovery_difference_per_game':ds[chosen],'heldout_difference_per_game':hm,'heldout_seed_count':len(held),'same_nonzero_direction':hm is not None and hm*ds[chosen]>0,'interpretation':'Descriptive check of a discovery-selected action count; no significance claim from two heldout seeds.'}
 ids=[];invalid=0;request_files=list((ROOT/'cases').glob('*/*.request.json'));repair_files=list((ROOT/'cases').glob('*/*.validation-error.json'))
 for path in (ROOT/'cases').glob('*/*.response.json'):
  v=load(path)['raw'];ids.append(v.get('id'))
 summary['audit']={'saved_requests':len(request_files),'saved_responses':len(ids),'format_validation_errors':len(repair_files),'network_errors':len(list((ROOT/'cases').glob('*/*.error.json'))),'duplicate_provider_response_ids':len([x for x in ids if x])-len(set(x for x in ids if x)),'main_model_aliases':sorted({load(x)['raw'].get('model','') for x in (ROOT/'cases').glob('*/*.response.json')})}
 summary['collection']=load(ROOT/'campaign-finished.json')
 dump('results.json',summary);dump('game-metrics.json',detail)
 # Only compact observations and selected actions are embedded in the local report.
 explore=[]
 for job in p['jobs']:
  if job['id'] not in traces:continue
  row={'id':job['id'],'seed_index':job['seed_index'],'seat':job['seat'],'arm':job['arm'],'payout':results[job['id']]['payout'],'rank':results[job['id']]['rank']+1,'steps':[]}
  for t in traces[job['id']]:
   own=t['state']['participants'][job['seat']];after=t['state']['candidates'][t['choice']]['after_own_action'];row['steps'].append({'round':t['round'],'phase':t['phase'],'stage':t['stage'],'role_holder':t['state']['role']['holder_idx'],'law_id':t['state']['law_id'],'source':t['source'],'action':t['action'],'cash_before':own['cash'],'cash_after_action':after['cash'],'influence_before':own['influence'],'influence_after_action':after['influence'],'reason':t.get('reason')})
  explore.append(row)
 dump('explorer-data.json',explore)
 report(summary,explore)
 print(json.dumps({k:v for k,v in summary.items() if k not in ['blocks','seed_clusters','collection']},ensure_ascii=False,indent=2))

def report(s,explore):
 esc=html.escape;primary=s['primary'];f=lambda x:'нет данных' if x is None else f'{x/PESO:.3f}'
 parts=['<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Alashi: названия в целых партиях</title><style>body{font:16px/1.5 system-ui;max-width:1200px;margin:36px auto;padding:0 18px;color:#18292c;background:#fbfcfa}h1{font-size:30px}table{border-collapse:collapse;width:100%;font-size:14px;margin:18px 0}th,td{padding:9px;border-bottom:1px solid #cdd9d4;text-align:left}th{background:#edf2ed}.map{padding:18px;background:#e8f2ec;border-radius:10px}select{font:inherit;padding:8px}small{color:#526962}.scroll{overflow-x:auto}a{color:#145e4d}</style>',
 '<h1>Названия ролей в целых партиях alashi</h1><p class="map">8 игровых seed × 3 позиции × (2 исходных запуска + 1 нейтральный) → 6 раундов → стратегия и итоговая выплата. Все деньги симулированы.</p>',
 f'<p>Завершено {s["completed_games"]} из {s["planned_games"]} партий; ошибок партий {s["failed_games"]}. Модель GLM-5.3, рассуждение low. Протокол опубликован до первых основных ответов: <a href="https://github.com/Jakisheff/alashi/commit/{s["preregistration"]["preregistration_commit"]}">{s["preregistration"]["preregistration_commit"][:7]}</a>.</p>']
 if 'difference' in primary:
  label='Убедительный эффект по первичному критерию обнаружен в этой выборке.' if primary['conclusion_eligible'] and primary['p_two_sided']<.05 else 'Убедительный эффект по первичному критерию не обнаружен в этой выборке.'
  if not primary['conclusion_eligible']:label='Полных групп недостаточно для запланированного основного вывода.'
  parts += [f'<h2>Выплаты</h2><p>{label} Средняя разница N минус A0/A1: {f(primary["difference"])} alashi. 95% bootstrap-интервал по seed: [{f(primary["ci95"][0])}; {f(primary["ci95"][1])}]. Двусторонний перестановочный p = {primary["p_two_sided"]:.4f}, полных seed {primary["complete_seed_clusters"]}, перестановок {primary["assignments"]}.</p>']
 parts += ['<table><tr><th>Вариант</th><th>Партий</th><th>Средняя выплата, alashi</th><th>Чистый результат после взноса, alashi</th><th>Побед лицензии</th><th>Первые по богатству</th></tr>']
 for a,g in s['groups'].items():parts.append(f'<tr><td>{a}</td><td>{g["n"]}</td><td>{f(g["mean_payout"])}</td><td>{f(g["mean_net_profit"])}</td><td>{g["license_wins"]}</td><td>{g["wealth_first_count"]}</td></tr>')
 parts += ['</table><p>A0 и A1: президент / фракция. N: держатель X / элемент. Ранг богатства и максимальная выплата могут различаться из-за ренты лицензии.</p>', '<h2>Стратегия и изменчивость повторов</h2>']
 q=s['secondary']
 sd=lambda x:'нет данных' if x is None else f'{x:.3f}'
 parts.append('<table><tr><th>Вариант</th><th>Передано за влияние, alashi</th><th>Совокупная ставка, alashi</th><th>Раундов с ролью X из 6</th><th>Применений veto</th></tr>')
 for a,g in s['groups'].items():parts.append(f'<tr><td>{a}</td><td>{f(g["mean_bribe_spend"])}</td><td>{f(g["mean_bid_committed"])}</td><td>{sd(g["mean_role_rounds"])}</td><td>{sd(g["mean_vetoes"])}</td></tr>')
 parts.append('</table><p>Ставка здесь означает помещённую в эскроу сумму; проигравшим она возвращается. Занятие роли X показано как дополнительное описание поведения, без отдельного теста значимости.</p>')
 parts += [f'<p>Средняя дистанция профилей действий между N и исходными вариантами: {sd(q["label_strategy_distance"])}; между A0 и A1: {sd(q["repeat_strategy_distance"])}. Ноль означает одинаковые доли действий, единица полное различие по всем фазам. Это описательные показатели. Среднее абсолютное расхождение выплат между названиями: {f(q["label_pay_gap"])} alashi; при повторе исходных названий: {f(q["repeat_pay_gap"])} alashi.</p>', '<h2>Все группы seed</h2><table><tr><th>Seed</th><th>A0</th><th>A1</th><th>N</th><th>N минус среднее исходных, alashi</th></tr>']
 for c in s['seed_clusters']:
  values=c.get('arm_mean_payouts',{});parts.append('<tr><td>'+str(c['seed_index']+1)+'</td>'+''.join('<td>'+f(values.get(a))+'</td>' for a in ARMS)+'<td>'+f(c.get('difference'))+'</td></tr>')
 parts += ['</table><h2>Просмотр решений партии</h2><p>Выберите партию. Деньги после действия показаны до закрытия фазы; будущая девальвация и расчёт закона сюда не включены.</p><select id="game"></select><p id="chosen"></p><div class="scroll"><table><thead><tr><th>Раунд и фаза</th><th>Держатель X</th><th>Карта</th><th>Действие</th><th>Деньги до / после, alashi</th><th>Влияние до / после</th><th>Краткое объяснение модели</th></tr></thead><tbody id="steps"></tbody></table></div>',
 '<h2>Границы вывода</h2><p>Сбор прерывался из-за нехватки диска после 30 партий. Продолжение по сохранённым ходам публично зафиксировано до новых ответов модели; все 72 исходных задания сохранены. Изменение серверной версии модели за календарный перерыв исключить нельзя. Подробности в recovery-amendment.md. Профиль стратегии считает типы действий; различия сумм и количества товара не входят в эту дистанцию и доступны в журнале отдельных ходов.</p><p>Один псевдоним модели, ограниченное меню evalgame и фиксированные соперники greedy/tactical. Полные партии не воспроизводят гонку сетевых задержек живой арены. Коррекции формата и все провайдерские попытки сохранены. Объяснение модели и первое различие ходов не доказывают причинность отдельного хода для итоговой выплаты. Изменения длины текста и токенизации сопровождают замену названий. Внутренний механизм ассоциаций не установлен.</p>',
 f'<p>Запросов API: {s["audit"]["saved_requests"]}, ответов: {s["audit"]["saved_responses"]}, ошибок валидации формата: {s["audit"]["format_validation_errors"]}, сетевых ошибок: {s["audit"]["network_errors"]}. Сырые записи в cases/, основной расчёт в results.json, условия в protocol.json.</p>']
 data=json.dumps(explore,ensure_ascii=False).replace('<','\\u003c')
 script='''const records=DATA;const select=document.getElementById('game');for(const r of records){const o=document.createElement('option');o.value=r.id;o.textContent=r.id;select.appendChild(o);}function show(){const r=records.find(x=>x.id===select.value);if(!r)return;document.getElementById('chosen').textContent=`Выплата ${(r.payout/1000000).toFixed(3)} alashi, место по богатству ${r.rank}.`;const body=document.getElementById('steps');body.replaceChildren();for(const s of r.steps){const tr=document.createElement('tr');for(const value of [`${s.round} ${s.phase} ${s.stage}`,s.role_holder===null?'нет':s.role_holder,s.law_id===null?'нет':s.law_id,JSON.stringify(s.action),`${s.cash_before/1000000} / ${s.cash_after_action/1000000}`,`${s.influence_before} / ${s.influence_after_action}`,s.source==='forced'?'Единственный допустимый вариант':s.reason||'']){const td=document.createElement('td');td.textContent=value;tr.appendChild(td);}body.appendChild(tr);}}select.addEventListener('change',show);show();'''.replace('DATA',data)
 parts.append('<script>'+script+'</script></html>');(ROOT/'report.html').write_text('\n'.join(parts))
if __name__=='__main__':main()
