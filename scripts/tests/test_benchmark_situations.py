"""Inert admission and reduction controls; no runtime execution is simulated."""
import copy
import importlib.util
import json
from pathlib import Path
import statistics
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('benchmark_situations', SCRIPTS / 'benchmark-situations.py')
m = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(m)


def case():
    return dict(file='request.js', label='Request', category='state', scope='prepared requests',
                operations=64, stdout='3\n', invocations=[dict(input=seed, stdout=str(seed) + '\n')
                    for seed in (3, 5, 7, 11)])


def identity():
    return dict(schema='scriptw-precompiled-shared-host-v1', source_commit='a' * 40,
                source_fingerprint='b' * 64, compiler_identity_scheme='source-v1',
                executable_sha256='c' * 64,
                shared_configuration=dict(consumer_compiler_enabled=False,
                    runtime_policy='scriptw-required-wasmtime-experimental-v1', collector='copying',
                    gc_initial_bytes=16777216, wasmtime_version='47.0.0', target='generic',
                    target_triple='x86_64-unknown-linux-gnu', native_compilation_mode='size-optimized',
                    register_allocation='backtracking'))


def native(producer=False):
    row = identity()
    row.update(schema='scriptw-retained-call-execution-v2', native_policy='fast',
               source_format='module', compiler_enabled=producer, excluded_from_ordinary_cohorts=True,
               excluded_from_scored_cohorts=True,
               native_configuration=dict(cranelift_opt_level=None, register_allocation=None,
                   parallel_compilation=False, target=None))
    row['shared_configuration'].update(native_compilation_mode='fast', register_allocation='single-pass')
    if producer:
        row['schema'] = 'scriptw-retained-call-producer-v2'
        row['native_configuration'].update(cranelift_opt_level='None', register_allocation='SinglePass',
                                           target='x86_64-unknown-linux-gnu')
    return row


def execution(actor='v8'):
    c = case()
    row = dict(schema='scriptw-v8-retained-situations-v1', file=c['file'], source_sha256='d' * 64,
               node='v24.12.0', v8='14.1', samples=m.SAMPLES, warmups=m.WARMUPS,
               native_syntax_absent=True, execution_policy=dict(jit='normal', gc='natural'),
               phase_ns=dict(parse=1, link=2, init=3), observations=[])
    if actor != 'v8':
        row = native()
        row.update(samples=m.SAMPLES, warmups=m.WARMUPS, initialization_count=1,
                   producer={'inert': 'receipt'},
                   phases_ns=dict(native_admission=1, setup=2, module_initialization=3), observations=[])
    for index in range(m.SAMPLES + m.WARMUPS):
        invocation = c['invocations'][index % 4]
        observed = dict(index=index, invocation_index=index % 4, input=invocation['input'],
                        warmup=index < m.WARMUPS, call_ns=10 + index, prepare_ns=2,
                        stdout=invocation['stdout'], matched=True)
        if actor != 'v8':
            observed.update(completion='normal', stdout_sha256=m.sha(invocation['stdout'].encode()))
        row['observations'].append(observed)
    return row


