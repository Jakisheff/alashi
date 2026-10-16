"""Render/review the saved diorama while reusing the exact current pose functions."""
import bpy, math, json, sys, ast, importlib.util
from pathlib import Path
from mathutils import Vector,Matrix
ROOT=Path(__file__).resolve().parent
SOURCE=ROOT/'source-model'
spec=importlib.util.spec_from_file_location('original_degenie',SOURCE/'desk_genie.py');art=importlib.util.module_from_spec(spec);spec.loader.exec_module(art)
scene=bpy.context.scene
worker=bpy.data.objects['Degenie-A01'];chief=bpy.data.objects['Chief-of-Staff'];cam=bpy.data.objects['Camera']
tablet_root=bpy.data.objects['result-tablet'];answer=bpy.data.objects['quote'];receipt=bpy.data.objects['receipt-pad']
bar_parts=sorted([o for o in scene.objects if o.name.startswith('calculation-bar')],key=lambda o:o.name)
cyan=bpy.data.materials['screen-active'];purple=bpy.data.materials['lavender']
poses=json.loads((SOURCE/'poses.json').read_text())['clips'];base=poses['idle'][0].copy()
tree=ast.parse((ROOT/'build_scene.py').read_text()); names={'sm','ease','mix','idle','reach','pose','contact_report'}
functions=ast.Module(body=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in names],type_ignores=[])
exec(compile(functions,str(ROOT/'build_scene.py'),'exec'))
mode=sys.argv[sys.argv.index('--')+1] if '--' in sys.argv else 'keyframes'
if mode=='keyframes':
    scene.render.resolution_x=960;scene.render.resolution_y=540;scene.render.resolution_percentage=100
    scene.cycles.samples=3;scene.cycles.denoising_prefilter='FAST';scene.cycles.denoising_quality='FAST';scene.cycles.max_bounces=4;scene.cycles.diffuse_bounces=2;scene.cycles.glossy_bounces=2
    scene.render.image_settings.file_format='JPEG';scene.render.image_settings.quality=93
    for frame in [157,224,357,366,395]:
        pose(frame/15);scene.render.filepath=str(ROOT/'frames'/f'{frame:04d}.jpg');bpy.ops.render.render(write_still=True)
        print('REVIEW_FRAME',frame,flush=True)
elif mode=='validate':
    def snapshot():
        return [v for b in worker.pose.bones for row in b.matrix for v in row]+[v for row in tablet_root.matrix_world for v in row]
    pose(18.5);original=snapshot();pose(24.4);pose(1);pose(18.5);revisited=snapshot()
    error=max(abs(a-b) for a,b in zip(original,revisited));assert error<1e-5,error
    rows=[contact_report(t) for t in [10.5,11,14.9,18.5,23.8,24.4,25.5]]
    (ROOT/'evidence'/'contacts.json').write_text(json.dumps(rows,indent=2))
    (ROOT/'evidence'/'rewind-geometry.json').write_text(json.dumps({'passed':True,'max_error':error,'sequence':[18.5,24.4,1,18.5]}))
    print('REWIND_PASS',error,flush=True)
elif mode=='bake':
    scene.render.fps=15;scene.frame_start=1;scene.frame_end=420
    accepted_pad=receipt.copy();accepted_pad.data=receipt.data.copy();accepted_pad.name='accepted-pad';scene.collection.objects.link(accepted_pad);accepted_pad.data.materials[0]=cyan
    for frame in range(1,421):
        # The pure pose reset prevents accumulation and makes reverse scrubbing exact.
        scene.frame_set(frame);pose((frame-1)/15)
        for ob in [worker,chief,cam,tablet_root]:
            for attr in ['location','rotation_euler','scale']:ob.keyframe_insert(attr,frame=frame)
        cam.data.keyframe_insert('ortho_scale',frame=frame)
        for bone in worker.pose.bones:
            for attr in ['location','rotation_euler','scale']:bone.keyframe_insert(attr,frame=frame)
        receipt.data.materials[0]=purple;receipt.hide_render=(frame-1)/15>=24.4;accepted_pad.hide_render=not receipt.hide_render
        for ob in bar_parts+[answer,receipt,accepted_pad]:
            ob.hide_viewport=ob.hide_render
            ob.keyframe_insert('hide_render',frame=frame);ob.keyframe_insert('hide_viewport',frame=frame)
    for action in bpy.data.actions:
        for curve in action.fcurves:
            for key in curve.keyframe_points:key.interpolation='LINEAR'
    scene.frame_set(1);bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/'degenie-cycle-animated.blend'))
    print('BAKED_420_FRAMES',flush=True)
