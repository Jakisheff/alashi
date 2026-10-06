"""DeskGenie: procedural Blender build -> desk-genie.glb.

Run inside Blender (MCP execute or Text Editor). Rebuilds everything in the
"DeskGenie" collection from scratch, so this file is the single source of truth.

Axes: Blender is Z-up with the character facing -Y. The glTF exporter converts to
three.js Y-up with +Z toward the viewer: three (x, y, z) -> blender (x, -z, y).
Sides are anatomical: the character's left is +X (screen right when it faces you).
"""

import json
import math
import os

import bpy
from mathutils import Matrix, Vector

ART = os.path.dirname(os.path.abspath(__file__))
COL = "DeskGenie"
GLB = os.path.join(ART, "..", "public", "models", "desk-genie.glb")

# Face constants shared by the build and the rig (must match src/genie/DeskGenie.tsx).
EYE_Z, EYE_HALF, LID_H, LID_REST = 0.07, 0.138, 0.18, 0.3
BROW_R, BROW_ARC = 0.14, 1.3
TAIL_PTS = [(0, 0, -0.62), (0, -0.02, -0.97), (-0.08, -0.05, -1.32), (0.05, -0.05, -1.64),
            (0.35, -0.02, -1.77), (0.55, 0, -1.62), (0.5, 0, -1.42)]


def lid_offset(eye_open, lid):
    """Lid centre above the eye centre: blink closes whatever the lid leaves open."""
    cover = 1 - min(max(eye_open, 0), 1) * (1 - min(max(lid, 0), 1))
    return EYE_HALF - cover * 2 * EYE_HALF + LID_H / 2


# ---------- scene plumbing ----------

def reset():
    old = bpy.data.objects.get("Cube")
    if old and old.data and old.data.name == "Cube":  # factory-startup cube only
        bpy.data.objects.remove(old, do_unlink=True)
    col = bpy.data.collections.get(COL)
    if col:
        for o in list(col.all_objects):
            bpy.data.objects.remove(o, do_unlink=True)
    else:
        col = bpy.data.collections.new(COL)
        bpy.context.scene.collection.children.link(col)
    for blocks in (bpy.data.meshes, bpy.data.curves, bpy.data.armatures, bpy.data.materials, bpy.data.actions):
        for b in list(blocks):
            if b.users == 0:
                blocks.remove(b)
    vl = bpy.context.view_layer
    vl.active_layer_collection = vl.layer_collection.children[COL]
    if bpy.context.object and bpy.context.object.mode != "OBJECT":
        bpy.ops.object.mode_set(mode="OBJECT")
    return col


