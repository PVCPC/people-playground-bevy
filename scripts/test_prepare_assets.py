import unittest
from unittest.mock import patch
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path

spec = spec_from_file_location("prepare_assets", Path(__file__).with_name("prepare-assets.py"))
module = module_from_spec(spec)
spec.loader.exec_module(module)


class UnityDocumentTests(unittest.TestCase):
    def test_simulation_settings_follow_export_and_reject_invalid_values(self):
        contents = {
            "TimeManager.asset": "--- !u!5 &1\nTimeManager:\n  Fixed Timestep: 0.01\n",
            "Physics2DSettings.asset": "--- !u!19 &1\nPhysics2DSettings:\n  m_Gravity: {x: 1, y: -4}\n  m_VelocityIterations: 12\n  m_PositionIterations: 8\n  m_MaxAngularCorrection: 6\n  m_MaxTranslationSpeed: 50\n  m_MaxRotationSpeed: 180\n",
        }
        with patch.object(Path, "read_text", autospec=True,
                          side_effect=lambda path, **kwargs: contents[path.name]):
            self.assertEqual(module.simulation_settings(Path("export")),
                             {"fixed_delta": 0.01, "gravity": [1.0, -4.0],
                              "velocity_iterations": 12, "position_iterations": 8,
                              "max_angular_correction_degrees": 6.0,
                              "max_translation": 50.0, "max_rotation_degrees": 180.0})
            contents["Physics2DSettings.asset"] = contents["Physics2DSettings.asset"].replace(
                "m_PositionIterations: 8", "m_PositionIterations: 0")
            with self.assertRaises(ValueError):
                module.simulation_settings(Path("export"))

    def test_static_pose_preserves_original_degree_value_and_single_precision_radians(self):
        person = {"Poses": [{"Name": "Rest", "Rigidity": 1, "ForceMultiplier": 1,
                             "ShouldStandUpright": 0, "UprightForceMultiplier": 1,
                             "DragInfluence": 0, "State": 0, "ShouldStumble": 1,
                             "AnimationSpeedMultiplier": 1,
                             "Angles": [{"Name": "Leg", "Animated": 0, "Angle": 0.1,
                                         "PoseRigidityModifier": 0}]}]}
        angle = module.poses({1: ("MonoBehaviour", person)})[0]["angles"][0]
        self.assertEqual(angle["angle_degrees"], 0.1)
        self.assertEqual(angle["angle"], 0.001745329238474369)

    def test_material_follows_runtime_properties_and_rejects_mixed_fixtures(self):
        contents = {
            "human.asset": "--- !u!114 &11400000\nMonoBehaviour:\n  PhysicMaterial: {fileID: 6200000, guid: body}\n",
            "body.asset": "--- !u!62 &6200000\nPhysicsMaterial2D:\n  friction: 1\n  bounciness: 0.2\n",
            "other.asset": "--- !u!62 &6200000\nPhysicsMaterial2D:\n  friction: 0.1\n  bounciness: 0\n",
        }
        with patch.object(Path, "read_text", autospec=True,
                          side_effect=lambda path, **kwargs: contents[path.name]):
            guids = {"properties": Path("human.asset"), "body": Path("body.asset"), "other": Path("other.asset")}
            components = [("MonoBehaviour", {"Properties": {"fileID": 11400000, "guid": "properties"}})]
            self.assertEqual(module.material_settings({}, components, guids),
                             {"friction": 1.0, "restitution": 0.2})
            override = {"m_Material": {"fileID": 6200000, "guid": "other"}}
            self.assertEqual(module.material_settings(override, components, guids)["friction"], 0.1)
            components.append(("BoxCollider2D", override))
            with self.assertRaises(ValueError):
                module.material_settings({}, components, guids)
            contents["body.asset"] = contents["body.asset"].replace("friction: 1", "friction: .nan")
            with self.assertRaises(ValueError):
                module.material_settings({}, components[:1], guids)

    def test_multiple_objects_preserve_reference_ids(self):
        text = "%YAML 1.1\n--- !u!1 &123\nGameObject:\n  m_Name: Example\n--- !u!4 &456\nTransform:\n  m_GameObject: {fileID: 123}\n"
        parsed = module.documents(text)
        self.assertEqual(parsed[123][1]["m_Name"], "Example")
        self.assertEqual(parsed[456][1]["m_GameObject"]["fileID"], 123)

    def test_invalid_document_is_rejected(self):
        with self.assertRaises(ValueError):
            module.documents("not a Unity export")

    def test_muscle_settings_include_strength_modifiers(self):
        components = [("MonoBehaviour", {"BaseStrength": 10, "StrengthMultipliers": [0.5],
                                         "FakeUprightForce": 0.02, "HasBrain": 1, "DoBalanceJerk": 1,
                                         "BalanceMuscleMovement": 3, "DoStumble": 0,
                                         "BloodMuscleStrengthRatio": 4})]
        settings = module.muscle_settings(components)
        self.assertEqual((settings["strength"], settings["multipliers"]), (10, [0.5]))
        self.assertTrue(settings["has_brain"])

    def test_weighted_curve_is_rejected(self):
        with self.assertRaises(ValueError):
            module.curve({"m_Curve": [{"weightedMode": 1}], "m_PreInfinity": 0, "m_PostInfinity": 0})

if __name__ == "__main__":
    unittest.main()
