"""Inert consumer-diagnostic admission controls; never execute native images."""
import copy
import importlib.util
from pathlib import Path
import sys
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location('consumer_capability', SCRIPTS / 'benchmark-consumer-capability.py')
consumer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(consumer)


def identity():
    return dict(comparison_cohort=consumer.COHORT, excluded_from_scored_cohorts=True,
                excluded_from_ordinary_cohorts=True, native_source_compiler_enabled=True,
                source_commit='1' * 40, source_fingerprint='2' * 64,
                compiler_identity_scheme='scriptw-program-cache-compiler-v3', executable_sha256='3' * 64,
                shared_configuration=dict(consumer.shared.SHARED_CONFIGURATION,
                                          target_triple='x86_64-unknown-linux-gnu'))


def execution():
    memory = dict(scope=consumer.shared.MEMORY_SCOPE, rss_bytes=100, pss_bytes=80, private_dirty_bytes=20)
    result = identity()
    result.update(schema='scriptw-precompiled-execution-v1', compiler_enabled=False, samples=7, warmups=10,
                  image_bytes=500, image_sha256='4' * 64, program_wasm_sha256='5' * 64, runtime_key='6' * 64)
    result['observations'] = [dict(index=i, warmup=i < 10, stdout='42\n', completion='normal',
                                  phases_ns=dict(state_linker=1, register=1, instantiate=1, execution=1, total=4),
                                  process_snapshots={k: dict(memory) for k in consumer.shared.PROCESS_PHASES},
                                  gc_capacity_before_bytes=16, gc_capacity_after_bytes=16)
                              for i in range(17)]
    return result


class ConsumerCapabilityTests(unittest.TestCase):
    def check(self, raw):
        return consumer.check_execution(raw, 'enabled', {'sha256': '3' * 64}, identity(),
            dict(bytes=500, sha256='4' * 64, program_wasm_sha256='5' * 64, runtime_key='6' * 64),
            dict(stdout='42\n'), 'scriptw')

    def test_every_case_adapter_reverses_capability_order_even_for_even_case_counts(self):
        for count in (1, 2, 3, 17):
            cases = [dict(file=f'{i}.js') for i in range(count)]
            plan = consumer.plan(cases, ['scriptw', 'wasi'])
            self.assertEqual(len(plan), count * 8)
            for case in cases:
                for adapter in ('scriptw', 'wasi'):
                    rounds = [[r['variant'] for r in plan if r['round'] == n and r['file'] == case['file'] and r['adapter'] == adapter] for n in (0, 1)]
                    self.assertEqual(rounds[0], rounds[1][::-1])
                    self.assertEqual(set(rounds[0]), set(consumer.VARIANTS))

    def test_identity_rejects_wrong_capability_source_configuration_or_publication_scope(self):
        for key, value in [('native_source_compiler_enabled', False), ('source_commit', '7' * 40),
                           ('source_fingerprint', '8' * 64), ('executable_sha256', '9' * 64),
                           ('excluded_from_scored_cohorts', False), ('comparison_cohort', consumer.shared.SCHEMA)]:
            with self.subTest(key=key):
                raw = identity(); raw[key] = value
                with self.assertRaises(ValueError):
                    consumer.check_identity(raw, 'enabled', '3' * 64, identity())
        raw = identity(); raw['shared_configuration']['gc_initial_bytes'] = 8 * 1024**2
        with self.assertRaises(ValueError):
            consumer.check_identity(raw, 'enabled', '3' * 64)

    def test_admits_actual_scriptw_fields_without_inventing_matched_or_stderr(self):
        raw = execution()
        unchanged = copy.deepcopy(raw)
        self.assertEqual(len(self.check(raw)), 7)
        self.assertEqual(raw, unchanged)

    def test_every_warmup_checksum_and_completion_is_required(self):
        for index in (0, 9, 10, 16):
            for key, value in [('stdout', '43\n'), ('completion', 'throw'), ('stderr', 'failure'), ('matched', False)]:
                with self.subTest(index=index, key=key):
                    raw = execution(); raw['observations'][index][key] = value
                    with self.assertRaises(ValueError):
                        self.check(raw)

    def test_missing_or_reordered_observations_cannot_supply_scores(self):
        raw = execution(); raw['observations'].pop()
        with self.assertRaises(ValueError): self.check(raw)
        raw = execution(); raw['observations'][10]['warmup'] = True
        with self.assertRaises(ValueError): self.check(raw)
        raw = execution(); raw['observations'][10]['index'] = 11
        with self.assertRaises(ValueError): self.check(raw)

    def test_image_runtime_and_compiler_drift_is_rejected(self):
        for key, value in [('image_sha256', '0' * 64), ('program_wasm_sha256', '0' * 64),
                           ('runtime_key', '0' * 64), ('compiler_enabled', True), ('samples', 6)]:
            with self.subTest(key=key):
                raw = execution(); raw[key] = value
                with self.assertRaises(ValueError): self.check(raw)

    def test_observer_reports_cannot_be_used_as_memory_controls(self):
        for key in ('phase_barriers', 'execution_windows',
                    'diagnostic_total_timings_include_barrier_waits'):
            for value in (None, False, {}):
                with self.subTest(key=key, value=value):
                    raw = execution(); raw[key] = value
                    with self.assertRaises(ValueError): self.check(raw)

    def test_memory_and_time_endpoints_require_valid_complete_records(self):
        raw = execution(); del raw['observations'][12]['process_snapshots']['after_store_drop']
        with self.assertRaises(ValueError): self.check(raw)
        raw = execution(); raw['observations'][12]['process_snapshots']['after_execution']['pss_bytes'] = 101
        with self.assertRaises(ValueError): self.check(raw)
        raw = execution(); raw['observations'][12]['phases_ns']['total'] = 3
        with self.assertRaises(ValueError): self.check(raw)
        raw = execution(); del raw['observations'][12]['gc_capacity_after_bytes']
        with self.assertRaises(ValueError): self.check(raw)


if __name__ == '__main__':
    unittest.main()
