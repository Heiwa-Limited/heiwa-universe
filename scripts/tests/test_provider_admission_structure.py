#!/usr/bin/env python3
"""Structural admission checks must detect restored construction bypasses."""
from pathlib import Path
import importlib.util
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("provider_admission", ROOT / "scripts/check_provider_admission.py")
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


class AdmissionStructureTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="heiwa-admission-structure-")
        self.root = Path(self.temporary.name)
        for path in [ROOT / "crates/heiwa_provider/src/admission.rs", ROOT / "crates/heiwa_provider/src/routing.rs", ROOT / "apps/heiwa_shell/src/main.rs", *(ROOT / "crates/heiwa_provider/src/providers").glob("*.rs")]:
            target = self.root / path.relative_to(ROOT)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)

    def tearDown(self):
        self.temporary.cleanup()

    def test_reviewed_structure_passes(self):
        self.assertEqual(policy.violations(self.root), [])

    def test_public_new_default_unit_shape_and_wrong_witness_are_rejected(self):
        path = self.root / "crates/heiwa_provider/src/providers/codex_cli.rs"
        original = path.read_text()
        mutations = [
            original.replace("pub(crate) fn from_admitted_lane", "pub fn new"),
            original + "\nimpl Default for CodexCliAdapter { fn default() -> Self { unimplemented!() } }\n",
            original.replace("pub struct CodexCliAdapter {\n    binary: String,\n}", "pub struct CodexCliAdapter;"),
            original.replace("&crate::admission::RegistryAdmittedLane", "&str"),
        ]
        for mutation in mutations:
            with self.subTest(mutation=mutation[-80:]):
                self.assertNotEqual(mutation, original)
                path.write_text(mutation)
                self.assertTrue(policy.violations(self.root))
        path.write_text(original)

    def test_another_runtime_cannot_construct_an_adapter_or_witness(self):
        path = self.root / "apps/probe/src/forged.rs"
        path.parent.mkdir(parents=True)
        for source in ["let adapter = CodexCliAdapter::new();", "let lane = RegistryAdmittedLane { account, model };"]:
            path.write_text(source)
            self.assertTrue(policy.violations(self.root))

    def test_plain_runtime_resolver_is_rejected(self):
        path = self.root / "apps/heiwa_shell/src/main.rs"
        original = path.read_text()
        self.assertIn("ModelCallExecutor::new_registry(", original)
        path.write_text(original.replace("ModelCallExecutor::new_registry(", "ModelCallExecutor::new("))
        self.assertTrue(policy.violations(self.root))


if __name__ == "__main__":
    unittest.main()
