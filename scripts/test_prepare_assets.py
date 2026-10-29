import unittest
from unittest.mock import patch
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path

spec = spec_from_file_location("prepare_assets", Path(__file__).with_name("prepare-assets.py"))
module = module_from_spec(spec)
spec.loader.exec_module(module)


class UnityDocumentTests(unittest.TestCase):
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
                                         "FakeUprightForce": 0.02, "HasBrain": 1})]
        settings = module.muscle_settings(components)
        self.assertEqual(settings["strength"], 5)
        self.assertTrue(settings["has_brain"])

    def test_animated_pose_is_not_silently_imported_as_static(self):
        person = {"Poses": [{"Name": "Rest", "Angles": [{"Animated": 1}]}]}
        with self.assertRaises(ValueError):
            module.static_poses({1: ("MonoBehaviour", person)})


if __name__ == "__main__":
    unittest.main()
