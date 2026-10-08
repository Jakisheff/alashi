"""Local sale experiment. Run through Blender MCP; preserve the existing character."""
import bpy
import math
import os
import numpy as np
from mathutils import Vector

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
assert bpy.context.mode == 'OBJECT'
assert 'MarketSale-Experiment' not in bpy.data.collections, 'Experiment already exists; inspect before replacing'
selected = list(bpy.context.selected_objects)
active = bpy.context.view_layer.objects.active
col = bpy.data.collections.new('MarketSale-Experiment')
bpy.context.scene.collection.children.link(col)

def material(name, color, metallic=0, roughness=.6):
    m = bpy.data.materials.new(name)
    m.diffuse_color = (*color, 1)
    m.use_nodes = True
    bs = m.node_tree.nodes.get('Principled BSDF')
    bs.inputs['Base Color'].default_value = (*color, 1)
    bs.inputs['Metallic'].default_value = metallic
    bs.inputs['Roughness'].default_value = roughness
    return m

wood = material('sale-warm-wood', (.40, .22, .10))
edge = material('sale-wood-frame', (.27, .12, .045))
iron = material('sale-dark-nails', (.045, .06, .052), .65, .35)
gold = material('sale-gold', (.9, .55, .12), .78, .26)
ivory = material('sale-coin-letter', (.98, .82, .43), .55, .3)
cloth = material('sale-canvas', (.78, .69, .51))
# A small packed grain texture exports to glTF; no Blender-only procedural nodes.
rng = np.random.default_rng(28)
w = h = 256
y, x = np.mgrid[0:h, 0:w]
grain = .82 + .09*np.sin(x*.25+2*np.sin(y*.033)) + .045*np.sin(x*.93+y*.013) + rng.normal(0,.022,(h,w))
pixels = np.ones((h,w,4),dtype=np.float32)
pixels[:,:,:3] = grain[:,:,None]*np.array([.57,.36,.19])
im = bpy.data.images.new('sale-wood-grain',width=w,height=h)
im.pixels.foreach_set(pixels.ravel())
im.filepath_raw = os.path.join(ROOT,'art/experiments/sale-wood-grain.png')
im.file_format = 'PNG'
im.save()
im.pack()
tex = wood.node_tree.nodes.new('ShaderNodeTexImage');tex.image=im
wood.node_tree.links.new(tex.outputs['Color'],wood.node_tree.nodes.get('Principled BSDF').inputs['Base Color'])

def adopt(o, name, parent, mat):
    o.name=name
    for old in list(o.users_collection): old.objects.unlink(o)
    col.objects.link(o)
    o.parent=parent
    if mat: o.data.materials.append(mat)
    return o

def empty(name):
    o=bpy.data.objects.new(name,None);col.objects.link(o);return o

def box(name, pos, size, parent, mat=wood, radius=.018):
    bpy.ops.mesh.primitive_cube_add(size=1, location=pos)
    o=adopt(bpy.context.object,name,parent,mat);o.scale=size
    bpy.context.view_layer.objects.active=o
    bpy.ops.object.transform_apply(location=False,rotation=False,scale=True)
    if radius:
        mod=o.modifiers.new('soft wooden edges','BEVEL');mod.width=radius;mod.segments=3
        o.modifiers.new('weighted normals','WEIGHTED_NORMAL')
    return o

def beam(name, a, b, thickness, parent, mat=edge):
    a,b=Vector(a),Vector(b)
    o=box(name,(a+b)*.5,(thickness,thickness,(b-a).length),parent,mat,.009)
    o.rotation_mode='QUATERNION';o.rotation_quaternion=Vector((0,0,1)).rotation_difference(b-a)
    return o

stand=empty('market-stand')
for i in range(7):
    box('sale-counter-plank-'+str(i),((i-3)*.31,0,0),( .305,.62,.10),stand)
    box('sale-front-plank-'+str(i),((i-3)*.31,-.20,-.40),(.305,.055,.68),stand)
