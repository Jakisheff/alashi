import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { clone } from 'three/addons/utils/SkeletonUtils.js';
import { MeshoptDecoder } from './vendor/meshoptimizer/meshopt_decoder.mjs';

const $=id=>document.getElementById(id), roles=['Рынок','Варианты','Парламент','Проверка'];
const names={idle:'Ждёт поручение',order:'Получил приказ',outbound:'Летит к задаче',work:'Работает',return:'Возвращает отчёт',handoff:'Передаёт отчёт',done:'Отчёт доставлен'};
const colors=[0xa78bfa,0x69c8d3], units=[], landmarks=[], labels=[], chiefs=[], batches=[];
let state=null, selected=0, latest=0, resetPending=false, lastFrame=performance.now(), reduced=matchMedia('(prefers-reduced-motion: reduce)').matches;
$('reduced').checked=reduced;
const renderer=new THREE.WebGLRenderer({canvas:$('world'),antialias:true,alpha:true});
renderer.setPixelRatio(1);renderer.shadowMap.enabled=false;
renderer.outputColorSpace=THREE.SRGBColorSpace;renderer.toneMapping=THREE.ACESFilmicToneMapping;renderer.toneMappingExposure=1.35;
const scene=new THREE.Scene();scene.background=new THREE.Color('#efedf4');
scene.add(new THREE.HemisphereLight(0xffffff,0xb6a5ca,3));
const sun=new THREE.DirectionalLight(0xffffff,3.4);sun.position.set(-5,15,10);sun.castShadow=true;sun.shadow.mapSize.set(1024,1024);
Object.assign(sun.shadow.camera,{left:-12,right:12,top:12,bottom:-12,near:.1,far:45});sun.shadow.bias=-.0004;scene.add(sun);
const camera=new THREE.OrthographicCamera();camera.position.set(12,17,19);camera.lookAt(0,0,0);
const mats={floor:new THREE.MeshStandardMaterial({color:0xe1dbe9,roughness:.9}),white:new THREE.MeshStandardMaterial({color:0xf9f7fc,roughness:.7}),silver:new THREE.MeshStandardMaterial({color:0xc4bdcf,metalness:.3,roughness:.5}),dark:new THREE.MeshStandardMaterial({color:0x453452,roughness:.7})};
function box(w,h,d,x,y,z,mat=mats.white,parent=scene){const mesh=new THREE.Mesh(new THREE.BoxGeometry(w,h,d),mat);mesh.position.set(x,y,z);mesh.castShadow=true;mesh.receiveShadow=true;parent.add(mesh);return mesh}
function cyl(r,h,x,y,z,mat=mats.white,parent=scene){const mesh=new THREE.Mesh(new THREE.CylinderGeometry(r,r,h,48),mat);mesh.position.set(x,y,z);mesh.castShadow=true;mesh.receiveShadow=true;parent.add(mesh);return mesh}
box(18,.25,13,0,-.18,0,mats.floor);box(18.3,.18,13.3,0,-.4,0,mats.silver);
const floorLines=new THREE.GridHelper(18,18,0xc5b8d4,0xd5cadd);floorLines.scale.z=13/18;floorLines.position.y=-.045;scene.add(floorLines);
function marker(text,position){const el=document.createElement('div');el.className='landmark';el.textContent=text;$('labels').append(el);landmarks.push({el,position:new THREE.Vector3(...position)});}
function path(points,color){const curve=new THREE.CatmullRomCurve3(points.map(p=>new THREE.Vector3(p[0],.02,p[1])));const line=new THREE.Line(new THREE.BufferGeometry().setFromPoints(curve.getPoints(50)),new THREE.LineDashedMaterial({color,dashSize:.16,gapSize:.18,transparent:true,opacity:.4}));line.computeLineDistances();scene.add(line);return line;}
for(let s=0;s<2;s++){
 const x=s?6:-6,z=s?4:-3.6,mat=new THREE.MeshStandardMaterial({color:colors[s],roughness:.6});
 cyl(1.7,.22,x,.03,z);cyl(1.45,.12,x,.2,z,mat);cyl(1.25,.14,x,.32,z);marker('CHIEF '+('АБ'[s]),[x,.1,z+1.8]);
 for(let i=0;i<3;i++)box(.16,.18,.6,x-1.1+i*.3,.18,z+1.3,mat);
}
// The owner's central market and parliament, with task terminals between the HQs.
const market=new THREE.Group();market.position.set(-1.4,0,-2.8);scene.add(market);
cyl(1.05,.25,0,.05,0,mats.white,market);cyl(.75,.2,0,.24,0,mats.silver,market);
const globe=new THREE.Mesh(new THREE.IcosahedronGeometry(.55,1),new THREE.MeshStandardMaterial({color:0xb59bd8,metalness:.25,roughness:.4,wireframe:true}));globe.position.y=1.04;market.add(globe);
cyl(.16,.5,0,.6,0,mats.white,market);marker('Рынок',[-1.4,.1,-1.25]);
const parliament=new THREE.Group();parliament.position.set(1.7,0,3.1);scene.add(parliament);
box(2,.2,1.35,0,.05,0,mats.white,parliament);box(1.9,.15,1.2,0,1.15,0,mats.white,parliament);
for(let i=0;i<4;i++)cyl(.1,.85,-.68+i*.45,.62,.3,mats.silver,parliament);
box(1.5,.6,.25,0,.5,-.35,mats.dark,parliament);marker('Парламент',[1.7,.1,4.2]);
for(const [x,z,label] of [[0,.5,'Варианты'],[0,-4.6,'Проверка']]){
 cyl(.7,.23,x,.07,z);box(.9,.65,.22,x,.6,z,mats.silver);
 box(.72,.42,.03,x,.66,z+.13,new THREE.MeshStandardMaterial({color:0xc7b4e3,emissive:0x72549c,emissiveIntensity:.35}));marker(label,[x,.1,z+.85]);
}
for(let s=0;s<2;s++)for(const [x,z] of [[-1.4,-2.8],[0,.5],[1.7,3.1]])path([[s?6:-6,s?1:-1],[s?3:-3,z*.5],[x+(s?.9:-.9),z+.7]],colors[s]);

