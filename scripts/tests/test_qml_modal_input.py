"""Exercise the popup input gate with small QML surfaces."""
import runpy
import unittest
from pathlib import Path

violations = runpy.run_path(str(Path(__file__).resolve().parents[2] / "tools/lint/qml-modal-input"))["violations"]


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


if __name__ == "__main__":
    unittest.main()
