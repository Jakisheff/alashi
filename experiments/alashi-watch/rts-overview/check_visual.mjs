import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {routePoint,unitPose,replayState} from './map-motion.js';
const layout=JSON.parse(readFileSync(new URL('./map-layout.json',import.meta.url))),T=layout.timing;
assert.equal(layout.routes.length,8);
let samples=0;
for(const [id,route] of layout.routes.entries()){
 assert.equal(route.length,4);assert.deepEqual(routePoint(route,0),route[0]);assert.deepEqual(routePoint(route,1),route[3]);
 for(let i=0;i<=100;i++){const p=routePoint(route,i/100);assert(p[0]>0&&p[0]<layout.width&&p[1]>0&&p[1]<layout.height);samples++;}
 for(const phase of ['idle','order','outbound','work','return','handoff','done']){
  const u={id,owner:Math.floor(id/4),route,phase,phase_t:.2};const pose=unitPose(u,.08,T);
  assert(pose.direction>=0&&pose.direction<8);assert(pose.progress>=0&&pose.progress<=1);
  const still=unitPose(u,0,T,true);assert.deepEqual([still.x,still.y],['outbound','work'].includes(phase)?route[3]:route[0]);
 }
}
const source={cycle:1,accepted:[true,true],holds:[0,0],units:[{id:0,owner:0,role:0,route:layout.routes[0],duration:4,result:'Расчёт 39'}],events:[{event_id:'1:1',t:.5,kind:'assigned',unit:0,side:0},{event_id:'1:2',t:1.65,kind:'work',unit:0,side:0},{event_id:'1:3',t:3.28,kind:'return',unit:0,side:0},{event_id:'1:4',t:4.55,kind:'done',unit:0,side:0},{event_id:'1:5',t:4.7,kind:'accepted',side:0}]};
assert.equal(replayState(source,.4,T).units[0].phase,'idle');
assert.equal(replayState(source,.6,T).units[0].phase,'order');
assert.equal(replayState(source,1.4,T).units[0].phase,'outbound');
assert.equal(replayState(source,2,T).units[0].phase,'work');
assert.equal(replayState(source,3.3,T).units[0].phase,'return');
assert.equal(replayState(source,4.4,T).units[0].phase,'handoff');
assert.equal(replayState(source,4.6,T).units[0].phase,'done');
assert.equal(replayState(source,3.2,T).units[0].result,null);
assert.equal(replayState(source,3.3,T).units[0].result,'Расчёт 39');
assert.deepEqual(replayState(source,4.6,T).accepted,[false,false]);
assert.deepEqual(replayState(source,4.8,T).accepted,[true,false]);
for(let frame=0;frame<150;frame++){const t=frame/30,state=replayState(source,t,T);assert(state.events.every(e=>e.t<=t));assert(state.units[0].phase_t>=0);}
assert.equal(source.events.length,5);assert.equal(source.units[0].result,'Расчёт 39');
console.log(JSON.stringify({passed:true,route_samples:samples,units:8,phases:7,event_gated_replay:true,reduced_motion:true,travel_seconds:T.outbound+T.return}));