const portraitRenderer=new THREE.WebGLRenderer({canvas:$('portrait'),antialias:true,alpha:true});portraitRenderer.setPixelRatio(Math.min(devicePixelRatio,1.5));portraitRenderer.setSize(90,112,false);portraitRenderer.outputColorSpace=THREE.SRGBColorSpace;portraitRenderer.toneMapping=THREE.ACESFilmicToneMapping;portraitRenderer.toneMappingExposure=1.2;
const portraitScene=new THREE.Scene();portraitScene.add(new THREE.HemisphereLight(0xffffff,0xa995bb,3));const pl=new THREE.DirectionalLight(0xffffff,3);pl.position.set(-3,5,6);portraitScene.add(pl);
const pcamera=new THREE.PerspectiveCamera(34,90/112,.1,30);pcamera.position.set(0,1.45,3.8);pcamera.lookAt(0,1,0);
let portrait, stageWidth=1,stageHeight=1,portraitNext=0;
const mapMaterials=new Map();
const shadowCanvas=document.createElement('canvas');shadowCanvas.width=64;shadowCanvas.height=64;
const shadowCtx=shadowCanvas.getContext('2d'),gradient=shadowCtx.createRadialGradient(32,32,0,32,32,32);gradient.addColorStop(0,'rgba(55,30,80,.22)');gradient.addColorStop(1,'rgba(55,30,80,0)');shadowCtx.fillStyle=gradient;shadowCtx.fillRect(0,0,64,64);
const shadowMat=new THREE.MeshBasicMaterial({map:new THREE.CanvasTexture(shadowCanvas),transparent:true,depthWrite:false});
const loader=new GLTFLoader().setMeshoptDecoder(MeshoptDecoder);
function character(gltf,height,parent){
 const model=clone(gltf.scene),bounds=new THREE.Box3().setFromObject(model),size=bounds.getSize(new THREE.Vector3());
 const root=new THREE.Group();root.add(model);model.scale.setScalar(height/size.y);model.position.y=-bounds.min.y*model.scale.x;
 model.traverse(o=>{if(o.isMesh){
  o.castShadow=false;o.receiveShadow=false;
  if(parent===scene&&!Array.isArray(o.material)){
   const original=o.material;
   if(!mapMaterials.has(original.uuid))mapMaterials.set(original.uuid,new THREE.MeshLambertMaterial({name:original.name,color:original.color,map:original.map,emissive:original.emissive,transparent:original.transparent,opacity:original.opacity,depthWrite:original.depthWrite,side:original.side,alphaTest:original.alphaTest}));
   o.material=mapMaterials.get(original.uuid);
  }
 }});parent.add(root);
 if(parent===scene){const shadow=new THREE.Mesh(new THREE.PlaneGeometry(height*.95,height*.7),shadowMat);shadow.rotation.x=-Math.PI/2;shadow.position.y=.01;root.add(shadow);}
 const mixer=new THREE.AnimationMixer(model),actions={};for(const c of gltf.animations)if(['idle','act','accepted'].includes(c.name))actions[c.name]=mixer.clipAction(c);
 actions.accepted.setLoop(THREE.LoopOnce,1);actions.accepted.clampWhenFinished=true;
 actions.idle.play();return {root,model,mixer,actions,clip:'idle'};
}
function setClip(u,name){if(u.clip===name)return;for(const action of Object.values(u.actions))action.stop();u.actions[name].reset().play();u.clip=name;}
function select(id){selected=id;for(let i=0;i<8;i++)$('select-'+i).setAttribute('aria-pressed',String(i===id));if(state)updateText();}
for(let s=0;s<2;s++)for(let r=0;r<4;r++){
 const id=s*4+r,button=document.createElement('button');button.id='select-'+id;button.setAttribute('aria-label',`Degenie ${'АБ'[s]}${r+1}, ${roles[r]}`);button.setAttribute('aria-pressed',String(id===0));
 const name=document.createElement('span');name.textContent=`${r+1} ${roles[r]}`;const status=document.createElement('small');status.textContent='·';button.append(name,status);button.onclick=()=>select(id);$('roster-'+(s?'b':'a')).append(button);
}
try{
 const gltf=await loader.loadAsync('./assets/desk-genie.glb');
 for(let i=0;i<8;i++){
  const c=character(gltf,1.15,scene),s=Math.floor(i/4);
  const ring=new THREE.Mesh(new THREE.TorusGeometry(.37,.025,6,40),new THREE.MeshBasicMaterial({color:colors[s]}));ring.rotation.x=-Math.PI/2;ring.position.y=.05;c.root.add(ring);c.ring=ring;
  const data=new THREE.Mesh(new THREE.BoxGeometry(.32,.2,.025),new THREE.MeshStandardMaterial({color:0xffffff,emissive:colors[s],emissiveIntensity:.45}));data.position.set(0,1.42,0);data.visible=false;c.root.add(data);c.data=data;
  c.root.position.set(s?6:-6,0,0);c.target=new THREE.Vector3();units.push(c);
  const el=document.createElement('div');el.className='unit-label';el.dataset.side=String(s);el.textContent=`${'АБ'[s]}${i%4+1}`;$('labels').append(el);labels.push(el);
 }
 for(let s=0;s<2;s++){const c=character(gltf,2.15,scene);c.root.position.set(s?6:-6,.4,s?4:-3.6);c.root.rotation.y=s?-.4:.4;chiefs.push(c);}
 // Reuse one draw per shared rigid part across all ten characters. The original
 // skeletons still animate every joint; only submission to the GPU is batched.
 const groups=new Map();
 for(const c of [...units,...chiefs])c.model.traverse(o=>{
  if(!o.isMesh||o.isSkinnedMesh||Array.isArray(o.material))return;
  const key=o.geometry.uuid+':'+o.material.uuid;
  if(!groups.has(key))groups.set(key,[]);groups.get(key).push(o);
 });
 for(const sources of groups.values()){
  const mesh=new THREE.InstancedMesh(sources[0].geometry,sources[0].material,sources.length);
  mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);mesh.frustumCulled=false;mesh.castShadow=false;mesh.receiveShadow=false;
  for(const source of sources)source.visible=false;
  scene.add(mesh);batches.push({mesh,sources});
 }
 portrait=character(gltf,1.8,portraitScene);$('loading').hidden=true;
}catch(e){$('loading').textContent='Degenie не загрузился: '+e.message;console.error(e);}

