'use strict';
const $=id=>document.getElementById(id),video=$('film');
const phases=[
 {at:0,key:1.5,title:'Штаб выдаёт поручение',detail:'Degenie замечает адресата и принимает задание.'},
 {at:3.4,key:6,title:'Путь к терминалу',detail:'Тот же исполнитель отправляется за данными рынка.'},
 {at:9.3,key:11.5,title:'Проверяет цены',detail:'Degenie работает у экрана. Четыре цены появляются по очереди.'},
 {at:14,key:14.6,title:'Расчёт готов',detail:'Ответ на планшете. Штаб его ещё не получил.'},
 {at:16,key:18.5,title:'Несёт ответ штабу',detail:'Degenie возвращается с планшетом в руках.'},
 {at:22.8,key:24,title:'Передаёт планшет',detail:'Результат переходит на приёмную панель штаба.'},
 {at:24.4,key:26.3,title:'Ответ принят',detail:'Штаб получил расчёт: четыре товара можно продать за 39.'}
];
let ready=false, reduced=matchMedia('(prefers-reduced-motion: reduce)').matches;
$('reduced').checked=reduced;
function indexAt(t){let i=phases.length-1;while(i>0&&t<phases[i].at)i--;return i;}
function fmt(t){return '00:'+String(Math.floor(t||0)).padStart(2,'0')}
function paint(){
 const t=video.currentTime||0,i=indexAt(t),p=phases[i];
 $('clock').textContent=fmt(t)+' / 00:28';$('seek').value=t;
 $('phase-number').textContent=String(i+1).padStart(2,'0');$('phase-title').textContent=p.title;$('phase-detail').textContent=p.detail;
 document.querySelectorAll('[data-time]').forEach((b,j)=>{b.classList.toggle('active',j===i);b.setAttribute('aria-current',j===i?'step':'false')});
 $('play').textContent=reduced?(i===6?'Начать сначала':'Следующий этап'):video.paused?(t>=27.9?'Повторить цикл':t>.1?'Продолжить':'Отправить Degenie'):'Пауза';
 [12,10,9,8].forEach((n,j)=>$('p'+j).textContent=t>=10.5+j*.7?String(n):'·');$('total').textContent=t>=14?'39':'?';
 $('result-title').textContent=t>=24.4?'Штаб принял расчёт':t>=14?'Ответ готов у Degenie':'Штаб ждёт расчёт';
 $('result-note').textContent=t>=24.4?'Расчёт доставлен. Chief может решить, продавать ли товары.':t>=14?'Планшет у исполнителя. Приёмка штабом ещё впереди.':'Четыре товара. Ответ ещё не получен.';
 document.body.dataset.phase=String(i);document.body.dataset.accepted=String(t>=24.4);
}
function seek(t){video.pause();video.currentTime=Math.max(0,Math.min(Number.isFinite(video.duration)?video.duration:28,t));paint();}
$('play').onclick=async()=>{if(!ready)return;if(reduced){const i=indexAt(video.currentTime);seek(phases[i===6?0:i+1].key);return;}if(video.paused){if(video.currentTime>=27.9)video.currentTime=0;try{await video.play();}catch{$('loading').textContent='Нажми «Продолжить», чтобы запустить запись.';$('loading').hidden=false;}}else video.pause();paint();};
$('restart').onclick=()=>seek(reduced?phases[0].key:0);
$('seek').oninput=()=>seek(Number($('seek').value));
$('reduced').onchange=()=>{reduced=$('reduced').checked;video.pause();if(reduced)seek(phases[indexAt(video.currentTime)].key);paint();};
$('hints').onchange=()=>document.body.classList.toggle('blind',!$('hints').checked);
if(new URLSearchParams(location.search).has('blind')){$('hints').checked=false;document.body.classList.add('blind','viewer-test');}
for(const b of document.querySelectorAll('[data-time]'))b.onclick=()=>{const i=phases.findIndex(p=>p.at===Number(b.dataset.time));seek(reduced?phases[i].key:phases[i].at);};
function onReady(){ready=true;$('play').disabled=false;$('loading').hidden=true;paint();}
video.addEventListener('loadeddata',onReady);
if(video.readyState>=2)onReady();
video.querySelector('source').addEventListener('error',()=>{$('loading').textContent='Запись ещё готовится. Обнови страницу после завершения рендера.';});
for(const e of ['timeupdate','play','pause','seeked','ended'])video.addEventListener(e,paint);
document.addEventListener('visibilitychange',()=>{if(document.hidden)video.pause()});
function tick(){if(!video.paused)paint();requestAnimationFrame(tick)}paint();requestAnimationFrame(tick);
