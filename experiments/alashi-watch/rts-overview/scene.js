import {clamp,routePoint,unitPose,replayState} from './map-motion.js';
const $=id=>document.getElementById(id);
const ROLES=['Рынок','Анализ','Парламент','Проверка'];
const PHASES={idle:'Ждёт поручение',order:'Получил приказ',outbound:'Летит к задаче',work:'Работает',return:'Возвращает отчёт',handoff:'Передаёт отчёт',done:'Отчёт доставлен'};
const SHORT={idle:'Ожидание',order:'Приказ',outbound:'В пути',work:'Работа',return:'С отчётом',handoff:'Передача',done:'Доставлен'};
const COLORS=['#c3a0ff','#b4e9ef'];
const canvas=$('world'),ctx=canvas.getContext('2d',{alpha:true,desynchronized:true}),pc=$('portrait').getContext('2d');
const reduceQuery=matchMedia('(prefers-reduced-motion: reduce)');let reduced=reduceQuery.matches;
$('reduced').checked=reduced;
let exporting=false;
let state=null,selected=0,receivedAt=0,lastCycle=null,lastUiKey='',connected=false,resetPending=false,recording=null,lastFrame=performance.now(),sampleStart=performance.now(),frameCount=0;
let frames=[],cpuSamples=[],maxConcurrent=0,finalSince=null;
const records=[],doneAt=new Map(),acceptedAt=new Map();
const loadImage=src=>new Promise((resolve,reject)=>{const image=new Image();image.onload=()=>resolve(image);image.onerror=()=>reject(Error('Не загрузился ресурс '+src));image.src=src;});
const [layout,atlas,terrain,sprites]=await Promise.all([
 fetch('map-layout.json').then(r=>r.json()),fetch('assets/degenie-atlas.json').then(r=>r.json()),loadImage('assets/alashi-terrain.png'),loadImage('assets/degenie-atlas.png')
]).catch(error=>{$('loading').textContent=error.message;throw error;});
const W=layout.width,H=layout.height,T=layout.timing;
const clipSheets=await Promise.all(atlas.clips.map((_,clip)=>createImageBitmap(sprites,0,clip*atlas.directions*atlas.tileHeight,atlas.width,atlas.directions*atlas.tileHeight)));
$('loading').hidden=true;canvas.dataset.renderedRobots='10';
const TW=atlas.tileWidth,TH=atlas.tileHeight,F=atlas.frames,D=atlas.directions;
function fitCanvas(){const width=recording?1280:Math.min(1280,Math.round(canvas.getBoundingClientRect().width*Math.min(devicePixelRatio,1.5)));if(width>0&&canvas.width!==width){canvas.width=width;canvas.height=Math.round(width*H/W);}}
new ResizeObserver(fitCanvas).observe(canvas);fitCanvas();
let portraitNext=0,progressNext=0,lastClock='';
function stamp(text,x,y,color='#d3bddf',size=14,align='center'){
 ctx.font=`600 ${size}px -apple-system,BlinkMacSystemFont,Segoe UI,sans-serif`;ctx.textAlign=align;ctx.textBaseline='middle';const width=ctx.measureText(text).width;
 ctx.fillStyle='rgba(19,14,24,.82)';ctx.fillRect(align==='left'?x-7:x-width/2-8,y-12,width+16,24);ctx.fillStyle=color;ctx.fillText(text,x,y);
}
function line(points,color,width=2,alpha=1,dash=[]){ctx.save();ctx.globalAlpha=alpha;ctx.strokeStyle=color;ctx.lineWidth=width;ctx.setLineDash(dash);ctx.beginPath();for(let i=0;i<points.length;i++)i?ctx.lineTo(...points[i]):ctx.moveTo(...points[i]);ctx.stroke();ctx.restore();}
function ring(x,y,r,color,alpha=1){ctx.save();ctx.globalAlpha=alpha;ctx.strokeStyle=color;ctx.lineWidth=2;ctx.beginPath();ctx.ellipse(x,y,r,r*.43,0,0,Math.PI*2);ctx.stroke();ctx.restore();}
function sprite(context,x,y,height,clip=0,time=0,direction=0,tilt=0,once=false){
 const duration=atlas.durations[clip],position=once?clamp(time/duration,0,.9999)*F:((time%duration+duration)%duration)/duration*F;
 const index=Math.floor(position);
 const width=height*TW/TH;context.save();context.translate(x,y);context.rotate(tilt);
 const draw=(frame,alpha)=>{context.globalAlpha=alpha;context.drawImage(clipSheets[clip],frame*TW,direction*TH,TW,TH,-width*atlas.anchor[0],-height*atlas.anchor[1],width,height);};
 // Rig poses use the baked frame rate; map translation remains continuous.
 draw(index,1);context.restore();
}
function choose(id){selected=id;lastUiKey='';if(state)updateText();}
for(let side=0;side<2;side++)for(let role=0;role<4;role++){
 const id=side*4+role,button=document.createElement('button');button.id='select-'+id;button.setAttribute('aria-label',`Degenie ${'АБ'[side]}${role+1}, ${ROLES[role]}`);button.setAttribute('aria-pressed',String(id===0));
 const name=document.createElement('span'),tick=document.createElement('b'),phase=document.createElement('small'),progress=document.createElement('i');
 name.textContent=`${'АБ'[side]}${role+1}`;tick.textContent='·';phase.textContent=ROLES[role];button.append(name,tick,phase,progress);button.onclick=()=>choose(id);$('roster-'+(side?'b':'a')).append(button);
}
function updateText(){
 const u=state.units[selected];
 $('unit-title').textContent=`${'АБ'[u.owner]}${u.role+1} · ${ROLES[u.role]}`;$('unit-state').textContent=PHASES[u.phase];
 $('mission').textContent=u.role===3?'Сверить три доставленных отчёта. Вернуть проверенное решение Chief.':`Получить данные: ${ROLES[u.role].toLowerCase()}. Доставить результат Chief ${'АБ'[u.owner]}.`;
 $('report').textContent=u.phase==='done'?u.result:u.result?'Расчёт готов. Degenie доставляет его в штаб.':'Отчёт ещё не получен';
 $('cycle').textContent=`Цикл ${state.cycle}`;
 for(let side=0;side<2;side++){
  const done=state.units.slice(side*4,side*4+4).filter(u=>u.phase==='done').length,key=side?'b':'a';
  $('score-'+key).textContent=`${done} / 4`;
  $('chief-'+key).textContent=state.holds[side]>0?`Думает ещё ${state.holds[side].toFixed(1)} с`:state.accepted[side]?'Решение принято':done>=3?'Ждёт проверку':'Собирает данные';
 }
 for(const unit of state.units){const button=$('select-'+unit.id);button.dataset.phase=unit.phase;button.setAttribute('aria-pressed',String(unit.id===selected));button.querySelector('b').textContent=unit.phase==='done'?'✓':unit.phase==='idle'?'·':'↗';button.querySelector('small').textContent=unit.phase==='idle'?ROLES[unit.role]:SHORT[unit.phase];}
 const active=state.units.filter(u=>!['idle','done'].includes(u.phase)).length;maxConcurrent=Math.max(maxConcurrent,active);
 $('activity').textContent=state.accepted.every(Boolean)?'Оба решения приняты':`${active} Degenie выполняют поручения`;
 const eventKey=state.events.at(-1)?.event_id;
 if($('events').dataset.latest!==eventKey){
  $('events').dataset.latest=eventKey;$('events').replaceChildren(...state.events.slice(-5).reverse().map(e=>{const li=document.createElement('li'),time=document.createElement('time');time.textContent=`${e.t.toFixed(1)} С · ${e.side===null?'ОБЩИЙ ЦИКЛ':'CHIEF '+'АБ'[e.side]}`;li.append(time,document.createTextNode(e.text));return li;}));
 }
 const stage=['idle','order','outbound'].includes(u.phase)?0:u.phase==='work'?1:['return','handoff'].includes(u.phase)?2:3;
 [...$('task-flow').children].forEach((el,i)=>el.classList.toggle('active',i<=stage&&u.phase!=='idle'));
}
function setState(value){
 state=value;receivedAt=performance.now();connected=true;
 if(lastCycle!==state.cycle){lastCycle=state.cycle;lastUiKey='';doneAt.clear();acceptedAt.clear();finalSince=null;maxConcurrent=0;}
 for(const e of state.events){if(e.kind==='done')doneAt.set(e.unit,e.t);if(e.kind==='accepted')acceptedAt.set(e.side,e.t);}
 const key=JSON.stringify([state.cycle,selected,state.units.map(u=>u.phase),state.accepted,state.holds.map(h=>Math.ceil(h)),state.events.length]);
 if(key!==lastUiKey){updateText();lastUiKey=key;}
 $('connection').textContent='Runtime подключён';$('connection').dataset.error='false';
}
async function getState(){const r=await fetch('/state',{cache:'no-store'});const value=await r.json();if(!r.ok)throw Error(typeof value.error==='string'?value.error:value.error?.message||'HTTP '+r.status);return value;}
async function post(path){const r=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json'},body:'{}'});const value=await r.json();if(!r.ok)throw Error(typeof value.error==='string'?value.error:value.error?.message||'HTTP '+r.status);return value;}
async function restart(){if(resetPending)return;resetPending=true;$('restart').disabled=true;try{await post('/demo/reset');setState(await getState());frames=[];cpuSamples=[];maxConcurrent=0;}catch(error){$('connection').textContent=error.message;}finally{resetPending=false;$('restart').disabled=false;}}
$('restart').onclick=restart;$('delay').onclick=()=>post('/demo/delay').catch(error=>{$('connection').textContent=error.message;});
$('reduced').onchange=e=>{reduced=e.target.checked;};reduceQuery.addEventListener('change',e=>{reduced=e.matches;$('reduced').checked=reduced;});
$('wide').onclick=()=>{const wide=document.body.classList.toggle('wide');$('wide').textContent=wide?'Вернуть панель':'Развернуть карту';};
async function poll(){
 try{setState(await getState());if(recording?.waiting&&state.accepted.every(Boolean)){recording.waiting=false;await exportReplay(structuredClone(state));}if(!recording&&$('loop').checked&&state.accepted.every(Boolean)&&state.t>12)await restart();}
 catch(error){connected=false;$('connection').textContent='Связь потеряна · позиции остановлены';$('connection').dataset.error='true';}
 setTimeout(poll,80);
}
poll();
function download(blob,name){const url=URL.createObjectURL(blob),link=document.createElement('a');link.href=url;link.download=name;link.click();setTimeout(()=>URL.revokeObjectURL(url),60000);}
$('snapshot').onclick=()=>{const image=document.createElement('canvas');image.width=canvas.width;image.height=canvas.height;const context=image.getContext('2d');context.drawImage(terrain,0,0,image.width,image.height);context.drawImage(canvas,0,0);image.toBlob(blob=>download(blob,'alashi-map-frame.png'),'image/png');};
function measurement(){const sorted=[...frames].sort((a,b)=>a-b),cpu=[...cpuSamples].sort((a,b)=>a-b);return {fps:frames.length?1000/(frames.reduce((a,b)=>a+b,0)/frames.length):0,frame_p95_ms:sorted[Math.floor(sorted.length*.95)]||0,cpu_p95_ms:cpu[Math.floor(cpu.length*.95)]||0,samples:frames.length,max_concurrent:maxConcurrent,rendered_robots:10,reduced_motion:reduced,viewport:{width:innerWidth,height:innerHeight},canvas:{width:canvas.width,height:canvas.height}};}
$('export-evidence').onclick=()=>download(new Blob([JSON.stringify({measurement:measurement(),state,recordings:records},null,2)],{type:'application/json'}),'alashi-visual-evidence.json');
async function exportReplay(source){
 exporting=true;$('record-status').textContent='Собираю повтор из событий · 30 кадров/с…';
 let job;
 try{
  const capture=await post('/demo/export/start');job=capture.job;source=capture.source;
  canvas.width=1280;canvas.height=720;finalSince=null;
  for(let frame=0;frame<capture.frames;frame++){
   state=replayState(source,frame/30,T);paint(performance.now(),true);
   const blob=await new Promise(resolve=>canvas.toBlob(resolve,'image/jpeg',.84));
   const response=await fetch(`/demo/export/frame?job=${job}&index=${frame}`,{method:'POST',headers:{'Content-Type':'image/jpeg'},body:blob});
   if(!response.ok){const failure=await response.json();throw Error((failure.error||'frame_failed')+' · кадр '+frame);}
   $('record-status').dataset.frame=String(frame);
   if(frame%10===0)$('record-status').textContent=`Сборка повтора · ${Math.round(frame/capture.frames*100)}%`;
  }
  $('record-status').textContent='Кадры готовы · кодирование видео…';
  const result=await post('/demo/export/finish?job='+job);
  $('record-download').href=result.url;$('record-download').download='alashi-degenie-cycle.mp4';$('record-download').hidden=false;
  $('replay-player').src=result.url;$('watch').textContent='Смотреть этот повтор';$('watch').hidden=false;records.push(result);
  $('record-status').textContent=`Повтор готов · ${result.frames} кадров · 30 кадров/с`;
 }catch(error){if(job)await post('/demo/export/cancel?job='+job).catch(()=>{});$('record-status').textContent='Ошибка повтора: '+error.message;console.error(error);}
 finally{exporting=false;recording=null;$('record').disabled=false;fitCanvas();frames=[];cpuSamples=[];frameCount=0;sampleStart=performance.now();setState(await getState());}
}
$('record').onclick=async()=>{
 if(recording)return;
 if(!state?.can_export){$('record-status').textContent='Для создания нового видео запустите runtime с FFmpeg в PATH';return;}
 $('loop').checked=false;$('record').disabled=true;$('record-download').hidden=true;$('watch').hidden=true;
 try{await restart();recording={waiting:true};$('record-status').textContent='Идёт полный цикл · после приёмки будет создан повтор';}
 catch(error){recording=null;$('record').disabled=false;$('record-status').textContent=error.message;}
};
$('watch').onclick=()=>{$('replay-dialog').showModal();$('replay-player').play();};
$('close-replay').onclick=()=>{$('replay-player').pause();$('replay-dialog').close();};
const screenUnits=[];
canvas.addEventListener('pointerdown',e=>{const rect=canvas.getBoundingClientRect(),x=(e.clientX-rect.left)/rect.width*W,y=(e.clientY-rect.top)/rect.height*H;let best=55,id=-1;for(const u of screenUnits){const d=Math.hypot(u.x-x,(u.y-35)-y);if(d<best){best=d;id=u.id;}}if(id>=0)choose(id);});
document.addEventListener('keydown',e=>{if(e.target instanceof HTMLInputElement||e.target instanceof HTMLButtonElement)return;const id=Number(e.key)-1;if(id>=0&&id<8)choose(id);});
function paint(now,exportFrame=false){
 if(exporting&&!exportFrame){requestAnimationFrame(paint);return;}
 const cpuStart=performance.now(),dt=now-lastFrame;lastFrame=now;
 ctx.setTransform(canvas.width/W,0,0,canvas.height/H,0,0);ctx.globalAlpha=1;ctx.clearRect(0,0,W,H);if(exportFrame)ctx.drawImage(terrain,0,0,W,H);
 const age=!exportFrame&&connected?clamp((now-receivedAt)/1000,0,.18):0,t=state?state.t+age:0;
 screenUnits.length=0;
 if(state){
  const clock=t.toFixed(1)+' с';if(clock!==lastClock){$('clock').textContent=clock;lastClock=clock;}
  for(const unit of state.units){
   const pose=unitPose(unit,age,T,reduced),active=!['idle','done'].includes(unit.phase),color=COLORS[unit.owner];
   screenUnits.push({...pose,id:unit.id});
   if(active||unit.id===selected){
    const points=[];for(let i=0;i<=32;i++)points.push(routePoint(unit.route,i/32));line(points,color,unit.id===selected?2:1.3,active?.36:.18,[7,10]);
    if(active&&!reduced){const p=unit.phase==='return'?1-((t*.55+unit.id*.17)%1):(t*.55+unit.id*.17)%1,q=routePoint(unit.route,p),r=routePoint(unit.route,clamp(p+.025));line([q,r],color,4,.7);}
   }
   if(unit.phase==='order'){
    const chief=layout.chiefs[unit.owner];line([[chief[0],chief[1]-75],[pose.x,pose.y-25]],color,2,.85);ring(pose.x,pose.y,25+unit.phase_t*30,color,.8);
   }
   if(unit.phase==='work'){
    const progress=clamp((unit.phase_t+age)/(unit.duration-T.order-T.outbound-T.return-T.handoff));
    ctx.save();ctx.translate(pose.x,pose.y);ctx.scale(1,.44);ctx.lineWidth=4;ctx.strokeStyle=color;ctx.beginPath();ctx.arc(0,0,27,-Math.PI/2,-Math.PI/2+Math.PI*2*progress);ctx.stroke();ctx.restore();
    if(!reduced){const flicker=.55+.2*Math.sin(t*5);ctx.fillStyle=`rgba(190,165,250,${flicker})`;for(let j=0;j<3;j++)ctx.fillRect(pose.x+24,pose.y-45-j*8,12+j*4,2);}
   }
  }
  const entities=[...state.units.map(u=>({unit:u,pose:screenUnits[u.id],y:screenUnits[u.id].y,chief:false})),...layout.chiefs.map((p,s)=>({side:s,x:p[0],y:p[1],chief:true}))].sort((a,b)=>a.y-b.y);
  for(const entity of entities){
   if(entity.chief){
    const {side,x,y}=entity,accepted=state.accepted[side],elapsed=t-(acceptedAt.get(side)??t);
    ring(x,y+1,73,COLORS[side],.65);sprite(ctx,x,y,204,accepted?2:0,reduced?accepted?1.23:0:accepted?elapsed:t+side,side?7:1,0,accepted);
    stamp(accepted?'CHIEF '+'АБ'[side]+' · ПРИНЯЛ':'CHIEF '+'АБ'[side],x,y+43,COLORS[side],17);
    if(accepted&&elapsed<.7&&!reduced)ring(x,y,80+elapsed*55,COLORS[side],1-elapsed/.7);
    continue;
   }
   const {unit:u,pose:p}=entity,color=COLORS[u.owner],sel=u.id===selected;
   ctx.fillStyle='rgba(17,9,26,.32)';ctx.beginPath();ctx.ellipse(p.x,p.y+2,23,10,0,0,Math.PI*2);ctx.fill();
   ring(p.x,p.y,sel?28:22,color,sel?1:.45);
   let clip=u.phase==='work'?1:u.phase==='done'?2:0;
   const clipTime=u.phase==='work'?u.phase_t+age:u.phase==='done'?t-(doneAt.get(u.id)??t):t+u.id*.37;
   sprite(ctx,p.x,p.y,102,clip,reduced?clip===2?1.23:0:clipTime,p.direction,p.moving&&!reduced?Math.sin(p.direction*Math.PI/4)*.09:0,u.phase==='done');
   if(u.result&&u.phase!=='done'){
    const bob=reduced?0:Math.sin(t*6)*2;ctx.fillStyle=color;ctx.fillRect(p.x+23,p.y-81+bob,17,21);ctx.fillStyle='#241b32';ctx.fillRect(p.x+27,p.y-76+bob,9,2);ctx.fillRect(p.x+27,p.y-71+bob,7,2);
   }
   stamp(`${'АБ'[u.owner]}${u.role+1}${u.phase==='done'?' ✓':''}`,p.x,p.y+17,color,13);
   if(u.phase==='handoff'){const chief=layout.chiefs[u.owner];line([[p.x+26,p.y-70],[chief[0],chief[1]-65]],color,2,.7,[4,6]);}
   if(sel){ctx.strokeStyle=color;ctx.lineWidth=1.5;ctx.strokeRect(p.x-37,p.y-91,74,101);}
   const elapsed={idle:0,order:u.phase_t,outbound:T.order+u.phase_t,work:T.order+T.outbound+u.phase_t,return:u.duration-T.return-T.handoff+u.phase_t,handoff:u.duration-T.handoff+u.phase_t,done:u.duration}[u.phase];
   const progress=clamp(elapsed/u.duration);
   if(now>=progressNext)$('select-'+u.id).querySelector('i').style.width=(progress*100).toFixed(0)+'%';
  }
  for(const [text,x,y] of layout.labels)stamp(text,x,y,'#e1cbb3',12);
  progressNext=now>=progressNext?now+120:progressNext;
  const u=state.units[selected];if(now>=portraitNext){pc.clearRect(0,0,112,128);const clip=u.phase==='work'?1:u.phase==='done'?2:0;
  sprite(pc,56,118,139,clip,reduced?clip===2?1.23:0:clip===1?u.phase_t+age:clip===2?t-(doneAt.get(u.id)??t):t,0,0,clip===2);portraitNext=now+100;}
  if(state.accepted.every(Boolean)){
   finalSince??=t;ctx.fillStyle='rgba(22,15,28,.9)';ctx.fillRect(565,35,540,63);ctx.strokeStyle='#ad8acb';ctx.strokeRect(565,35,540,63);ctx.fillStyle='#ead8fa';ctx.font='600 19px system-ui';ctx.textAlign='center';ctx.fillText('ОБА ШТАБА ПРИНЯЛИ ПРОВЕРЕННЫЕ ОТЧЁТЫ',835,61);ctx.font='12px system-ui';ctx.fillStyle='#bcaeae';ctx.fillText('8 поручений завершены · баланс не изменился',835,83);
  }
  if(exportFrame){
   ctx.fillStyle='rgba(16,11,22,.88)';ctx.fillRect(0,H-39,W,39);ctx.font='600 15px system-ui';ctx.textAlign='left';ctx.fillStyle='#e4d5eb';ctx.fillText('ALASHI · ДЕМО ПОРУЧЕНИЙ',22,H-16);ctx.textAlign='right';ctx.fillText(`${t.toFixed(1)} с   |   А ${state.units.slice(0,4).filter(u=>u.phase==='done').length}/4   ·   Б ${state.units.slice(4).filter(u=>u.phase==='done').length}/4`,W-22,H-16);
  }
 }
 if(exportFrame)return;
 if(dt<150&&dt>0){frames.push(dt);cpuSamples.push(performance.now()-cpuStart);if(frames.length>900){frames.shift();cpuSamples.shift();}}
 frameCount++;
 if(now-sampleStart>1000){const fps=frameCount*1000/(now-sampleStart),m=measurement();canvas.dataset.fps=fps.toFixed(1);canvas.dataset.cpuMs=m.cpu_p95_ms.toFixed(2);canvas.dataset.measurement=JSON.stringify(m);$('performance').textContent=`${fps.toFixed(0)} кадров/с · CPU p95 ${m.cpu_p95_ms.toFixed(2)} мс · одновременно ${maxConcurrent} поручений`;frameCount=0;sampleStart=now;}
 requestAnimationFrame(paint);
}
requestAnimationFrame(paint);
