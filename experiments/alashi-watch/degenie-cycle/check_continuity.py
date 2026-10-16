"""Measure native animation continuity, including Euler turns hidden at frame boundaries."""
import bpy,json,math
from pathlib import Path
root=Path(__file__).resolve().parent
scene=bpy.context.scene
names=['Degenie-A01','result-tablet','Camera']
previous={};peaks={name:{'translation':0,'rotation_radians':0,'frame':None} for name in names}
length_error=0.0
for frame in range(1,421):
 scene.frame_set(frame)
 for name in names:
  ob=bpy.data.objects[name];pos=ob.location.copy();rot=ob.rotation_euler.copy()
  if name in previous:
   oldpos,oldrot=previous[name]
   dp=(pos-oldpos).length;dr=max(abs(a-b) for a,b in zip(rot,oldrot))
   peaks[name]['translation']=max(peaks[name]['translation'],dp)
   if dr>peaks[name]['rotation_radians']:peaks[name].update(rotation_radians=dr,frame=frame)
  previous[name]=(pos,rot)
 worker=bpy.data.objects['Degenie-A01']
 for side in ['left','right']:
  pb=worker.pose.bones
  for a,b,length in [(side+'-arm',side+'-forearm',.32),(side+'-forearm',side+'-hand',.36)]:
   length_error=max(length_error,abs((pb[a].head-pb[b].head).length-length))
result={'passed':all(x['translation']<(.6 if name=='Camera' else .2) and x['rotation_radians']<.4 for name,x in peaks.items()) and length_error<1e-5,'frames':420,'frame_interval_seconds':1/15,'peaks':peaks,'max_joint_length_error':length_error,'limits':{'translation_worker_and_tablet':.2,'translation_camera':.6,'rotation_radians':.4,'joint_length_error':1e-5},'scope':'Root transforms and worker arm lengths; visual movement still needs inspection.'}
(root/'evidence'/'continuity-checks.json').write_text(json.dumps(result,indent=2)+'\n')
print('CONTINUITY_RESULT',json.dumps(result),flush=True)
