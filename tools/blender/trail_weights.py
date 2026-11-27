"""Bake Scrapburn's two steering joints from the Loopback models source scenes.

blender -b --python tools/blender/trail_weights.py -- ../models
"""

import json
import struct
import sys
from pathlib import Path

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

KARTS = Path(__file__).resolve().parents[2] / "assets/models/karts"


def positions(name: str) -> list[Vector]:
    """Read the shipped mesh in vertex order, converting glTF to Blender axes."""
    data = (KARTS / name).read_bytes()
    size = struct.unpack_from("<I", data, 12)[0]
    doc = json.loads(data[20 : 20 + size])
    primitive = doc["meshes"][0]["primitives"][0]
    accessor = doc["accessors"][primitive["attributes"]["POSITION"]]
    view = doc["bufferViews"][accessor["bufferView"]]
    offset = 28 + size + view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    points = []
    for i in range(accessor["count"]):
        x, y, z = struct.unpack_from(
            "<fff", data, offset + i * view.get("byteStride", 12)
        )
        points.append(Vector((x, -z, y)))
    return points


def controls(source: Path) -> None:
    bpy.ops.wm.open_mainfile(filepath=str(source / "source/north-star-570-parts.blend"))
    groups = [([], []), ([], [])]
    moving = {"Handlebars", "BrakeLever", "Display", "ThumbThrottle"}
    for obj in bpy.data.objects:
        if obj.type != "MESH" or not obj.name.startswith("SM_NorthStar570_"):
            continue
        vertices, faces = groups[obj.name.removeprefix("SM_NorthStar570_") in moving]
        start = len(vertices)
        vertices.extend(obj.matrix_world @ vertex.co for vertex in obj.data.vertices)
        faces.extend(
            tuple(start + i for i in face.vertices) for face in obj.data.polygons
        )
    body, bars = [BVHTree.FromPolygons(v, f) for v, f in groups]
    points = positions("north-star-570.glb")
    mask = bytearray((len(points) + 7) // 8)
    for i, point in enumerate(points):
        if (
            point.z >= 0.85
            and bars.find_nearest(point)[3] < body.find_nearest(point)[3]
        ):
            mask[i // 8] |= 1 << (i % 8)
    if not any(mask):
        raise ValueError("No handlebar vertices found")
    (KARTS / "north-star-controls.mask").write_bytes(mask)


def joint_weights(name: str) -> tuple[float, float]:
    if name.startswith(("hand", "thumb", "index", "middle", "ring", "pinky")):
        return 1, 0
    if name.startswith("lowerarm"):
        return 0.70, 0.30
    if name.startswith("upperarm"):
        return 0.25, 0.75
    if name.startswith(("neck", "head", "clavicle", "spine_03", "spine_02")):
        return 0, 1
    if name.startswith("spine_01"):
        return 0, 0.55
    return 0, 0


def rider(models: Path, quad: Path) -> None:
    sys.path.insert(0, str(models / "scripts"))
    from characters import rider_body, rider_rig

    source = models / "assets/characters/rider"
    bpy.ops.wm.open_mainfile(filepath=str(source / "source/rider.blend"))
    sockets = json.loads((quad / "parts/sockets.json").read_text())
    arm = next(obj for obj in bpy.data.objects if obj.type == "ARMATURE")
    rider_rig.pose_seated(
        arm, json.loads((source / "exports/rider.json").read_text()), sockets
    )
    mesh = bpy.data.objects["SK_Rider_LOD1"].evaluated_get(
        bpy.context.evaluated_depsgraph_get()
    )
    seat = Vector(rider_body.socket_points(sockets)["Seat"])
    tree = KDTree(len(mesh.data.vertices))
    weights = []
    for vertex in mesh.data.vertices:
        tree.insert(mesh.matrix_world @ vertex.co - seat, vertex.index)
        grip = lean = 0.0
        for group in vertex.groups:
            a, b = joint_weights(mesh.vertex_groups[group.group].name.lower())
            grip += a * group.weight
            lean += b * group.weight
        weights.append((grip, lean))
    tree.balance()
    packed = bytearray()
    for point in positions("rider-seated.glb"):
        nearest = tree.find_n(point, 3)
        if nearest[0][2] > 0.06:
            raise ValueError("Rider pose differs from the shipped mesh")
        factors = [1 / max(distance, 0.00005) ** 2 for _, _, distance in nearest]
        for joint in range(2):
            value = sum(weights[i][joint] * w for (_, i, _), w in zip(nearest, factors))
            packed.append(round(255 * min(1, value / sum(factors))))
    (KARTS / "rider-seated.weights").write_bytes(packed)


if __name__ == "__main__":
    models = Path(sys.argv[sys.argv.index("--") + 1]).resolve()
    quad = models / "assets/vehicles/north-star-570"
    controls(quad)
    rider(models, quad)
