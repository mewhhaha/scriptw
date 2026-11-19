#!/usr/bin/env python3
"""Primary schema, coverage and source-identity admission controls; no product."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

TREE = Path(__file__).resolve().parents[2]
PRIMARY = Path(os.environ.get('SCRIPTW_LOCALE_TEXT_PRIMARY_ROOT', str(TREE)))
spec = importlib.util.spec_from_file_location('locale_text_generator', TREE / 'scripts/generate-intl-locale-text-profile.py')
gen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen)


class LocaleTextProfileAdmission(unittest.TestCase):
    def temporary_native(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        path = Path(temporary.name) / 'tree'
        # Copy only this feature's native/data leaves, even in a full source join.
        for owned in gen.PRODUCTION + [gen.NATIVE / 'tests.rs', gen.NATIVE / 'profile_identity.rs', gen.SOURCE, gen.DEST / 'LICENSE']:
            destination = path / owned
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(TREE / owned, destination)
        return path

    def test_all_primary_scripts_and_counts_are_derived(self):
        rows, _ = gen.extract((TREE / gen.SOURCE).read_bytes())
        self.assertEqual(len(rows), 177)
        self.assertEqual(sum(row['direction'] == 'rtl' for row in rows), 36)
        self.assertEqual(sum(row['direction'] == 'ltr' for row in rows), 137)

    def test_unknown_scripts_remain_explicit_null(self):
        rows, _ = gen.extract((TREE / gen.SOURCE).read_bytes())
        self.assertEqual([row['script'] for row in rows if row['direction'] is None], ['Brai', 'Zinh', 'Zyyy', 'Zzzz'])

    def test_duplicate_script_is_rejected(self):
        line = b'Latn;2;004C;IT;1;RECOMMENDED;NO;NO;MIN;NO;YES\n'
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            gen.extract(line + line)

    def test_unknown_direction_value_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'direction value'):
            gen.extract(b'Latn;2;004C;IT;1;RECOMMENDED;MAYBE;NO;MIN;NO;YES\n')

    def test_malformed_script_and_changed_field_schema_are_rejected(self):
        with self.assertRaisesRegex(ValueError, 'script spelling'):
            gen.extract(b'LATN;2;004C;IT;1;RECOMMENDED;NO;NO;MIN;NO;YES\n')
        with self.assertRaisesRegex(ValueError, 'field schema'):
            gen.extract(b'Latn;2;NO\n')

    def test_exact_primary_bytes_are_required(self):
        native = self.temporary_native()
        with (native / gen.SOURCE).open('ab') as f:
            f.write(b'\n# modified primary\n')
        with self.assertRaisesRegex(ValueError, 'exact pinned'):
            gen.outputs(PRIMARY, native)

    def test_same_commit_license_is_required(self):
        native = self.temporary_native()
        (native / gen.DEST / 'LICENSE').write_text('different license')
        with self.assertRaisesRegex(ValueError, 'license'):
            gen.outputs(PRIMARY, native)

    def test_production_change_rebinds_kernel_without_changing_data(self):
        native = self.temporary_native()
        old = gen.outputs(PRIMARY, native)
        with (native / gen.NATIVE / 'script.rs').open('a') as f:
            f.write('\n// source identity control\n')
        new = gen.outputs(PRIMARY, native)
        self.assertEqual(old[gen.DEST / 'profile.json'], new[gen.DEST / 'profile.json'])
        self.assertNotEqual(old[gen.IDENTITY], new[gen.IDENTITY])

    def test_missing_and_unreviewed_production_modules_are_rejected(self):
        native = self.temporary_native()
        (native / gen.NATIVE / 'unexpected.rs').write_text('pub fn unexpected() {}\n')
        with self.assertRaisesRegex(ValueError, 'source reachability'):
            gen.outputs(PRIMARY, native)
        (native / gen.NATIVE / 'unexpected.rs').unlink()
        (native / gen.NATIVE / 'script.rs').unlink()
        with self.assertRaisesRegex(ValueError, 'source reachability'):
            gen.outputs(PRIMARY, native)

    def test_all_generated_outputs_reproduce_checked_bytes(self):
        outputs = gen.outputs(PRIMARY, TREE)
        self.assertEqual(len(outputs), 3)
        for path, data in outputs.items():
            self.assertEqual((TREE / path).read_bytes(), data, str(path))
        provenance = json.loads(outputs[gen.DEST / 'provenance.json'])
        # Four text owners and 42 shared image/data/admission inputs bind
        # the selected image, generator and actual artifact consumers.
        sources = provenance['kernel_input']['sources']
        self.assertEqual(len(sources), len(gen.PRODUCTION) + len(gen.HELPERS))
        self.assertEqual(len(sources), 46)
        paths = {source['path'] for source in sources}
        self.assertEqual(len(paths), 46)
        self.assertIn('crates/scriptw-intl/src/provider/keyword_aliases/identity.rs', paths)
        self.assertIn('crates/scriptw-intl/src/selection/minimal_admission.rs', paths)
        self.assertEqual(provenance['inputs'][0]['sha256'], gen.SOURCE_SHA)


if __name__ == '__main__':
    unittest.main()
