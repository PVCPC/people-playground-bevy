"""Convert a local AssetRipper export into the small cache used by the Rust prototype."""
import argparse
import json
import math
import re
import shutil
import struct
from pathlib import Path

import yaml


def documents(text):
    result = {}
    for kind, object_id, body in re.findall(
        r"--- !u!(\d+) &(-?\d+)\s*\n(.*?)(?=\n--- !u!|\Z)", text, re.S
    ):
        name, data = next(iter(yaml.safe_load(body).items()))
        result[int(object_id)] = (name, data)
    if not result:
        raise ValueError("No Unity objects found")
    return result


def xy(value):
    return [float(value["x"]), float(value["y"])]


def source_stamp(game, relative):
    stat = (game / relative).stat()
    return {"path": relative, "bytes": stat.st_size, "modified_ns": stat.st_mtime_ns}


def prepare(game, exported, output, write):
    required = ["People Playground.exe", "People Playground_Data/Managed/Assembly-CSharp.dll",
                "People Playground_Data/resources.assets", "People Playground_Data/sharedassets0.assets"]
    for relative in required:
        if not (game / relative).is_file():
            raise ValueError(f"Required installed game file is missing: {game / relative}")
    assets = exported / "Assets"
    prefab = assets / "GameObject/Person.prefab"
    objects = documents(prefab.read_text(encoding="utf-8-sig"))
    guids = {}
    for meta in assets.rglob("*.meta"):
        match = re.search(r"^guid: ([0-9a-f]+)$", meta.read_text(encoding="utf-8-sig"), re.M)
        if match:
            guids[match[1]] = meta.with_suffix("")
    bodies = {oid: data for oid, (kind, data) in objects.items() if kind == "Rigidbody2D"}
    names = {oid: objects[body["m_GameObject"]["fileID"]][1]["m_Name"] for oid, body in bodies.items()}
    parts, joints, textures = [], [], {}

    def pose(transform_id, visiting=None):
        visiting = set() if visiting is None else visiting
        if transform_id in visiting:
            raise ValueError("Cyclic transform hierarchy")
        visiting.add(transform_id)
        transform = objects[transform_id][1]
        position, scale = xy(transform["m_LocalPosition"]), xy(transform["m_LocalScale"])
        rotation = transform["m_LocalRotation"]
        angle = 2 * math.atan2(rotation["z"], rotation["w"])
        parent = transform["m_Father"]["fileID"]
        if parent:
            p, a, s = pose(parent, visiting)
            x, y = position[0] * s[0], position[1] * s[1]
            position = [p[0] + math.cos(a)*x - math.sin(a)*y, p[1] + math.sin(a)*x + math.cos(a)*y]
            angle += a
            scale = [scale[i] * s[i] for i in range(2)]
        return position, angle, scale

    for body_id, body in bodies.items():
        go = objects[body["m_GameObject"]["fileID"]][1]
        components = [objects[c["component"]["fileID"]] for c in go["m_Component"]]
        transform_id = next(c["component"]["fileID"] for c in go["m_Component"]
                            if objects[c["component"]["fileID"]][0] == "Transform")
        position, angle, scale = pose(transform_id)
        if any(abs(s - 1) > 0.0001 for s in scale):
            raise ValueError("Scaled rigid bodies need explicit collider conversion")
        renderer = next(data for kind, data in components if kind == "SpriteRenderer")
        sprite_path = guids[renderer["m_Sprite"]["guid"]]
        sprite = next(iter(documents(sprite_path.read_text(encoding="utf-8-sig")).values()))[1]
        texture_guid = sprite["m_RD"]["texture"]["guid"]
        texture = guids[texture_guid]
        if texture.suffix.lower() != ".png":
            raise ValueError(f"Expected a PNG texture: {texture}")
        with texture.open("rb") as stream:
            header = stream.read(24)
        if header[:8] != b"\x89PNG\r\n\x1a\n":
            raise ValueError(f"Invalid PNG: {texture}")
        width, height = struct.unpack(">II", header[16:24])
        rect = sprite["m_Rect"]
        x, y, w, h = (float(rect[k]) for k in ("x", "y", "width", "height"))
        if not (w > 0 and h > 0 and x >= 0 and y >= 0 and x+w <= width and y+h <= height):
            raise ValueError("Sprite rectangle is outside its texture")
        textures[texture_guid + ".png"] = texture
        colliders = []
        for kind, data in components:
            if kind == "BoxCollider2D" and not data.get("m_IsTrigger", 0):
                colliders.append({"offset": xy(data["m_Offset"]), "size": xy(data["m_Size"])})
        if not colliders:
            raise ValueError(f"No supported collider on {names[body_id]}")
        parts.append({"name": names[body_id], "position": position, "angle": angle,
                      "mass": float(body["m_Mass"]), "linear_drag": float(body["m_LinearDrag"]),
                      "angular_drag": float(body["m_AngularDrag"]), "colliders": colliders,
                      "texture": texture_guid + ".png", "rect": [x, height-y-h, w, h],
                      "size": [w/sprite["m_PixelsToUnits"], h/sprite["m_PixelsToUnits"]],
                      "pivot": xy(sprite["m_Pivot"]), "layer": renderer["m_SortingOrder"]})
        for kind, data in components:
            if kind != "HingeJoint2D":
                continue
            limits = data["m_AngleLimits"]
            joints.append({"a": names[body_id], "b": names[data["m_ConnectedRigidBody"]["fileID"]],
                           "anchor_a": xy(data["m_Anchor"]), "anchor_b": xy(data["m_ConnectedAnchor"]),
                           "limits": [math.radians(limits[k]) for k in ("m_LowerAngle", "m_UpperAngle")]
                           if data["m_UseLimits"] else None})
    if len({p["name"] for p in parts}) != len(parts) or len(joints) != len(parts) - 1:
        raise ValueError("Human rig is not a single named body tree")
    stamps = [source_stamp(game, p) for p in required]
    rig = {"source_files": stamps, "parts": parts, "joints": joints}
    print(json.dumps({"preview": not write, "bodies": len(parts), "joints": len(joints),
                      "textures": list(textures), "destination": str(output)}))
    if write:
        output.mkdir(parents=True, exist_ok=True)
        if (output / "human.json").exists():
            raise ValueError("Cache already exists; choose a new --output directory")
        for filename, source in textures.items():
            if (output / filename).exists():
                raise ValueError(f"Cache texture already exists: {filename}")
            shutil.copyfile(source, output / filename)
        (output / "human.json").write_text(json.dumps(rig, indent=2), encoding="utf-8")
    return rig


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game", required=True, type=Path)
    parser.add_argument("--export", type=Path, default=Path(__file__).resolve().parents[1] / "unity-export/ExportedProject")
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[1] / "local-cache")
    parser.add_argument("--write", action="store_true", help="Write the previewed cache; default is dry-run")
    args = parser.parse_args()
    prepare(args.game, args.export, args.output, args.write)
