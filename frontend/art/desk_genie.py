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
from mathutils.bvhtree import BVHTree

ART = os.path.dirname(os.path.abspath(__file__))
COL = "DeskGenie"
GLB = os.path.join(ART, "desk-genie.glb")  # raw export; npm run optimize:glb writes public/models

# Face constants shared by the build and the rig.
SCREEN_Z, SCREEN_H = 0.25, 0.72  # screen centre height and height; the lower band shows text
EYE_Z, EYE_R, LID_REST = SCREEN_Z + 0.15, 0.13, 0.3
EYE_HALF = EYE_R * 1.15
EYE_TOP = EYE_Z + EYE_HALF
EYES_X = {"left": 0.23, "right": -0.2}  # anatomical left is +X
BROW_R, BROW_ARC = 0.15, 1.3
BROW_Z = EYE_Z + 0.18 - BROW_R  # arc centre; the arc top sits 0.18 above the eye centre
MOUTH = (0.06, SCREEN_Z - 0.02)  # (x, z) of the grin's circle centre
TEXT_Z = SCREEN_Z - 0.25  # centre of the text band (R3F types replies there)
# Tail: wide at the body, curling into a smoke spiral on the viewer's left.
TAIL_PTS = [(0, 0, -0.5), (0.04, -0.02, -0.85), (0.1, -0.04, -1.2), (0.0, -0.04, -1.52),
            (-0.28, -0.02, -1.7), (-0.55, 0, -1.6), (-0.62, 0, -1.38), (-0.48, 0, -1.26), (-0.4, 0, -1.36)]
TAIL_R = [0.31, 0.29, 0.25, 0.2, 0.15, 0.11, 0.075, 0.045, 0.015]
JOINTS = {}  # bone heads filled by build() for the arms


def lid_cover(eye_open, lid):
    """Share of the eye hidden by the lid shutter: blink closes whatever the lid leaves open."""
    return 1 - min(max(eye_open, 0), 1) * (1 - min(max(lid, 0), 1))


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
    for a in list(bpy.data.actions):  # clips keep a fake user, so drop them explicitly before rebuilding
        bpy.data.actions.remove(a)
    for blocks in (bpy.data.meshes, bpy.data.curves, bpy.data.armatures, bpy.data.materials):
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

def surface_hit(bvh, origin, direction):
    hit = bvh.ray_cast(Vector(origin), Vector(direction).normalized())
    if hit[0] is None:
        raise RuntimeError(f"no surface from {origin} along {direction}")
    return hit[0], hit[1]


def front(bvh, x, z):
    """Point and normal on the body's front face at (x, z)."""
    return surface_hit(bvh, (x, -3, z), (0, 1, 0))


def align(o, normal, axis=(0, -1, 0)):
    """Rotate o so that its local `axis` follows the surface normal."""
    o.rotation_mode = "QUATERNION"
    o.rotation_quaternion = Vector(axis).rotation_difference(normal)
    return o


def tube(name, points, thickness, mat):
    """Round tube through world-space points."""
    cu = bpy.data.curves.new(name, "CURVE")
    cu.dimensions = "3D"
    cu.bevel_depth = thickness
    cu.bevel_resolution = 3
    cu.use_fill_caps = True
    sp = cu.splines.new("POLY")
    sp.points.add(len(points) - 1)
    for p, c in zip(sp.points, points):
        p.co = (*c, 1)
    o = bpy.data.objects.new(name, cu)
    bpy.context.collection.objects.link(o)
    if mat:
        cu.materials.append(mat)
    apply_all(o)
    return finish(o)


def cut(target, cutter):
    m = target.modifiers.new("Cut", "BOOLEAN")
    m.object = cutter
    m.operation = "DIFFERENCE"
    m.solver = "EXACT"
    apply_all(target)
    bpy.data.objects.remove(cutter, do_unlink=True)


