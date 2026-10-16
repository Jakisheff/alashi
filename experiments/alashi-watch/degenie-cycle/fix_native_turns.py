"""Remove equivalent full turns from the tablet Euler track; keep every frame pose."""
import bpy,math,json
from pathlib import Path
root=Path(__file__).resolve().parent
ob=bpy.data.objects['result-tablet'];curve=next(c for c in ob.animation_data.action.fcurves if c.data_path=='rotation_euler' and c.array_index==2)
old=[float(k.co.y) for k in curve.keyframe_points];previous=-1.5*math.pi;max_error=0
for k,value in zip(curve.keyframe_points,old):
 corrected=value+round((previous-value)/(2*math.pi))*2*math.pi
 k.co.y=corrected;previous=corrected
 max_error=max(max_error,abs(math.cos(value)-math.cos(k.co.y)),abs(math.sin(value)-math.sin(k.co.y)))
curve.update();assert max_error<1e-6,max_error
bpy.context.scene.frame_set(1)
bpy.ops.wm.save_as_mainfile(filepath=str(root/'degenie-cycle-animated.blend'))
result={'keyframes':len(old),'max_rotation_basis_error':max_error,'preserved_sampled_poses':True,'reason':'Equivalent angles at pickup and release produced extra revolutions when interpolating between frames.'}
(root/'evidence'/'native-turn-fix.json').write_text(json.dumps(result,indent=2)+'\n')
print('NATIVE_TURN_FIX',json.dumps(result),flush=True)
