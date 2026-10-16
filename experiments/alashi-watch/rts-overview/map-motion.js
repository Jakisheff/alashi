export const clamp = (x, low=0, high=1) => Math.max(low,Math.min(high,x));
export const smooth = x => {x=clamp(x);return x*x*(3-2*x);};
export function routePoint(route, progress){
 const p=clamp(progress),q=1-p,w=[q*q*q,3*q*q*p,3*q*p*p,p*p*p];
 return [0,1].map(axis=>route.reduce((sum,point,i)=>sum+point[axis]*w[i],0));
}
export function unitPose(unit, age, timing, reduced=false){
 let p=0;
 if(unit.phase==='outbound')p=smooth((unit.phase_t+age)/timing.outbound);
 else if(unit.phase==='return')p=1-smooth((unit.phase_t+age)/timing.return);
 else if(unit.phase==='work')p=1;
 if(reduced)p=['outbound','work'].includes(unit.phase)?1:0;
 const point=routePoint(unit.route,p);
 const a=routePoint(unit.route,Math.max(0,p-.01)),b=routePoint(unit.route,Math.min(1,p+.01));
 let angle=Math.atan2(b[0]-a[0],b[1]-a[1]);
 if(unit.phase==='return')angle+=Math.PI;
 const moving=['outbound','return'].includes(unit.phase);
 return {x:point[0],y:point[1],progress:p,moving,direction:moving||unit.phase==='order'?((Math.round(angle/(Math.PI/4))%8)+8)%8:(unit.owner?7:1)};
}

export function replayState(source, time, timing){
 const events=source.events.filter(e=>e.t<=time);
 return {...source,t:time,events,holds:[0,0],accepted:[0,1].map(side=>events.some(e=>e.kind==='accepted'&&e.side===side)),units:source.units.map(unit=>{
  const assigned=events.find(e=>e.kind==='assigned'&&e.unit===unit.id);
  let phase='idle',phase_t=0,result=null;
  if(assigned){
   const elapsed=Math.max(0,time-assigned.t),workStart=timing.order+timing.outbound,returnStart=unit.duration-timing.return-timing.handoff;
   if(events.some(e=>e.kind==='done'&&e.unit===unit.id)){phase='done';result=unit.result;}
   else if(elapsed<timing.order){phase='order';phase_t=elapsed;}
   else if(!events.some(e=>e.kind==='work'&&e.unit===unit.id)){phase='outbound';phase_t=Math.max(0,elapsed-timing.order);}
   else if(!events.some(e=>e.kind==='return'&&e.unit===unit.id)){phase='work';phase_t=Math.max(0,elapsed-workStart);}
   else if(elapsed<unit.duration-timing.handoff){phase='return';phase_t=Math.max(0,elapsed-returnStart);result=unit.result;}
   else{phase='handoff';phase_t=Math.max(0,elapsed-unit.duration+timing.handoff);result=unit.result;}
  }
  return {...unit,phase,phase_t,result};
 })};
}
