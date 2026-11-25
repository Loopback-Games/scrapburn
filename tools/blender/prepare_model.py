"""Prepare a Tripo model (kart or prop) for the engine: join meshes, decimate to a triangle budget, keep only the
base color texture at 1024x1024, export GLB. Run headless:

    flatpak run org.blender.Blender -b --python tools/blender/prepare_model.py -- IN.glb OUT.glb [TRIS]
"""

import sys

import bpy
import mathutils

argv = sys.argv[sys.argv.index("--") + 1:]
src, dst = argv[0], argv[1]
budget = int(argv[2]) if len(argv) > 2 else 24000

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=src)
meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
bpy.ops.object.select_all(action="DESELECT")
for o in meshes:
    o.select_set(True)
bpy.context.view_layer.objects.active = meshes[0]
if len(meshes) > 1:
    bpy.ops.object.join()
ob = bpy.context.view_layer.objects.active
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)

tris = sum(len(p.vertices) - 2 for p in ob.data.polygons)
print(f"PREP source triangles: {tris}")
if tris > budget:
    mod = ob.modifiers.new("decimate", "DECIMATE")
    mod.ratio = budget / tris
    bpy.ops.object.modifier_apply(modifier=mod.name)
tris = sum(len(p.vertices) - 2 for p in ob.data.polygons)
print(f"PREP final triangles: {tris}")

# Keep only base color; drop normal/metallic/roughness maps (the engine shades its own way).
for mat in bpy.data.materials:
    if not mat.use_nodes:
        continue
    bsdf = next((n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
    if bsdf is None:
        continue
    for name in ("Normal", "Metallic", "Roughness", "Emission Color", "Alpha"):
        sock = bsdf.inputs.get(name)
        if sock is None:
            continue
        for link in list(sock.links):
            mat.node_tree.links.remove(link)
    bsdf.inputs["Metallic"].default_value = 0.0
    bsdf.inputs["Roughness"].default_value = 0.6
for img in bpy.data.images:
    if img.size[0] > 1024 or img.size[1] > 1024:
        img.scale(1024, 1024)

bb = [ob.matrix_world @ mathutils.Vector(c) for c in ob.bound_box]
lo = mathutils.Vector((min(v.x for v in bb), min(v.y for v in bb), min(v.z for v in bb)))
hi = mathutils.Vector((max(v.x for v in bb), max(v.y for v in bb), max(v.z for v in bb)))
print(f"PREP bounds (Blender Z-up): min {tuple(round(x, 3) for x in lo)} max {tuple(round(x, 3) for x in hi)}")

bpy.ops.export_scene.gltf(
    filepath=dst,
    export_format="GLB",
    use_selection=False,
    export_image_format="JPEG",
    export_jpeg_quality=88,
    export_tangents=False,
    export_animations=False,
)
print(f"PREP wrote {dst}")
