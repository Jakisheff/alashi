"""One Degenie task, rendered from the owner's original Blender rig.
Run with Blender --background desk-genie.blend --python build_scene.py -- preview|movie.
"""
import bpy, math, json, sys, importlib.util
from pathlib import Path
from mathutils import Vector, Matrix
ROOT=Path(__file__).resolve().parent
SOURCE=ROOT/'source-model'
spec=importlib.util.spec_from_file_location('original_degenie',SOURCE/'desk_genie.py')
art=importlib.util.module_from_spec(spec);spec.loader.exec_module(art)
scene=bpy.context.scene
FPS=24; DURATION=28
mode=sys.argv[sys.argv.index('--')+1] if '--' in sys.argv else 'preview'
# Rebuild the exact source model: supplied .blend is Blender 5.2, this renderer is 4.2.
# Original geometry builder and rig remain unmodified.
cache=ROOT/'degenie-native.blend'
if cache.exists():
    with bpy.data.libraries.load(str(cache),link=False) as (src,dst):dst.collections=['DeskGenie']
    col=dst.collections[0];scene.collection.children.link(col);worker=bpy.data.objects['DeskGenie-rig']
else:
    col=art.reset();art.build();worker=art.rig(col)
    bpy.data.libraries.write(str(cache),{col})
source_objects=list(col.all_objects)
worker.animation_data_clear()
for obj in source_objects: obj.hide_render=False
# Chief is a baked pose of the same original rig, avoiding a second animated skeleton.
# Only the worker performs contact work in this isolated shot.
chief_pose=json.loads((SOURCE/'poses.json').read_text())['clips']['idle'][0].copy()
chief_pose['rArmFwd']=.7;chief_pose['rPoint']=1
art.apply_pose(worker,chief_pose,0);bpy.context.view_layer.update()
chief=bpy.data.objects.new('Chief-of-Staff',None);scene.collection.objects.link(chief)
chief_objects=[]
dg=bpy.context.evaluated_depsgraph_get()
for obj in source_objects:
    if obj.type=='MESH':
        evaluated=obj.evaluated_get(dg)
        mesh=bpy.data.meshes.new_from_object(evaluated,depsgraph=dg)
        cp=bpy.data.objects.new('chief-'+obj.name,mesh);scene.collection.objects.link(cp)
        cp.matrix_world=obj.matrix_world.copy();cp.parent=chief
        chief_objects.append(cp)
for obj in list(scene.objects):
    if obj not in source_objects and obj not in chief_objects and obj!=chief: bpy.data.objects.remove(obj,do_unlink=True)
worker.name='Degenie-A01';worker.scale=(.64,)*3
chief.scale=(1.05,)*3;chief.location=(-3.75,1.0,2.48);chief.rotation_euler.z=.20
# Enrich source materials without replacing the character.
for mat in bpy.data.materials:
    if mat.use_nodes:
        bs=mat.node_tree.nodes.get('Principled BSDF')
        if bs and 'tail' in mat.name:
            bs.inputs['Base Color'].default_value=art.srgb('#9871dc')
            bs.inputs['Emission Color'].default_value=art.srgb('#9d75eb')
            bs.inputs['Emission Strength'].default_value=.6
            bs.inputs['Alpha'].default_value=.87

def mat(name,col,metal=0,rough=.5,emit=0): return art.material(name,col,rough=rough,metal=metal,emit=col if emit else None,strength=emit)
cream=mat('arena-ivory','#d4c6ab',.12,.34); edge=mat('edge-silver','#a6a9ae',.6,.3)
dark=mat('dark-metal','#30323e',.6,.28);ground=mat('ground','#706a67',0,.9)
pave=[mat('paver-'+str(i),c,0,.85) for i,c in enumerate(['#686468','#797174','#615d63','#827977'])]
purple=mat('lavender','#9e78e6',.25,.33);glow=mat('energy','#b590ff',.2,.25,3)
green=mat('plant','#58634a',0,.9);gold=mat('brass','#c89c58',.7,.28)
screen=mat('screen-dark','#172a30',.15,.26);cyan=mat('screen-active','#87ded6',.1,.4,1.8)
sand=mat('sand','#c9b496',0,.95)