for z in [-.09,-.74]:box('sale-counter-rail',(0,-.25,z),(2.24,.09,.12),stand,edge)
for x0 in [-1.04,1.04]:box('sale-counter-post',(x0,0,-.48),(.12,.51,.90),stand,edge)
beam('sale-counter-brace-a',(-1.0,-.31,-.68),(-.05,-.31,-.16),.085,stand)
beam('sale-counter-brace-b',(.05,-.31,-.16),(1.0,-.31,-.68),.085,stand)
box('sale-linen-sign',(0,-.335,-.32),(.58,.025,.26),stand,cloth,.012)
# Stylized price/market symbol; no numeric payout is implied.
beam('sale-sign-basket-a',(-.12,-.355,-.27),(-.08,-.355,-.38),.021,stand,edge)
beam('sale-sign-basket-b',(.12,-.355,-.27),(.08,-.355,-.38),.021,stand,edge)
beam('sale-sign-basket-c',(-.08,-.355,-.38),(.08,-.355,-.38),.021,stand,edge)
beam('sale-sign-basket-d',(-.15,-.355,-.26),(.15,-.355,-.26),.021,stand,edge)

crate=empty('market-crate')
for i in range(4):
    for sy in [-1,1]:box('sale-crate-slat',((i-1.5)*.125,sy*.24,.255),(.12,.035,.47),crate)
    for sx in [-1,1]:box('sale-crate-side',(sx*.24,(i-1.5)*.125,.255),(.035,.12,.47),crate)
    box('sale-crate-lid',((i-1.5)*.125,0,.505),(.12,.49,.035),crate)
for z in [.035,.49]:
    for sy in [-1,1]:box('sale-crate-edge',(0,sy*.267,z),(.55,.055,.06),crate,edge,.008)
    for sx in [-1,1]:box('sale-crate-edge',(sx*.267,0,z),(.055,.50,.06),crate,edge,.008)
for sy in [-1,1]:
    beam('sale-crate-diagonal',(-.22,sy*.29,.085),(.22,sy*.29,.445),.065,crate)

coin=empty('market-coin')
bpy.ops.mesh.primitive_cylinder_add(vertices=64,radius=.145,depth=.032,rotation=(math.pi/2,0,0))
o=adopt(bpy.context.object,'sale-coin-disc',coin,gold)
mod=o.modifiers.new('coin edge','BEVEL');mod.width=.008;mod.segments=3
for sy in [-1,1]:
    bpy.ops.mesh.primitive_torus_add(major_radius=.127,minor_radius=.008,major_segments=48,minor_segments=8,location=(0,sy*.020,0),rotation=(math.pi/2,0,0))
    adopt(bpy.context.object,'sale-coin-rim',coin,ivory)
    # Raised Bitcoin mark on both faces: a B with two short strokes above/below.
    bpy.ops.object.text_add(location=(0,sy*.023,0),rotation=(math.pi/2 if sy < 0 else -math.pi/2,0,0))
    letter=adopt(bpy.context.object,'sale-coin-bitcoin',coin,ivory)
    letter.data.body='B';letter.data.align_x='CENTER';letter.data.align_y='CENTER'
    letter.data.size=.22;letter.data.extrude=.002;letter.data.bevel_depth=.001
    bpy.ops.object.convert(target='MESH')
    for x0 in [-.026,.004]:
        for z in [-.094,.094]:box('sale-bitcoin-stroke',(x0,sy*.025,z),(.009,.008,.042),coin,ivory,.002)

bpy.ops.object.select_all(action='DESELECT')
for o in col.objects:o.select_set(True)
bpy.context.view_layer.objects.active=stand
bpy.ops.export_scene.gltf(filepath=os.path.join(ROOT,'art/experiments/market-sale-props.glb'),export_format='GLB',use_selection=True,export_animations=False,export_apply=True)
bpy.data.libraries.write(os.path.join(ROOT,'art/experiments/market-sale-props.blend'),{col},fake_user=True)
# Hide just the experiment in the existing viewport. Keep the original character/file intact.
col.hide_viewport=True;col.hide_render=True
bpy.ops.object.select_all(action='DESELECT')
for o in selected:o.select_set(True)
bpy.context.view_layer.objects.active=active
result={'objects':len(col.objects),'roots':[stand.name,crate.name,coin.name],'original_blend':bpy.data.filepath,'export':os.path.join(ROOT,'art/experiments/market-sale-props.glb')}
