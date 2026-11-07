#!/usr/bin/env python3
import collections,hashlib,html,itertools,json,random,statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parent
CONDITIONS=['PP','NP','PN','NN']
def disagreements(a,b):
    n=len(a);m=len(b);assert min(n,m)>1
    ca=collections.Counter(a);cb=collections.Counter(b)
    cross=1-sum(ca[k]*cb[k] for k in ca)/(n*m)
    wa=1-sum(v*(v-1) for v in ca.values())/(n*(n-1))
    wb=1-sum(v*(v-1) for v in cb.values())/(m*(m-1))
    return {'cross':cross,'repeat_a':wa,'repeat_b':wb,'excess':cross-(wa+wb)/2}
assert disagreements(['a']*6,['b']*6)['excess']==1
assert disagreements(['a']*6,['a']*6)['excess']==0
assert disagreements(['a','b'],['a','b'])['excess']==-0.5
rows={r['id']:r for r in json.loads((ROOT/'fixtures.json').read_text())}
responses=[json.loads(line) for line in (ROOT/'responses.jsonl').read_text().splitlines()]
assert len({r['id'] for r in responses})==len(responses),'Duplicate request id'
protocol=json.loads((ROOT/'protocol.json').read_text())
assert len(responses)==protocol['planned_requests'], 'Collection incomplete'
assert [r['id'] for r in responses]==[j['id'] for j in json.loads((ROOT/'schedule.json').read_text())], 'Schedule mismatch'
for name,digest in protocol['hashes'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
by=collections.defaultdict(list)
for r in responses:
    if r['status']=='ok':by[r['scenario'],r['condition']].append(r['parsed']['action'])
summary={'planned':protocol['planned_requests'],'completed':len(responses),'status_counts':dict(collections.Counter(r['status'] for r in responses)),
 'models':dict(collections.Counter(r.get('raw',{}).get('model','missing') for r in responses)),
 'usage':{key:sum(r.get('raw',{}).get('usage',{}).get(key,0) for r in responses) for key in ['prompt_tokens','completion_tokens','total_tokens']},
 'sampling':protocol['settings'],'groups':{},'by_scenario':[]}
for cond in CONDITIONS:
    n=optimal=informative_n=informative_optimal=0;regret=0
    for (sid,c),actions in by.items():
      if c!=cond:continue
      values={o['id']:o['outcome']['payout'] for o in rows[sid]['options']};best=max(values.values());informative=len(set(values.values()))>1
      for action in actions:
        n+=1;optimal+=values[action]==best;regret+=best-values[action]
        if informative:informative_n+=1;informative_optimal+=values[action]==best
    summary['groups'][cond]={'n':n,'optimal':optimal,'informative_n':informative_n,'informative_optimal':informative_optimal,'mean_regret':regret/n if n else None}
for sid,row in rows.items():
    item={'id':sid,'phase':row['phase'],'payouts':{o['id']:o['outcome']['payout'] for o in row['options']},'choices':{c:dict(collections.Counter(by[sid,c])) for c in CONDITIONS}}
    if min(len(by[sid,'PP']),len(by[sid,'NN']))>1:item['primary']=disagreements(by[sid,'PP'],by[sid,'NN'])
    summary['by_scenario'].append(item)
usable=[sid for sid in rows if min(len(by[sid,'PP']),len(by[sid,'NN']))>1]
assert usable,'No analyzable paired scenarios'
observed=statistics.mean(disagreements(by[s,'PP'],by[s,'NN'])['excess'] for s in usable)
rng=random.Random(20261020);exceed=0
for _ in range(10000):
    values=[]
    for s in usable:
      n=len(by[s,'PP']);pooled=by[s,'PP']+by[s,'NN'];rng.shuffle(pooled)
      values.append(disagreements(pooled[:n],pooled[n:])['excess'])
    exceed+=statistics.mean(values)>=observed-1e-12
summary['primary']={'scenario_count':len(usable),'between_disagreement':statistics.mean(disagreements(by[s,'PP'],by[s,'NN'])['cross'] for s in usable),
 'repeat_PP':statistics.mean(disagreements(by[s,'PP'],by[s,'NN'])['repeat_a'] for s in usable),
 'repeat_NN':statistics.mean(disagreements(by[s,'PP'],by[s,'NN'])['repeat_b'] for s in usable),
 'excess':observed,'permutation_p_one_sided':(exceed+1)/10001,'permutations':10000}
summary['secondary_excess']={a+'_'+b:statistics.mean(disagreements(by[s,a],by[s,b])['excess'] for s in rows if min(len(by[s,a]),len(by[s,b]))>1) for a,b in [('PP','NP'),('PN','NN'),('PP','PN'),('NP','NN')]}
# Post-collection sensitivity diagnostic: bound the statistic over legal choices for missing PP/NN answers.
bounds=[]
for sid,row in rows.items():
    a=by[sid,'PP'];b=by[sid,'NN'];ma=6-len(a);mb=6-len(b);options=[o['id'] for o in row['options']]
    if len(options)**(ma+mb)>100000:
        bounds=[];break
    vals=[disagreements(a+list(fill[:ma]),b+list(fill[ma:]))['excess'] for fill in itertools.product(options,repeat=ma+mb)]
    bounds.append((min(vals),max(vals)))
summary['posthoc_missing_sensitivity']={'method':'Full 6+6 responses per scenario; enumerate every legal action assignment to missing PP/NN responses; not a preregistered endpoint.', 'excess_min':statistics.mean(x[0] for x in bounds) if bounds else None,'excess_max':statistics.mean(x[1] for x in bounds) if bounds else None}
ids=[r['raw']['id'] for r in responses if 'raw' in r and 'id' in r['raw']]
summary['duplicate_provider_response_ids']=len(ids)-len(set(ids))
summary['limits']=protocol['limits']
(ROOT/'results.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n')
# A compact local report, with the complete distributions rather than a selected anecdote.
labels={'PP':'президент / фракция','NP':'держатель X / фракция','PN':'президент / элемент','NN':'держатель X / элемент'}
p=summary['primary'];esc=html.escape
parts=['<!doctype html><html lang="ru"><meta charset="utf-8"><title>Alashi: эксперимент с названиями</title><style>body{font:17px/1.5 system-ui;margin:40px auto;max-width:1100px;padding:0 20px;color:#17262d}table{border-collapse:collapse;width:100%;font-size:14px}td,th{padding:9px;border:1px solid #ccd5d8;text-align:left}h1{font-size:28px}pre{white-space:pre-wrap;background:#f2f5f5;padding:15px}small{color:#53636a}</style>',
 '<h1>Alashi: меняется ли решение при замене названий?</h1>',
 '<p>Карта опыта: 12 состояний движка → 4 варианта названий → 6 отдельных запросов на вариант → действие → расчёт выплаты тем же движком.</p>',
 '<p>Правила, числа, цель и порядок допустимых действий совпадают. Варианты отличаются только названиями. Это отдельный агент с общим промптом, а не повтор партий исторических агентов.</p>',
 f'<p>Выполнено {summary["completed"]} из {summary["planned"]} запросов. Статусы: {esc(str(summary["status_counts"]))}. Модель: {esc(str(summary["models"]))}.</p>',
 '<p>Серия прервалась при нехватке места после отправки запросов 125 и 126. Сохранённые 124 записи проверены; два потерянных ответа отмечены storage_error. Продолжены только ещё не отправленные запросы. Изменение процесса записано в resume-amendment.json, первоначальный протокол сохранён.</p>',
 '<h2>Основное сравнение: PP и NN</h2>',
 f'<p>Расхождение действий между названиями: {p["between_disagreement"]:.1%}. При повторе PP: {p["repeat_PP"]:.1%}; при повторе NN: {p["repeat_NN"]:.1%}. Превышение над средним повторным расхождением: {100*p["excess"]:.2f} процентного пункта. Перестановочный p = {p["permutation_p_one_sided"]:.4f} (односторонний, 10000 перестановок).</p>',
 f'<p>Дополнительная проверка чувствительности: если подставить за пропуски любые допустимые действия, превышение над повторным расхождением лежит между {100*summary["posthoc_missing_sensitivity"]["excess_min"]:.2f} и {100*summary["posthoc_missing_sensitivity"]["excess_max"]:.2f} процентного пункта. Эта проверка добавлена после технических сбоев и не заменяет первичный тест.</p>',
 '<p>Действия с одинаковой выплатой могут отличаться без изменения выгоды. Ниже оптимальность рассчитана только для заданного меню и зафиксированного продолжения.</p>',
 '<table><tr><th>Названия</th><th>Допустимых ответов</th><th>Лучший вариант, все состояния</th><th>Лучший вариант, только 7 состояний с разной выгодой</th><th>Средняя потеря, alashi</th></tr>']
for c,g in summary['groups'].items():parts.append(f'<tr><td>{labels[c]}</td><td>{g["n"]}</td><td>{g["optimal"]}/{g["n"]}</td><td>{g["informative_optimal"]}/{g["informative_n"]}</td><td>{g["mean_regret"]/1000000:.3f}</td></tr>')
parts.append('</table><h2>Все состояния и ответы</h2><table><tr><th>Состояние</th><th>PP</th><th>NP</th><th>PN</th><th>NN</th><th>Выплаты, alashi</th></tr>')
for item in summary['by_scenario']:
  parts.append('<tr><td>'+item['id']+'</td>'+''.join('<td>'+esc(', '.join(f'{k}: {v}' for k,v in item['choices'][c].items()))+'</td>' for c in CONDITIONS)+'<td>'+esc(', '.join(f'{k}: {v/1000000:g}' for k,v in item['payouts'].items()))+'</td></tr>')
parts += ['</table><h2>Границы вывода</h2><p>Одна модель, синтетические состояния последнего раунда, ограниченное меню действий. Другие модели и целые партии не проверены. Длина и токенизация слов меняются вместе с названиями; нейтральные слова тоже могут нести ассоциации. Причину изменения внутри модели этот опыт не устанавливает.</p>',
 '<p>Метод зафиксирован до сбора ответов в protocol.json. Технические параметры сверены с <a href="https://docs.z.ai/api-reference/llm/chat-completion">документацией Z.AI</a>. prompts.json хранит все запросы, responses.jsonl все полученные ответы, fixtures.json содержит результаты ветвей движка, results.json полный расчёт.</p></html>']
(ROOT/'report.html').write_text('\n'.join(parts))
print(json.dumps({k:v for k,v in summary.items() if k not in ['by_scenario','limits']},ensure_ascii=False,indent=2))