def box(name,loc,size,material,bevel=.06): return art.box(name,size,loc,material,bevel,segments=3)
def cyl(name,loc,r,depth,material): return art.cylinder(name,r,depth,loc,material,verts=48,edge=.035,segments=3)
def torus(name,loc,r,minor,material,rot=(0,0,0)):
    bpy.ops.mesh.primitive_torus_add(major_segments=64,minor_segments=12,location=loc,major_radius=r,minor_radius=minor,rotation=rot)
    o=bpy.context.object;o.name=name;o.data.materials.append(material)
    for p in o.data.polygons:p.use_smooth=True
    return o

def text3(name,body,loc,size,material,rot=(math.pi/2,0,0)):
    cu=bpy.data.curves.new(name,'FONT');cu.body=body;cu.align_x='CENTER';cu.align_y='CENTER';cu.size=size;cu.extrude=.0015
    ob=bpy.data.objects.new(name,cu);scene.collection.objects.link(ob);ob.location=loc;ob.rotation_euler=rot;cu.materials.append(material);return ob

# Architecture is a focused reconstruction of the left camp in the supplied concept.
box('sand-plinth',(0,0,-.38),(14,8,.65),sand,.3)
box('arena-base',(0,0,-.05),(12,6.5,.25),dark,.18)
for row in range(12):
    for col in range(23):
        x=-5.7+col*.5+(row%2)*.25;y=-2.95+row*.5
        box('paving',(x,y,.095),(.475,.475,.065),pave[(col*7+row*3)%4],.015)
# Chief's portal, four substantial concentric steps.
for radius,z,d,m in [(1.65,.22,.22,dark),(1.5,.38,.15,cream),(1.34,.48,.12,edge),(1.19,.55,.08,dark)]:cyl('headquarters',(-3.75,1,z),radius,d,m)
torus('portal-energy',(-3.75,1,.61),1.1,.055,glow)
torus('portal-rim',(-3.75,1,.59),1.29,.065,edge)
for a in [0,math.pi/2,math.pi,3*math.pi/2]:
    x=-3.75+1.38*math.cos(a);y=1+1.38*math.sin(a)
    box('portal-support',(x,y,.74),(.3,.32,.45),cream)
    box('portal-indicator',(x,y,.99),(.12,.17,.06),glow,.02)
# Receive desk sits on the open front edge, reachable by the worker.
box('receipt-desk',(-2.6,-.45,.43),(1.05,.6,.70),cream,.14)
box('receipt-inset',(-2.6,-.45,.82),(.9,.6,.08),dark,.04)
box('receipt-support',(-2.6,-.62,.93),(.38,.24,.12),edge,.02)
receipt=box('receipt-pad',(-2.6,-.62,.9975),(.38,.24,.015),purple,.008)
# Terminal is a compact archive with a globe, facing the worker along the path.
box('terminal-foot',(2.6,.3,.3),(1.65,1.5,.45),cream,.15)
box('terminal-core',(2.75,.55,.95),(1.15,1.1,.95),dark,.13)
for x in [2.19,3.31]:box('terminal-pillar',(x,.55,1.06),(.2,1.2,1.25),cream,.06)
cyl('globe-foot',(2.75,.55,1.65),.58,.13,edge)
for rot in [(0,0,0),(math.pi/2,0,0),(math.pi/2,0,math.pi/2)]:torus('analytical-globe',(2.75,.55,2.12),.47,.018,glow,rot)
bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1,radius=.23,location=(2.75,.55,2.12));bpy.context.object.data.materials.append(purple)
box('work-desk',(2.52,-.52,.62),(1.2,.8,.93),cream,.1)
box('worktop',(2.52,-.52,1.1),(1.27,.88,.11),dark,.06)
# Screen sits horizontally: palms and fingertips have an unambiguous contact plane.
tablet_root=bpy.data.objects.new('result-tablet',None);scene.collection.objects.link(tablet_root)
tablet_parts=[]
for ob in [box('tablet-case',(0,0,0),(.82,.32,.07),edge,.04),box('tablet-screen',(0,0,.04),(.74,.26,.018),screen,.02)]:
    ob.parent=tablet_root;tablet_parts.append(ob)
