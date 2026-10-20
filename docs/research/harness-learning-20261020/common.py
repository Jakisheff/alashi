#!/usr/bin/env python3
"""Bounded full-game lexical experiment. Pure engine; no arena or transactions."""
import concurrent.futures, datetime, hashlib, json, os, random, re, secrets, shutil, subprocess, sys, threading, time, urllib.error, urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
BINARY=ROOT/'bin/evalgame'
ENDPOINT='https://api.z.ai/api/coding/paas/v4/chat/completions'
SETTINGS={'model':'glm-5.3','thinking':{'type':'enabled'},'reasoning_effort':'low','temperature':0.7,'top_p':0.95,'max_tokens':3072,'response_format':{'type':'json_object'}}
LABELS={'A0':('президент','фракция'),'A1':('президент','фракция'),'H':('президент','фракция')}
RULES='''Ты принимаешь решения в игре alashi. Цель: максимизировать собственную итоговую выплату. Деньги в игре симулированы. Тип участника: «@ENTITY@». Название роли X: «@ROLE@». Не используй инструменты. Выбирай одно действие из candidates. Копируй явное поле choice выбранного кандидата: нумерация начинается с 0. Не вычисляй номер по памяти. Возвращай JSON {"choice":целое число,"reason":"одно короткое предложение"}.
В игре три участника, шесть раундов. Каждый раунд: market, action, law. Порядок ходов вращается по раундам. У каждого свой номер; твой own_idx. Имена строк обозначают тип участника. Соперники реагируют на состояние. Их будущие решения и игровые случайности неизвестны. История в запросе содержит только уже сделанные собственные ходы и увиденные публичные состояния. Память между партиями отсутствует.
1000000 минимальных единиц = 1 alashi. В начале cash=0, goods=0, influence=1; банк включает взносы 10 alashi каждого. Начало раунда 2 и далее: cash=floor(cash/100)*62, затем promissory прибавляется к cash и обнуляется. hard не девальвирует.
market: одно действие. sell продаёт units товара; sell_credit продаёт так же, но выручка*1.25 становится promissory. После последнего раунда вексель не погашается. Цена по мере продаж: [12,10,9,8,7,6,5,4,3,3,2,2,2,1,1,1] alashi за единицу, далее 1. price_shift и boom добавляются к цене, минимум 1; налог tax_bps удерживается с выручки. buy покупает товар, двигает счётчик продаж назад; точный немедленный результат дан в кандидате.
action: produce даёт 2+subsidy_goods товара. donkey стоит 1 alashi и даёт 1 товар. shuttle даёт 3+subsidy_goods товара; серый товар может быть изъят на границе с вероятностью около 25%. При льготной границе за серый ход платится min(cash,2 alashi) в пользу текущего обладателя роли X после определения влияния в конце action. Изъятия нет у чёрного тарифа защиты. bribe переводит amount получателю to и даёт отправителю floor(amount/5000000) влияния. produce/donkey/shuttle/bribe/roof расходуют действие. pass его пропускает.
roof переводит получателю 30% cash при tariff=1 (чёрный) или 10% при tariff=2 (красный). Активная защита отменяет первый налоговый закон против самого богатого. Тариф действует и для границы: чёрный защищает, красный несёт риск около 25% потери всего товара. Эти риски не открыты заранее.
В action раунда 4 доступен аукцион лицензии. bid с amount участвует в аукционе. Наибольшая ставка побеждает, при равенстве используется tie_order. Проигравшим сумма возвращается при закрытии action; ставка победителя добавляется в банк. После bid доступно основное действие; интерфейс предлагает одну совокупную ставку за ход. Доход лицензии примерно 20..60 alashi, скрыт до итога и выплачивается сверх доли банка, в ранг богатства не входит. Чужие ставки скрыты.
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
