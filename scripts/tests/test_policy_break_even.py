"""Reject incomplete policy evidence and unavailable repayment claims."""
import copy
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from policy_break_even import controls, main, observed_break_even, validate


POLICY = dict(native_mode='fast', gc_initial_mib=16, register_allocation='single-pass', native_inlining='disabled')


def observation():
    native = dict(actual_policy=POLICY, engine_ns=2, compile_and_validate_ns=100,
                  native_text_bytes=32, serialized_native_bytes=64,
                  native_module_cache='disabled', incremental_function_cache='disabled')
    execution = dict(actual_policy=POLICY, stdout='42\n', execution_and_completion_ns=10,
                     total_invocation_ns=30, module_cache='Hit', matched_runtime_image_cache='Hit')
    return dict(schema='scriptw-native-policy-break-even-v1', case='small.js',
                requested_policy=POLICY, samples=3, warmups=3,
                native_compilation=[dict(native, warmup=i == 0) for i in range(4)],
                execution=[dict(execution, warmup=i < 3) for i in range(6)])


class NativePolicyEvidenceTests(unittest.TestCase):
    def test_policy_child_explicitly_uses_the_sha_control_when_unset_or_requested(self):
        for requested in [None, 'sha256']:
            with tempfile.TemporaryDirectory() as temporary, self.subTest(requested=requested):
                environment = {}
                if requested is not None:
                    environment['SCRIPTW_COMPILER_CACHE_IMAGE_HASH'] = requested
                arguments = ['policy_break_even.py', '--runner', 'unused-runner', '--time', 'unused-time',
                             '--output-dir', str(Path(temporary) / 'report'), '--case', 'small.js',
                             '--cohort', 'fast-16-single-pass', '--idle-machine']
                with (patch.dict('os.environ', environment, clear=True), patch.object(sys, 'argv', arguments),
                      patch('policy_break_even.require_kernel_budget'),
                      patch('policy_break_even.os.sched_getaffinity', return_value={0}),
                      patch('policy_break_even.digest', return_value='a' * 64),
                      patch('policy_break_even.subprocess.run', return_value=SimpleNamespace(
                          returncode=0, stdout=b'42\n', stderr=b'')),
                      patch('policy_break_even.measure_process', side_effect=RuntimeError('launch intercepted')) as measure):
                    with self.assertRaisesRegex(RuntimeError, 'launch intercepted'):
                        main()
                    child_environment = measure.call_args.kwargs['env']
                    self.assertEqual(child_environment['SCRIPTW_COMPILER_CACHE_IMAGE_HASH'], 'sha256')
                    self.assertEqual(child_environment['SCRIPTW_CACHE_DIR'],
                                     str(Path(temporary) / 'report/native-cache'))

    def check(self, row):
        return validate(row, POLICY, 'small.js', '42\n', 3, 3)

    def test_complete_cold_compilation_and_warm_execution_are_distinct(self):
        summary = self.check(observation())
        self.assertEqual(summary['cold_compile_ns'], 100)
        self.assertEqual(summary['execution_ns'], 10)
        self.assertEqual(summary['total_invocation_ns'], 30)

    def test_wrong_policy_output_and_cached_compilation_are_rejected(self):
        for group, field, value in [
            ('native_compilation', 'actual_policy', dict(POLICY, native_mode='speed')),
            ('native_compilation', 'native_module_cache', 'enabled'),
            ('native_compilation', 'incremental_function_cache', 'enabled'),
            ('native_compilation', 'engine_ns', True),
            ('native_compilation', 'native_text_bytes', True),
            ('native_compilation', 'compile_and_validate_ns', 0),
            ('execution', 'stdout', 'wrong\n'),
            ('execution', 'module_cache', 'Miss'),
            ('execution', 'matched_runtime_image_cache', 'Miss'),
            ('execution', 'total_invocation_ns', 1),
            ('execution', 'warmup', 0),
        ]:
            raw = copy.deepcopy(observation())
            raw[group][-1][field] = value
            with self.subTest(group=group, field=field), self.assertRaises(ValueError):
                self.check(raw)

    def test_truncated_cold_and_execution_inventory_is_rejected(self):
        for group in ('native_compilation', 'execution'):
            raw = observation(); raw[group].pop()
            with self.subTest(group=group), self.assertRaises(ValueError):
                self.check(raw)

    def test_repays_only_positive_execution_or_invocation_savings(self):
        before = dict(cold_compile_ns=100, execution_ns=20, total_invocation_ns=40)
        after = dict(cold_compile_ns=140, execution_ns=16, total_invocation_ns=38)
        row = observed_break_even(before, after)
        self.assertEqual((row['invocations'], row['invocations_using_total']), (10, 20))
        after['execution_ns'] = 21
        after['total_invocation_ns'] = 41
        row = observed_break_even(before, after)
        self.assertIsNone(row['invocations'])
        self.assertIsNone(row['invocations_using_total'])
        after['execution_ns'] = 19
        after['cold_compile_ns'] = 50
        self.assertEqual(observed_break_even(before, after)['invocations'], 0)

    def test_controls_have_independent_checksums_and_balanced_call_arities(self):
        entries = controls()
        self.assertEqual(len(entries), 17)
        self.assertEqual(len({row[0] for row in entries}), 17)
        for strict in ('strict', 'sloppy'):
            self.assertEqual(sum(row[0].startswith('calls-') and row[0].endswith(strict + '.js')
                                 for row in entries), 6)
        self.assertEqual({row[3] for row in entries}, {'small', 'retained', 'ephemeral', 'call-arity'})
