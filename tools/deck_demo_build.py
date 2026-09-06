#!/usr/bin/env python3
"""Build editable slides and an offline browser deck from one scene description."""
from pathlib import Path
import base64, html, json, shutil
from pptx import Presentation
from pptx.util import Inches, Pt
from pptx.dml.color import RGBColor
from pptx.enum.shapes import MSO_SHAPE
from pptx.enum.text import MSO_ANCHOR

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'docs/out'; OUT.mkdir(exist_ok=True)
PACKAGE=Path('/Users/amir/Desktop/Alashi Presentation');PACKAGE.mkdir(exist_ok=True)
VIDEO=Path('/Users/amir/Desktop/alashi-aitore-globe.mp4')
BG='#09131c'; PANEL='#102330'; FG='#edf3ed'; MUTED='#a9bbc5'; GOLD='#e0c68c'; CYAN='#8adad1'; LINE='#29414f'
slides=[]
def slide(title,seconds,notes,sources=''):
 s={'title':title,'seconds':seconds,'notes':notes,'sources':sources,'nodes':[]};slides.append(s);return s

def text(s,value,x,y,w,h,size=42,color=FG,bold=False,font='Jura',**kw):
 s['nodes'].append(dict(kind='text',value=value,x=x,y=y,w=w,h=h,size=size,color=color,bold=bold,font=font,**kw))
def rect(s,x,y,w,h,color=PANEL,**kw):s['nodes'].append(dict(kind='rect',x=x,y=y,w=w,h=h,color=color,**kw))
def image(s,path,x,y,w,h,kind='image'):s['nodes'].append(dict(kind=kind,path=str(path),x=x,y=y,w=w,h=h))
def common(s,kicker,title):
 text(s,kicker.upper(),100,70,1720,44,26,CYAN,True)
 text(s,title,100,145,1720,220,78,FG,True)
 rect(s,100,968,1720,2,LINE)
 text(s,'ALASHI',100,994,400,34,22,MUTED,True)
 text(s,f'{len(slides):02d}',1730,994,90,34,22,MUTED,font='Menlo')

from deck_uber_content import add_pitch
add_pitch(slide,text,rect,image,common,ROOT,FG,MUTED,GOLD,CYAN,PANEL,BG)

s=slide('Запасной: что именно показано',0,
'Запасной слайд для вопросов. Видеокадр победителя не используем как результат эксперимента. В исходной партии №21 первым по рангу был Aisultan, максимальную выплату получил Zhambyl. Aitore в ролике выбран владельцем как герой художественного финала. Внутренние единицы HTTP-арены не являются выплатами SOL. По отдельному локальному ончейн-прогону доступны транзакции.',
'docs/ops/PARALLEL_VIDEO_20260906.json; docs/parties/LIVE_GAME21_90S_REPORT.md')
common(s,'Запасной / источники демо','Два независимых источника')
for y,label,body in [(418,'ПОВЕДЕНИЕ АГЕНТОВ','HTTP-партия №21: 6 агентов, 101 попытка действия.'),(591,'ОНЧЕЙН-ПРОГОН','Локальный валидатор: 6 join, один слот, проверенные подписи.')]:
 rect(s,100,y,1720,137);text(s,label,130,y+20,1650,44,28,CYAN,True);text(s,body,130,y+75,1640,52,36,FG)
text(s,'Доступны исходные JSON и воспроизводимые проверки.',100,825,1700,80,38,MUTED)

s=slide('Запасной: обучение и экономика',0,
'Запасной слайд. Мы пока наблюдаем адаптацию стратегий в партии; обучение весов модели не измерено. Датасет можно использовать для дальнейших экспериментов. Рейк пять процентов задан механикой протокола. На раннем этапе подтверждаем полезность для команд и повторное участие, а не заявляем выручку или mainnet traction.',
'docs/NUMBERS.md; docs/QA.md; docs/research/LEARNING_CURVE.md')
common(s,'Запасной / следующие проверки','Что предстоит измерить')
for y,title,body in [(416,'Адаптация стратегии','Повторные партии и сравнение решений после изменения правил.'),(602,'Готовность возвращаться','Внешние команды, повторное участие, затем проверка оплаты.')]:
 rect(s,100,y,1720,152);text(s,title,134,y+20,1635,59,42,FG,True);text(s,body,134,y+88,1635,52,33,MUTED)
text(s,'Рейк протокола: 5%. Коммерческая модель ещё проверяется.',100,850,1720,65,37,GOLD)

