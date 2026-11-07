#!/usr/bin/env python3
"""Completion-only recovery after the initial analysis-turn limit."""
import json,sys,time
from pathlib import Path
import common as c
from discover import validate_submission
from model_api import API
ROOT=Path(__file__).resolve().parent

def main():
 assert json.loads((ROOT/'discovery-amendment-registration.json').read_text())['push_confirmed_before_completion']
 folder=ROOT/'discovery/initial';games=json.loads((ROOT/'training.json').read_text());p=json.loads((ROOT/'protocol.json').read_text());api=API(folder/'calls',p['discovery_limits'])
 if (folder/'result.json').exists():print('Initial candidate already persisted');return
 messages=json.loads((folder/'messages.json').read_text());messages.append({'role':'user','content':'Бюджет исследования исчерпан. Сейчас оформи уже найденный тобой вывод. Следующий ответ должен быть только JSON с tool="submit" по исходной схеме. Новые calculate и read_game не выполняются. Выбери обоснованную имеющимися данными правку и укажи ограничения. Если механизма не установлено, честно обозначь гипотезу. Не придумывай новые наблюдения.'})
 for attempt in range(2):
  body={**p['discovery_settings'],'messages':messages};raw,serial=api.call(f'completion-{attempt}',body);choice=raw['choices'][0];content=choice['message'].get('content','');messages.append({'role':'assistant','content':content})
  try:
   assert raw.get('model')=='glm-5.3' and choice['finish_reason']=='stop';value=json.loads(content);validate_submission(value,games)
   candidate=ROOT/'candidate-initial';candidate.mkdir(exist_ok=False);(candidate/'memory.md').write_text(value['memory_md']+'\n');(candidate/'helper.py').write_text(value['helper_py']+'\n');c.write_json(candidate/'claim.json',{k:v for k,v in value.items() if k not in ['memory_md','helper_py']});c.write_json(folder/'completion-messages.json',messages);c.write_json(folder/'result.json',{'status':'submitted','utc':c.utc(),'candidate':candidate.name,'completion_only_amendment':True,'stats':api.stats()});print('Initial candidate submitted:',value['hypothesis'],flush=True);return
  except Exception as error:
   c.write_json(folder/f'completion-{attempt}.validation-error.json',{'error':type(error).__name__+': '+str(error)[:1500]});messages.append({'role':'user','content':'Ошибка валидации: '+type(error).__name__+': '+str(error)[:1500]+'. Исправь формат или совместимость helper с исходным контрактом. Дополнительные данные или стратегические подсказки не предоставляются. Верни только исправленный submit.'});c.write_json(folder/'completion-messages.json',messages)
 raise RuntimeError('Completion-only attempts exhausted')
if __name__=='__main__':main()