def srgb(hex_color):
    h = hex_color.lstrip("#")
    c = [int(h[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    return [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c] + [1.0]


def material(name, color, rough=0.5, metal=0.0, emit=None, strength=0.0, alpha=1.0, coat=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = srgb(color)
    b.inputs["Roughness"].default_value = rough
    b.inputs["Metallic"].default_value = metal
    b.inputs["Coat Weight"].default_value = coat
    if emit:
        b.inputs["Emission Color"].default_value = srgb(emit)
        b.inputs["Emission Strength"].default_value = strength
    if alpha < 1:
        b.inputs["Alpha"].default_value = alpha
    return m


def finish(o, mat=None, smooth=True):
    if mat:
        o.data.materials.clear()
        o.data.materials.append(mat)
    if smooth and o.type == "MESH":
        o.data.polygons.foreach_set("use_smooth", [True] * len(o.data.polygons))
    return o


def active():
    return bpy.context.view_layer.objects.active


def bevel(o, width, segments=4, angle=True):
    m = o.modifiers.new("Bevel", "BEVEL")
    m.width = width
    m.segments = segments
    m.limit_method = "ANGLE" if angle else "NONE"
    return m


def weighted_normals(o):
    o.modifiers.new("WeightedNormal", "WEIGHTED_NORMAL").keep_sharp = True


def apply_all(o):
    bpy.ops.object.select_all(action="DESELECT")
    o.select_set(True)
    bpy.context.view_layer.objects.active = o
    bpy.ops.object.convert(target="MESH")
    return o


def box(name, size, loc, mat, radius, segments=5, rot=(0, 0, 0)):
    bpy.ops.mesh.primitive_cube_add(size=1, location=loc, rotation=rot)
    o = active()
    o.name = name
    o.data.transform(Matrix.Diagonal((*size, 1)))
    bevel(o, radius, segments, angle=False)
    weighted_normals(o)
    return finish(o, mat)


def cylinder(name, radius, depth, loc, mat, rot=(0, 0, 0), verts=48, edge=0.0, segments=3):
    bpy.ops.mesh.primitive_cylinder_add(vertices=verts, radius=radius, depth=depth, location=loc, rotation=rot)
    o = active()
    o.name = name
    if edge:
        bevel(o, edge, segments)
        weighted_normals(o)
    return finish(o, mat)


def capsule(name, radius, length, loc, mat, rot=(0, 0, 0)):
    """Cylinder with fully rounded ends; `length` is the overall length."""
    o = cylinder(name, radius, length, loc, mat, rot=rot, verts=24)
    bevel(o, radius * 0.98, 6)
    return o


def sphere(name, radius, loc, mat, scale=(1, 1, 1), segs=(32, 16)):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=segs[0], ring_count=segs[1], radius=radius, location=loc)
    o = active()
    o.name = name
    o.scale = scale
    return finish(o, mat)


def disc(name, radius, loc, mat, scale=(1, 1, 1), rot_y=0.0, verts=40):
    """Flat disc facing the viewer (-Y); scale is in the disc's own plane (x, height, normal)."""
    bpy.ops.mesh.primitive_circle_add(vertices=verts, radius=radius, fill_type="NGON", location=loc,
                                      rotation=(math.pi / 2, rot_y, 0))
    o = active()
    o.name = name
    o.scale = scale
    return finish(o, mat, smooth=False)


def arc(name, radius, start, end, thickness, loc, mat, rot_y=0.0, n=24):
    """Round tube along a circular arc in the XZ plane (angles in radians, 0 = +X)."""
    cu = bpy.data.curves.new(name, "CURVE")
    cu.dimensions = "3D"
    cu.bevel_depth = thickness
    cu.bevel_resolution = 4
    cu.use_fill_caps = True
    sp = cu.splines.new("POLY")
    sp.points.add(n)
    for i in range(n + 1):
        a = start + (end - start) * i / n
        sp.points[i].co = (radius * math.cos(a), 0, radius * math.sin(a), 1)
    o = bpy.data.objects.new(name, cu)
    bpy.context.collection.objects.link(o)
    o.location = loc
    o.rotation_euler = (0, rot_y, 0)
    cu.materials.append(mat)
    apply_all(o)
    return finish(o)


def rounded_rect(name, w, h, r, thickness, loc, mat, n=8):
    """Round tube along a rounded rectangle in the XZ plane, centred on loc."""
    cu = bpy.data.curves.new(name, "CURVE")
    cu.dimensions = "3D"
    cu.bevel_depth = thickness
    cu.bevel_resolution = 3
    sp = cu.splines.new("POLY")
    corners = ((w / 2 - r, h / 2 - r, 0), (-(w / 2 - r), h / 2 - r, 1), (-(w / 2 - r), -(h / 2 - r), 2),
               (w / 2 - r, -(h / 2 - r), 3))
    coords = [(cx + r * math.cos((q + i / n) * math.pi / 2), cz + r * math.sin((q + i / n) * math.pi / 2))
              for cx, cz, q in corners for i in range(n + 1)]
    sp.points.add(len(coords) - 1)
    for p, (x, z) in zip(sp.points, coords):
        p.co = (x, 0, z, 1)
    sp.use_cyclic_u = True
    o = bpy.data.objects.new(name, cu)
    bpy.context.collection.objects.link(o)
    o.location = loc
    cu.materials.append(mat)
    apply_all(o)
    return finish(o)


def join(objs, name):
    """Bake modifiers and merge rigid parts that share a bone into one mesh (fewer draw calls)."""
    for o in objs:
        apply_all(o)
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    o = active()
    o.name = name
    return o


# ---------- the character ----------

def build():
    m = {
        "ivory": material("ivory-plastic", "#ebe1cc", rough=0.45, coat=0.15),
        "teal": material("teal-rubber", "#1f4a43", rough=0.55),
        "orange": material("orange-rubber", "#e06a2c", rough=0.6),
        "joint": material("joint-dark", "#30332f", rough=0.5, metal=0.2),
        "screen": material("screen-glass", "#1c1e1a", rough=0.15, coat=0.8, emit="#1f2a22", strength=0.6),
        "glow": material("face-glow", "#ffd27a", rough=0.4, emit="#ffbf4d", strength=0.9),
        "pupil": material("pupil", "#2a2820", rough=0.6),
        "brow": material("brow", "#c2b291", rough=0.5),
        "steel": material("steel", "#8d8a80", rough=0.3, metal=0.8),
        "tail": material("tail-glass", "#2fa392", rough=0.08, emit="#14786b", strength=0.8, alpha=0.55, coat=1.0),
        "spark": material("tail-spark", "#fff1b0", emit="#ffe08a", strength=1.6),
    }

    m["glint"] = material("eye-glint", "#ffffff", emit="#ffffff", strength=2.0)

    # Head-body: a soft, pillowy CRT housing (bevelled cage + subdivision), narrower at the bottom.
    bpy.ops.mesh.primitive_cube_add(size=1)
    shell = active()
    shell.name = "body-shell"
    shell.data.transform(Matrix.Diagonal((1.3, 1.0, 1.15, 1)))
    for v in shell.data.vertices:
        if v.co.z < 0:
            v.co.x *= 0.9
    bevel(shell, 0.26, 3, angle=False)
    shell.modifiers.new("Subsurf", "SUBSURF").levels = 2
    # Screen recess cut into the front face (front face is y = -0.5).
    cutter = box("cutter", (0.9, 0.2, 0.7), (0.02, -0.55, 0.07), None, 0.08, 6)
    boolean = shell.modifiers.new("Recess", "BOOLEAN")
    boolean.object = cutter
    boolean.operation = "DIFFERENCE"
    boolean.solver = "EXACT"
    apply_all(shell)
    bpy.data.objects.remove(cutter, do_unlink=True)
    finish(shell, m["ivory"])

    # Screen: dark glass, a raised ivory lip around the opening and a dark gasket inside it.
    box("screen", (0.86, 0.04, 0.66), (0.02, -0.45, 0.07), m["screen"], 0.06, 5)
    rounded_rect("screen-lip", 0.94, 0.74, 0.11, 0.024, (0.02, -0.49, 0.07), m["ivory"])
    rounded_rect("screen-gasket", 0.87, 0.67, 0.07, 0.012, (0.02, -0.468, 0.07), m["joint"])

    # Face on the screen (front of the glass is y = -0.47).
    fy = -0.475
    for side, x in (("left", 0.22), ("right", -0.18)):
        disc(f"{side}-eye", 0.12, (x, fy, EYE_Z), m["glow"], scale=(1, 1.15, 1))
        disc(f"{side}-pupil", 0.055, (x, fy - 0.003, EYE_Z), m["pupil"])
        disc(f"{side}-pupil-glint", 0.016, (x + 0.022, fy - 0.0045, EYE_Z + 0.024), m["glint"], verts=16)
    # Lids: patches of screen that slide over the eyes (rest coverage 0.3 = sly).
    lid_z = EYE_Z + lid_offset(1, LID_REST)
    for side, x, tilt in (("left", 0.22, 0.12), ("right", -0.18, -0.12)):
        bpy.ops.mesh.primitive_plane_add(size=1, location=(x, fy - 0.006, lid_z), rotation=(math.pi / 2, tilt, 0))
        lid = active()
        lid.name = f"{side}-lid"
        lid.data.transform(Matrix.Diagonal((0.3, LID_H, 1, 1)))
        finish(lid, m["screen"], smooth=False)
    # Arched brow ridges, asymmetric: the character's left brow sits higher (smirk).
    for side, x, raise_, tilt in (("left", 0.22, 0.021, -0.1), ("right", -0.18, 0.0, 0.05)):
        arc(f"{side}-brow", BROW_R, math.pi / 2 - BROW_ARC / 2, math.pi / 2 + BROW_ARC / 2, 0.03,
            (x, fy - 0.012, 0.27 - BROW_R + raise_), m["brow"], rot_y=tilt)
    # Crooked grin: lower half circle; the rig scales it vertically for smile/frown.
    arc("mouth", 0.1, math.pi, 2 * math.pi, 0.022, (0.07, fy - 0.004, -0.13), m["glow"], rot_y=-0.12)

    # Forest-teal side "ears": dark rim, orange slot at the front, speaker holes at the back.
    for sx in (1, -1):
        side = "left" if sx > 0 else "right"
        cylinder(f"{side}-ear", 0.34, 0.12, (0.66 * sx, 0, 0.05), m["teal"], rot=(0, math.pi / 2, 0), edge=0.05,
                 segments=4)
        bpy.ops.mesh.primitive_torus_add(major_radius=0.29, minor_radius=0.018, location=(0.72 * sx, 0, 0.05),
                                         rotation=(0, math.pi / 2, 0))
        finish(active(), m["joint"]).name = f"{side}-ear-rim"
        box(f"{side}-ear-slot", (0.04, 0.07, 0.24), (0.735 * sx, -0.11, 0.05), m["orange"], 0.02, 4)
        holes = [(0, 0)] + [(0.045 * math.cos(i * math.pi / 3), 0.045 * math.sin(i * math.pi / 3)) for i in range(6)] \
            + [(0.09 * math.cos(i * math.pi / 6), 0.09 * math.sin(i * math.pi / 6)) for i in range(12)]
        join([sphere(f"{side}-hole-{i}", 0.012, (0.722 * sx, 0.11 + dy, 0.05 + dz), m["joint"], scale=(0.4, 1, 1),
                     segs=(10, 6)) for i, (dy, dz) in enumerate(holes)], f"{side}-ear-grille")

    # Top button, sensor, vents and ports.
    box("top-button", (0.26, 0.14, 0.07), (0.12, 0.0, 0.58), m["orange"], 0.025, 4)
    sphere("sensor", 0.03, (0.42, -0.495, 0.46), m["steel"], segs=(16, 8))
    for i, z in enumerate((-0.305, -0.355)):
        box(f"vent-{i}", (0.2, 0.03, 0.03), (0.12, -0.5, z), m["orange"], 0.012, 3)
    for i, x in enumerate((-0.32, -0.25, -0.18)):
        sphere(f"port-{i}", 0.02, (x, -0.495, -0.33), m["joint"], segs=(12, 6))

    # Rounded "chin" under the body; a dark collar where the tail comes out.
    sphere("lower-housing", 0.5, (0, 0, -0.56), m["ivory"], scale=(0.86, 0.72, 0.5), segs=(48, 24))
    bpy.ops.mesh.primitive_torus_add(major_radius=0.43, minor_radius=0.014, location=(0, 0, -0.56))
    seam = finish(active(), m["joint"])
    seam.name = "seam-ring"
    seam.scale = (1, 0.82, 1)
    bpy.ops.mesh.primitive_torus_add(major_radius=0.21, minor_radius=0.035, location=(0, 0, -0.79))
    collar = finish(active(), m["joint"])
    collar.name = "tail-collar"
    collar.scale = (1, 0.85, 1)

    # Arms hang straight down at rest; the rig poses them (rest pose = all zeros).
    for sx in (1, -1):
        side = "left" if sx > 0 else "right"
        sh = Vector((0.58 * sx, -0.15, -0.38))
        el = sh + Vector((0, 0, -0.32))
        hd = el + Vector((0, 0, -0.38))
        sphere(f"{side}-shoulder", 0.11, sh, m["joint"])
        capsule(f"{side}-upper-arm", 0.088, 0.32, sh + Vector((0, 0, -0.16)), m["ivory"])
        sphere(f"{side}-elbow", 0.086, el, m["joint"])
        capsule(f"{side}-forearm", 0.083, 0.28, el + Vector((0, 0, -0.15)), m["ivory"])
        cylinder(f"{side}-wrist", 0.09, 0.05, el + Vector((0, 0, -0.29)), m["orange"], verts=24, edge=0.012)
        box(f"{side}-palm", (0.2, 0.12, 0.18), hd, m["teal"], 0.055, 4)
        disc(f"{side}-palm-pad", 0.05, hd + Vector((0, -0.062, 0.005)), m["orange"])
        xs = (-0.069, -0.023, 0.023, 0.069)
        join([sphere(f"{side}-k{i}", 0.032, hd + Vector((fx, -0.01, -0.085)), m["joint"], segs=(12, 8))
              for i, fx in enumerate(xs)], f"{side}-knuckles")
        fingers = [capsule(f"{side}-f{i}", 0.03, 0.12, hd + Vector((fx, -0.01, -0.145)), m["ivory"])
                   for i, fx in enumerate(xs)]
        fingers.append(capsule(f"{side}-t", 0.03, 0.1, hd + Vector((0.11 * sx, -0.025, -0.01)), m["ivory"],
                               rot=(0, -0.9 * sx, 0)))
        join(fingers, f"{side}-fingers")

    # Glass tail: a tapering NURBS tube that curls up at the tip, with sparks inside.
    pts = TAIL_PTS
    radii = [0.23 * (1 - i / 6) ** 1.1 + 0.015 for i in range(7)]
    cu = bpy.data.curves.new("tail", "CURVE")
    cu.dimensions = "3D"
    cu.bevel_depth = 1.0
    cu.bevel_resolution = 6
    cu.resolution_u = 16
    cu.use_fill_caps = True
    sp = cu.splines.new("NURBS")
    sp.points.add(len(pts) - 1)
    for p, (x, y, z), r in zip(sp.points, pts, radii):
        p.co = (x, y, z, 1)
        p.radius = r
    sp.order_u = 4
    sp.use_endpoint_u = True
    tail = bpy.data.objects.new("tail", cu)
    bpy.context.collection.objects.link(tail)
    cu.materials.append(m["tail"])
    apply_all(tail)
    finish(tail)
    if hasattr(m["tail"], "surface_render_method"):
        m["tail"].surface_render_method = "BLENDED"
    # Sparks only where the tube is thick enough to keep them inside.
    for i in (1, 2, 3):
        for j, f in enumerate((0.3, 0.7)):
            a, b_ = Vector(pts[i]), Vector(pts[i + 1])
            c = a.lerp(b_, f) + Vector((0.04 * (1 if (i + j) % 2 else -1), 0, 0))
            sphere(f"tail-spark-{i}-{j}", 0.014, c, m["spark"], segs=(10, 6))

    return m


# ---------- rig, clips, export ----------
# Rigid parts ride on bones (bone parenting); only the tail is skinned. Every non-tail bone points
# straight up with roll 0, so its local axes are (X, Z_up, -Y) == three.js (x, y, z): pose values
# from src/genie/pose.ts map onto bone-local transforms one to one.

FACE_BONES = {  # bone: head position
    "left-pupil": (0.22, -0.478, EYE_Z), "right-pupil": (-0.18, -0.478, EYE_Z),
    "left-lid": (0.22, -0.481, EYE_Z + lid_offset(1, LID_REST)),
    "right-lid": (-0.18, -0.481, EYE_Z + lid_offset(1, LID_REST)),
    "left-brow": (0.22, -0.485, 0.27 - BROW_R + 0.021), "right-brow": (-0.18, -0.485, 0.27 - BROW_R),
    "mouth": (0.07, -0.479, -0.13),
}


def bone_for(name):
    for b in FACE_BONES:
        if name == b or name.startswith(b + "-"):
            return b
    if name in ("left-eye", "right-eye"):
        return "face"
    for side in ("left", "right"):
        if name in (f"{side}-shoulder", f"{side}-upper-arm"):
            return f"{side}-arm"
        if name in (f"{side}-elbow", f"{side}-forearm", f"{side}-wrist"):
            return f"{side}-forearm"
        if name.startswith((f"{side}-palm", f"{side}-knuckles", f"{side}-fingers")):
            return f"{side}-hand"
    if name.startswith("tail-spark-"):
        return f"tail-{int(name.split('-')[2])}"
    return "body"


def rig(col):
    arm_data = bpy.data.armatures.new("DeskGenie-rig")
    arm = bpy.data.objects.new("DeskGenie-rig", arm_data)
    col.objects.link(arm)
    bpy.ops.object.select_all(action="DESELECT")
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    eb = arm_data.edit_bones

    def bone(name, head, parent=None, tail=None):
        b = eb.new(name)
        b.head = head
        b.tail = tail or (Vector(head) + Vector((0, 0, 0.08)))
        b.roll = 0
        b.use_deform = name.startswith("tail")
        if parent:
            b.parent = eb[parent]
        return b

    bone("body", (0, 0, 0))
    bone("face", (0.02, -0.47, EYE_Z), "body")
    for name, head in FACE_BONES.items():
        bone(name, head, "face")
    for sx, side in ((1, "left"), (-1, "right")):
        bone(f"{side}-arm", (0.58 * sx, -0.15, -0.38), "body")
        bone(f"{side}-forearm", (0.58 * sx, -0.15, -0.70), f"{side}-arm")
        bone(f"{side}-hand", (0.58 * sx, -0.15, -1.0), f"{side}-forearm")
    prev = "body"
    for i in range(len(TAIL_PTS) - 1):
        name = "tail" if i == 0 else f"tail-{i}"
        b = bone(name, TAIL_PTS[i], prev, tail=TAIL_PTS[i + 1])
        b.use_connect = i > 0
        prev = name
    bpy.ops.object.mode_set(mode="OBJECT")

    # Bone-parent every rigid part without moving it (a bone child sits at the bone's tail).
    for o in list(col.objects):
        if o in (arm,) or o.name == "tail":
            continue
        pb = arm.pose.bones[bone_for(o.name)]
        world = o.matrix_world.copy()
        o.parent = arm
        o.parent_type = "BONE"
        o.parent_bone = pb.name
        o.matrix_parent_inverse = (arm.matrix_world @ pb.matrix @ Matrix.Translation((0, pb.bone.length, 0))).inverted()
        o.matrix_world = world

    tail = bpy.data.objects["tail"]
    bpy.ops.object.select_all(action="DESELECT")
    tail.select_set(True)
    arm.select_set(True)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")
    for pb in arm.pose.bones:
        pb.rotation_mode = "ZYX"  # three.js Euler order XYZ
    arm.pose.bones["body"].rotation_mode = "ZXY"  # three.js order YXZ
    return arm


def apply_pose(arm, p, t):
    pb = arm.pose.bones
    pb["body"].location = (0, p["y"], 0)
    pb["body"].rotation_euler = (p["tiltX"], 0, p["tiltZ"])  # yaw stays in the scene
    # Anatomical left (+X) is the preview's "r" side.
    pb["left-arm"].rotation_euler = (-p["rArmFwd"], 0, p["rArmOut"])
    pb["right-arm"].rotation_euler = (-p["lArmFwd"], 0, -p["lArmOut"])
    pb["left-forearm"].rotation_euler = (-p["rElbow"], 0, 0)
    pb["right-forearm"].rotation_euler = (-p["lElbow"], 0, 0)
    for side in ("left", "right"):
        pb[f"{side}-pupil"].location = (p["lookX"] * 0.045, p["lookY"] * 0.05, 0)
    lid_y = lid_offset(p["eyeOpen"], p["lid"]) - lid_offset(1, LID_REST)
    pb["left-lid"].location = (0, lid_y, 0)
    pb["right-lid"].location = (0, lid_y, 0)
    pb["left-lid"].rotation_euler = (0, 0, -p["browTilt"] * 0.45)
    pb["right-lid"].rotation_euler = (0, 0, p["browTilt"] * 0.45)
    pb["left-brow"].location = (0, (p["browR"] - 0.35) * 0.06, 0)
    pb["right-brow"].location = (0, p["browL"] * 0.06, 0)
    pb["left-brow"].rotation_euler = (0, 0, -p["browTilt"] * 0.45)
    pb["right-brow"].rotation_euler = (0, 0, p["browTilt"] * 0.45)
    s = p["smile"]
    pb["mouth"].scale = (1, s if abs(s) >= 0.12 else math.copysign(0.12, s or 1), 1)
    w = 2 * math.pi / 4
    for i in range(len(TAIL_PTS) - 1):
        k = (i + 1) / (len(TAIL_PTS) - 1)
        name = "tail" if i == 0 else f"tail-{i}"
        pb[name].rotation_euler = (0.16 * k * math.sin(2 * w * t - i * 0.7) * p["tailSway"], 0,
                                   0.1 * k * math.cos(2 * w * t - i * 0.6) * p["tailSway"])


def animate(arm):
    with open(os.path.join(ART, "poses.json")) as f:
        data = json.load(f)
    fps = data["fps"]
    bpy.context.scene.render.fps = fps
    arm.animation_data_create()
    for clip, frames in data["clips"].items():
        action = bpy.data.actions.new(clip)
        action.use_fake_user = True
        arm.animation_data.action = action
        for f, p in enumerate(frames):
            apply_pose(arm, p, f / fps)
            for pb in arm.pose.bones:
                pb.keyframe_insert("location", frame=f + 1)
                pb.keyframe_insert("rotation_euler", frame=f + 1)
                pb.keyframe_insert("scale", frame=f + 1)
        action.use_frame_range = True
        action.frame_start, action.frame_end = 1, len(frames)
    arm.animation_data.action = bpy.data.actions["idle"]
    bpy.context.scene.frame_start, bpy.context.scene.frame_end = 1, len(data["clips"]["idle"])


def export():
    os.makedirs(os.path.dirname(GLB), exist_ok=True)
    vl = bpy.context.view_layer
    vl.active_layer_collection = vl.layer_collection.children[COL]
    bpy.ops.export_scene.gltf(filepath=os.path.abspath(GLB), export_format="GLB", use_active_collection=True,
                              export_apply=True, export_animations=True, export_animation_mode="ACTIONS",
                              export_force_sampling=True, export_yup=True, export_cameras=False, export_lights=False)
    return os.path.abspath(GLB)


# ---------- preview camera and lights ----------

def look_at(obj, target):
    obj.rotation_euler = (Vector(target) - obj.location).to_track_quat("-Z", "Y").to_euler()


def stage():
    scene = bpy.context.scene
    scene.render.engine = "BLENDER_EEVEE"
    scene.render.resolution_x, scene.render.resolution_y = 1200, 800
    scene.view_settings.view_transform = "Standard"  # keep the concept hex colors
    world = scene.world or bpy.data.worlds.new("World")
    scene.world = world
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = srgb("#ece6da")
    bg.inputs["Strength"].default_value = 0.8

    cam = bpy.data.objects["Camera"]
    cam.location = (3.0, -7.6, 0.5)
    cam.data.lens = 50
    look_at(cam, (0.1, 0, -0.6))

    key = bpy.data.objects["Light"]
    key.data.type = "AREA"
    key.data.energy = 600
    key.data.size = 4
    key.location = (-3, -4, 4)
    look_at(key, (0, 0, 0))
    rim = bpy.data.objects.get("Rim")
    if not rim:
        rim = bpy.data.objects.new("Rim", bpy.data.lights.new("Rim", "AREA"))
        bpy.context.scene.collection.objects.link(rim)
    rim.data.energy = 300
    rim.data.size = 3
    rim.data.color = (0.85, 1.0, 0.95)
    rim.location = (3.5, 3, 2.5)
    look_at(rim, (0, 0, 0))


def render(name):
    path = os.path.join(ART, "renders", f"{name}.png")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    bpy.context.scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    return path


def main(step="render"):
    """step: "render" (model + preview render) or "export" (model, rig, clips, GLB, render)."""
    col = reset()
    build()
    stage()
    out = {"objects": len(col.all_objects)}
    if step == "export":
        arm = rig(col)
        animate(arm)
        out["glb"] = export()
        bpy.context.scene.frame_set(1)
    out["render"] = render("stage-1")
    return out
