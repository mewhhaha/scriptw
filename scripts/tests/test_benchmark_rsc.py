"""Campaign schedule/reduction contracts and real Node output-boundary controls."""
import importlib.util
import copy
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('benchmark_rsc', SCRIPTS / 'benchmark-rsc.py')
m = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(m)


def observations(values=(100, 1000, 4, 8), warmups=2):
    return [dict(index=index, warmup=index < warmups, elapsed_ns=value,
                 exact_node_output_checked=True, output_json_sha256=m.sha(b'"flight"'),
                 output_json_bytes=8) for index, value in enumerate(values)]


class Schedule(unittest.TestCase):
    def test_actor_policies_are_explicit_and_reject_unknown_or_duplicate_hosts(self):
        hosts = {'baseline': Path('baseline'), 'candidate': Path('candidate')}
        self.assertEqual(m.actor_policies(hosts, 'fast', ['candidate=speed']),
                         {'baseline': 'fast', 'candidate': 'speed'})
        for overrides in (['v8=speed'], ['candidate=invalid'], ['candidate=speed', 'candidate=fast']):
            with self.subTest(overrides=overrides), self.assertRaises(ValueError):
                m.actor_policies(hosts, 'fast', overrides)

    def test_reverse_blocks_balance_actor_and_case_position(self):
        plan = m.schedule(['baseline', 'candidate', 'v8'], m.CASES, 4)
        half = len(plan) // 2
        self.assertEqual(len(plan), 48)
        self.assertEqual([{key: row[key] for key in ('actor', 'case', 'repetition')} for row in plan[half:]],
                         [{key: row[key] for key in ('actor', 'case', 'repetition')} for row in reversed(plan[:half])])
        for actor in ('baseline', 'candidate', 'v8'):
            for case in m.CASES:
                self.assertEqual(sum(row['actor'] == actor and row['case'] == case for row in plan), 8)
        self.assertEqual({row['block'] for row in plan}, {0, 1})
        self.assertEqual([row['ordinal'] for row in plan[:half]], list(range(half)))

    def test_invalid_or_duplicate_selections_rejected(self):
        for actors, cases, count in ((['v8', 'v8'], m.CASES, 4), (['v8'], ['bogus'], 4),
                                     (['v8'], ['rsc-sync', 'rsc-sync'], 4), (['v8'], m.CASES, 0)):
            with self.assertRaises(ValueError):
                m.schedule(actors, cases, count)


