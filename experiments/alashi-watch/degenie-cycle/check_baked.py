import bpy,json
from pathlib import Path
root=Path(__file__).resolve().parent
s=bpy.context.scene
worker=bpy.data.objects['Degenie-A01'];tablet=bpy.data.objects['result-tablet']
def capture(f):
 s.frame_set(f)
 return [v for row in tablet.matrix_world for v in row]+[v for b in worker.pose.bones for row in b.matrix for v in row]
a=capture(279);capture(367);capture(1);b=capture(279)
error=max(abs(x-y) for x,y in zip(a,b));assert error<1e-6,error
s.frame_set(1);early=bpy.data.objects['quote'].hide_render
s.frame_set(367);late=bpy.data.objects['quote'].hide_render
assert early and not late
assert s.frame_start==1 and s.frame_end==420 and s.render.fps==15
result={'passed':True,'frames':420,'fps':15,'duration':28,'rewind_max_error':error,'quote_hidden_initially':early,'quote_visible_at_acceptance':not late,'action_count':len(bpy.data.actions),'bone_count':len(worker.pose.bones)}
(root/'evidence'/'baked-scene-checks.json').write_text(json.dumps(result,indent=2));print('BAKED_SCENE_PASS',json.dumps(result),flush=True)