bar_parts=[]
for i,h in enumerate([.06,.09,.12,.15]):
    ob=box('calculation-bar',(-.27+i*.12,-.02,.057),(.055,h,.012),cyan,.004);ob.parent=tablet_root;bar_parts.append(ob)
# Screen shows four prices only after calculation. 39 is a quote, never a cash payout.
answer=text3('quote','39',(.22,.02,.064),.13,cyan,rot=(0,0,0));answer.parent=tablet_root
for x,y in [(-5.25,2.6),(-1.9,2.65),(1,2.65),(4.55,1.95),(4.55,-1.95),(-4.9,-2.5)]:
    box('planter',(x,y,.3),(.62,.65,.4),cream,.1)
    box('soil',(x,y,.51),(.49,.51,.04),dark,.02)
    for i in range(6):
        a=i*2.4
        bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=1,radius=.2,location=(x+math.cos(a)*.15,y+math.sin(a)*.15,.68+(i%3)*.12))
        bpy.context.object.scale=(.7,.8,1.6);bpy.context.object.data.materials.append(green)
# Banners and warm practical lamps echo the owner concept, kept behind the action.
for x,y in [(-5.25,1.4),(-2.3,2.25),(3.9,1.9)]:
    cyl('banner-base',(x,y,.25),.22,.28,dark);cyl('banner-pole',(x,y,1.45),.035,2.4,edge)
    box('lavender-banner',(x+.23,y,2),(.45,.045,.8),purple,.02)
    text3('brand-mark','∀',(x+.23,y-.029,2.02),.3,cream)
    cyl('lamp',(x,y,2.72),.085,.18,glow)
for x in [-1.8,-.7,.4,1.5]:box('path-inlay',(x,-1.45,.14),(.2,.05,.015),glow,.008)
# Camera and studio-quality light.
cam=bpy.data.objects.new('Camera',bpy.data.cameras.new('Camera'));scene.collection.objects.link(cam);scene.camera=cam
cam.data.type='ORTHO';cam.data.ortho_scale=10.5;cam.data.lens=50
cam.location=(8,-12,9);art.look_at(cam,(0,0,1))
world=bpy.data.worlds.new('warm-world');scene.world=world;world.use_nodes=True
world.node_tree.nodes['Background'].inputs[0].default_value=(.56,.62,.73,1)
world.node_tree.nodes['Background'].inputs[1].default_value=.5
for name,loc,energy,size,color in [('sun-soft',(-3,-4,9),1700,7,(1,.82,.63)),('sky-fill',(4,-1,6),1100,6,(.68,.77,1)),('rim',(-1,5,5),1300,5,(.9,.8,1))]:
    ob=bpy.data.objects.new(name,bpy.data.lights.new(name,'AREA'));scene.collection.objects.link(ob);ob.location=loc;ob.data.energy=energy;ob.data.shape='DISK';ob.data.size=size;ob.data.color=color;art.look_at(ob,(0,0,0))
scene.render.engine='CYCLES';scene.cycles.samples=12;scene.cycles.use_denoising=True
scene.cycles.use_adaptive_sampling=True;scene.cycles.adaptive_threshold=.12
scene.render.resolution_x=1280;scene.render.resolution_y=720;scene.render.resolution_percentage=100
scene.render.fps=FPS;scene.frame_start=1;scene.frame_end=DURATION*FPS
scene.render.film_transparent=False;scene.view_settings.view_transform='AgX'
scene.render.image_settings.file_format='PNG';scene.render.image_settings.color_mode='RGB'
scene.render.use_persistent_data=True
poses=json.loads((SOURCE/'poses.json').read_text())['clips']
base=poses['idle'][0].copy()

