import {playbackFrame} from '../src/live/playbackFrame.ts'
import {visualStage} from '../src/live/visualTime.ts'
// Exercise the actual useFrame callbacks and GLB/rig code without a browser or GPU.
// React effect updates are applied explicitly by the harness; no GPU/browser.
import assert from 'node:assert/strict'
import {readFileSync} from 'node:fs'
import {readFile} from 'node:fs/promises'
import {runInNewContext} from 'node:vm'
import ts from 'typescript'
import { Euler, Group, PropertyBinding, Quaternion, Texture, Vector3 } from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'meshoptimizer'
import {clone} from 'three/examples/jsm/utils/SkeletonUtils.js'
import {createRig} from '../src/live/actions/rig.ts'
import {createScenarioProps} from '../src/live/actions/props.ts'
import {applyScenario,displayTime} from '../src/live/actions/motion.ts'
import {ACTION_SECONDS,clampTime,actionStageAt} from '../src/live/actions/definitions.ts'
import {BUY_COUNTER,poseTradeHands,purchaseCrateOpacity,purchaseCratePosition} from '../src/live/market/hands.ts'
import {PURCHASE_CONTACT,purchaseMotionAt,purchaseReceiptAt} from '../src/live/market/purchase.ts'
import {saleMotionAt} from '../src/live/market/sale.ts'
async function load(name) {const bytes=await readFile(new URL('../public/models/'+name,import.meta.url));const loader=new GLTFLoader().setMeshoptDecoder(MeshoptDecoder);loader.register(()=>({name:'geometry-only',loadTexture:async()=>new Texture()}));return loader.parseAsync(bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength),'')}
const heroGLB=await load('desk-genie.glb'), propsGLB=await load('experiments/market-sale-props.glb')
const parse=name=>{const url=new URL('../src/live/'+name,import.meta.url);return ts.createSourceFile(url.pathname,readFileSync(url,'utf8'),ts.ScriptTarget.ES2023,true,ts.ScriptKind.TSX)}
function query(ast,predicate){let out;function walk(n){if(predicate(n))out=n;ts.forEachChild(n,walk)}walk(ast);assert.ok(out);return out}
function callback(ast,env,prefix='') {const n=query(ast,n=>ts.isCallExpression(n)&&n.expression.getText(ast)==='useFrame').arguments[0];const src=ts.transpileModule(`${prefix}\nmodule.exports = ${n.getText(ast)}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2023}}).outputText;const m={exports:null};const ctx={module:m,...env};runInNewContext(src,ctx);return {frame:m.exports,ctx}}
const snapshot = (...roots) => {const arr=[];for(const root of roots)root.traverse(o=>{arr.push(...o.position.toArray(),...o.quaternion.toArray(),...o.scale.toArray(),o.visible?1:0)});return arr}
function actualEffect(ast,ctx,marker) {
 const node=query(ast,n=>ts.isCallExpression(n)&&n.expression.getText(ast)==='useEffect'&&n.arguments[0]?.getText(ast).includes(marker))
 const code=ts.transpileModule(`module.exports = ${node.arguments[0].getText(ast)}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2023}}).outputText
 const m={exports:null},prior=ctx.module;ctx.module=m;runInNewContext(code,ctx);ctx.module=prior;return m.exports
}
function attachEffects(ast,b) {return {...b,resetEffect:actualEffect(ast,b.ctx,'time.current = 0'),seekEffect:actualEffect(ast,b.ctx,'preview.seek !== null')}}
function scenario() {
 const hero=clone(heroGLB.scene), rig=createRig(hero,heroGLB.animations), props=createScenarioProps(), root=new Group();root.add(hero,props.root)
 const time={current:0},lastReport={current:-1},finished={current:false},firstFrame={current:true},smokeRef={current:{uniforms:{uTime:{value:0}}}};let lastShown,completed=0;const visualReports=[],actualReports=[]
 const ast=parse('actions/ScenarioAction.tsx')
 const env={preview:{action:'mule',take:1,playing:true,speed:1,entry:'bottom',seek:null},reduced:true,time,lastReport,finished,firstFrame,shownTime:{current:null},visualReported:{current:null},visualListener:{current:undefined},playbackFrame,visualStage,smokeRef,rig,props,ACTION_SECONDS,clampTime,displayTime,applyScenario:(...args)=>{lastShown=args[3];return applyScenario(...args)},onTime:t=>actualReports.push(t),onVisualTime:t=>visualReports.push(t),onFinished:()=>completed++}
 const b=attachEffects(ast,callback(ast,env));b.resetEffect();b.seekEffect();return {...b,time,lastReport,finished,firstFrame,scene:()=>snapshot(root),shown:()=>lastShown,visualReports,actualReports,completed:()=>completed,end:ACTION_SECONDS}
}
function trade() {
 const hero=clone(heroGLB.scene), propScene=propsGLB.scene.clone(true),root=new Group(),stand=propScene.getObjectByName('market-stand'),crate=propScene.getObjectByName('market-crate'),coin=new Group();root.add(hero,stand,crate,coin)
 const bones=new Map();hero.traverse(o=>{if(o.isBone)bones.set(o.name.replace(/_\d+$/,''),o)})
 const rest=Array.from(bones.values(),bone=>({bone,position:bone.position.clone(),quaternion:bone.quaternion.clone(),scale:bone.scale.clone()}))
 const idle=heroGLB.animations.find(a=>a.name==='idle')
 const tracks=idle.tracks.flatMap(track=>{const bind=PropertyBinding.parseTrackName(track.name),bone=bones.get(bind.nodeName.replace(/_\d+$/,''));return !bone||!['position','quaternion','scale'].includes(bind.propertyName)?[]:[{bone,property:bind.propertyName,sample:track.InterpolantFactoryMethodLinear()}]})
 const ast=parse('market/MarketTrade.tsx')
 const decl=name=>query(ast,n=>ts.isVariableDeclaration(n)&&n.name.getText(ast)===name)
 const fade=query(ast,n=>ts.isFunctionDeclaration(n)&&n.name?.text==='fade')
 const prefix=`const ${decl('smooth').getText(ast)}\nconst ${decl('ramp').getText(ast)}\n${fade.getText(ast)}`
 const time={current:0},reported={current:-1},finished={current:false},firstFrame={current:true},smokeRef={current:{uniforms:{uTime:{value:0}}}};let lastShown,completed=0;const visualReports=[],actualReports=[]
 const props={stand,crate,crateMaterials:[],coin,materials:[],fadeCoin:()=>{}}
 const env={preview:{action:'buy',take:1,playing:true,speed:1,entry:'bottom',seek:null},reduced:true,root:{current:root},owned:{current:props},time,reported,finished,firstFrame,shownTime:{current:null},visualReported:{current:null},visualListener:{current:undefined},playbackFrame,visualStage,smokeRef,idleLayer:{rest,tracks},bones,palmUp:new Quaternion().setFromAxisAngle(new Vector3(1,0,0),-Math.PI/2),Quaternion,Vector3,Euler,TRADE_SECONDS:6.4,PURCHASE_CONTACT,BUY_COUNTER,purchaseMotionAt,saleMotionAt,poseTradeHands:(bones,buying,t,palm)=>{lastShown=t;poseTradeHands(bones,buying,t,palm)},purchaseCrateOpacity,purchaseCratePosition,confirmation:null,receipt:{current:{}},sparkle:{current:null},onTime:t=>actualReports.push(t),onVisualTime:t=>visualReports.push(t),onFinished:()=>completed++}
 const b=attachEffects(ast,callback(ast,env,prefix));b.resetEffect();b.seekEffect();return {...b,time,reported,finished,firstFrame,scene:()=>snapshot(root),shown:()=>lastShown,visualReports,actualReports,completed:()=>completed,end:6.4}
}
function effectSeek(s,seconds){s.ctx.preview={...s.ctx.preview,playing:false,seek:seconds};s.seekEffect()}
let failures=0
for(const [name,make,representative] of [['scenario',scenario,3.65],['trade',trade,4.25]]) {
 const s=make();s.frame({},1);s.frame({},1);assert.equal(s.shown(),representative,`${name} playing reduced frame`);const before=s.scene()
 s.ctx.preview={...s.ctx.preview,playing:false};s.frame({},.1)
 assert.equal(s.visualReports.at(-1),s.shown(),`${name} visual report matches the paused pose`)
 assert.equal(s.actualReports.at(-1),1,`${name} onTime keeps the actual playback clock`)
 if(name === 'trade') assert.deepEqual(purchaseReceiptAt({source:'solana',replay:true,units:2,cost:'1000000'},s.visualReports.at(-1)),purchaseReceiptAt({source:'solana',replay:true,units:2,cost:'1000000'},s.shown()),'Confirmed purchase receipt matches the visible pose')
 else assert.equal(actionStageAt('mule',s.visualReports.at(-1)).label,actionStageAt('mule',s.shown()).label,'Scenario stage matches the visible pose')
 try {assert.deepEqual(s.scene(),before,`${name} pause must hold pose`);console.log('PASS pause',name)} catch {failures++;console.log(`RED ${name}: shown playing=${representative}, paused=${s.shown()}`)}
 effectSeek(s,0);s.frame({},.1);assert.equal(s.shown(),0,`${name} seek=0 must show frame zero`);assert.equal(s.visualReports.at(-1),0,`${name} seek reports its exact displayed frame`)
 const seekPose=s.scene();s.frame({},.1);assert.deepEqual(s.scene(),seekPose,`${name} seek=0 frame remains paused`)
 s.ctx.preview={...s.ctx.preview,playing:true,seek:null};s.frame({},.5);assert.equal(s.shown(),representative,`${name} resume uses reduced pose`)
 s.frame({},20);assert.equal(s.completed(),1,`${name} completes once`);s.frame({},20);assert.equal(s.completed(),1,`${name} completion remains once`)
 const fresh=make();effectSeek(fresh,0);fresh.frame({},.1);assert.equal(fresh.shown(),0,`${name} first load paused at seek zero`)
 fresh.ctx.preview={...fresh.ctx.preview,playing:true,seek:null};fresh.frame({},1);assert.equal(fresh.shown(),representative,`${name} playing after initial seek`)
 fresh.ctx.preview={...fresh.ctx.preview,playing:false};fresh.frame({},.1);fresh.ctx.reduced=false;fresh.frame({},.1);assert.equal(fresh.shown(),representative,`${name} reduced-off while paused preserves displayed time`)
 fresh.ctx.reduced=true;fresh.frame({},.1);assert.equal(fresh.shown(),representative,`${name} reduced-on while paused preserves displayed time`)
 fresh.ctx.preview={...fresh.ctx.preview,take:2,playing:false,seek:null};fresh.resetEffect();fresh.seekEffect();fresh.frame({},.1);assert.equal(fresh.shown(),0,`${name} new take resets frame`)
 const normal=make();normal.ctx.reduced=false;normal.frame({},.1);normal.frame({},1);const normalBefore=normal.scene();assert.equal(normal.shown(),1,`${name} normal uses elapsed time`)
 normal.ctx.preview={...normal.ctx.preview,playing:false};normal.frame({},.5);assert.deepEqual(normal.scene(),normalBefore,`${name} normal pause freezes complete scene`)
 effectSeek(normal,0);normal.frame({},.5);assert.equal(normal.shown(),0,`${name} normal zero seek`);normal.ctx.preview={...normal.ctx.preview,playing:true,seek:null};normal.frame({},.2);assert.equal(normal.shown(),.2,`${name} normal resume advances`)
 const paced=make();paced.ctx.reduced=false;paced.frame({},.01);for(let i=0;i<60;i++)paced.frame({},.01)
 assert.equal(paced.visualReports.length,1,`${name} visual callback does not update on every RAF`)
 const switched=make(),nextVisual=[];switched.frame({},.1);switched.ctx.onVisualTime=t=>nextVisual.push(t);switched.frame({},.1)
 assert.deepEqual(nextVisual,[representative],`${name} reports the current pose to a new event listener even when its stage is unchanged`)
 console.log('PASS seek zero, resume, completion, initial seek, reduced toggle, new take and normal mode',name)
}
assert.notEqual(visualStage('buy',3.649),visualStage('buy',3.65),'Purchase stage changes exactly at contact')
assert.notEqual(visualStage('buy',5.499),visualStage('buy',5.5),'Receipt stage ends exactly at 5.5')
assert.equal(purchaseReceiptAt(undefined,4.25),null,'A displayed pose never invents a missing receipt')

