"""The three-runtime reference must retain source, outputs and process units."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
RENDER_SPEC = importlib.util.spec_from_file_location('render_runtime_comparison', ROOT / 'scripts/render-runtime-comparison.py')
renderer = importlib.util.module_from_spec(RENDER_SPEC)
RENDER_SPEC.loader.exec_module(renderer)
SPEC = importlib.util.spec_from_file_location('runtime_bench', ROOT / 'scripts/benchmark-runtime-comparison.py')
bench = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bench)


class RuntimeComparisonTests(unittest.TestCase):
    def test_runtime_summary_is_compact_and_detail_renderer_keeps_the_charts(self):
        authority = {'date': '2026-11-02'}
        report = {'source_fingerprint': 'a' * 64}
        node = {'versions': {'node': 'v24.12.0', 'v8': 'v8-test'}}
        identity = {'shared_configuration': {
            'wasmtime_version': '47.0.0', 'native_compilation_mode': 'speed',
            'register_allocation': 'backtracking', 'target': 'x86_64-test',
        }}
        rows = []
        for index in range(17):
            rows.append({'file': f'case-{index}.js', 'runtimes': {
                'scriptw': {'block_median_ms': [1.0, 1.0], 'status': 'ok', 'ms': 1.0, 'rss_mib': 2.0},
                'scriptc': {'block_median_ms': [2.0, 2.0], 'status': 'ok', 'ms': 2.0, 'rss_mib': 2.0},
                'v8': {'block_median_ms': [0.5, 0.5], 'status': 'ok', 'ms': 0.5, 'rss_mib': 2.0},
            }})
        summary = renderer.readme_section(authority, report, node, rows)
        self.assertIn('17/17 workloads', summary)
        self.assertIn('benchmarks.md#runtime-comparison', summary)
        self.assertIn('execution-current.json', summary)
        self.assertNotIn('<img', summary)
        self.assertNotIn('## Benchmarks', summary)
        details = renderer.details_section(authority, report, node, identity, rows)
        self.assertIn('<img ', details)
        self.assertIn('src="assets/benchmarks-current/', details)
        self.assertIn('(execution-current.json)', details)
        self.assertIn('(../../CONTRIBUTING.md#benchmark-charts)', details)

    def test_runtime_publication_requires_unique_ordered_markers(self):
        text = 'intro\n' + renderer.README_START + '\nold\n' + renderer.README_END + '\noutro\n'
        replaced = renderer.replace_marked_block(
            text, renderer.README_START, renderer.README_END, 'new section', 'README.md')
        self.assertEqual(replaced, 'intro\nnew section\noutro\n')
        for invalid in ('intro\n', renderer.README_START + renderer.README_START + renderer.README_END):
            with self.subTest(text=invalid), self.assertRaisesRegex(ValueError, 'README.md needs exactly one ordered'):
                renderer.replace_marked_block(
                    invalid, renderer.README_START, renderer.README_END, 'new', 'README.md')
        with self.assertRaisesRegex(ValueError, 'README.md markers are reversed'):
            renderer.replace_marked_block(
                renderer.README_END + renderer.README_START,
                renderer.README_START, renderer.README_END, 'new', 'README.md')

    def test_every_case_has_four_processes_per_actor_block_and_reversed_order(self):
        names = [f'case{index}.js' for index in range(17)]
        rows = bench.process_plan(names)
        self.assertEqual(len(rows), 408)
        for block in range(2):
            for name in names:
                for actor in bench.RUNTIMES:
                    self.assertEqual(sum((row['block'], row['case'], row['actor']) == (block, name, actor) for row in rows), 4)
        first = [row['case'] for row in rows if row['block'] == 0 and row['position'] == 0]
        last = [row['case'] for row in rows if row['block'] == 1 and row['position'] == 0]
        self.assertEqual(last, first[::-1])

    def test_v8_cannot_drop_warmups_relabel_source_or_ignore_failed_output(self):
        node = dict(versions=dict(node='24.12.0', v8='v8-test'))
        case = dict(file='case.js', stdout='ok\n')
        original = dict(schema='scriptw-v8-baseline-v2', node='v24.12.0', v8='v8-test', samples=3, warmups=1,
                        cases=[dict(file='case.js', source_sha256='source', compilation_ns=20,
                                    observations=[dict(warmup=index == 0, execution_ns=10, context_ns=30, stdout='ok\n') for index in range(4)])])
        def checked(report):
            return bench.checked_observations(report, 'v8', case, 'source', None, None, node, 3, 1)
        self.assertEqual(checked(original)['wall_ns'], [10, 10, 10])
        self.assertEqual(checked(original)['context_ns'], [30, 30, 30])
        for change in ('source', 'stdout', 'warmup', 'missing', 'version'):
            report = copy.deepcopy(original)
            if change == 'source':
                report['cases'][0]['source_sha256'] = 'changed'
            if change == 'stdout':
                report['cases'][0]['observations'][0]['stdout'] = 'bad\n'
            if change == 'warmup':
                report['cases'][0]['observations'][0]['warmup'] = False
            if change == 'missing':
                report['cases'][0]['observations'].pop()
            if change == 'version':
                report['v8'] = 'other'
            with self.subTest(change=change), self.assertRaises(AssertionError):
                checked(report)

    def test_reduction_rejects_missing_or_reordered_runtime_processes(self):
        names = [f'case{index}.js' for index in range(17)]
        rows = [dict(row, wall_ns=[10, 20, 30], resources=dict(peak_rss_bytes=1024)) for row in bench.process_plan(names)]
        result = bench.reduce_rows(rows, names)
        self.assertEqual(len(result), 17)
        self.assertEqual(result[0]['blocks'][0]['products']['v8']['wall_ns']['median'], 20)
        with self.assertRaises(AssertionError):
            bench.reduce_rows(rows[:-1], names)
        with self.assertRaises(AssertionError):
            bench.reduce_rows([rows[1], rows[0], *rows[2:]], names)

    def test_wasm_actors_must_share_one_store_allocation(self):
        identity = dict(source_fingerprint='a' * 64, executable_sha256='b' * 64, shared_configuration={})
        image = dict(image_sha256='c' * 64, wasm_sha256='d' * 64)
        case = dict(file='case.js', stdout='ok\n')
        pooled = dict(strategy='pooled', max_concurrent_stores=1,
                      reset=dict(mode='keep-dirty-pages-resident', keep_resident_bytes=16 * 1024**2))
        cpu = dict(status='sampled', scope='execution-worker-thread', elapsed_ns=5)
        raw = dict(identity, **image, schema='scriptw-precompiled-wasi-execution-v1', compiler_enabled=False,
                   comparison_cohort='scriptw-precompiled-comparison-v1', excluded_from_ordinary_cohorts=True,
                   samples=3, warmups=1, store_allocation=pooled,
                   observations=[dict(warmup=index == 0, stdout='ok\n', matched=True, completion='normal', stderr='',
                                      execution_thread_cpu=cpu, phases_ns=dict(execution=10)) for index in range(4)])
        checked = bench.checked_observations(raw, 'scriptc', case, 'source', identity, image, None, 3, 1)
        self.assertEqual(checked['store_allocation'], pooled)
        rows = [dict(actor='scriptw', store_allocation=pooled), dict(actor='scriptc', **checked), dict(actor='v8')]
        self.assertEqual(bench.campaign_store_allocation(rows), pooled)
        self.assertIsNone(bench.campaign_store_allocation([dict(actor='scriptw'), dict(actor='scriptc'), dict(actor='v8')]))
        for other in (dict(strategy='on-demand'), None):
            rows[1] = dict(actor='scriptc', store_allocation=other)
            with self.subTest(other=other), self.assertRaisesRegex(ValueError, 'differ in store allocation'):
                bench.campaign_store_allocation(rows)

    def summary(self, scriptw, other):
        names = [f'case{index}.js' for index in range(17)]
        products = lambda: {'scriptw': dict(wall_ns=dict(median=scriptw, mean=scriptw)),
                            'scriptc': dict(wall_ns=dict(median=other, mean=other)),
                            'v8': dict(wall_ns=dict(median=other, mean=other))}
        return [dict(case=name, blocks=[dict(block=block, products=products()) for block in range(2)]) for name in names]

    def test_published_comparisons_need_wins_in_both_statistics_and_blocks(self):
        summary = self.summary(1.0, 2.0)
        result = bench.comparisons(summary)
        self.assertEqual(len(result['scriptc']['wins_in_both_statistics_and_blocks']), 17)
        self.assertAlmostEqual(result['v8']['geomeans'][1]['median'], 0.5)
        summary[0]['blocks'][1]['products']['scriptw']['wall_ns']['mean'] = 3.0
        self.assertEqual(len(bench.comparisons(summary)['v8']['wins_in_both_statistics_and_blocks']), 16)

    def test_cohort_qualification_is_bound_to_its_receipt_and_claims_no_aa_gate(self):
        with tempfile.TemporaryDirectory() as temporary:
            frozen = Path(temporary)
            (frozen / 'native-qualified.json').write_text('{"status": "complete"}\n')
            receipt = dict(schema=bench.COHORT_QUALIFICATION, status='complete', publication_commit='a' * 40,
                           source_fingerprint='b' * 64, aa_gates='not-applicable',
                           native=dict(receipt=dict(path='native-qualified.json',
                                                    sha256=bench.digest(frozen / 'native-qualified.json'))))
            (frozen / 'qualification.json').write_text(json.dumps(receipt))
            authority, qualification = bench.scriptw_qualification(frozen, None, 'cohort')
            self.assertEqual((authority['publication_commit'], qualification['kind']), ('a' * 40, 'cohort'))
            for change in ('receipt', 'aa'):
                changed = copy.deepcopy(receipt)
                if change == 'receipt':
                    changed['native']['receipt']['sha256'] = 'c' * 64
                else:
                    changed['aa_gates'] = {}
                (frozen / 'qualification.json').write_text(json.dumps(changed))
                with self.subTest(change=change), self.assertRaises(AssertionError):
                    bench.scriptw_qualification(frozen, None, 'cohort')

    def test_runtime_manifest_must_frame_its_bound_outputs(self):
        target = b'x86_64-unknown-linux-gnu'
        fields = bytes(range(32)) + b'k' * 32 + b'p' * 32 + b'n' * 32
        manifest = bench.RUNTIME_MANIFEST_MAGIC + (5).to_bytes(4, 'little') + bytes(7) + len(target).to_bytes(2, 'little') + target + fields
        decoded = bench.decode_runtime_manifest(manifest)
        self.assertEqual((decoded['version'], decoded['target']), (5, target.decode()))
        self.assertEqual(decoded['package_sha256'], (b'p' * 32).hex())
        for invalid in (b'SCRWNR02' + manifest[8:], manifest[:-1]):
            with self.subTest(length=len(invalid)), self.assertRaises(ValueError):
                bench.decode_runtime_manifest(invalid)

    def test_inert_publication_admits_text_only_and_the_renderer_reads_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            campaign, root = Path(temporary) / 'campaign', Path(temporary)
            (campaign / 'candidate').mkdir(parents=True)
            (campaign / 'tmp').mkdir()
            (campaign / 'tmp/ignored.wasm').write_bytes(b'\0asm\1\0\0\0')
            (campaign / 'analysis.json').write_text('{"schema": "analysis"}\n')
            (campaign / 'candidate/identity.json').write_text('{"source_fingerprint": "' + 'f' * 64 + '"}\n')
            members = bench.inert_members(campaign, 'evidence/cohort')
            self.assertEqual(set(members), {'evidence/cohort/analysis.json', 'evidence/cohort/candidate/identity.json'})
            index = dict(schema='scriptw-inert-evidence-index-v1',
                         members=[dict(path=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
                                  for name, data in members.items()])
            archive = root / 'evidence-inert.tar.xz'
            count, expanded = bench.write_archive(archive, members, index)
            self.assertEqual(archive.stat().st_mode & 0o777, 0o644)
            checkpoint = dict(source_fingerprint='f' * 64,
                              reference17=dict(evidence=dict(path='analysis.json', sha256=bench.digest(campaign / 'analysis.json'))),
                              evidence=dict(archive=str(archive), sha256=bench.digest(archive), bytes=archive.stat().st_size,
                                            regular_utf8_members=count, expanded_utf8_bytes=expanded))
            analysis, identity = renderer.load_analysis(checkpoint)
            self.assertEqual((analysis['schema'], identity['source_fingerprint']), ('analysis', 'f' * 64))
            with self.assertRaises(FileExistsError):
                bench.write_archive(archive, members, index)
            (campaign / 'program.wasm').write_bytes(b'\0asm\1\0\0\0')
            with self.assertRaises(AssertionError):
                bench.inert_members(campaign, 'evidence/cohort')


if __name__ == '__main__':
    unittest.main()