def sm(x): x=max(0,min(1,x));return x*x*(3-2*x)
def ease(t,a,b):return sm((t-a)/(b-a))
def mix(a,b,u):return a+(b-a)*u

def idle(t):
    p=base.copy();p['y']=.035*math.sin(t*math.pi);p['tiltX']=.025*math.sin(t*math.pi-.6);p['tiltZ']=.018*math.sin(t*1.3)
    p['eyeOpen']=.12 if 1.9<t%4<2.04 else 1;p['tailSway']=.6;return p

def reach(arm,side,target,palm_up=True,weight=1):
    """Rotation-only two-bone IK in the original Blender rig coordinate system."""
    pb=arm.pose.bones;body=pb['body'].matrix @ arm.data.bones['body'].matrix_local.inverted()
    shoulder=arm.data.bones[side+'-arm'].head_local.copy()
    l1=(arm.data.bones[side+'-forearm'].head_local-shoulder).length
    l2=(arm.data.bones[side+'-hand'].head_local-arm.data.bones[side+'-forearm'].head_local).length
    delta=Vector(target)-shoulder;distance=max(abs(l1-l2)+.005,min(delta.length,l1+l2-.005));axis=delta.normalized()
    pole=Vector((1 if side=='left' else -1,.1,-.5));pole=(pole-axis*pole.dot(axis)).normalized()
    along=(l1*l1-l2*l2+distance*distance)/(2*distance)
    elbow=shoulder+axis*along+pole*math.sqrt(max(0,l1*l1-along*along));wrist=shoulder+axis*distance
    rest=arm.data.bones[side+'-arm'].matrix_local.to_3x3().to_4x4()
    for name,origin,end in [(side+'-arm',shoulder,elbow),(side+'-forearm',elbow,wrist)]:
        swing=Vector((0,0,-1)).rotation_difference((end-origin).normalized()).to_matrix().to_4x4()
        original=pb[name].rotation_euler.to_quaternion();pb[name].matrix=body @ Matrix.Translation(origin) @ swing @ rest
        desired=pb[name].rotation_euler.to_quaternion();pb[name].rotation_euler=original.slerp(desired,weight).to_euler(pb[name].rotation_mode);bpy.context.view_layer.update()
    # Palm faces upward; fingers extend away from the torso.
    rot=Matrix(((1,0,0,0),(0,0,1,0),(0,-1,0,0),(0,0,0,1))) if palm_up else Matrix(((-1,0,0,0),(0,0,1,0),(0,1,0,0),(0,0,0,1)))
    original=pb[side+'-hand'].rotation_euler.to_quaternion()
    pb[side+'-hand'].matrix=body @ Matrix.Translation(wrist) @ rot @ rest
    desired=pb[side+'-hand'].rotation_euler.to_quaternion();pb[side+'-hand'].rotation_euler=original.slerp(desired,weight).to_euler(pb[side+'-hand'].rotation_mode)
    for part in ['point','middle','curl','thumb','point-mid','point-tip','middle-mid','middle-tip','curl-mid','curl-tip','thumb-tip']:
        pb[side+'-'+part].rotation_euler=tuple(v*(1-weight) for v in pb[side+'-'+part].rotation_euler)
    bpy.context.view_layer.update()

# Explicit demo event timings match the animation and explanatory player.
events=[{'id':'a01-order','at':1.0,'type':'task_assigned'}, {'id':'a01-start','at':3.4,'type':'task_started'}, {'id':'a01-tool','at':10.5,'type':'market_read'}, {'id':'a01-ready','at':14.0,'type':'result_ready','quote':39}, {'id':'a01-return','at':16.0,'type':'return_started'}, {'id':'a01-accepted','at':24.4,'type':'result_accepted','quote':39}]
for e in events:e.update(agent_id='degenie-a01',task_id='market-quote-4',mode='demo')
(ROOT/'events.json').write_text(json.dumps(events,ensure_ascii=False,indent=2))