def body_mesh():
    """Square top that tapers toward the tail: three rings of (half width, half depth) at heights."""
    rings = [(0.8, 0.66, 0.52), (0.0, 0.66, 0.52), (-0.6, 0.36, 0.34)]
    verts = [(sx * w, sy * d, z) for z, w, d in rings for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
    faces = [(0, 1, 2, 3), (11, 10, 9, 8)]  # caps wound outward (top up, bottom down)
    for r in range(2):
        for i in range(4):
            a, b = r * 4 + i, r * 4 + (i + 1) % 4
            faces.append((a + 4, b + 4, b, a))
    me = bpy.data.meshes.new("body-shell")
    me.from_pydata(verts, [], faces)
    o = bpy.data.objects.new("body-shell", me)
    bpy.context.collection.objects.link(o)
    return o


def build():
    m = {
        "ivory": material("ivory-plastic", "#e9dfc9", rough=0.5, coat=0.1),
        "teal": material("teal-rubber", "#21493f", rough=0.55),
        "orange": material("orange-rubber", "#e06a2c", rough=0.6),
        "joint": material("joint-dark", "#2c302d", rough=0.45, metal=0.3),
        "steel": material("steel", "#9b968a", rough=0.3, metal=0.85),
        "screen": material("screen-glass", "#15171a", rough=0.12, coat=1.0, emit="#1b2420", strength=0.5),
        "glow": material("face-glow", "#ffd98f", rough=0.4, emit="#ffc766", strength=1.6),
        "cyan": material("screen-rim", "#6ee7ff", emit="#6ee7ff", strength=2.5),
        "divider": material("screen-divider", "#3b6f69", emit="#2f8f84", strength=0.6),
        "pupil": material("pupil", "#2a261d", rough=0.6),
        "glint": material("eye-glint", "#ffffff", emit="#ffffff", strength=3.0),
        "brow": material("brow", "#b8a888", rough=0.5),
        "tail": material("tail-smoke", "#3fbfa8", rough=0.1, emit="#1f9c88", strength=1.2, alpha=0.6, coat=1.0),
        "spark": material("tail-spark", "#fff1b0", emit="#ffe9a0", strength=4.0),
    }
    if hasattr(m["tail"], "surface_render_method"):
        m["tail"].surface_render_method = "BLENDED"

    # Body: square top tapering to the tail, softened by bevel + subdivision; screen recess cut in.
    shell = body_mesh()
    bevel(shell, 0.2, 3, angle=False)
    shell.modifiers.new("Subsurf", "SUBSURF").levels = 2
    apply_all(shell)
    cut(shell, box("cutter", (0.94, 0.2, SCREEN_H), (0.02, -0.6, SCREEN_Z), None, 0.1, 6))
    finish(shell, m["ivory"])
    bvh = BVHTree.FromObject(shell, bpy.context.evaluated_depsgraph_get())

    # Panel seams down the front corners, cut as shallow grooves that follow the surface.
    for sx in (1, -1):
        pts = []
        for i in range(10):
            z = -0.08 - i * 0.055
            w = 0.66 if z > 0.02 else 0.36 + (0.66 - 0.36) * (z + 0.6) / 0.62
            loc, _ = front(bvh, sx * (w - 0.17), z)
            pts.append(loc)
        cut(shell, tube("seam", pts, 0.011, None))
    bvh = BVHTree.FromObject(shell, bpy.context.evaluated_depsgraph_get())
    shell.data.polygons.foreach_set("use_smooth", [True] * len(shell.data.polygons))

    # Screen glass, raised lip, dark gasket and a thin cyan rim light along the top and right.
    box("screen", (0.9, 0.04, SCREEN_H - 0.04), (0.02, -0.5, SCREEN_Z), m["screen"], 0.08, 5)
    lip_y = front(bvh, 0.02, SCREEN_Z + SCREEN_H / 2 + 0.05)[0].y
    rounded_rect("screen-lip", 0.98, SCREEN_H + 0.04, 0.13, 0.024, (0.02, lip_y + 0.004, SCREEN_Z), m["ivory"])
    rounded_rect("screen-gasket", 0.91, SCREEN_H - 0.03, 0.09, 0.012, (0.02, -0.523, SCREEN_Z), m["joint"])
    top = SCREEN_Z + SCREEN_H / 2 - 0.035
    rim = [(x, top) for x in (-0.1, 0.1, 0.3)]
    rim += [(0.39 + 0.07 * math.sin(a), top - 0.07 + 0.07 * math.cos(a)) for a in (0.4, 0.8, 1.2, 1.57)]
    rim += [(0.46, top - dz) for dz in (0.15, 0.3)]
    tube("screen-rim", [(x, -0.527, z) for x, z in rim], 0.006, m["cyan"])
    # Dim divider above the text band, plus an empty node where R3F anchors the typed text.
    tube("screen-divider", [(x, -0.527, TEXT_Z + 0.115) for x in (-0.36, 0.4)], 0.004, m["divider"])
    text_anchor = bpy.data.objects.new("text-anchor", None)
    bpy.context.collection.objects.link(text_anchor)
    text_anchor.location = (0.02, -0.53, TEXT_Z)

    # Face on the glass (front of the glass is y = -0.52).
    fy = -0.525
    for side, x in EYES_X.items():
        disc(f"{side}-eye", EYE_R, (x, fy, EYE_Z), m["glow"], scale=(1, 1.15, 1))
        disc(f"{side}-pupil", 0.062, (x, fy - 0.003, EYE_Z), m["pupil"])
        disc(f"{side}-pupil-glint", 0.018, (x + 0.025, fy - 0.0045, EYE_Z + 0.028), m["glint"], verts=16)
    # Lids: screen-coloured shutters hanging from the eye top; the rig scales their height (= cover).
    for side, tilt in (("left", 0.12), ("right", -0.12)):
        bpy.ops.mesh.primitive_plane_add(size=1, location=(EYES_X[side], fy - 0.006, EYE_TOP),
                                         rotation=(math.pi / 2, tilt, 0))
        lid = active()
        lid.name = f"{side}-lid"
        lid.data.transform(Matrix.Translation((0, -EYE_HALF, 0)) @ Matrix.Diagonal((0.34, 2 * EYE_HALF + 0.01, 1, 1)))
        finish(lid, m["screen"], smooth=False)
    for side, raise_, tilt in (("left", 0.021, -0.1), ("right", 0.0, 0.05)):
        arc(f"{side}-brow", BROW_R, math.pi / 2 - BROW_ARC / 2, math.pi / 2 + BROW_ARC / 2, 0.034,
            (EYES_X[side], fy - 0.014, BROW_Z + raise_), m["brow"], rot_y=tilt)
    arc("mouth", 0.1, math.pi, 2 * math.pi, 0.024, (MOUTH[0], fy - 0.004, MOUTH[1]), m["glow"], rot_y=-0.14)

    # Front details placed on the surface: orange vent, three ports, slotted grille, sensor.
    for i, z in enumerate((-0.17, -0.225)):
        loc, n = front(bvh, -0.17, z)
        align(box(f"vent-{i}", (0.2, 0.035, 0.036), loc, m["orange"], 0.014, 3), n)
    for i, x in enumerate((0.14, 0.21, 0.28)):
        loc, n = front(bvh, x, -0.2)
        sphere(f"port-{i}", 0.022, loc - n * 0.008, m["joint"], segs=(12, 6))
    for i in range(4):
        loc, n = front(bvh, 0.21, -0.33 - i * 0.04)
        align(box(f"grille-{i}", (0.2, 0.02, 0.016), loc - n * 0.004, m["joint"], 0.007, 2), n)
    loc, n = front(bvh, 0.44, SCREEN_Z + SCREEN_H / 2 + 0.09)
    align(cylinder("sensor", 0.035, 0.03, loc, m["steel"], verts=24, edge=0.008), n, axis=(0, 0, 1))
    loc, n = surface_hit(bvh, (0.12, -0.05, 2), (0, 0, -1))
    box("top-button", (0.26, 0.15, 0.08), loc, m["orange"], 0.03, 4)
    box("top-ridge", (0.5, 0.36, 0.05), loc + Vector((-0.12, 0.06, -0.012)), m["joint"], 0.02, 3)

    # Ears: teal discs with a steel rim, a raised inner step and an orange gripped pill.
    for sx in (1, -1):
        side = "left" if sx > 0 else "right"
        ez = SCREEN_Z + 0.08
        loc, _ = surface_hit(bvh, (2 * sx, 0.04, ez), (-sx, 0, 0))
        cx = loc.x + 0.02 * sx
        cylinder(f"{side}-ear", 0.32, 0.12, (cx, 0.04, ez), m["teal"], rot=(0, math.pi / 2, 0), edge=0.05, segments=4)
        bpy.ops.mesh.primitive_torus_add(major_radius=0.32, minor_radius=0.016, location=(cx - 0.055 * sx, 0.04, ez),
                                         rotation=(0, math.pi / 2, 0))
        finish(active(), m["steel"]).name = f"{side}-ear-rim"
        cylinder(f"{side}-ear-step", 0.23, 0.04, (cx + 0.065 * sx, 0.04, ez), m["teal"], rot=(0, math.pi / 2, 0),
                 edge=0.015, segments=3)
        box(f"{side}-ear-pill", (0.05, 0.1, 0.22), (cx + 0.095 * sx, -0.02, ez), m["orange"], 0.05, 5)
        join([box(f"{side}-grip-{i}", (0.02, 0.07, 0.012), (cx + 0.122 * sx, -0.02, ez + dz), m["joint"], 0.004, 2)
              for i, dz in enumerate((-0.06, 0, 0.06))], f"{side}-ear-grips")

    # Dark collar where the body hands over to the tail.
    bpy.ops.mesh.primitive_torus_add(major_radius=0.3, minor_radius=0.05, location=(0, 0, -0.56))
    collar = finish(active(), m["joint"])
    collar.name = "tail-collar"
    collar.scale = (1.05, 0.95, 1)

    # Arms: dark socket in the body side, ball shoulder, ribbed joints, barrel forearm, big hands.
    for sx in (1, -1):
        side = "left" if sx > 0 else "right"
        sock, _ = surface_hit(bvh, (2 * sx, -0.1, -0.22), (-sx, 0, 0))
        cylinder(f"{side}-socket", 0.14, 0.1, sock + Vector((0.01 * sx, 0, 0)), m["joint"], rot=(0, math.pi / 2, 0),
                 edge=0.03)
        sh = sock + Vector((0.11 * sx, 0, 0))
        el = sh + Vector((0, 0, -0.32))
        wr = el + Vector((0, 0, -0.36))
        hd = wr + Vector((0, 0, -0.14))
        JOINTS[side] = (sh, el, wr)
        sphere(f"{side}-shoulder", 0.105, sh, m["joint"])
        capsule(f"{side}-upper-arm", 0.1, 0.3, sh + Vector((0, 0, -0.16)), m["ivory"])
        for dz in (0.035, -0.035):
            bpy.ops.mesh.primitive_torus_add(major_radius=0.085, minor_radius=0.022, location=el + Vector((0, 0, dz)))
            finish(active(), m["joint"]).name = f"{side}-elbow-rib"
        sphere(f"{side}-forearm", 0.13, el + Vector((0, 0, -0.19)), m["ivory"], scale=(0.95, 0.95, 1.35))
        for dz in (0.03, -0.01):
            bpy.ops.mesh.primitive_torus_add(major_radius=0.08, minor_radius=0.02, location=wr + Vector((0, 0, dz)))
            finish(active(), m["joint"]).name = f"{side}-wrist-rib"
        box(f"{side}-palm", (0.26, 0.16, 0.22), hd, m["teal"], 0.07, 4)
        cylinder(f"{side}-palm-button", 0.062, 0.03, hd + Vector((0, -0.085, 0.01)), m["orange"],
                 rot=(math.pi / 2, 0, 0), verts=24, edge=0.01)
        # Three phalanges per finger, two per thumb. Index and middle can extend independently.
        # Place the socket and its entire finger chain on the outer palm edge together.
        # Translating only the animated finger leaves it detached from the black socket.
        finger_base = hd + Vector((0, -0.06, 0))
        knuckles = []
        groups = {part: {"proximal": [], "middle": [], "distal": []} for part in ("point", "middle", "curl")}
        for i, fx in enumerate((-0.09, -0.03, 0.03, 0.09)):
            grp = groups[("middle" if abs(fx) < 0.05 else "point") if fx * sx > 0 else "curl"]
            knuckles.append(sphere(f"{side}-k{i}", 0.042, finger_base + Vector((fx, -0.01, -0.115)), m["joint"], segs=(14, 8)))
            grp["proximal"].append(capsule(f"{side}-p{i}", 0.04, 0.115, finger_base + Vector((fx, -0.014, -0.1725)), m["ivory"]))
            grp["proximal"].append(sphere(f"{side}-m{i}", 0.034, finger_base + Vector((fx, -0.015, -0.23)), m["joint"], segs=(14, 8)))
            grp["middle"].append(capsule(f"{side}-s{i}", 0.038, 0.085, finger_base + Vector((fx, -0.02, -0.2725)), m["ivory"]))
            grp["middle"].append(sphere(f"{side}-n{i}", 0.032, finger_base + Vector((fx, -0.025, -0.315)), m["joint"], segs=(14, 8)))
            grp["distal"].append(capsule(f"{side}-d{i}", 0.035, 0.07, finger_base + Vector((fx, -0.034, -0.35)), m["ivory"],
                               rot=(-0.25, 0, 0)))
        thumb_head = hd + Vector((0.13 * sx, -0.075, -0.03))
        knuckles.append(sphere(f"{side}-tk", 0.04, thumb_head, m["joint"], segs=(14, 8)))
        join(knuckles, f"{side}-knuckles")
        for part, pieces in groups.items():
            join(pieces["proximal"], f"{side}-{part}")
            join(pieces["middle"], f"{side}-{part}-mid")
            join(pieces["distal"], f"{side}-{part}-tip")
        thumb_dir = Vector((0.65 * sx, -0.23, -0.725)).normalized()
        thumb_rotation = Vector((0, 0, -1)).rotation_difference(thumb_dir).to_euler()
        thumb_joint = thumb_head + thumb_dir * 0.09
        join([capsule(f"{side}-thumb-base", 0.04, 0.09, thumb_head + thumb_dir * 0.045, m["ivory"], rot=thumb_rotation),
              sphere(f"{side}-thumb-joint", 0.033, thumb_joint, m["joint"], segs=(14, 8))], f"{side}-thumb")
        capsule(f"{side}-thumb-tip", 0.036, 0.07, thumb_joint + thumb_dir * 0.035, m["ivory"], rot=thumb_rotation)
        JOINTS[f"{side}-thumb-tip"] = thumb_joint
        JOINTS[f"{side}-fingers"] = (finger_base + Vector((0.06 * sx, -0.01, -0.115)),
                                     finger_base + Vector((-0.06 * sx, -0.01, -0.115)), thumb_head)

    # Smoke tail: a tapering NURBS tube; the R3F scene gives it a moving glow shader.
    cu = bpy.data.curves.new("tail", "CURVE")
    cu.dimensions = "3D"
    cu.bevel_depth = 1.0
    cu.bevel_resolution = 8
    cu.resolution_u = 20
    cu.use_fill_caps = True
    sp = cu.splines.new("NURBS")
    sp.points.add(len(TAIL_PTS) - 1)
    for p, (x, y, z), r in zip(sp.points, TAIL_PTS, TAIL_R):
        p.co = (x, y, z, 1)
        p.radius = r
    sp.order_u = 4
    sp.use_endpoint_u = True
    tail = bpy.data.objects.new("tail", cu)
    bpy.context.collection.objects.link(tail)
    cu.materials.append(m["tail"])
    apply_all(tail)
    finish(tail)
    for i in (1, 2, 3):
        for j, f in enumerate((0.3, 0.7)):
            c = Vector(TAIL_PTS[i]).lerp(Vector(TAIL_PTS[i + 1]), f) + Vector((0.06 * (1 if (i + j) % 2 else -1), 0, 0))
            sphere(f"tail-spark-{i}-{j}", 0.022, c, m["spark"], segs=(10, 6))

    return m


# ---------- rig, clips, export ----------
# Rigid parts ride on bones (bone parenting); only the tail is skinned. Every non-tail bone points
# straight up with roll 0, so its local axes are (X, Z_up, -Y) == three.js (x, y, z): pose values
# from src/genie/pose.ts map onto bone-local transforms one to one.

FACE_BONES = {  # bone: head position
    **{f"{side}-pupil": (x, -0.528, EYE_Z) for side, x in EYES_X.items()},
    **{f"{side}-lid": (x, -0.531, EYE_TOP) for side, x in EYES_X.items()},
    "left-brow": (EYES_X["left"], -0.539, BROW_Z + 0.021), "right-brow": (EYES_X["right"], -0.539, BROW_Z),
    "mouth": (MOUTH[0], -0.529, MOUTH[1]),
}


def bone_for(name):
    for b in FACE_BONES:
        if name == b or name.startswith(b + "-"):
            return b
    if name.startswith(("left-eye", "right-eye", "text-anchor")):
        return "face"
    for side in ("left", "right"):
        if name in (f"{side}-shoulder", f"{side}-upper-arm"):
            return f"{side}-arm"
        if name.startswith((f"{side}-elbow", f"{side}-forearm", f"{side}-wrist")):
            return f"{side}-forearm"
        for part in ("point", "middle", "curl", "thumb"):
            if name in (f"{side}-{part}", f"{side}-{part}-mid", f"{side}-{part}-tip"):
                return name
        if name.startswith((f"{side}-palm", f"{side}-knuckles")):
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
    bone("face", (0.02, -0.52, SCREEN_Z), "body")
    for name, head in FACE_BONES.items():
        bone(name, head, "face")
    for sx, side in ((1, "left"), (-1, "right")):
        sh, el, wr = JOINTS[side]
        bone(f"{side}-arm", sh, "body")
        bone(f"{side}-forearm", el, f"{side}-arm")
        bone(f"{side}-hand", wr, f"{side}-forearm")
        for part, head in zip(("point", "curl", "thumb"), JOINTS[f"{side}-fingers"]):
            bone(f"{side}-{part}", head, f"{side}-hand")
        bone(f"{side}-middle", JOINTS[f"{side}-fingers"][0] + Vector((-0.03 * sx, 0, 0)), f"{side}-hand")
        for part in ("point", "middle", "curl"):
            head = eb[f"{side}-{part}"].head.copy() + Vector((0, -0.005, -0.115))
            bone(f"{side}-{part}-mid", head, f"{side}-{part}")
            bone(f"{side}-{part}-tip", head + Vector((0, -0.01, -0.085)), f"{side}-{part}-mid")
        bone(f"{side}-thumb-tip", JOINTS[f"{side}-thumb-tip"], f"{side}-thumb")
    prev = "body"
    for i in range(len(TAIL_PTS) - 1):
        name = "tail" if i == 0 else f"tail-{i}"
        b = bone(name, TAIL_PTS[i], prev, tail=TAIL_PTS[i + 1])
        b.use_connect = i > 0
        prev = name
    bpy.ops.object.mode_set(mode="OBJECT")

    # Bone-parent every rigid part without moving it (a bone child sits at the bone's tail).
    for o in list(col.objects):
        if o is arm or o.name == "tail":
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


def wind_up_hands(arm, amount, turn, middle):
    """Bake two-bone reach and wrist turns into the GLB, only for the crank gesture.
    Anatomical left (screen-right) stays palm-up, matching the supplied frame reference.
    """
    pb = arm.pose.bones
    bpy.context.view_layer.update()
    body_matrix = pb["body"].matrix @ arm.data.bones["body"].matrix_local.inverted()
    rest_rotation = arm.data.bones["left-arm"].matrix_local.to_3x3().to_4x4()

    def blend_matrix(bone, desired, weight=amount):
        original = bone.matrix_basis.copy()
        bone.matrix = desired
        target = bone.matrix_basis.copy()
        loc0, rot0, scale0 = original.decompose()
        loc1, rot1, scale1 = target.decompose()
        bone.matrix_basis = Matrix.LocRotScale(loc0.lerp(loc1, weight), rot0.slerp(rot1, weight), scale0.lerp(scale1, weight))
        bpy.context.view_layer.update()

    def reach(side, wrist, hand_rotation):
        shoulder = arm.data.bones[f"{side}-arm"].head_local.copy()
        l1 = (arm.data.bones[f"{side}-forearm"].head_local - shoulder).length
        l2 = (arm.data.bones[f"{side}-hand"].head_local - arm.data.bones[f"{side}-forearm"].head_local).length
        # Move the winding arm along a wrist path before solving its elbow.
        # Blending each joint's world target separately stretches the arm and twists the wrist.
        if side == "right":
            original_wrist = body_matrix.inverted() @ pb[f"{side}-hand"].head
            original_elbow = body_matrix.inverted() @ pb[f"{side}-forearm"].head
            wrist = original_wrist.lerp(wrist, amount)
        delta = wrist - shoulder
        distance = min(delta.length, l1 + l2 - 0.005)
        axis = delta.normalized()
        wrist = shoulder + axis * distance
        pole = Vector((1 if side == "left" else -1, 0.1, -0.65))
        if side == "right":
            start_pole = original_elbow - shoulder
            start_pole = (start_pole - axis * start_pole.dot(axis)).normalized()
            end_pole = (pole - axis * pole.dot(axis)).normalized()
            pole = start_pole.lerp(end_pole, amount)
        pole = (pole - axis * pole.dot(axis)).normalized()
        along = (l1*l1 - l2*l2 + distance*distance) / (2 * distance)
        elbow = shoulder + axis * along + pole * math.sqrt(max(0, l1*l1 - along*along))
        for name, origin, end in ((f"{side}-arm", shoulder, elbow), (f"{side}-forearm", elbow, wrist)):
            direction = (end-origin).normalized()
            swing = Vector((0, 0, -1)).rotation_difference(direction).to_matrix().to_4x4()
            if side == "right" and name.endswith("forearm"):
                # Pronate the forearm around its own axis, keeping the wrist straight.
                # The orange palm button then faces down throughout the winding.
                normal = (swing.to_3x3() @ Vector((0, -1, 0))).normalized()
                down = Vector((0, 0, -1))
                down = (down - direction * down.dot(direction)).normalized()
                roll = math.atan2(direction.dot(normal.cross(down)), normal.dot(down))
                # Keep the same rotation branch as the elbow passes the +/-pi boundary.
                # Scaling a wrapped signed angle by the entry blend produces a visible snap.
                if roll < 0:
                    roll += 2 * math.pi
                swing = Matrix.Rotation(roll * amount, 4, direction) @ swing
            blend_matrix(pb[name], body_matrix @ Matrix.Translation(origin) @ swing @ rest_rotation,
                         1 if side == "right" else amount)
        if side == "right":
            # Keep the wrist aligned with its forearm; the arm describes the circle.
            pb[f"{side}-hand"].rotation_euler = (0, 0, 0)
        else:
            blend_matrix(pb[f"{side}-hand"], body_matrix @ Matrix.Translation(wrist) @ hand_rotation @ rest_rotation)

    # Palm (-Y) faces up; the knuckles point towards the viewer.
    fist_rotation = Matrix(((1, 0, 0, 0), (0, 0, 1, 0), (0, -1, 0, 0), (0, 0, 0, 1)))
    reach("left", Vector((0.27, -0.64, -0.30)), fist_rotation)
    # The second closed fist circles above/beside it, rather than spinning its wrist.
    finish = min(max((middle - 0.85) / 0.15, 0), 1)
    finish = finish * finish * (3 - 2 * finish)
    radius = 0.075 * (1 - finish)
    wrist = Vector((-0.43 - 0.09*finish + radius*math.sin(turn), -0.61,
                    -0.22 - 0.08*finish + radius*math.cos(turn)))
    reach("right", wrist, None)
    for side in ("left", "right"):
        for part in ("point", "curl", "middle"):
            finger = pb[f"{side}-{part}"]
            finger.rotation_euler.x = finger.rotation_euler.x * (1-amount) - 1.35 * amount
            pb[f"{side}-{part}-mid"].rotation_euler.x = -1.3 * amount
            pb[f"{side}-{part}-tip"].rotation_euler.x = -0.75 * amount
        thumb = pb[f"{side}-thumb"]
        thumb.rotation_euler.x = thumb.rotation_euler.x * (1-amount) - 0.6 * amount
        pb[f"{side}-thumb-tip"].rotation_euler = (-0.4 * amount, (0.5 if side == "left" else -0.5) * amount, 0)
    finger = pb["left-middle"]
    # With palm up, -pi/2 sends the middle finger straight up from the knuckle.
    finger.rotation_euler.x += (-math.pi/2 + 1.35) * middle * amount
    pb["left-middle-mid"].rotation_euler.x += 1.3 * middle * amount
    pb["left-middle-tip"].rotation_euler.x += 1.0 * middle * amount


def apply_pose(arm, p, t):
    pb = arm.pose.bones
    pb["body"].location = (0, p["y"], 0)
    pb["body"].rotation_euler = (p["tiltX"], 0, p["tiltZ"])  # yaw stays in the scene
    # Reach baking must not carry a wrist transform from the preceding sampled frame.
    for side in ("left", "right"):
        pb[f"{side}-hand"].rotation_euler = (0, 0, 0)
        for part in ("arm", "forearm", "hand"):
            pb[f"{side}-{part}"].location = (0, 0, 0)
    # Anatomical left (+X) is the preview's "r" side.
    pb["left-arm"].rotation_euler = (-p["rArmFwd"], 0, p["rArmOut"])
    pb["right-arm"].rotation_euler = (-p["lArmFwd"], 0, -p["lArmOut"])
    pb["left-forearm"].rotation_euler = (-p["rElbow"], 0, 0)
    pb["right-forearm"].rotation_euler = (-p["lElbow"], 0, 0)
    for side, k in (("left", "r"), ("right", "l")):
        for part in ("point", "middle", "curl", "thumb"):
            pb[f"{side}-{part}"].location = (0, 0, 0)
        for part in ("point", "middle", "curl"):
            pb[f"{side}-{part}-mid"].rotation_euler = (0, 0, 0)
            pb[f"{side}-{part}-tip"].rotation_euler = (0, 0, 0)
        pb[f"{side}-thumb-tip"].rotation_euler = (0, 0, 0)
        grip, point = p[f"{k}Grip"], p[f"{k}Point"]
        pb[f"{side}-curl"].rotation_euler = (-grip * 1.3, 0, 0)  # curl toward the palm (-Y)
        pb[f"{side}-point"].rotation_euler = (-grip * (1 - point) * 1.3, 0, 0)
        middle = max(point, p.get(f"{k}Middle", 0))
        pb[f"{side}-middle"].rotation_euler = (-grip * (1 - middle) * 1.3, 0, 0)
        pb[f"{side}-thumb"].rotation_euler = (-grip * 0.8, 0, 0)
        for part, extension in (("point", point), ("middle", middle), ("curl", 0)):
            pb[f"{side}-{part}-mid"].rotation_euler.x = -grip * (1 - extension) * 0.45
            pb[f"{side}-{part}-tip"].rotation_euler.x = -grip * (1 - extension) * 0.3
        pb[f"{side}-thumb-tip"].rotation_euler = (-grip * 0.2, (1 if side == "left" else -1) * grip * 0.25, 0)
    for side in ("left", "right"):
        pb[f"{side}-pupil"].location = (p["lookX"] * 0.045, p["lookY"] * 0.05, 0)
    cover = max(lid_cover(p["eyeOpen"], p["lid"]), 0.001)
    pb["left-lid"].scale = (1, cover, 1)
    pb["right-lid"].scale = (1, cover, 1)
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

    if p.get("crank", 0) > 0:
        wind_up_hands(arm, p["crank"], p["crankTurn"], p["rMiddle"])


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
        previous_eulers = {}
        for f, p in enumerate(frames):
            apply_pose(arm, p, f / fps)
            for pb in arm.pose.bones:
                if pb.name in previous_eulers:
                    pb.rotation_euler = pb.rotation_euler.to_quaternion().to_euler(pb.rotation_mode, previous_eulers[pb.name])
                previous_eulers[pb.name] = pb.rotation_euler.copy()
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


def closeup(name, target, offset=(0.9, -1.6, 0.3), lens=85):
    cam = bpy.data.objects["Camera"]
    keep = (cam.location.copy(), cam.rotation_euler.copy(), cam.data.lens)
    cam.location = Vector(target) + Vector(offset)
    cam.data.lens = lens
    look_at(cam, target)
    path = render(name)
    cam.location, cam.rotation_euler, cam.data.lens = keep
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
