"""Application results cannot hide unsupported cases or score failed output."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from application_benchmarks import (ACTORS, SAMPLES, WARMUPS, SCHEMA, campaign_store_allocation, checked_observations,
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

    def test_publication_refuses_wasm_actors_that_differ_in_store_allocation(self):
        pooled = dict(strategy='pooled', max_concurrent_stores=1,
                      reset=dict(mode='keep-dirty-pages-resident', keep_resident_bytes=16 * 1024**2))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'sources').mkdir()
            source = root / 'sources' / CASE['file']
            source.write_text('print(42);\n')
            available = {CASE['file']: list(ACTORS)}
            report = dict(schema=SCHEMA, status='complete', workloads=[CASE], plan=dict(available=available),
                          preparations={CASE['file']: {actor: dict(status='prepared', stage='execution', image=IMAGE)
                                                       for actor in ACTORS}},
                          consumer_identity=IDENTITY, node_identity=NODE,
                          inputs_before={'source': 'same'}, inputs_after={'source': 'same'})
            path = root / 'report.json'

            def publishable(allocations):
                report['rows'] = []
                for index, entry in enumerate(process_plan([CASE['file']], available)):
                    actor = entry['actor']
                    raw = v8_report(digest(source)) if actor == 'v8' else wasm_report(actor)
                    if allocations.get(actor) is not None:
                        raw['store_allocation'] = allocations[actor]
                    stdout, stderr = root / f'{index}.stdout', root / f'{index}.stderr'
                    stdout.write_text(json.dumps(raw))
                    stderr.write_text('')
                    row = dict(entry, status='sampled', stage='execution', stdout=stdout.name, stderr=stderr.name,
                               stdout_sha256=digest(stdout), stderr_sha256=digest(stderr),
                               resources=dict(exit_code=0, timed_out=False, elapsed_ns=3000, peak_rss_bytes=1024))
                    row.update(checked_observations(raw, actor, CASE, digest(source), IDENTITY, IMAGE, NODE))
                    report['rows'].append(row)
                report['retained_files'] = [dict(path=str(item.relative_to(root)), bytes=item.stat().st_size,
                                                 sha256=digest(item))
                                            for item in sorted(root.rglob('*')) if item.is_file() and item != path]
                report['summary'] = summarize(report)
                path.write_text(json.dumps(report))
                return campaign_store_allocation(load_report(path))

            self.assertEqual(publishable(dict(scriptw=pooled, scriptc=pooled)), pooled)
            # Reports from before pooled hosts record no allocation for either actor.
            self.assertIsNone(publishable({}))
            for allocations in (dict(scriptw=pooled, scriptc=dict(strategy='on-demand')),
                                dict(scriptw=pooled),
                                dict(scriptw=pooled, scriptc=dict(pooled, reset=dict(mode='release-all')))):
                with self.subTest(allocations=allocations), self.assertRaisesRegex(ValueError, 'differ in store allocation'):
                    publishable(allocations)


if __name__ == '__main__':
    unittest.main()


def script(name):
    import importlib.util
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / name)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def runtime_manifest(source, package, native, target=b'x86_64-unknown-linux-gnu'):
    """The Cargo producer's v5 encoding (crates/scriptw-engine/src/embedded_runtime/manifest.rs)."""
    return (b'SCRWNR01' + (5).to_bytes(4, 'little') + (7).to_bytes(4, 'little') + bytes([1, 3, 1])
            + len(target).to_bytes(2, 'little') + target + bytes.fromhex(source) + b'\x11' * 32
            + bytes.fromhex(package) + bytes.fromhex(native))


