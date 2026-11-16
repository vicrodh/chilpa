"""Require the shared primitive for effects whose native parent can be absent."""
import importlib.machinery
import importlib.util
from pathlib import Path
import unittest

path = Path(__file__).resolve().parents[2] / "tools/lint/qml-owned-shadow"
loader = importlib.machinery.SourceFileLoader("qml_owned_shadow", str(path))
spec = importlib.util.spec_from_loader(loader.name, loader)
module = importlib.util.module_from_spec(spec)
import sys
sys.path.insert(0, str(path.parent))
loader.exec_module(module)
imports_loader = importlib.machinery.SourceFileLoader("qml_imports", str(path.with_name("qml-imports")))
imports_spec = importlib.util.spec_from_loader(imports_loader.name, imports_loader)
imports_module = importlib.util.module_from_spec(imports_spec)
imports_loader.exec_module(imports_module)


class OwnedShadowTests(unittest.TestCase):
    def test_primitive_native_bootstrap_is_in_the_same_layer(self):
        self.assertTrue(imports_module.primitive_module_allowed("app.chilpa.kit.primitives"))
        self.assertTrue(imports_module.primitive_module_allowed("QtQuick.Effects"))

    def test_primitive_cannot_import_the_facade(self):
        for name in ["app.chilpa", "app.chilpa.kit", "app.chilpa.sources", "app.chilpa.kit.primitives.extra"]:
            with self.subTest(module=name):
                self.assertFalse(imports_module.primitive_module_allowed(name))

    def test_direct_native_shadow_is_rejected(self):
        self.assertTrue(module.violations("RectangularShadow {}"))

    def test_qualified_native_shadow_is_rejected(self):
        self.assertTrue(module.violations("Effects.RectangularShadow {}"))

    def test_comments_and_text_are_not_constructors(self):
        self.assertEqual(module.violations('// RectangularShadow {}\nText { text: "RectangularShadow {}" }'), [])

    def test_shared_shadow_is_allowed(self):
        self.assertEqual(module.violations("Shadow {}"), [])

    def test_only_the_shared_primitive_can_wrap_the_native_type(self):
        source = "RectangularShadow { MaterialLifetime.ownMaterial: true }"
        self.assertEqual(module.violations(source, primitive=True), [])
        self.assertTrue(module.violations(source))

    def test_shared_primitive_cannot_drop_its_owner(self):
        self.assertTrue(module.violations("RectangularShadow {}", primitive=True))

    def test_false_or_conditional_requests_are_rejected(self):
        for value in ["false", "true && false", "enabled"]:
            with self.subTest(value=value):
                self.assertTrue(module.violations("RectangularShadow { MaterialLifetime.ownMaterial: " + value + " }", primitive=True))

    def test_missing_native_effect_is_rejected(self):
        self.assertTrue(module.violations("Item { MaterialLifetime.ownMaterial: true }", primitive=True))


if __name__ == "__main__":
    unittest.main()
