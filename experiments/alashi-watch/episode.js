'use strict';
// Standalone staging study. Every event below is authored demo data, never live telemetry.
const episodes = [
 {duration:6,short:'Старт',phase:'Начало',type:'ЦЕЛЬ',title:'Равные ресурсы. Разные решения.',description:'По 100 денег и 4 товара. А ищет быструю продажу. Б сравнивает варианты и готовит влияние.',consequence:'Последний раунд. Выигрывает тот, у кого больше денег.',caption:'Равный старт. Один рынок.',focus:[500,280],crop:'Вся арена',routes:[],after:[100,100,4,4,2,2]},
 {duration:7,short:'Поручения',phase:'Рынок · работа роёв',type:'ПОРУЧЕНИЕ',title:'Два штаба отправляют рои.',description:'А поручает проверить продажу. Б сравнивает продажу с политическим преимуществом. Аналитики идут к данным рынка.',consequence:'У каждого исполнителя есть задача и адресат ответа.',caption:'Штаб → аналитики → расчёт.',focus:[290,186],crop:'Аналитики А',routes:[['A','AA','поручение'],['B','BA','поручение']],result:'Поручения доставлены аналитикам',after:[100,100,4,4,2,2]},
 {duration:6,short:'Ответ А',phase:'Рынок · ответ штабу',type:'РАСЧЁТ',title:'А получает готовый ответ.',description:'Четыре товара сейчас дадут 39 денег. Проверка подтверждает расчёт, штаб выбирает продажу.',consequence:'Готовый расчёт ещё не меняет баланс.',caption:'12 + 10 + 9 + 8 = 39.',focus:[150,359],crop:'Штаб А',routes:[['AA','A','ответ: 39']],after:[100,100,4,4,2,2]},
 {duration:7,short:'Продажа А',phase:'Market · 14-я секунда',type:'ДЕЙСТВИЕ',title:'А первым выходит на рынок.',description:'Команда: продать 4 товара. После исполнения продажи деньги поступают в штаб.',consequence:'Б ещё сравнивает варианты. Следующие продажи идут по более низкой цене.',caption:'Продажа А: 4 товара → 39 денег.',focus:[500,235],crop:'Общий рынок',routes:[['A','M','продать 4']],after:[139,100,0,4,2,2],result:'Продажа исполнена: А +39'},
 {duration:7,short:'Продажа Б',phase:'Market · 23-я секунда',type:'ПОСЛЕДСТВИЕ',title:'Б получает уже 22.',description:'Б тоже продаёт 4 товара. После продажи А цены ниже: 7 + 6 + 5 + 4.',consequence:'Одинаковое количество товара. Разница выручки 17 денег.',caption:'Продажа Б: 4 товара, выручка 22.',focus:[500,235],crop:'Общий рынок',routes:[['B','M','продать 4']],after:[139,122,0,0,2,2],result:'Продажа исполнена: Б +22'},
 {duration:7,short:'Влияние',phase:'Action · выбор Б',type:'СМЕНА ПЛАНА',title:'Б платит за влияние.',description:'Б отдаёт А 20 денег и получает 4 влияния. А пропускает действие: новый товар уже не успеть продать.',consequence:'Налоговая карта ещё неизвестна. Влияние не обязывает А голосовать за Б.',caption:'Б → А: 20 денег. Б: +4 влияния.',focus:[860,106],crop:'Штаб Б',routes:[['B','A','20 денег']],after:[159,102,0,0,2,6],result:'Обмен исполнен: Б получает +4 влияния'},
 {duration:6,short:'Карта',phase:'Law · карта открыта',type:'НЕИЗВЕСТНОСТЬ РАСКРЫТА',title:'Открывается налоговая карта.',description:'Теперь штабы видят закон. Б выбирает голос за, А против. В учебном примере нет вето.',consequence:'Сможет ли политическая победа изменить денежный итог?',caption:'До этой фазы штабы не знали карту.',focus:[500,410],crop:'Парламент',routes:[],after:[159,102,0,0,2,6]},
 {duration:7,short:'Голосование',phase:'Law · результат',type:'ГОЛОСОВАНИЕ',title:'Б проводит закон.',description:'Вес голосов: 6 за и 2 против. Налог принят. Используется вес по влиянию без поправки на размер капитала.',consequence:'Следующего рынка нет. Налог не меняет деньги этой партии.',caption:'Закон принят: 6 за, 2 против.',focus:[500,410],crop:'Парламент',routes:[['B','P','за: 6'],['A','P','против: 2']],after:[159,102,0,0,2,6],result:'Закон принят · 6: 2'},
 {duration:6,short:'Итог',phase:'Партия завершена',type:'РАЗВЯЗКА',title:'159: 102. Выигрывает А.',description:'Продажа раньше принесла А на 17 денег больше. Обмен влияния добавил ещё 20 к его балансу и убрал 20 у Б.',consequence:'Победа в голосовании и победа в партии разошлись.',caption:'Денежный итог: А 159 · Б 102.',focus:[150,359],crop:'Штаб А',routes:[],after:[159,102,0,0,2,6]},
 {duration:5,short:'Разбор',phase:'После партии',type:'ИСТОРИЯ ИСПОЛНИТЕЛЯ',title:'Кто сделал работу хорошо?',description:'Расчёт продажи Б верен: 22. Но штаб потратил деньги на влияние, которое уже не улучшило итог.',consequence:'Оценка расчёта относится к аналитику; выбор цели относится к штабу.',caption:'Проигрыш штаба не делает каждого субагента плохим.',focus:[730,214],crop:'Аналитики Б',routes:[],after:[159,102,0,0,2,6]}
];
const points={A:[150,359],AA:[290,186],B:[860,106],BA:[730,214],M:[500,235],P:[500,410]};
const durations=episodes.map(e=>e.duration);
const starts=episodes.map((_,i)=>durations.slice(0,i).reduce((s,v)=>s+v,0));
const total=durations.reduce((s,v)=>s+v,0);
const $=id=>document.getElementById(id);
let time=0,playing=false,last=null,selectedTeam='A',lastCompleted=-1;
const motionPreference=matchMedia('(prefers-reduced-motion: reduce)');
$('reduced').checked=motionPreference.matches;
motionPreference.addEventListener('change',event=>{$('reduced').checked=event.matches;render();});
function stateAt(at){
 const t=Math.max(0,Math.min(total,at));
 let index=starts.findIndex((start,i)=>t>=start&&t<start+durations[i]);
 if(index<0)index=episodes.length-1;
 const progress=Math.min(1,(t-starts[index])/durations[index]);
 const event=episodes[index];
 const arrived=progress>=.7||!event.routes.length;
 const state=arrived?event.after:(episodes[Math.max(0,index-1)].after);
 return {index,progress,event,arrived,state};
}
function formatTime(t){return `${Math.floor(t/60).toString().padStart(2,'0')}:${Math.floor(t%60).toString().padStart(2,'0')}`;}
function svg(tag,attrs){const el=document.createElementNS('http://www.w3.org/2000/svg',tag);for(const [key,value]of Object.entries(attrs))el.setAttribute(key,String(value));return el;}
function pathGeometry(from,to){const [x,y]=points[from],[tx,ty]=points[to];return {x,y,tx,ty,cx:(x+tx)/2,cy:Math.min(y,ty)-45};}
function renderSignals(frame){
 const group=$('signals');group.replaceChildren();
 const reduced=$('reduced').checked;
 for(const [from,to]of frame.event.routes){
 const {x,y,tx,ty,cx,cy}=pathGeometry(from,to),color=from==='B'||from==='BA'?'#d9d9e2':'#a78bfa';
 group.append(svg('path',{d:`M${x},${y} Q${cx},${cy} ${tx},${ty}`,stroke:color,class:'route'}));
 const p=Math.max(0,Math.min(1,(frame.progress-.18)/.52)),q=reduced?(frame.arrived?1:0):p*p*(3-2*p);
 const px=(1-q)*(1-q)*x+2*(1-q)*q*cx+q*q*tx,py=(1-q)*(1-q)*y+2*(1-q)*q*cy+q*q*ty;
 group.append(svg('circle',{cx:px,cy:py,r:frame.arrived?6:5,fill:color,class:'packet'}));
 }
}
const tasks={
 A:[['Аналитики','2 субагента','Сравнить продажу с ожиданием.','39 денег до продаж соперника.',2],['Экономика','4 субагента','Подготовить продажу и действие.','Продать 4 товара; затем пропустить действие.',5],['Политика','1 субагент','Оценить открытую налоговую карту.','Голос против, вес 2.',7],['Проверка','1 субагент','Сверить расчёт и исполнение.','Продажа 39; поступление 20; итог 159.',8]],
 B:[['Аналитики','3 субагента','Сравнить экономические варианты.','Выручка после продажи А: 22.',4],['Экономика','5 субагентов','Подготовить продажу и покупку влияния.','Продажа 22; передача 20 штабу А.',5],['Политика','2 субагента','Оценить закон после открытия карты.','Голос за, вес 6; закон принят.',7],['Проверка','2 субагента','Сверить исполнение и цель.','Команды допустимы. Влияние не улучшило финал.',8]]
};
function renderTasks(index){
 $('task-note').textContent=selectedTeam==='A'?'А стремится превратить запас в деньги до завершения партии.':'Б сравнивает больше вариантов и получает политическое преимущество. По итогам эпизода его полезность можно оценить отдельно.';
 $('task-list').replaceChildren();
 for(const [role,count,task,answer,readyAt]of tasks[selectedTeam]){
 const card=document.createElement('article');card.className='task';
 const countEl=document.createElement('span');countEl.textContent=count;
 const title=document.createElement('h3');title.textContent=role;
 const assignment=document.createElement('p');assignment.textContent=task;
 const status=document.createElement('p');const label=document.createElement('b');label.textContent=index>=readyAt?'Итог эпизода: ':'Текущий статус: ';
 status.append(label,document.createTextNode(index>=readyAt?answer:'результат ещё не показан.'));
 card.append(countEl,title,assignment,status);$('task-list').append(card);
 }
 document.querySelectorAll('[data-team]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.team===selectedTeam)));
}
function render(){
 const frame=stateAt(time),{event,index,progress,arrived,state}=frame;
 ['cash-a','cash-b','goods-a','goods-b','influence-a','influence-b'].forEach((id,i)=>$(id).textContent=state[i]);
 $('play').textContent=playing?'Пауза':time>=total?'Смотреть снова':time===0?'Смотреть эпизод':'Продолжить';
 $('clock').textContent=`${formatTime(time)} / ${formatTime(total)}`;$('seek').value=String(time);
 $('seek').setAttribute('aria-valuetext',`${formatTime(time)}, ${event.short}`);
 $('moment-number').textContent=`${String(index+1).padStart(2,'0')} / 10`;
 $('phase').textContent=event.phase;$('moment-type').textContent=event.type;$('moment-title').textContent=event.title;
 $('moment-description').textContent=event.description;$('consequence').textContent=event.consequence;$('stage-caption').textContent=event.caption;
 const label=$('signal-label');label.hidden=!event.routes.length;
 label.textContent=arrived?(event.result||'Ответ получен в учебном сценарии'):event.routes.map(r=>(event.routes.length>1?(r[0]==='B'?'Б: ':'А: '):'')+r[2]).join(' · ');
 const target=event.routes.length&&progress<.18?points[event.routes[0][0]]:event.focus;$('spot').setAttribute('cx',target[0]);$('spot').setAttribute('cy',target[1]);
 $('spot').setAttribute('rx',index===0?0:55);$('spot').setAttribute('ry',index===0?0:36);
 $('closeup-label').textContent=event.crop;
 const image=$('closeup-art'),crop=event.focus,box=image.parentElement;
 box.hidden=index===0;box.parentElement.classList.toggle('is-start',index===0);
 if(index!==0){
 const w=box.clientWidth,h=box.clientHeight,zoom=w<95?1.9:w<150?2.4:3;
 const scale=Math.max(w/1672,h/941)*zoom,iw=1672*scale,ih=941*scale;
 image.style.width=iw+'px';image.style.height=ih+'px';
 image.style.left=Math.min(0,Math.max(w-iw,w/2-crop[0]/1000*iw))+'px';
 image.style.top=Math.min(0,Math.max(h-ih,h/2-crop[1]/563*ih))+'px';
 }
 $('resolution').hidden=index<8;
 document.querySelectorAll('[data-chapter]').forEach(b=>b.setAttribute('aria-current',String(Number(b.dataset.chapter)===index)));
 renderSignals(frame);
 const completed=arrived?index:Math.max(0,index-1);
 if(completed!==lastCompleted){renderTasks(completed);lastCompleted=completed;}
}
function pause(){playing=false;last=null;}
function seek(t){pause();time=Math.max(0,Math.min(total,t));render();}
for(let i=0;i<episodes.length;i++){
 const b=document.createElement('button');b.dataset.chapter=i;b.setAttribute('aria-label',`Момент ${i+1}: ${episodes[i].short}`);
 const number=document.createElement('span');number.textContent=String(i+1).padStart(2,'0');b.append(number,document.createTextNode(episodes[i].short));
 b.addEventListener('click',()=>seek(starts[i]));$('chapters').append(b);
}
$('play').addEventListener('click',()=>{if(playing)pause();else{if(time>=total)time=0;playing=true;last=null;}render();});
$('restart').addEventListener('click',()=>seek(0));
$('seek').addEventListener('input',event=>seek(Number(event.target.value)));
$('reduced').addEventListener('change',render);
document.querySelectorAll('[data-team]').forEach(button=>button.addEventListener('click',()=>{selectedTeam=button.dataset.team;renderTasks(stateAt(time).arrived?stateAt(time).index:Math.max(0,stateAt(time).index-1));}));
document.querySelectorAll('[data-inspect]').forEach(button=>button.addEventListener('click',()=>{pause();selectedTeam=button.dataset.inspect;$('tasks').open=true;renderTasks(stateAt(time).arrived?stateAt(time).index:Math.max(0,stateAt(time).index-1));render();$('tasks').scrollIntoView({block:'nearest',behavior:'instant'});}));
document.addEventListener('visibilitychange',()=>{if(document.hidden){pause();render();}});
function tick(stamp){
 if(playing){if(last!==null)time=Math.min(total,time+(stamp-last)/1000);last=stamp;if(time>=total)pause();render();}
 requestAnimationFrame(tick);
}
window.addEventListener('resize',render);
render();requestAnimationFrame(tick);