class CohortAndPublicationTests(unittest.TestCase):
    def runtime_receipt(self, *, source='a' * 64, corrupt_package=False):
        freeze = script('freeze-application-cohort.py')
        with tempfile.TemporaryDirectory() as directory:
            out, output = Path(directory) / 'out', Path(directory) / 'cohort'
            out.mkdir()
            output.mkdir()
            for prefix in ('scriptw-runtime', 'scriptw-runtime-portable'):
                (out / f'{prefix}-package.bin').write_bytes(prefix.encode() + b' package')
                (out / f'{prefix}-native.bin').write_bytes(prefix.encode() + b' native')
                package = digest(out / f'{prefix}-package.bin')
                if corrupt_package:
                    package = 'f' * 64
                (out / f'{prefix}-manifest.bin').write_bytes(
                    runtime_manifest(source, package, digest(out / f'{prefix}-native.bin')))
            identity = dict(source_fingerprint='a' * 64,
                            shared_configuration=dict(target_triple='x86_64-unknown-linux-gnu'))
            return freeze.runtime_receipt(output, dict(out_dir=str(out)), identity)

    def test_frozen_runtime_images_must_come_from_the_host_source(self):
        files = self.runtime_receipt()
        package = files['scriptw-runtime-portable-package.bin']['package']
        self.assertEqual(package['package_source_fingerprint'], 'a' * 64)
        self.assertEqual(package['runtime_key'], '11' * 32)
        with self.assertRaisesRegex(ValueError, 'source differs'):
            self.runtime_receipt(source='b' * 64)
        with self.assertRaisesRegex(ValueError, 'does not bind'):
            self.runtime_receipt(corrupt_package=True)

    def test_publication_refuses_binary_evidence_instead_of_dropping_it(self):
        publish = script('publish-application-benchmarks.py')
        with tempfile.TemporaryDirectory() as directory:
            for value in (b'\0asm\1\0\0\0', b'\x7fELF', b'text\0more', b'\xff\xfe'):
                path = Path(directory) / 'log.stdout'
                path.write_bytes(value)
                with self.assertRaises((ValueError, UnicodeDecodeError)):
                    publish.inert_text(path)

    def test_renderer_reads_exactly_the_archive_the_publisher_writes(self):
        publish, render = script('publish-application-benchmarks.py'), script('render-application-benchmarks.py')
        selected = {'evidence/target/campaign/report.json': b'{}\n', 'evidence/scripts/tool.py': b'print(1)\n'}
        index = dict(schema='scriptw-inert-evidence-index-v1', members=[
            dict(path=name, bytes=len(value), sha256=digest_bytes(value)) for name, value in sorted(selected.items())])
        with tempfile.TemporaryDirectory() as directory:
            receipt = publish.write_archive(Path(directory) / 'evidence.tar.xz', selected, index)
            files = render.read_archive(dict(archive=receipt), Path(directory))
            self.assertEqual({name: files[name] for name in selected}, selected)
            tampered = dict(receipt, regular_utf8_members=receipt['regular_utf8_members'] + 1)
            with self.assertRaisesRegex(ValueError, 'inventory'):
                render.read_archive(dict(archive=tampered), Path(directory))

    def test_failure_diagnostic_names_the_deadline_or_the_last_log_line(self):
        render = script('render-application-benchmarks.py')
        files = {'evidence/run/logs/a.stderr': b'/w/input.js:2:5 - error SC0001: types\n  2 | code\n2 errors.\n\n',
                 'evidence/run/logs/a.stdout': b'', 'evidence/run/logs/c.stdout': b'',
                 'evidence/run/logs/c.stderr': b'uncaught throw: ReferenceError: Request is not defined\n',
                 'evidence/run/logs/b.stderr': b'', 'evidence/run/logs/b.stdout': b''}
        failed = dict(stdout='logs/a.stdout', stderr='logs/a.stderr', resources=dict(exit_code=1, timed_out=False))
        self.assertEqual(render.failure_line(files, Path('run'), failed, 60), 'exit 1: error SC0001: types')
        thrown = dict(stdout='logs/c.stdout', stderr='logs/c.stderr', resources=dict(exit_code=1, timed_out=False))
        self.assertEqual(render.failure_line(files, Path('run'), thrown, 60),
                         'exit 1: uncaught throw: ReferenceError: Request is not defined')
        timed_out = dict(stdout='logs/b.stdout', stderr='logs/b.stderr', resources=dict(exit_code=None, timed_out=True))
        self.assertEqual(render.failure_line(files, Path('run'), timed_out, 60), 'timed out (60 s)')


def digest_bytes(value):
    import hashlib
    return hashlib.sha256(value).hexdigest()