def pose(t):
    p=idle(t);cp=idle(t+.9);cp['tailSway']=.35
    # Chief turns toward the worker, then nods after receiving the report.
    order=math.sin(math.pi*ease(t,.6,2.6)) if t<2.6 else 0
    cp['rArmFwd']+=.75*order;cp['rPoint']=order;cp['lookX']=.6*order
    if 25.5<t<27.5:cp['tiltX']+=.2*math.sin((t-25.5)*math.pi*2)*math.sin(math.pi*(t-25.5)/2)
    chief.rotation_euler.x=cp['tiltX']*.5;chief.rotation_euler.z=.20+.08*order
    start=Vector((-2.55,-1.58,1.24));end=Vector((1.50,-.52,1.24))
    out=ease(t,3.4,9.3);back=ease(t,16.0,21.8)
    loc=start.lerp(end,out) if t<16 else end.lerp(start,back)
    loc.y-=.45*math.sin(math.pi*(out if t<16 else back))
    if t>=21.8:loc=loc.lerp(Vector((-2.6,-1.12,1.24)),ease(t,21.8,22.8))
    yaw=mix(-2.71,-math.pi*1.5,ease(t,2.6,3.4))
    if t>=14.8:yaw=mix(-math.pi*1.5,-math.pi*2.5,ease(t,14.8,16))
    if t>=21.8:yaw=mix(-math.pi*2.5,-math.pi*3,ease(t,21.8,22.6))
    speed=math.sin(math.pi*out) if 3.4<t<9.3 else math.sin(math.pi*back) if 16<t<21.8 else 0
    p['tiltX']+=.16*speed;p['tailSway']=.6+speed*.5;p['lookX']=.5 if 2.2<t<3.4 else 0
    if .9<t<2.4:p['tiltX']+=.1*math.sin((t-.9)*math.pi*2)
    # Freeze hover during contact so hands and desk share a stable frame.
    work=9.3<=t<13.6;carry=13.6<=t<24.4
    if work or carry or 24.4<=t<25.5:p['y']=0;p['tiltX']=0;p['tiltZ']=0
    if work:p['lookY']=-.5
    worker.location=loc;worker.rotation_euler.z=yaw;art.apply_pose(worker,p,t)
    bpy.context.view_layer.update()
    # Tablet rests on desk, then remains attached to the pair of supporting palms.
    tablet_root.location=(2.08,-.52,1.19);tablet_root.rotation_euler=(0,0,-math.pi*1.5)
    if work:
        u=ease(t,9.3,10.5)
        # Fingers touch the near edge of the physical screen. Brief taps produce visible bars.
        tap=.055*abs(math.sin((t-10.5)*math.pi*2)) if 10.5<t<13.5 else 0
        reach(worker,'right',(-.55,-.55,.136+tap),palm_up=False,weight=ease(t,9.3,10.3)*(1-ease(t,13.1,13.6)))
        p['lookY']=-.5
    if carry:
        lift=ease(t,14.3,15.0);lower=ease(t,22.8,24.4)
        local=Vector((0,-.85,.07))
        target=worker.matrix_world @ local
        if t<15.0:target=Vector((2.08,-.52,1.19)).lerp(target,lift)
        if t>=22.8:target=target.lerp(Vector((-2.6,-.62,1.04)),lower)
        # Both palms follow the prop until it reaches the physical receiving pad.
        local_target=worker.matrix_world.inverted() @ target
        for side,x in [('left',.55),('right',-.55)]:
            wrist=local_target+Vector((x,.26,-.1925))
            reach(worker,side,wrist,weight=ease(t,13.6,14.3))
        tablet_root.location=target;tablet_root.rotation_euler.z=yaw
    if t>=24.4:
        tablet_root.location=(-2.6,-.62,1.04);tablet_root.rotation_euler.z=-math.pi*3
        # Hold contact, then withdraw hands together before resuming idle.
        release=1-ease(t,24.55,25.5)
        if release>0:
            local_target=worker.matrix_world.inverted() @ Vector((-2.6,-.62,1.04))
            for side,x in [('left',.55),('right',-.55)]:
                reach(worker,side,local_target+Vector((x,.26+.3*(1-release),-.1925-.18*(1-release))),weight=release)
    for i,ob in enumerate(bar_parts):ob.hide_render=t<10.5+i*.7
    answer.hide_render=t<14
    receipt.data.materials[0]=cyan if t>=24.4 else purple
    # The moving camera follows this single worker. Its center survives the mobile crop.
    close=ease(t,6.5,9.3)*(1-ease(t,15,16.5))
    handoff=ease(t,21,22.8)*(1-ease(t,24.4,26))
    focus=Vector((loc.x*.72,loc.y*.5,mix(mix(1.8,1.15,ease(t,2.7,5)),1.8,ease(t,24.4,26))))
    wide=mix(10.8,8.7,ease(t,2.7,5));wide=mix(wide,10.8,ease(t,24.4,26));camera_scale=mix(wide,5.6,max(close,handoff))
    cam.data.ortho_scale=camera_scale
    cam.location=focus+Vector((7+5*handoff,-12+6*handoff,8.5+3*handoff));art.look_at(cam,focus)
    bpy.context.view_layer.update()


