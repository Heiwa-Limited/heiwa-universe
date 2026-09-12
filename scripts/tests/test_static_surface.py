"""Exercise the public topology contract without network or runtime state."""
from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "check_static_surface", ROOT / "apps/heiwa_app/scripts/check_static_surface.py"
)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class PublicTopologyTests(unittest.TestCase):
    def setUp(self):
        self.manifest = json.loads(
            (checker.WEB_ROOT / "assets/domains.bootstrap.json").read_text()
        )
        self.manifest["platform"].pop("public_web", None)
        self.manifest["platform"].update(
            public_shell="cloudflare_pages", public_docs="github_pages"
        )

    def validate(self, manifest):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.json"
            path.write_text(json.dumps(manifest))
            return checker.check_domain_manifest(path)

    def test_accepts_independent_shell_and_docs_hosts(self):
        self.assertEqual([], self.validate(self.manifest))

    def test_rejects_obsolete_single_host_manifest(self):
        platform = self.manifest["platform"]
        platform.pop("public_shell")
        platform.pop("public_docs")
        platform["public_web"] = "github_pages"
        self.assertTrue(self.validate(self.manifest))

    def test_rejects_host_drift_and_missing_ownership(self):
        for field, wrong in (("public_shell", "github_pages"), ("public_docs", "cloudflare_pages")):
            for value in (wrong, None):
                with self.subTest(field=field, value=value):
                    manifest = copy.deepcopy(self.manifest)
                    manifest["platform"][field] = value
                    self.assertTrue(self.validate(manifest))

    def test_rejects_legacy_field_even_alongside_new_fields(self):
        self.manifest["platform"]["public_web"] = "github_pages"
        self.assertTrue(self.validate(self.manifest))


if __name__ == "__main__":
    unittest.main()
