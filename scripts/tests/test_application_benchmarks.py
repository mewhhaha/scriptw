"""Application results cannot hide unsupported cases or score failed output."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from application_benchmarks import (ACTORS, SAMPLES, WARMUPS, SCHEMA, checked_observations,
    checked_product_stdout, load_report, process_plan, summarize, validate_manifest)
from benchmark_metrics import digest


CASE = dict(file='request.js', stdout='42\n', label='Request', category='edge', operations=10,
            scope='Synchronous request kernel')
NODE = dict(versions=dict(node='25.1.0', v8='test-v8'))
IDENTITY = dict(source_fingerprint='a' * 64, executable_sha256='b' * 64,
                shared_configuration=dict(consumer_compiler_enabled=False))
IMAGE = dict(image_sha256='c' * 64, program_wasm_sha256='d' * 64,
             wasm_sha256='d' * 64, runtime_key='e' * 64)


def v8_report(source_sha='source'):
    return dict(schema='scriptw-v8-baseline-v2', node='v25.1.0', v8='test-v8', samples=SAMPLES, warmups=WARMUPS,
        cases=[dict(file=CASE['file'], source_sha256=source_sha, compilation_ns=20,
            observations=[dict(warmup=index < WARMUPS, execution_ns=100, context_ns=30, stdout=CASE['stdout'])
                          for index in range(SAMPLES + WARMUPS)])])


def wasm_report(actor):
    schema = 'scriptw-precompiled-execution-v1'
    if actor == 'scriptc':
        schema = 'scriptw-precompiled-wasi-execution-v1'
    return dict(IDENTITY, **IMAGE, schema=schema, compiler_enabled=False,
        comparison_cohort='scriptw-precompiled-comparison-v1', excluded_from_ordinary_cohorts=True,
        samples=SAMPLES, warmups=WARMUPS, load_phases_ns=dict(engine=10),
        observations=[dict(warmup=index < WARMUPS, matched=True, completion='normal', stderr='',
                           stdout=CASE['stdout'], phases_ns=dict(execution=100)) for index in range(SAMPLES + WARMUPS)])


class ApplicationBenchmarkTests(unittest.TestCase):
    def test_product_output_requires_real_wasm_backend_outcome(self):
        outcome = 'run outcome: RunOutcome { backend_used: WasmAot, note: "wasm-aot completion: object(object)" }\n'
        self.assertTrue(checked_product_stdout('42\n' + outcome, '42\n'))
        for value in ('42\n', 'wrong\n' + outcome, '42\n' + outcome + 'extra\n',
                      '42\n' + outcome.replace('WasmAot', 'SpecExec')):
            with self.subTest(value=value), self.assertRaises(ValueError):
                checked_product_stdout(value, '42\n')

    def test_manifest_rejects_traversal_duplicates_and_missing_work_units(self):
        self.assertEqual(validate_manifest(dict(execution=[CASE])), [CASE])
        for field, value in (('file', '../escape.js'), ('operations', True), ('operations', 0),
                             ('label', ''), ('scope', None), ('stdout', 42)):
            case = dict(CASE, **{field: value})
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                validate_manifest(dict(execution=[case]))
        with self.assertRaises(ValueError):
            validate_manifest(dict(execution=[CASE, CASE]))

    def test_partial_actor_plan_keeps_eight_fresh_processes_in_two_blocks(self):
        names = ['request.js', 'render.js', 'none.js']
        available = {'request.js': list(ACTORS), 'render.js': ['scriptw', 'v8'], 'none.js': []}
        rows = process_plan(names, available)
        self.assertEqual(len(rows), 40)
        for name in names:
            for actor in ACTORS:
                for block in range(2):
                    count = sum((row['case'], row['actor'], row['block']) == (name, actor, block) for row in rows)
                    expected = 0
                    if actor in available[name]:
                        expected = 4
                    self.assertEqual(count, expected)
        with self.assertRaises(ValueError):
            process_plan(names, dict(available, **{'render.js': ['v8', 'v8']}))

    def test_v8_requires_exact_source_output_and_all_warmups(self):
        original = v8_report()
        checked = lambda report: checked_observations(report, 'v8', CASE, 'source', IDENTITY, None, NODE)
        self.assertEqual(checked(original)['entry_wall_ns'], [100] * SAMPLES)
        for mutation in ('source', 'output', 'warmup', 'missing', 'version', 'context'):
            report = copy.deepcopy(original)
            case = report['cases'][0]
            if mutation == 'source':
                case['source_sha256'] = 'changed'
            if mutation == 'output':
                case['observations'][0]['stdout'] = 'incorrect\n'
            if mutation == 'warmup':
                case['observations'][0]['warmup'] = False
            if mutation == 'missing':
                case['observations'].pop()
            if mutation == 'version':
                report['v8'] = 'other'
            if mutation == 'context':
                case['observations'][0]['context_ns'] = None
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                checked(report)

    def test_native_consumers_require_compiler_disabled_image_identity_and_normal_output(self):
        for actor in ('scriptw', 'scriptc'):
            original = wasm_report(actor)
            checked = lambda report: checked_observations(report, actor, CASE, 'source', IDENTITY, IMAGE, NODE)
            self.assertEqual(checked(original)['entry_wall_ns'], [100] * SAMPLES)
            for mutation in ('compiler', 'image', 'source', 'configuration', 'exception', 'output', 'stderr'):
                report = copy.deepcopy(original)
                if mutation == 'compiler':
                    report['compiler_enabled'] = True
                if mutation == 'image':
                    report['image_sha256'] = 'other'
                if mutation == 'source':
                    report['source_fingerprint'] = 'other'
                if mutation == 'configuration':
                    report['shared_configuration'] = {}
                if mutation == 'exception':
                    report['observations'][0]['completion'] = 'throw'
                if mutation == 'output':
                    report['observations'][0]['stdout'] = 'incorrect\n'
                if mutation == 'stderr':
                    report['observations'][0]['stderr'] = 'error'
                with self.subTest(actor=actor, mutation=mutation), self.assertRaises(ValueError):
                    checked(report)

    def sample_report(self):
        available = {CASE['file']: ['v8']}
        preparations = {CASE['file']: dict(
            scriptw=dict(status='failed', stage='source-emission-native-preparation', reason='unsupported syntax'),
            scriptc=dict(status='failed', stage='source-to-wasm', reason='build error'),
            v8=dict(status='prepared', stage='execution'))}
        rows = [dict(entry, status='sampled', stage='execution', entry_wall_ns=[100] * SAMPLES,
                     resources=dict(exit_code=0, timed_out=False, elapsed_ns=3000, peak_rss_bytes=1024))
                for entry in process_plan([CASE['file']], available)]
        return dict(schema=SCHEMA, status='complete', workloads=[CASE], preparations=preparations,
                    plan=dict(available=available), rows=rows)

    def test_failed_preparation_stays_visible_without_a_speed_score(self):
        report = self.sample_report()
        actors = summarize(report)[0]['actors']
        self.assertEqual(set(actors), set(ACTORS))
        self.assertEqual(actors['scriptw']['status'], 'failed')
        self.assertEqual(actors['scriptw']['stage'], 'source-emission-native-preparation')
        self.assertNotIn('entry_wall_ns', actors['scriptw'])
        self.assertEqual(actors['v8']['processes'], 8)
        report['preparations'][CASE['file']].pop('scriptw')
        with self.assertRaises(ValueError):
            summarize(report)

    def test_one_failed_or_timed_out_execution_prevents_an_actor_score(self):
        report = self.sample_report()
        report['rows'][0].update(status='failed', reason='child timed out',
                                resources=dict(exit_code=-9, timed_out=True, elapsed_ns=3000))
        actor = summarize(report)[0]['actors']['v8']
        self.assertEqual(actor['status'], 'failed')
        self.assertEqual(actor['successful_processes'], 7)
        self.assertNotIn('entry_wall_ns', actor)
        report = self.sample_report()
        report['rows'][0]['resources']['timed_out'] = True
        with self.assertRaises(ValueError):
            summarize(report)

    def test_reduction_rejects_missing_and_reordered_processes(self):
        report = self.sample_report()
        report['rows'].pop()
        with self.assertRaises(ValueError):
            summarize(report)
        report = self.sample_report()
        report['rows'][0], report['rows'][1] = report['rows'][1], report['rows'][0]
        with self.assertRaises(ValueError):
            summarize(report)

    def test_retained_raw_log_cannot_disagree_with_recomputed_samples(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'sources').mkdir()
            source = root / 'sources' / CASE['file']
            source.write_text('print(42);\n')
            raw = v8_report(digest(source))
            report = self.sample_report()
            report.update(consumer_identity=IDENTITY, node_identity=NODE,
                          inputs_before={'source': 'same'}, inputs_after={'source': 'same'})
            for index, row in enumerate(report['rows']):
                stdout = root / f'{index}.stdout'
                stderr = root / f'{index}.stderr'
                stdout.write_text(json.dumps(raw))
                stderr.touch()
                row.update(stdout=stdout.name, stderr=stderr.name, stdout_sha256=digest(stdout), stderr_sha256=digest(stderr))
                row.update(checked_observations(raw, 'v8', CASE, digest(source), IDENTITY, None, NODE))
            report['retained_files'] = [dict(path=str(path.relative_to(root)), bytes=path.stat().st_size, sha256=digest(path))
                                        for path in sorted(root.rglob('*')) if path.is_file()]
            report['summary'] = summarize(report)
            path = root / 'report.json'
            path.write_text(json.dumps(report))
            self.assertEqual(load_report(path)['schema'], SCHEMA)
            report['rows'][0]['entry_wall_ns'][0] += 1
            report['summary'] = summarize(report)
            path.write_text(json.dumps(report))
            with self.assertRaises(ValueError):
                load_report(path)


if __name__ == '__main__':
    unittest.main()
