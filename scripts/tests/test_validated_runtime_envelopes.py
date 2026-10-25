"""The source proof must retain version and body-validation authority."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    'validated_profile', ROOT / 'scripts/check-abi8-validated-runtime-profile.py')
controls = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controls)
direct_spec = importlib.util.spec_from_file_location(
    'native_direct_eval', ROOT / 'scripts/check-abi8-native-direct-eval.py')
direct_controls = importlib.util.module_from_spec(direct_spec)
direct_spec.loader.exec_module(direct_controls)


class ValidatedEnvelopeControls(unittest.TestCase):
    def test_receipt_profile_reuse_requires_both_authorities_and_real_raw_fallback(self):
        sources = {name: (ROOT / name).read_text() for name in controls.FILES}
        self.assertEqual(controls.check(sources), [])
        direct_sources = {name: (ROOT / name).read_text() for name in direct_controls.FILES}
        self.assertEqual(direct_controls.check(direct_sources), [])
        path = controls.A + 'runtime_artifact.rs'
        for before, after in [
            ('self.build_admission.is_some()', 'true'),
            ('Arc::ptr_eq(data, selected)', 'true'),
            ('crate::runtime_function::runtime_profile::require_runtime_profile(self.bytes(), selection)',
             'Ok(())'),
        ]:
            with self.subTest(before=before):
                self.assertEqual(sources[path].count(before), 1)
                changed = dict(sources)
                changed[path] = sources[path].replace(before, after)
                self.assertIn('receipt_profile_authority', {row['invariant'] for row in controls.check(changed)})
                direct_changed = dict(direct_sources)
                direct_changed[path] = changed[path]
                self.assertIn('complete cached witness and exact R/profile: retained opaque profile admission',
                              direct_controls.check(direct_changed))
        before = 'runtime.require_intl_profile(&selection)?;'
        self.assertEqual(sources[controls.P].count(before), 1)
        changed = dict(sources)
        changed[controls.P] = sources[controls.P].replace(before, 'let _ = runtime.require_intl_profile(&selection);')
        self.assertIn('single_profile_admission', {row['invariant'] for row in controls.check(changed)})
        direct_changed = dict(direct_sources)
        direct_changed[controls.P] = changed[controls.P]
        self.assertIn('complete cached witness and exact R/profile: retained opaque profile admission',
                      direct_controls.check(direct_changed))

    def test_actual_version_and_external_body_validation_cannot_be_removed(self):
        sources = {name: (ROOT / name).read_text() for name in controls.FILES}
        self.assertEqual(controls.check(sources), [])
        for path, before, after in [
            (controls.A + 'runtime_artifact/cache.rs',
             '        version,\n        jobs,\n        super::validation::FunctionBodyValidation::AtUse,',
             '        version,\n        jobs,\n        super::validation::FunctionBodyValidation::ValidatedAtBuild,'),
            (controls.A + 'runtime_artifact/build_package/envelope.rs',
             'cache::admit_payload(payload, identity, version, jobs, body_validation)?',
             'cache::admit_payload(payload, identity, version, jobs, FunctionBodyValidation::AtUse)?'),
            (controls.A + 'runtime_artifact/build_package/envelope.rs',
             'CurrentRuntimeLayoutVersion::admit(u32::from_le_bytes(cache::take(&mut payload)?))?',
             'CurrentRuntimeLayoutVersion::emitted_current()'),
        ]:
            with self.subTest(path=path, before=before):
                self.assertEqual(sources[path].count(before), 1)
                changed = dict(sources)
                changed[path] = sources[path].replace(before, after)
                failures = {row['invariant'] for row in controls.check(changed)}
                self.assertIn('checked_envelopes', failures)

    def test_shared_linkage_proof_and_every_emitter_call_are_required(self):
        sources = {name: (ROOT / name).read_text() for name in controls.FILES}
        self.assertEqual(controls.check(sources), [])
        direct_sources = {name: (ROOT / name).read_text() for name in direct_controls.FILES}
        self.assertEqual(direct_controls.check(direct_sources), [])
        for before, after in [
            ('emitted.runtime_layout.is_some()', 'false'),
            ('Arc::ptr_eq(actual, self.runtime())', 'true'),
        ]:
            with self.subTest(before=before):
                changed = dict(sources)
                self.assertEqual(changed[controls.P].count(before), 1)
                changed[controls.P] = changed[controls.P].replace(before, after)
                self.assertIn('exact_linkage_owner', {row['invariant'] for row in controls.check(changed)})
                direct_changed = dict(direct_sources)
                direct_changed[controls.P] = changed[controls.P]
                self.assertIn(
                    'complete cached witness and exact R/profile: shared linkage owner',
                    direct_controls.check(direct_changed))
        for kind in controls.KINDS:
            with self.subTest(kind=kind):
                path = controls.A + 'runtime_' + kind + '.rs'
                changed = dict(sources)
                changed[path] = changed[path].replace('validated.require_linked_program(', 'unrelated_check(')
                self.assertIn('exact_emission_pair', {row['invariant'] for row in controls.check(changed)})
                if kind == 'direct_eval':
                    direct_changed = dict(direct_sources)
                    direct_changed[path] = changed[path]
                    self.assertTrue(any(
                        row.startswith('complete cached witness and exact R/profile: validated.require_linked_program(')
                        for row in direct_controls.check(direct_changed)))
