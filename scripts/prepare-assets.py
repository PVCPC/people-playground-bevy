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


def radians_f32(degrees):
    def single(value):
        return struct.unpack("<f", struct.pack("<f", value))[0]
    return single(single(degrees) * single(0.017453292))


def simulation_settings(exported):
    def load(filename, kind):
        objects = documents((exported / "ProjectSettings" / filename).read_text(encoding="utf-8-sig"))
        return next(data for name, data in objects.values() if name == kind)

    time = load("TimeManager.asset", "TimeManager")
    physics = load("Physics2DSettings.asset", "Physics2DSettings")
    result = {"fixed_delta": float(time["Fixed Timestep"]),
              "gravity": xy(physics["m_Gravity"]),
              "velocity_iterations": int(physics["m_VelocityIterations"]),
              "position_iterations": int(physics["m_PositionIterations"]),
              "max_angular_correction_degrees": float(physics["m_MaxAngularCorrection"]),
              "max_translation": float(physics["m_MaxTranslationSpeed"]),
              "max_rotation_degrees": float(physics["m_MaxRotationSpeed"])}
    if (not math.isfinite(result["fixed_delta"]) or not 0.0001 <= result["fixed_delta"] <= 1
            or not all(math.isfinite(v) for v in result["gravity"])
            or not 1 <= result["velocity_iterations"] <= 256
            or not 1 <= result["position_iterations"] <= 256
            or not 0 <= result["max_angular_correction_degrees"] <= 360
            or not math.isfinite(result["max_translation"]) or result["max_translation"] < 0
            or not math.isfinite(result["max_rotation_degrees"]) or result["max_rotation_degrees"] < 0):
        raise ValueError("Invalid simulation settings")
    return result


def source_stamp(game, relative):
    stat = (game / relative).stat()
    return {"path": relative, "bytes": stat.st_size, "modified_ns": stat.st_mtime_ns}


def muscle_settings(components):
    limb = next((data for kind, data in components
                 if kind == "MonoBehaviour" and "BaseStrength" in data), None)
    if limb is None:
        raise ValueError("Human body has no limb strength settings")
    return {"strength": float(limb["BaseStrength"]),
            "multipliers": [float(m) for m in limb.get("StrengthMultipliers", [])],
            "upright_force": float(limb["FakeUprightForce"]),
            "has_brain": bool(limb["HasBrain"]),
            "do_balance_jerk": bool(limb["DoBalanceJerk"]),
            "balance_muscle_movement": float(limb["BalanceMuscleMovement"]),
            "do_stumble": bool(limb["DoStumble"]),
            "blood_strength_ratio": float(limb["BloodMuscleStrengthRatio"])}


def material_settings(body, components, guids):
    def load(reference):
        path = guids[reference["guid"]]
        return documents(path.read_text(encoding="utf-8-sig"))[reference["fileID"]][1]

    reference = body.get("m_Material", {"fileID": 0})
    if not reference.get("fileID"):
        properties = next((data["Properties"] for kind, data in components
                           if kind == "MonoBehaviour" and "Properties" in data), None)
        if properties is None:
            raise ValueError("Human physical properties are missing")
        reference = load(properties)["PhysicMaterial"]
    if not reference.get("fileID"):
        raise ValueError("Human physics material is missing; implicit Unity defaults are unsupported")
    material = load(reference)
    for kind, collider in components:
        if kind == "BoxCollider2D" and not collider.get("m_IsTrigger", 0):
            override = collider.get("m_Material", {"fileID": 0})
            if override.get("fileID"):
                other = load(override)
                if any(other[key] != material[key] for key in ("friction", "bounciness")):
                    raise ValueError("Different collider materials on one body are unsupported")
    friction, restitution = float(material["friction"]), float(material["bounciness"])
    if not math.isfinite(friction) or friction < 0 or not 0 <= restitution <= 1:
        raise ValueError("Invalid physics material coefficients")
    return {"friction": friction, "restitution": restitution}


def curve(data):
    keys = data["m_Curve"]
    if any(int(k.get("weightedMode", 0)) for k in keys):
        raise ValueError("Weighted animation curve keys are unsupported")
    return {"keys": [[float(k[f]) for f in ("time", "value", "inSlope", "outSlope")] for k in keys],
            "pre": int(data["m_PreInfinity"]), "post": int(data["m_PostInfinity"])}


def poses(objects):
    person = next(data for kind, data in objects.values()
                  if kind == "MonoBehaviour" and "Poses" in data)
    result = []
    for pose in person["Poses"]:
        result.append({"name": pose["Name"], "state": int(pose["State"]),
                       "rigidity": float(pose["Rigidity"]),
                       "force_multiplier": float(pose["ForceMultiplier"]),
                       "stand_upright": bool(pose["ShouldStandUpright"]),
                       "upright_multiplier": float(pose["UprightForceMultiplier"]),
                       "drag_influence": float(pose["DragInfluence"]),
                       "should_stumble": bool(pose["ShouldStumble"]),
                       "animation_speed": float(pose["AnimationSpeedMultiplier"]),
                       "angles": [{"part": angle["Name"], "angle": radians_f32(angle["Angle"]),
                                   "angle_degrees": float(angle["Angle"]),
                                   "rigidity_modifier": float(angle["PoseRigidityModifier"]),
                                   **({"animation": {"start": float(angle["StartAngle"]),
                                                     "end": float(angle["EndAngle"]),
                                                     "duration": float(angle["AnimationDuration"]),
                                                     "random_influence": float(angle["RandomInfluence"]),
                                                     "random_speed": float(angle["RandomSpeed"]),
                                                     "time_offset": float(angle["TimeOffset"]),
                                                     "curve": curve(angle["AnimationCurve"])}}
                                      if angle["Animated"] else {})}
                                  for angle in pose["Angles"]]})
    if not result or result[0]["name"] != "Rest":
        raise ValueError("Human prefab has no default rest pose")
    return result


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
                      "pivot": xy(sprite["m_Pivot"]), "layer": renderer["m_SortingOrder"],
                      "muscle": muscle_settings(components),
                      "physics_material": material_settings(body, components, guids)})
        for kind, data in components:
            if kind != "HingeJoint2D":
                continue
            limits = data["m_AngleLimits"]
            joints.append({"a": names[body_id], "b": names[data["m_ConnectedRigidBody"]["fileID"]],
                           "anchor_a": xy(data["m_Anchor"]), "anchor_b": xy(data["m_ConnectedAnchor"]),
                           "limits": [radians_f32(limits[k]) for k in ("m_LowerAngle", "m_UpperAngle")]
                           if data["m_UseLimits"] else None,
                           "limits_degrees": [float(limits[k]) for k in ("m_LowerAngle", "m_UpperAngle")]
                           if data["m_UseLimits"] else None})
    if len({p["name"] for p in parts}) != len(parts) or len(joints) != len(parts) - 1:
        raise ValueError("Human rig is not a single named body tree")
    stamps = [source_stamp(game, p) for p in required]
    rig = {"source_files": stamps, "parts": parts, "joints": joints, "poses": poses(objects),
           "simulation": simulation_settings(exported)}
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
