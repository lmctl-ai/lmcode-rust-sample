#!/usr/bin/env python3
"""Version detection must distinguish pending ports from verified runtimes."""
import json
from pathlib import Path
import runpy
import tempfile
import unittest
API = runpy.run_path(str(Path(__file__).with_name('run-all-samples.py')))
class CatalogEpochTests(unittest.TestCase):
    def test_lock_has_explicit_legacy_digest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            self.assertIsNone(API['pinned_std_digest'](root))
            (root/'sdk.lock').write_text(json.dumps({'sha256':{'std/lmcode-std.schema.json':API['LEGACY_STD_SHA256']}}))
            self.assertEqual(API['pinned_std_digest'](root),API['LEGACY_STD_SHA256'])
            (root/'sdk.lock').write_text('{broken')
            with self.assertRaises(ValueError): API['pinned_std_digest'](root)
    def test_go_module_has_native_build_recipe(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'go.mod').write_text('module fixture\n')
            self.assertEqual(API['build_for']('go',root,{})[:2],['go','build'])
            self.assertEqual(API['build_for']('go',root,{'SAMPLE_GO_BUILD':'["custom"]'}),['custom'])
    def test_legacy_surface_is_complete(self):
        self.assertEqual(len(API['legacy_signature_set']()),72)
if __name__=='__main__':unittest.main()