# Keep sources and notes next to the deck, and in PowerPoint speaker notes.
(OUT/'deck_manifest.json').write_text(json.dumps({'main_slides':8,'demo_slide':9,'total_seconds':sum(s['seconds'] for s in slides),'slides':slides},ensure_ascii=False,indent=2)+'\n')
notes=['# Выступление Alashi','', 'План: 5 минут вместе с двухминутным видео. Речь: слайды 1-8, ровно 180 секунд по плану. Демо: слайд 9, 120 секунд. Слайды 10-11 для вопросов.','']
for i,s in enumerate(slides,1):notes += [f'## {i}. {s["title"]}',f'Время: {s["seconds"]} секунд.' if s['seconds'] else 'Запасной слайд.',s['notes'],'',f'Источники: {s["sources"]}','']
(ROOT/'docs/DEMO_TALK_20260906.md').write_text('\n'.join(notes))
md=['# Alashi: презентация демо-дня','', 'Восемь слайдов питча, отдельный слайд демо и два запасных. Суммарный целевой хронометраж: 5 минут, включая видео 2 минуты.','']
for i,s in enumerate(slides,1):
 md += [f'## {i}. {s["title"]}','']+[n['value'].replace('\n',' ') for n in s['nodes'] if n['kind']=='text']+['']
(ROOT/'docs/DECK.md').write_text('\n'.join(md))

# PPTX: actual editable text and shapes, plus embedded video on slide 4.
prs=Presentation();prs.slide_width=Inches(13.333333);prs.slide_height=Inches(7.5)
def measure(v):return Inches(v/144)
def rgb(c):return RGBColor.from_string(c.lstrip('#'))
for data in slides:
 page=prs.slides.add_slide(prs.slide_layouts[6]);page.background.fill.solid();page.background.fill.fore_color.rgb=rgb(BG)
 for n in data['nodes']:
  x,y,w,h=[measure(n[k]) for k in ['x','y','w','h']]
  if n['kind']=='rect':
   sh=page.shapes.add_shape(MSO_SHAPE.RECTANGLE,x,y,w,h);sh.fill.solid();sh.fill.fore_color.rgb=rgb(n['color']);sh.line.fill.background()
  elif n['kind']=='image':page.shapes.add_picture(n['path'],x,y,w,h)
  elif n['kind']=='video':page.shapes.add_movie(str(VIDEO),x,y,w,h,poster_frame_image=n['path'],mime_type='video/mp4')
  else:
   sh=page.shapes.add_textbox(x,y,w,h);tf=sh.text_frame;tf.margin_left=tf.margin_right=tf.margin_top=tf.margin_bottom=0;tf.word_wrap=True;tf.vertical_anchor=MSO_ANCHOR.TOP
   for j,line in enumerate(n['value'].split('\n')):
    para=tf.paragraphs[0] if j==0 else tf.add_paragraph();para.text=line;para.space_before=Pt(0);para.space_after=Pt(0);para.line_spacing=1.08
    for run in para.runs:
     run.font.name=n.get('font','Jura');run.font.size=Pt(n['size']/2);run.font.bold=n.get('bold',False);run.font.color.rgb=rgb(n['color'])
     if n.get('url'):run.hyperlink.address=n['url']
 page.notes_slide.notes_text_frame.text=data['notes']+'\n\nИсточники: '+data['sources']
prs.save(OUT/'deck.pptx')