// Model the hook's layout-effect lifecycle to exercise event and seek identity.
const visualSource=new URL('../src/live/visualTime.ts',import.meta.url)
const visualAst=ts.createSourceFile(visualSource.pathname,readFileSync(visualSource,'utf8'),ts.ScriptTarget.ES2023,true,ts.ScriptKind.TS)
const visualHook=query(visualAst,n=>ts.isFunctionDeclaration(n)&&n.name?.text==='useVisualTime')
const slots=[],pending=[],deferred=[];let cursor=0,deferUpdates=false
const useRef=value=>slots[cursor++]??(slots[cursor-1]={current:value})
const useState=value=>{const i=cursor++;if(!(i in slots))slots[i]=value;return [slots[i],next=>{const apply=()=>{slots[i]=typeof next==='function'?next(slots[i]):next};if(deferUpdates)deferred.push(apply);else apply()}]}
const useMemo=(factory,deps)=>{const i=cursor++,old=slots[i];if(!old||deps.some((v,n)=>v!==old.deps[n]))slots[i]={deps,value:factory()};return slots[i].value}
const useLayoutEffect=(effect,deps)=>{const i=cursor++,old=slots[i];if(!old||deps.some((v,n)=>v!==old.deps[n]))pending.push(()=>{old?.cleanup?.();slots[i]={deps,cleanup:effect()}})}
const useCallback=fn=>fn
const visualModule={exports:{}}
runInNewContext(ts.transpileModule(`${visualHook.getText(visualAst)}\nmodule.exports=useVisualTime`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2023}}).outputText,{module:visualModule,exports:visualModule.exports,useMemo,useRef,useState,useLayoutEffect,useCallback})
const renderVisual=(identity,seek=null,fallback=0)=>{cursor=0;const value=visualModule.exports(identity,seek,fallback);while(pending.length)pending.shift()();return value}
let view=renderVisual('same-take:buy:event-a');const stale=view[1];stale(4.25);view=renderVisual('same-take:buy:event-a');assert.equal(view[0],4.25)
view=renderVisual('same-take:buy:event-b');assert.equal(view[0],0,'A different event cannot inherit a receipt even with the same take')
stale(4.25);view=renderVisual('same-take:buy:event-b');assert.equal(view[0],0,'A stale event callback cannot overwrite the new event')
view[1](4.25);view=renderVisual('same-take:buy:event-b');assert.equal(view[0],4.25)
const beforeSeek=view[1];view=renderVisual('same-take:buy:event-b',0,1);assert.equal(view[0],0,'A paused seek to zero is visible immediately')
beforeSeek(4.25);view=renderVisual('same-take:buy:event-b',0,1);assert.equal(view[0],0,'A stale callback cannot undo a newer seek')
view[1](0);view=renderVisual('same-take:buy:event-b',0,1);assert.equal(view[0],0)
view=renderVisual('same-take:buy:event-b',null,0);beforeSeek(4.25);view=renderVisual('same-take:buy:event-b',null,0)
assert.equal(view[0],0,'A callback from before a seek stays stale when the original key returns')
renderVisual(null);view=renderVisual('same-take:buy:event-b');assert.equal(view[0],0,'Reusing a take after the scene clears cannot inherit its previous frame')
view=renderVisual('queued-event-a');deferUpdates=true;view[1](4.25);deferUpdates=false
renderVisual('queued-event-b');deferred.shift()();view=renderVisual('queued-event-b')
assert.equal(view[0],0,'A queued update cannot overwrite an event committed later')
console.log('PASS visual time stages, missing receipt, event identity, stale callbacks and zero seek')
if(failures)process.exitCode=1