function updateText(){
 const u=state.units[selected],side=u.owner,letter='АБ'[side];
 $('unit-title').textContent=`${letter}${u.role+1} · ${roles[u.role]}`;$('unit-state').textContent=names[u.phase];
 $('mission').textContent=u.role===3?'Дождаться трёх отчётов, проверить их и вернуть решение Chief.':`Получить данные: ${roles[u.role].toLowerCase()}. Вернуть отчёт Chief ${letter}.`;
 $('report').textContent=u.phase==='done'?u.result:u.result?'Отчёт готов. Degenie доставляет его в штаб.':'Отчёт ещё не получен';
 $('cycle').textContent=`Цикл ${state.cycle}`;
 for(let s=0;s<2;s++){
  const list=state.units.filter(u=>u.owner===s),done=list.filter(u=>u.phase==='done').length;
  $('score-'+(s?'b':'a')).textContent=`${done} / 4`;
  $('chief-'+(s?'b':'a')).textContent=state.holds[s]>0?`Думает ещё ${state.holds[s].toFixed(1)} с`:state.accepted[s]?'Принял расчёт 39':done>=3?'Проверяет отчёты':'Готовит решение';
 }
 for(const u of state.units){const status=$('select-'+u.id).querySelector('small');status.textContent=u.phase==='done'?'✓':u.phase==='idle'?'·':u.phase==='work'?'●':'↗';}
 $('activity').textContent=`В работе: ${state.units.filter(u=>!['idle','done'].includes(u.phase)).length} / 8 Degenie`;
 $('events').replaceChildren(...state.events.slice(-5).reverse().map(e=>{const li=document.createElement('li'),t=document.createElement('time');t.textContent=e.t.toFixed(1)+' с';li.append(t,document.createTextNode(e.text));return li;}));
}
async function post(path){const r=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json'},body:'{}'});if(!r.ok)throw Error(`HTTP ${r.status}`);return r.json();}
async function restart(){if(resetPending)return;resetPending=true;$('restart').disabled=true;try{await post('/demo/reset');}catch(e){$('connection').textContent=e.message;}finally{resetPending=false;$('restart').disabled=false;}}
$('restart').onclick=restart;$('delay').onclick=()=>post('/demo/delay').catch(e=>{$('connection').textContent=e.message;});$('reduced').onchange=e=>{reduced=e.target.checked;};
async function poll(){
 try{const r=await fetch('/state',{cache:'no-store'});if(!r.ok)throw Error('HTTP '+r.status);state=await r.json();latest=performance.now();updateText();$('connection').textContent='Runtime подключён';$('connection').dataset.error='false';
  if($('loop').checked&&state.accepted.every(Boolean)&&state.t>15)await restart();
 }catch(e){$('connection').textContent='Нет связи с runtime';$('connection').dataset.error='true';}
 setTimeout(poll,160);
}
poll();
function resize(){const {width,height}=$('stage').getBoundingClientRect();stageWidth=width;stageHeight=height;renderer.setSize(width,height,false);const aspect=width/height;const halfHeight=Math.max(7.7,11.2/aspect);camera.left=-halfHeight*aspect;camera.right=halfHeight*aspect;camera.top=halfHeight;camera.bottom=-halfHeight;camera.near=.1;camera.far=100;camera.updateProjectionMatrix();}
new ResizeObserver(resize).observe($('stage'));resize();
const v=new THREE.Vector3(),q=new THREE.Quaternion(),up=new THREE.Vector3(0,1,0);
function labelAt(el,pos){v.copy(pos).project(camera);el.style.left=((v.x*.5+.5)*stageWidth)+'px';el.style.top=((-v.y*.5+.5)*stageHeight)+'px';}
const ray=new THREE.Raycaster(),pointer=new THREE.Vector2();
$('world').addEventListener('pointerdown',e=>{const rect=$('world').getBoundingClientRect();pointer.set((e.clientX-rect.left)/rect.width*2-1,-(e.clientY-rect.top)/rect.height*2+1);ray.setFromCamera(pointer,camera);const hits=ray.intersectObjects(units.map(u=>u.root),true);if(hits.length){let o=hits[0].object;while(o.parent&&!units.some(u=>u.root===o))o=o.parent;const id=units.findIndex(u=>u.root===o);if(id>=0)select(id);}});
let frameCount=0,fpsStart=performance.now();
function animate(now){
 const cpuStart=performance.now();
 const dt=Math.min(.05,(now-lastFrame)/1000);lastFrame=now;
 if(state){
  $('clock').textContent=(state.t+Math.min(.25,(now-latest)/1000)).toFixed(1)+' с';
  for(const u of state.units){const c=units[u.id];if(!c)continue;
   const keyPosition=['work','return'].includes(u.phase)?u.target:u.home;
   c.target.set(reduced?keyPosition[0]:u.x,0,reduced?keyPosition[1]:u.z);if(reduced)c.root.position.copy(c.target);else c.root.position.lerp(c.target,1-Math.exp(-dt*18));
   q.setFromAxisAngle(up,u.yaw);if(reduced)c.root.quaternion.copy(q);else c.root.quaternion.slerp(q,1-Math.exp(-dt*14));
   const moving=['outbound','return'].includes(u.phase);c.model.rotation.x=moving&&!reduced?.15:0;
   setClip(c,u.phase==='work'?'act':u.phase==='done'?'accepted':'idle');c.mixer.update(reduced?0:dt*(u.phase==='work'?1.1:1));
   c.data.visible=!!u.result&&u.phase!=='done';c.ring.scale.setScalar(selected===u.id?1.3:1);labels[u.id].classList.toggle('active',selected===u.id);
   labelAt(labels[u.id],c.root.position.clone().add(new THREE.Vector3(0,1.8,0)));
  }
  for(let s=0;s<chiefs.length;s++){setClip(chiefs[s],state.accepted[s]?'accepted':'idle');chiefs[s].mixer.update(reduced?0:dt);}
  if(portrait){const u=state.units[selected];setClip(portrait,u.phase==='work'?'act':u.phase==='done'?'accepted':'idle');portrait.mixer.update(reduced?0:dt);portrait.root.rotation.y=.15;}
 }
 for(const m of landmarks)labelAt(m.el,m.position);
 scene.updateMatrixWorld(true);
 for(const {mesh,sources} of batches){sources.forEach((source,i)=>mesh.setMatrixAt(i,source.matrixWorld));mesh.instanceMatrix.needsUpdate=true;}
 renderer.render(scene,camera);if(portrait&&now>=portraitNext){portraitRenderer.render(portraitScene,pcamera);portraitNext=now+100;}
 // Only static architecture casts real shadows; its map needs rendering once.
 renderer.shadowMap.autoUpdate=false;
 frameCount++;if(now-fpsStart>1000){$('world').dataset.fps=String(Math.round(frameCount*1000/(now-fpsStart)));$('world').dataset.renderedRobots=String(units.length+chiefs.length);$('world').dataset.drawCalls=String(renderer.info.render.calls);$('world').dataset.cpuMs=(performance.now()-cpuStart).toFixed(1);frameCount=0;fpsStart=now;}
 requestAnimationFrame(animate);
}
requestAnimationFrame(animate);