font64=base64.b64encode((ROOT/'assets/fonts/Jura.ttf').read_bytes()).decode()
css='''@font-face{font-family:Jura;src:url(data:font/ttf;base64,FONT)}*{box-sizing:border-box}html,body{margin:0;width:100%;height:100%;overflow:hidden;background:#03080d;color:#edf3ed}.slide{position:absolute;width:1920px;height:1080px;left:0;top:0;background:#09131c;display:none;transform-origin:top left}.slide.active{display:block}.node{position:absolute;white-space:pre-wrap;margin:0;line-height:1.08}.node.text{font-family:Jura,sans-serif}.node.image{object-fit:contain}.node.video{object-fit:contain;background:#000}#controls{position:fixed;bottom:10px;right:16px;display:flex;gap:12px;align-items:center;color:#a9bbc5;font:16px Jura;background:#09131cdd;padding:8px;border-radius:6px}button{color:inherit;background:#142634;border:1px solid #29414f;border-radius:4px;padding:5px 12px;font:inherit;cursor:pointer}.capture #controls{display:none}#notes{position:fixed;left:30px;right:30px;bottom:65px;padding:24px;background:#142634f5;font:23px/1.4 Jura;display:none}a{color:inherit;text-decoration:none}@media print{@page{size:1920px 1080px;margin:0}html,body{overflow:visible;height:auto}.slide{position:relative;display:block!important;left:0!important;top:0!important;transform:none!important;page-break-after:always}#controls,#notes{display:none!important}video{display:none}img.poster{display:block!important}}'''.replace('FONT',font64)
sections=[]
for i,s in enumerate(slides):
 nodes=[]
 for n in s['nodes']:
  style=';'.join(f'{k}:{n[k]}px' for k in ['left','top','width','height'] if k in n)
  style=f'left:{n["x"]}px;top:{n["y"]}px;width:{n["w"]}px;height:{n["h"]}px;'
  if n['kind']=='rect':nodes.append(f'<div class="node" style="{style}background:{n["color"]}"></div>')
  elif n['kind'] in ['image','video']:
   dataurl='data:image/png;base64,'+base64.b64encode(Path(n['path']).read_bytes()).decode()
   if n['kind']=='video':nodes.append(f'<img class="node image poster" style="{style}display:none" src="{dataurl}"><video class="node video" style="{style}" controls preload="metadata" poster="{dataurl}" src="out/alashi-demo.mp4"></video>')
   else:nodes.append(f'<img class="node image" style="{style}" src="{dataurl}">')
  else:
   value=html.escape(n['value']);value=f'<a href="{n["url"]}" target="_blank" rel="noopener">{value}</a>' if n.get('url') else value
   nodes.append(f'<div class="node text" style="{style}font-size:{n["size"]}px;color:{n["color"]};font-weight:{700 if n.get("bold") else 500};font-family:{n.get("font","Jura")},sans-serif">{value}</div>')
 sections.append(f'<section class="slide" aria-label="{html.escape(s["title"])}" data-slide="{i+1}">'+''.join(nodes)+'</section>')
script='''const slides=[...document.querySelectorAll('.slide')],notes=NOTES;let index=Math.max(0,Math.min(slides.length-1,Number(new URLSearchParams(location.search).get('slide')||1)-1));if(location.search.includes('capture'))document.body.classList.add('capture');function fit(){const scale=Math.min(innerWidth/1920,innerHeight/1080);slides.forEach(s=>{s.style.transform=`scale(${scale})`;s.style.left=(innerWidth-1920*scale)/2+'px';s.style.top=(innerHeight-1080*scale)/2+'px'})}function show(i){document.querySelectorAll('video').forEach(v=>v.pause());index=Math.max(0,Math.min(slides.length-1,i));slides.forEach((s,j)=>s.classList.toggle('active',j===index));document.querySelector('#count').textContent=(index+1)+' / '+slides.length;document.querySelector('#notes').textContent=notes[index];fit()}document.querySelector('#next').onclick=()=>show(index+1);document.querySelector('#prev').onclick=()=>show(index-1);document.querySelector('#full').onclick=()=>document.fullscreenElement?document.exitFullscreen():document.documentElement.requestFullscreen();document.querySelector('#show-notes').onclick=()=>{const n=document.querySelector('#notes');n.style.display=n.style.display==='block'?'none':'block'};addEventListener('resize',fit);addEventListener('keydown',e=>{if(e.target.tagName==='VIDEO')return;if(['ArrowRight','PageDown'].includes(e.key)){e.preventDefault();show(index+1)}if(['ArrowLeft','PageUp'].includes(e.key)){e.preventDefault();show(index-1)}});window.Deck={show,slides};show(index);'''.replace('NOTES',json.dumps([s['notes'] for s in slides],ensure_ascii=False).replace('<','\\u003c'))
page='<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Alashi · Демонстрация</title><style>'+css+'</style><body>'+''.join(sections)+'<div id="notes"></div><nav id="controls"><button id="prev">←</button><span id="count"></span><button id="next">→</button><button id="show-notes">Заметки</button><button id="full">⛶</button></nav><script>'+script+'</script></body></html>'
(ROOT/'docs/deck.html').write_text(page)
shutil.copy2(VIDEO,OUT/'alashi-demo.mp4')
(PACKAGE/'Alashi.html').write_text(page.replace('out/alashi-demo.mp4','alashi-demo.mp4'))
shutil.copy2(VIDEO,PACKAGE/'alashi-demo.mp4');shutil.copy2(OUT/'deck.pptx',PACKAGE/'Alashi.pptx');shutil.copy2(ROOT/'docs/DEMO_TALK_20260906.md',PACKAGE/'Речь.md');shutil.copy2(ROOT/'assets/fonts/Jura.ttf',PACKAGE/'Jura.ttf')
shutil.copy2(ROOT/'docs/ops/DECK_OPEN_20260906.txt',PACKAGE/'Как открыть.txt')
print(json.dumps({'slides':len(slides),'main':8,'seconds':sum(s['seconds'] for s in slides),'pptx_bytes':(OUT/'deck.pptx').stat().st_size,'package':str(PACKAGE)},ensure_ascii=False))