class Admission(unittest.TestCase):
    def validate(self, row, actor='v8'):
        return m.validate_execution(row, case(), actor, 'd' * 64, identity(), {'inert': 'receipt'})

    def test_every_prepared_input_and_warmup_is_checked(self):
        self.assertEqual(self.validate(execution()), list(range(13, 20)))
        for index in range(m.SAMPLES + m.WARMUPS):
            row = execution()
            row['observations'][index]['stdout'] = 'wrong\n'
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.validate(row)

    def test_input_rotation_order_and_timing_domains_cannot_drift(self):
        for field, value in [('index', 99), ('invocation_index', 2), ('input', 11),
                             ('warmup', False), ('matched', False), ('call_ns', 0),
                             ('call_ns', float('nan')), ('prepare_ns', True)]:
            row = execution()
            row['observations'][0][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                self.validate(row)
        row = execution()
        row['observations'].pop()
        with self.assertRaises(ValueError):
            self.validate(row)

    def test_v8_requires_identical_source_and_ordinary_pinned_runtime(self):
        for field, value in [('source_sha256', 'different'), ('node', 'v24.13.0'),
                             ('native_syntax_absent', False),
                             ('execution_policy', dict(jit='normal', gc='forced'))]:
            row = execution()
            row[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.validate(row)

    def test_native_requires_frozen_producer_and_disabled_consumer_compiler(self):
        self.validate(execution('scriptw-fast'), 'scriptw-fast')
        for field, value in [('producer', {'different': 'image'}), ('initialization_count', 2),
                             ('executable_sha256', 'different'), ('native_policy', 'speed')]:
            row = execution('scriptw-fast')
            row[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.validate(row, 'scriptw-fast')
        row = execution('scriptw-fast')
        row['native_configuration']['cranelift_opt_level'] = 'None'
        with self.assertRaises(ValueError):
            self.validate(row, 'scriptw-fast')

    def test_actual_native_policy_must_match_requested_cohort(self):
        m.validate_native(native(True), identity(), 'fast', producer=True)
        for field, value in [('cranelift_opt_level', 'Speed'), ('register_allocation', 'Backtracking'),
                             ('parallel_compilation', True), ('target', 'host')]:
            row = native(True)
            row['native_configuration'][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                m.validate_native(row, identity(), 'fast', producer=True)

    def test_manifests_reject_traversal_duplicate_cases_and_unchecked_inputs(self):
        plan = dict(schema='scriptw-retained-situations-v1', source_format='module', execution=[case()])
        m.validate_manifest(plan)
        for change in ('path', 'duplicate', 'seed', 'checksum', 'operations', 'wrapped'):
            row = copy.deepcopy(plan)
            if change == 'path':
                row['execution'][0]['file'] = '../request.js'
            elif change == 'duplicate':
                row['execution'].append(case())
            elif change == 'seed':
                row['execution'][0]['invocations'][1]['input'] = 3
            elif change == 'checksum':
                row['execution'][0]['stdout'] = '0\n'
            elif change == 'operations':
                row['execution'][0]['operations'] = True
            else:
                row['source_format'] = 'wrapped-script'
            with self.subTest(change=change), self.assertRaises(ValueError):
                m.validate_manifest(row)

    def test_release_cargo_receipt_binds_actual_artifact_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            host = Path(directory) / 'host'
            host.write_bytes(b'inert identity-only artifact')
            row = dict(reason='compiler-artifact', executable=str(host),
                       package_id='path+file:///checkout/crates/scriptw-cli#0.1.0',
                       target=dict(name='compiler_precompiled_baseline', kind=['example'],
                                   crate_types=['bin'], src_path=str(m.ROOT / m.HOST_SOURCE)), features=[],
                       profile=dict(opt_level='3', debug_assertions=False, test=False, overflow_checks=False))
            m.validate_cargo([row], host)
            frozen = Path(directory) / 'frozen'
            frozen.write_bytes(host.read_bytes())
            m.validate_cargo([row], frozen)
            for field, value in [('opt_level', '2'), ('debug_assertions', True), ('test', True)]:
                invalid = copy.deepcopy(row)
                invalid['profile'][field] = value
                with self.subTest(field=field), self.assertRaises(ValueError):
                    m.validate_cargo([invalid], host)
            invalid = copy.deepcopy(row)
            invalid['features'] = ['hidden-debug-interpreter']
            with self.assertRaises(ValueError):
                m.validate_cargo([invalid], host)
            host.write_bytes(b'changed artifact')
            with self.assertRaises(ValueError):
                m.validate_cargo([row], frozen)

    def test_native_preparation_requires_exact_sources_raw_wasm_and_sidecars(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sources, images = root / 'sources', root / 'images'
            sources.mkdir()
            images.mkdir()
            c = case()
            (sources / 'manifest.json').write_text(json.dumps(dict(
                schema='scriptw-retained-situations-v1', source_format='module', execution=[c])))
            source = b'export function prepare(input) {}\nexport function run() {}\n'
            (sources / c['file']).write_bytes(source)
            image, wasm = b'inert serialized image bytes', b'\0asm\x01\0\0\0'
            raw = native(True)
            raw['manifest_sha256'] = m.digest(sources / 'manifest.json')
            receipt = dict(raw, **c, original_source_file='original.js', derived_source_file='source.mjs',
                original_source_sha256=m.sha(source), derived_source_sha256=m.sha(source),
                image='request.retained.swpc', image_sha256=m.sha(image), image_bytes=len(image),
                program_wasm_sha256=m.sha(wasm), program_wasm_bytes=len(wasm), native_serialized_bytes=8,
                phases_ns=dict(source_compilation=5, native_preparation=7))
            raw['cases'] = [receipt]
            for name, data in [('original.js', source), ('source.mjs', source),
                               ('request.retained.swpc', image), ('request.js.retained.wasm', wasm)]:
                (images / name).write_bytes(data)
            (images / 'request.retained.json').write_text(json.dumps(receipt))
            (images / 'producer.json').write_text(json.dumps(raw))
            self.assertEqual(m.validate_preparation(raw, [c], images, sources, identity(), 'fast'), [receipt])
            for path in list(images.iterdir()) + [sources / 'manifest.json']:
                previous = path.read_bytes()
                path.write_bytes(previous + b'changed')
                with self.subTest(path=path.name), self.assertRaises(ValueError):
                    m.validate_preparation(raw, [c], images, sources, identity(), 'fast')
                path.write_bytes(previous)


class Reduction(unittest.TestCase):
    def rows(self):
        rows = []
        for index, median in enumerate((10, 20, 30, 40, 50, 60, 70, 1000)):
            samples = [median] * m.SAMPLES
            if index < 4:
                samples = [median] * 4 + [1000000] * 3
            rows.append(dict(block=index // m.ROUNDS, round=index % m.ROUNDS, command=str(index),
                             samples_ns=samples, median_ns=median, status='ok',
                             accounting=dict(peak_rss_bytes=1024 * (index + 1))))
        return rows

    def test_reduction_retains_process_clusters_and_between_block_variation(self):
        rows = self.rows()
        result = m.reduce_runtime(rows, 3)
        self.assertEqual(result['median_ns'], 45)
        self.assertEqual(result['ms'], 45 / 1e6)
        self.assertEqual(result['block_medians_ns'], [25, 65])
        self.assertEqual(result['range_ns'], [10, 1000])
        self.assertEqual(result['ns_per_operation'], 15)
        self.assertEqual(result['peak_rss_bytes'], dict(median=4608, minimum=1024, maximum=8192))
        self.assertNotEqual(result['median_ns'], statistics.median([n for row in rows for n in row['samples_ns']]))
        self.assertEqual(len(result['processes']), 8)

    def test_missing_failed_duplicate_and_tampered_processes_never_become_zero(self):
        for change in ('missing', 'failure', 'duplicate', 'median', 'zero'):
            rows = self.rows()
            if change == 'missing':
                rows.pop()
            elif change == 'failure':
                rows[0]['status'] = 'failed'
            elif change == 'duplicate':
                rows[-1]['block'], rows[-1]['round'] = 0, 0
            elif change == 'median':
                rows[0]['median_ns'] = 0
            else:
                rows[0]['samples_ns'] = [0] * m.SAMPLES
            with self.subTest(change=change), self.assertRaises(ValueError):
                m.reduce_runtime(rows, 3)

    def test_balanced_schedule_reverses_first_last_for_each_case_and_round(self):
        cases = [dict(file='a.js'), dict(file='b.js'), dict(file='c.js')]
        actors = ['scriptw-fast', 'scriptw-speed', 'v8']
        plan = list(m.measurement_plan(cases, actors))
        self.assertEqual(len(plan), len(cases) * len(actors) * 8)
        for c in cases:
            for actor in actors:
                rows = [row for row in plan if row['file'] == c['file'] and row['actor'] == actor]
                self.assertEqual(len(rows), 8)
                self.assertEqual({row['block'] for row in rows}, {0, 1})
            for round_index in range(m.ROUNDS):
                orders = [[row['actor'] for row in plan if row['file'] == c['file']
                           and row['round'] == round_index and row['block'] == block] for block in range(2)]
                self.assertEqual(orders[0], list(reversed(orders[1])))


if __name__ == '__main__':
    unittest.main()