def contact_report(t):
    pose(t)
    def bounds(name):
        ob=bpy.data.objects[name]
        v=[ob.matrix_world @ Vector(p) for p in ob.bound_box]
        return {'min':[min(p[i] for p in v) for i in range(3)],'max':[max(p[i] for p in v) for i in range(3)]}
    return {'time':t,'tablet':bounds('tablet-case'),'palms':{side:bounds(side+'-palm') for side in ['left','right']},'fingers':{side:bounds(side+'-point-tip') for side in ['left','right']},'rest_shoulders':{side:list(worker.data.bones[side+'-arm'].head_local) for side in ['left','right']},'wrists':{side:list(worker.pose.bones[side+'-hand'].head) for side in ['left','right']},'lengths':{side:[(worker.pose.bones[side+'-forearm'].head-worker.pose.bones[side+'-arm'].head).length,(worker.pose.bones[side+'-hand'].head-worker.pose.bones[side+'-forearm'].head).length] for side in ['left','right']}}

pose(0)
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/'degenie-cycle.blend'))
if mode=='validate':
    results=[contact_report(t) for t in [10.5,11,14.9,18.5,23.8,24.4,25.5]]
    (ROOT/'evidence'/'contacts.json').write_text(json.dumps(results,indent=2))
    print('CONTACTS_WRITTEN',flush=True)
elif mode in ('preview','contacts'):
    scene.render.resolution_percentage=65;scene.cycles.samples=8
    for t in ([1.5,11.5,14.6,24.0] if mode=='contacts' else [1.5,6.0,11.5,14.6,18.5,24.0,25.4]):
        pose(t);scene.render.filepath=str(ROOT/'evidence'/('pose-'+str(t)+'.png'));bpy.ops.render.render(write_still=True)
elif mode=='movie':
    FPS=15;scene.render.fps=FPS;scene.frame_end=DURATION*FPS
    scene.render.resolution_x=960;scene.render.resolution_y=540;scene.cycles.samples=3;scene.cycles.denoising_prefilter='FAST';scene.cycles.denoising_quality='FAST'
    scene.cycles.max_bounces=4;scene.cycles.diffuse_bounces=2;scene.cycles.glossy_bounces=2
    scene.render.image_settings.file_format='JPEG';scene.render.image_settings.quality=93
    for frame in range(DURATION*FPS):
        target=ROOT/'frames'/f'{frame:04d}.jpg'
        if target.exists():continue
        pose(frame/FPS);scene.render.filepath=str(target);bpy.ops.render.render(write_still=True)
        print(f'COMPLETED_FRAME {frame+1}/{DURATION*FPS}',flush=True)
    print('ALL_FRAMES_COMPLETE',flush=True)
