#!/usr/bin/env python3
"""Add verified interpretation to the frozen analyzer's unchanged raw HTML output."""
import html,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent

def main():
 r=json.loads((ROOT/'results.json').read_text());p=r['primary'];cf=json.loads((ROOT/'counterfactual-check.json').read_text());n=r['completed_games'];fmt=lambda x:f'{x/1000000:.3f}'
 source=ROOT/'report.generated.html'
 if not source.exists():source.write_bytes((ROOT/'report.html').read_bytes())
 page=source.read_text();old=f'<p>Завершено {n} из {r["planned_games"]} новых партий. Найденный агентом вывод:</p>'
 panel=f'''<section class="map"><p><strong>Устойчивое повышение выплаты не подтверждено.</strong> H: {fmt(r['groups']['H']['mean_payout'])} alashi; контрольные повторы: {fmt(r['groups']['A0']['mean_payout'])} и {fmt(r['groups']['A1']['mean_payout'])}.</p><p>Разница H: +{fmt(p['difference'])} alashi; 95% интервал [{fmt(p['ci95'][0])}; {fmt(p['ci95'][1])}], p = {p['p_two_sided']:.6f}. Проверены все {n} партий; единица вывода: 12 seed.</p><p>Агент сам нашёл предел чужой ставки 8 alashi и создал memory.md и helper.py. H выиграл лицензию 35 раз из 36; контроли 30 и 29 раз. Новая пара соперников сохраняет тот же предел 8, поэтому перенос против более высокой ставки не установлен.</p><p>В объяснении есть ошибки. Чужой ставки 1 alashi в старом корпусе нет. Контрпример к запрету переплаты: ставка 25 → 26 при прежних местах дала +{fmt(cf['difference'])} alashi итоговой выплаты. Взнос победителя увеличивает распределяемый банк.</p><p>Проверены исходные ответы и точное воспроизведение. Был один сетевой сбой с разрешённым повтором; сбоев партий нет. <a href="README.md">Полный отчёт с проверками и стоимостью в токенах</a>.</p></section><details><summary>Исходные утверждения и прогноз агента, включая ошибки</summary><p>Текст модели сохранён без содержательной правки:</p>'''
 assert old in page;page=page.replace(old,panel,1);marker='<p>Разница выплаты H минус';assert marker in page;page=page.replace(marker,'</details>\n'+marker,1)
 page=page.replace('</style>','table:last-of-type{display:table;table-layout:fixed}table:last-of-type th:nth-child(1){width:6%}table:last-of-type th:nth-child(2){width:13%}table:last-of-type th:nth-child(3){width:19%}table:last-of-type th:nth-child(4){width:32%}table:last-of-type th:nth-child(5){width:30%}#steps td{overflow-wrap:anywhere;vertical-align:top}</style>',1)
 (ROOT/'report.html').write_text(page);print('Verified summary added; original analyzer output and model claims preserved')
if __name__=='__main__':main()
