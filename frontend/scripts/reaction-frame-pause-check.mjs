import {advanceReactionFrame, completeReaction, prepareReactionFrame, shouldReportReactionTime} from '../src/live/reactions/reactionClock.ts'
import {playbackFrame} from '../src/live/playbackFrame.ts'
import assert from 'node:assert/strict'
import {readFileSync} from 'node:fs'
import {createRequire} from 'node:module'
import {runInNewContext} from 'node:vm'
import ts from 'typescript'
import {REACTION_SECONDS, REACTION_REDUCED_FRAME} from '../src/live/reactions/definitions.ts'
assert.equal(playbackFrame(1, 1, 0, false, false), 0, 'A saved frame zero is still a frame when paused')
const url = new URL('../src/live/reactions/LivingGenie.tsx', import.meta.url)
const ast = ts.createSourceFile(url.pathname, readFileSync(url, 'utf8'), ts.ScriptTarget.ES2023, true, ts.ScriptKind.TSX)
let component
function visit(n) {
  if (ts.isFunctionDeclaration(n) && n.name?.text === 'LivingGenie') component = n
  ts.forEachChild(n, visit)
}
visit(ast); assert.ok(component)
const code = ts.transpileModule(`${component.getText(ast)}\nmodule.exports = LivingGenie`, {compilerOptions:{module:ts.ModuleKind.CommonJS, jsx:ts.JsxEmit.ReactJSX,target:ts.ScriptTarget.ES2023}}).outputText
let reduced = false, cursor = 0, frame, lastPose = null, smoke = {material:{}, uniforms:{uTime:{value:0}}}, finished = 0, reported = [], bursts = [], order = []
const refs = [], module = {exports:{}}
runInNewContext(code, {require:createRequire(url), module, exports:module.exports, HERO:'test',
  useGLTF:()=>({scene:{getObjectByName:()=>null},animations:[]}),clone:x=>x,
  useMemo:fn=>fn(),useEffect:()=>{},useRef:value=>refs[cursor++]??(refs[cursor-1]={current:value}),
  createLivingRig:()=>({idleAt:(time, motion)=>{lastPose=['idle',time,motion]}, reactionAt:(kind,time,idle,motion)=>{lastPose=[kind,time,idle,motion]}}),
  createSmokeMaterial:()=>smoke,createVictoryFireworks:()=>({group:{},update:(t,active)=>bursts.push([t,active]),dispose(){}}),
  useReducedMotion:()=>reduced,useFrame:fn=>{frame=fn},REACTION_SECONDS,REACTION_REDUCED_FRAME,playbackFrame,advanceReactionFrame,completeReaction,prepareReactionFrame,shouldReportReactionTime,
})
const render=(reaction, extra={})=>{cursor=0; module.exports({reaction,onTime:t=>{reported.push(t);order.push(['time',t,refs[1].current.finished])},onFinished:()=>{finished++;order.push(['finished',refs[1].current.finished])},...extra}); return frame}
let preview={kind:'victory',take:1,playing:true,speed:1,seek:null}
let tick=render(preview)
tick({},0.8);assert.equal(lastPose[1],0,'A new action begins at frame zero')
tick({},1);assert.equal(lastPose[1],1,'Playback advances the reaction')
preview={...preview,playing:false};tick=render(preview); const atPause=lastPose.slice(); tick({},2)
assert.deepEqual(lastPose,atPause,'Pausing holds the whole pose')
preview={...preview,seek:0.25};tick=render(preview);tick({},0.5);assert.equal(lastPose[1],0.25,'Backward seek shows the exact requested time')
preview={...preview,playing:true};tick=render(preview);order=[];tick({},10);assert.equal(lastPose[1],REACTION_SECONDS);assert.equal(finished,1,'The action ends once')
assert.deepEqual(order,[['time',REACTION_SECONDS,false],['finished',true]],'When the last tick is reported, onTime precedes setting finished and onFinished')
tick({},10);assert.equal(finished,1,'Additional frames do not repeat completion')
preview={...preview,take:2,seek:null};tick=render(preview);tick({},2);assert.equal(lastPose[1],0,'A new take starts again at zero')
tick({},10);assert.equal(finished,2,'A new take can complete exactly once')
console.log('PASS: play, pause, backward seek, completion, repeated frames and replay')
reduced=true;preview={...preview,take:3,seek:null,playing:true};tick=render(preview);tick({},1);tick({},1)
assert.equal(lastPose[1],REACTION_REDUCED_FRAME.victory,'Reduced motion holds its selected victory pose while playing')
assert.equal(smoke.uniforms.uTime.value,0,'Reduced motion holds smoke still')
assert.equal(bursts.at(-1)[1],false,'Reduced motion suppresses victory fireworks')
const reducedAtPause=lastPose.slice();preview={...preview,playing:false};tick=render(preview);tick({},0.2)
let failed=false
try { assert.deepEqual(lastPose,reducedAtPause,'Pausing reduced-motion replay must preserve the pose already on screen') } catch { failed=true; console.log(`RED reaction: shown playing=${reducedAtPause[1]}, paused=${lastPose[1]}`) }
preview={...preview,seek:0};tick=render(preview);tick({},.1);assert.equal(lastPose[1],0,'Paused seek=0 shows frame zero')
const held=lastPose.slice();tick({},.1);assert.deepEqual(lastPose,held,'The zero frame stays paused')
preview={...preview,playing:true,seek:null};tick=render(preview);tick({},.5);assert.equal(lastPose[1],REACTION_REDUCED_FRAME.victory,'Resume returns to reduced pose')
preview={...preview,playing:false};tick=render(preview);tick({},.1);const displayedBeforeToggle=lastPose[1]
reduced=false;tick=render(preview);tick({},.1);assert.equal(lastPose[1],displayedBeforeToggle,'Reduced-off while paused preserves displayed time')
reduced=true;tick=render(preview);tick({},.1);assert.equal(lastPose[1],displayedBeforeToggle,'Reduced-on while paused preserves displayed time')
preview={...preview,take:4,playing:false,seek:0};tick=render(preview);tick({},.1);assert.equal(lastPose[1],0,'A new take paused at seek zero shows frame zero')
preview={...preview,playing:true,seek:null};tick=render(preview);tick({},.1);assert.equal(lastPose[1],REACTION_REDUCED_FRAME.victory,'New take resumes reduced pose')
preview={...preview,take:5,playing:false,seek:null};tick=render(preview);tick({},.1);assert.equal(lastPose[1],0,'A new paused take without a seek does not inherit the old pose')
// The last report can be less than .09 seconds before the end; the exact
// terminal report is then delivered after marking the action finished.
reduced=false;preview={kind:'victory',take:6,playing:true,speed:1,seek:null};tick=render(preview);tick({},.1);tick({},5.15);order=[];tick({},.06)
assert.deepEqual(order,[['time',REACTION_SECONDS,true],['finished',true]],'A short final step reports the exact end once, after marking finished')
const completedBeforeIdle=finished
preview={kind:'thinking',take:6,playing:true,speed:1,seek:null};tick=render(preview);tick({},5)
assert.equal(lastPose[1],0,'Changing reaction kind begins the new take at zero')
tick({},1);const beforeIdle=lastPose[2]
tick=render(null);tick({},.05);assert.ok(Math.abs(lastPose[1]-(beforeIdle+.05))<1e-9,'Returning to idle continues from the action clock')
preview={kind:'victory',take:7,playing:true,speed:1,seek:null};tick=render(preview);tick({},10)
assert.equal(lastPose[1],0,'A reaction after idle begins at zero even after a long frame')
assert.ok(Math.abs(lastPose[2]-(beforeIdle+.05))<1e-9,'A reaction after idle uses the current idle pose as its base')
assert.equal(finished,completedBeforeIdle,'Starting or interrupting a reaction does not report completion')
console.log('PASS: seek zero, resume, reduced toggle, new take, callbacks and idle transition')
if(failed)process.exitCode=1