class Calls(unittest.TestCase):
    def test_warmup_curve_is_retained_but_excluded_from_scores(self):
        result = m.checked_calls(observations(), samples=2, warmups=2, golden='"flight"\n')
        self.assertEqual(result['warmup_curve_ns'], [100, 1000])
        self.assertEqual(result['request_ns']['samples'], [4, 8])
        self.assertEqual(result['request_ns']['mean'], 6)
        self.assertEqual(result['request_ns']['median'], 6)
        self.assertEqual(result['request_ns']['p95'], 8)

    def test_every_call_including_warmups_must_match(self):
        for field, value in (('exact_node_output_checked', False), ('output_json_sha256', 'bad'),
                             ('output_json_bytes', 7), ('warmup', False), ('index', 9), ('elapsed_ns', 0)):
            rows = observations()
            rows[0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                m.checked_calls(rows, samples=2, warmups=2, golden='"flight"\n')
        with self.assertRaises(ValueError):
            m.checked_calls(observations()[:-1], samples=2, warmups=2, golden='"flight"\n')

    def test_aggregation_uses_process_medians_not_pooled_calls(self):
        rows = [dict(case='rsc-sync', actor='v8', block=block, request_ns=m.stats(values),
                     accounting=dict(peak_rss_bytes=1000)) for block, values in
                ((0, [1, 1, 10000]), (1, [100, 100, 100]))]
        result = m.aggregate(rows, ['v8'], ['rsc-sync'])['rsc-sync']['v8']
        self.assertEqual(result['process_medians_ns']['samples'], [1, 100])
        self.assertEqual(result['process_medians_ns']['median'], 50.5)
        self.assertEqual(result['process_means_ns']['samples'], [3334, 100])
        self.assertEqual(result['process_p95_ns']['samples'], [10000, 100])
        self.assertEqual(result['block_medians_ns'], {'0': 1, '1': 100})

    def test_profiled_dumped_and_wrong_policy_hosts_are_rejected(self):
        raw = dict(schema='scriptw-react-retained-baseline-v1', status='complete', inputs={},
            executable_sha256='host', native_mode='fast', case_selection='rsc-sync',
            cases=[dict(case='rsc-sync', samples=2, warmups=2, observations=observations())],
            configuration=dict(compilation_jobs=1, native_parallel_compilation=False,
                               cranelift_opt_level='None', register_allocation='SinglePass', wasmtime='47.0.0',
                               native_inlining='No', native_inlining_requested_flags={}),
            source_fingerprint='source', compiler_identity_scheme='scheme', toolchain={'profile': 'release'},
            runtime={'key': 'runtime'}, program_wasm_sha256='program', phases_ns={'setup': 1},
            request_clock='full Flight completion', gc_policy='natural')
        def check(report):
            return m.check_report(report, dict(actor='scriptw', case='rsc-sync'), {},
                                  {'expected': {'rsc-sync': '"flight"\n'}}, 2, 2, 'host', 'fast')
        self.assertEqual(check(raw)['request_ns']['median'], 6)
        for marker in ('profiling', 'artifact_dump'):
            altered = copy.deepcopy(raw)
            altered[marker] = {'excluded_from_scored_cohorts': True}
            with self.subTest(marker=marker), self.assertRaises(ValueError):
                check(altered)
        altered = copy.deepcopy(raw)
        altered['configuration']['register_allocation'] = 'Backtracking'
        with self.assertRaises(ValueError):
            check(altered)
        for field, value in (('native_inlining', 'Yes'),
                             ('native_inlining_requested_flags', {'wasmtime_inlining_sum_size_threshold': '2000'})):
            altered = copy.deepcopy(raw)
            altered['configuration'][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'inlining policy'):
                check(altered)

    def test_missing_case_or_mismatched_source_is_rejected(self):
        raw = dict(inputs=dict(source_sha256='wrong'))
        with self.assertRaisesRegex(ValueError, 'source/wrapper'):
            m.check_report(raw, dict(actor='v8', case='rsc-sync'), dict(source_sha256='right'), {}, 2, 2, None, 'fast')


@unittest.skipUnless(shutil.which('node'), 'Node not installed')
class NodeBoundary(unittest.TestCase):
    def probe(self, root, *arguments, source=None):
        if source is None:
            source = '''globalThis.applicationRun=async mode=>({toJSON(){
              performance.validationTick();return {mode};}});'''
        (root / 'rsc.js').write_text(source)
        (root / 'support.json').write_text(json.dumps(dict(
            versions={'react': '19.3.1', 'react-server-dom-webpack': '19.3.1'},
            expected={'rsc-sync': '{"mode":"sync"}\n', 'rsc-async': '{"mode":"async"}\n'})))
        # A deterministic host clock advances by 1000 only during serialization.
        # If encoding/comparison moved inside the request timer, each sample is 1001.
        preload = root / 'clock.cjs'
        preload.write_text('let tick=0n;process.hrtime.bigint=()=>++tick;'
                           'performance.validationTick=()=>{tick+=1000n;};')
        return subprocess.run([shutil.which('node'), '--experimental-vm-modules', '--no-warnings',
                               '--require', str(preload), str(m.ROOT / m.DRIVER),
                               'retained-module', str(root), *arguments],
                              text=True, capture_output=True, timeout=10)

    def test_selected_case_and_configurable_counts_exclude_encoding_clock(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.probe(Path(directory), '2', '1', 'rsc-async')
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report['case_order'], ['rsc-async'])
            self.assertEqual(report['samples'], 2)
            self.assertEqual(report['warmups'], 1)
            self.assertEqual([row['elapsed_ns'] for row in report['observations']], [1, 1, 1])
            self.assertEqual([row['warmup'] for row in report['observations']], [True, False, False])
            self.assertTrue(all('stdout' not in row for row in report['observations']))
            self.assertEqual(report['inputs']['adapted_source_sha256'],
                             m.sha(((Path(directory) / 'rsc.js').read_text() + m.WRAPPER).encode()))
            checked = m.check_report(report, dict(actor='v8', case='rsc-async'),
                {key: report['inputs'][key] for key in ('source_sha256', 'support_sha256', 'wrapper', 'adapted_source_sha256')},
                json.loads((Path(directory) / 'support.json').read_text()), 2, 1, None, 'fast')
            self.assertEqual(checked['request_ns']['median'], 1)

    def test_old_cli_defaults_keep_both_cases(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.probe(Path(directory))
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report['case_order'], list(m.CASES))
            self.assertEqual(len(report['observations']), 20)
            self.assertEqual((report['samples'], report['warmups']), (7, 3))

    def test_warmup_output_mismatch_fails_before_scored_samples(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.probe(Path(directory), '1', '1', 'rsc-sync',
                                source='globalThis.applicationRun=async mode=>({mode:"wrong"});')
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('retained-module output changed', result.stderr)
            self.assertEqual(result.stdout, '')

    def test_invalid_counts_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            for args in (('0', '0'), ('1', '-1'), ('1.2', '0'), ('1000', '1'), ('1', '0', 'bad')):
                result = self.probe(Path(directory), *args)
                self.assertNotEqual(result.returncode, 0)


if __name__ == '__main__':
    unittest.main()
