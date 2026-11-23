"""The Porffor comparison times only checked programs, in a balanced fresh-process order."""
import collections
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))


def script(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


peer = script('wasm_peer_benchmark', 'scripts/benchmark-porffor.py')
runtime = script('runtime_comparison_benchmark', 'scripts/benchmark-runtime-comparison.py')
NAMES = [row['file'] for row in peer.cases()]


def observations(stdout, count=4, completion='normal'):
    return [dict(warmup=index == 0, stdout=stdout, matched=True, completion=completion, error='',
                 execution_ns=10 + index, phases_ns=dict(execution=20 + index)) for index in range(count)]


class PeerBenchmarkTests(unittest.TestCase):
    def test_sampling_matches_the_published_runtime_panel(self):
        self.assertEqual((peer.SAMPLES, peer.WARMUPS), (runtime.SAMPLES, runtime.WARMUPS))
        self.assertEqual(peer.ADAPTER, 'function print(value) { console.log(String(value)); }\n')

    def test_every_admitted_actor_gets_four_processes_per_block_and_leads_equally(self):
        admitted = {name: list(peer.ACTORS) for name in NAMES}
        plan = peer.process_plan(NAMES, admitted)
        self.assertEqual(len(plan), 17 * 2 * 16)
        counts = collections.Counter((row['block'], row['case'], row['actor']) for row in plan)
        self.assertEqual(set(counts.values()), {4})
        for block in range(2):
            for name in NAMES:
                rows = [row['actor'] for row in plan if (row['block'], row['case']) == (block, name)]
                leaders = collections.Counter(rows[start] for start in range(0, 16, 4))
                self.assertEqual(set(leaders.values()), {1}, (block, name))
        self.assertEqual([row['case'] for row in plan if row['block'] == 1][::16], NAMES[::-1])

    def test_failed_attempts_are_never_scheduled(self):
        admitted = {name: list(peer.ACTORS) for name in NAMES}
        admitted[NAMES[0]] = ['scriptw', 'v8']
        plan = peer.process_plan(NAMES, admitted)
        actors = collections.Counter(row['actor'] for row in plan if row['case'] == NAMES[0])
        self.assertEqual(actors, {'scriptw': 8, 'v8': 8})
        self.assertEqual(len(plan), 16 * 2 * 16 + 2 * 8)

    def test_compilation_order_is_abba_in_two_reversed_blocks(self):
        plan = peer.compile_plan(NAMES)
        self.assertEqual(len(plan), 17 * 8)
        counts = collections.Counter((row['block'], row['case'], row['actor']) for row in plan)
        self.assertEqual(set(counts.values()), {2})
        first = [row['actor'] for row in plan[:4]]
        self.assertEqual(first, ['scriptw', 'porffor', 'porffor', 'scriptw'])

    def test_each_failure_kind_is_classified_from_its_own_evidence(self):
        expected = '42\n'
        compiled = dict(status='compile-failed', detail='SyntaxError')
        self.assertEqual(peer.classify('porffor.v8', compiled, None, None, expected)['status'], 'compile-failed')
        self.assertEqual(peer.classify('porffor.wasmtime', None, 0, dict(status='module-rejected', error='bad'), expected),
                         dict(status='module-rejected', detail='bad'))
        thrown = dict(status='complete', observations=observations('', completion='throw'))
        self.assertEqual(peer.classify('porffor.v8', None, 0, thrown, expected)['status'], 'runtime-failed')
        wrong = dict(status='complete', observations=observations('41\n'))
        self.assertEqual(peer.classify('porffor.wasmtime', None, 0, wrong, expected)['status'], 'wrong-output')
        self.assertEqual(peer.classify('scriptw', None, 0, dict(observations=observations(expected)), expected)['status'], 'passed')
        self.assertEqual(peer.classify('v8', None, 1, None, expected)['status'], 'runtime-failed')
        v8 = dict(cases=[dict(observations=observations(expected))])
        self.assertEqual(peer.classify('v8', None, 0, v8, expected)['status'], 'passed')

    def test_timed_entries_require_exact_output_and_return_measured_entries(self):
        report = dict(schema='scriptw-wasm-peer-node-v1', status='complete', samples=3, warmups=1,
                      observations=observations('7\n'))
        self.assertEqual(peer.entry_times(report, 'porffor.v8', '7\n', 3, 1), [11, 12, 13])
        scriptw = dict(schema='scriptw-precompiled-execution-v1', compiler_enabled=False, samples=3, warmups=1,
                       observations=observations('7\n'))
        self.assertEqual(peer.entry_times(scriptw, 'scriptw', '7\n', 3, 1), [21, 22, 23])
        report['observations'][2]['stdout'] = '8\n'
        with self.assertRaisesRegex(ValueError, 'output differs'):
            peer.entry_times(report, 'porffor.v8', '7\n', 3, 1)

    def test_tables_show_failed_attempts_in_place_of_times(self):
        def actor(milliseconds):
            return dict(median_ns=milliseconds * 1e6, block_median_ns=[milliseconds * 1e6] * 2)
        runtime_rows = [dict(case='a.js', actors={name: actor(1.0) for name in peer.ACTORS}),
                        dict(case='b.js', actors=dict(scriptw=actor(2.0), v8=actor(1.0)))]
        value = dict(runtime=runtime_rows, comparisons=peer.comparisons(runtime_rows),
                     failures=[dict(case='b.js', actor=name, status='wrong-output', detail='printed "1"')
                               for name in ('porffor.wasmtime', 'porffor.v8')],
                     compile=dict(summary=[dict(case='a.js', actors=dict(
                         scriptw=dict(median_ns=5e6, wasm_bytes=10), porffor=dict(median_ns=9e6, wasm_bytes=20)))]))
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'summary.json'
            path.write_text(json.dumps(value))
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                peer.tables(type('Args', (), dict(summary=path)))
        text = output.getvalue()
        self.assertIn('| `b.js` | 2.000 | wrong output | wrong output | 1.000 | — |', text)
        self.assertIn('| Scriptw vs Porffor on Wasmtime | 1 | 0 | 1.000× | 1.00–1.00× |', text)
        self.assertEqual(value['comparisons']['v8']['kernels'], 2)


if __name__ == '__main__':
    unittest.main()
