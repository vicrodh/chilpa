"""Exercise the popup input gate with small QML surfaces."""
import runpy
import unittest
from pathlib import Path

root = Path(__file__).resolve().parents[2]
gate = runpy.run_path(str(root / "tools/lint/qml-modal-input"))
violations = gate["violations"]
import_violations = gate["import_violations"]


class ModalInputLint(unittest.TestCase):
    def test_custom_popup_requires_its_own_guard(self):
        self.assertTrue(violations("Popup { id: outer; Popup { id: inner; PopupInput { surface: inner } } }"))
        self.assertFalse(violations("Popup { id: panel; PopupInput { surface: panel } }"))

    def test_modals_close_on_release_including_parent_policy(self):
        self.assertTrue(violations("Modal { closePolicy: Popup.CloseOnPressOutsideParent }"))
        self.assertFalse(violations("Modal { closePolicy: Popup.CloseOnReleaseOutsideParent }"))

    def test_raw_modal_needs_a_close_policy(self):
        self.assertTrue(violations("Popup { id: panel; modal: true; PopupInput { surface: panel } }"))
        self.assertFalse(violations("Popup { id: panel; modal: true; closePolicy: Popup.CloseOnEscape; PopupInput { surface: panel } }"))

    def test_comments_do_not_supply_the_guard(self):
        self.assertTrue(violations("Popup { id: panel; /* PopupInput { surface: panel } */ }"))

    def test_rust_kit_module_does_not_export_qml_primitives(self):
        path = root / "crates/chilpa-qt-immersive/qml/Patterns/Fixture.qml"
        source = 'import app.chilpa.kit\nPopup { id: panel; PopupInput { surface: panel } }'
        self.assertTrue(import_violations(source, path))
        source = 'import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"\n' + source
        self.assertFalse(import_violations(source, path))

    def test_guard_import_namespace_must_match(self):
        path = root / "crates/chilpa-qt-immersive/qml/Patterns/Fixture.qml"
        source = 'import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit\nPopupInput { surface: panel }'
        self.assertTrue(import_violations(source, path))
        self.assertFalse(import_violations(source.replace("PopupInput {", "Kit.PopupInput {"), path))
        self.assertTrue(import_violations('/* import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" */\nPopupInput { surface: panel }', path))

    def test_local_kit_primitives_resolve(self):
        path = root / "crates/chilpa-qt-kit/qml/Components/Fixture.qml"
        source = 'import "../Primitives"\nPopupInput { surface: panel }'
        self.assertFalse(import_violations(source, path))
        path = path.parent.parent / "Primitives/Fixture.qml"
        self.assertFalse(import_violations('PopupInput { surface: panel }', path))


if __name__ == "__main__":
    unittest.main()
