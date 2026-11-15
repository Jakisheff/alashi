#!/usr/bin/env python3
"""Shared renderer/parser for the raw-action evalgame bridge (current main).

Engine: arena/src/bin/evalgame.rs — flat observation, full action JSON back,
parse errors answered with a Pass fallback and recorded per decision.
"""
import datetime, hashlib, json, os, random, re, secrets, shutil, subprocess, sys, threading, time, urllib.error, urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
BINARY=ROOT/'bin/evalgame'
ENDPOINT='https://api.z.ai/api/coding/paas/v4/chat/completions'
SETTINGS={'model':'glm-5.3','thinking':{'type':'enabled'},'reasoning_effort':'low','temperature':0.7,'top_p':0.95,'max_tokens':3072,'response_format':{'type':'json_object'}}
RULES='''Ты принимаешь решения в игре alashi. Цель: максимизировать собственную итоговую выплату. Деньги симулированы. Не используй инструменты. Возвращай ровно один JSON-объект {"action":"<имя>","reason":"одно короткое предложение"}.

Протокол (поле phase запроса задаёт допустимые действия):
- market: {"action":"sell","units":N} | {"action":"buy","units":N} | {"action":"sell_credit","units":N} | {"action":"pass"}
- action: {"action":"produce"} | {"action":"donkey"} | {"action":"shuttle"} | {"action":"bribe","to":I,"amount":A} | {"action":"roof","to":I,"tariff":T} | {"action":"bid","amount":A} | {"action":"pass"}
- law: {"action":"vote_yes"} | {"action":"vote_no"} | {"action":"vote_abstain"} | {"action":"veto"} | {"action":"pass"}
Действие не из фазы или с неверной схемой отклоняется и заменяется на pass. units — целое неотрицательное; to — индекс фракции; amount — целое число минимальных единиц, взятка кратна 5000000; tariff — 1 или 2. Законное по схеме, но невозможное по игре действие (продажа без товара, взятка не кратно, ставка не в раунд 4) отклоняется, ход сгорает.

Игра: три участника, шесть раундов; каждый раунд market, action, law. 1000000 единиц = 1 alashi. Старт: cash=0, goods=0, influence=1. На границе раундов 2+ cash=floor(cash/100)*62 (hard не девальвирует), promissory прибавляется к cash и обнуляется.
market: sell продаёт units товара по лестнице цен [12,10,9,8,7,6,5,4,3,3,2,2,2,1,1,1] alashi за единицу по мере общих продаж, далее 1; price_shift и boom добавляются к цене, минимум 1; налог tax_bps удерживается с выручки. buy покупает товар, двигает счётчик продаж назад. sell_credit как sell, но выручка*1.25 становится promissory и вернётся только на границе следующего раунда; после последнего раунда не погашается.
action: produce даёт 2+subsidy_goods товара; donkey стоит 1 alashi, даёт 1 товар; shuttle даёт 3+subsidy_goods товара, серый товар может быть конфискован на границе с вероятностью около 25% (у льготной границы вместо конфискации дань min(cash,2 alashi) обладателю роли после подсчёта влияния; чёрный тариф защиты защищает). bribe переводит amount получателю to и даёт floor(amount/5000000) влияния. roof переводит 30% cash при tariff=1 или 10% при tariff=2 получателю to и гасит первый анти-богатый закон против богатейшего. В action раунда 4 доступен аукцион лицензии: {"action":"bid","amount":A}; наибольшая ставка побеждает (при равенстве tie_order), проигравшим возвращается, ставка победителя идёт в банк; доход лицензии примерно 20..60 alashi, скрыт до итога, выплачивается сверх доли.
law: голос weighs influence плюс 2, если основное действие action было пропущено. Закон проходит, если yes строго больше no. Президент (максимальное влияние на входе в law, при равенстве tie_order) может применить veto бесплатно. Карты: 0 пусто; 1,2 налог 10/20%; 3 subsidy_goods=1; 4 +1 влияния самому беднему; 5 +1 влияния самому богатому; 6 цены следующего рынка -2; 7 цены +2; 8 списывает все непогашенные promissory.
Итог: ранг по cash+hard, затем tie_order. Комиссия 5% банка; остаток по весам 50:30:15; ещё 5% банка из комиссии получает участник с максимальным влиянием (при равенстве лучше cash+hard). Держатель лицензии получает её доход сверх доли.
state запроса: round, my_idx, is_president, veto_pending, decision_stage, law_card (255 = карты нет), sold_counter, active_tax_bps, active_price_shift, active_boom, active_subsidy_goods; массивы cash, goods, influence, alive, vote_weight, voted. Твои непогашенные sell_credit в state не отображаются — отслеживай их по history. history содержит твои прежние наблюдения и действия этой партии. Соперники реагируют на состояние; их будущие решения и случайности неизвестны. Выбирай долгосрочно по итоговой выплате, не по немедленному cash.'''

def utc():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def write_json(path,value):
 tmp=path.with_suffix(path.suffix+'.tmp');data=json.dumps(value,ensure_ascii=False,indent=2,allow_nan=False)+'\n'
 with tmp.open('w') as f:f.write(data);f.flush();os.fsync(f.fileno())
 tmp.replace(path)
def observation(obs,phase):
 state=json.loads(json.dumps(obs))
 state['is_president']=(state.pop('president',None) is not None and state.get('my_wallet')==obs.get('president')) if 'my_wallet' in state else False
 state.pop('my_wallet',None);state.pop('acted_stamp',None)
 state['phase']=phase
 return state
def render(state,history,role,entity):
 data=json.loads(json.dumps(state))
 system=RULES
 user=json.dumps({'history':history,'state':data},ensure_ascii=False,separators=(',',':'))
 return [{'role':'system','content':system},{'role':'user','content':user}]
def parse(raw):
 assert raw.get('model')=='glm-5.3','Unexpected model alias'
 choice=raw['choices'][0];assert choice.get('finish_reason')=='stop','Incomplete output'
 v=json.loads(choice['message']['content'])
 assert isinstance(v,dict) and isinstance(v.get('action'),str) and v['action'],'Action object with string action required'
 return v
def selfcheck():
 state={'round':1,'my_idx':0,'cash':[0,0,0],'goods':[0,0,0],'influence':[1,1,1],'promissory':[0,0,0],'alive':[True,True,True],'vote_weight':[3,3,3],'voted':[False,False,False],'law_card':255}
 rendered=render(state,[],'президент','фракция');assert 'market' in rendered[0]['content'] and 'history' in rendered[1]['content']
 raw={'model':'glm-5.3','choices':[{'finish_reason':'stop','message':{'content':'{"action":"produce","reason":"ok"}'}}]}
 assert parse(raw)['action']=='produce'
 for bad in ['"produce"','{"units":2}','{"action":2}','{}']:
  raw['choices'][0]['message']['content']=bad
  try:parse(raw)
  except (AssertionError,ValueError):pass
  else:raise AssertionError('Invalid action accepted: '+bad)
 print('Renderer and strict action checks passed',flush=True)
