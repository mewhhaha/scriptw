"""The three-runtime reference must retain source, outputs and process units."""
import copy
import importlib.util
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
SPEC = importlib.util.spec_from_file_location('runtime_bench', ROOT / 'scripts/benchmark-runtime-comparison.py')
bench = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bench)


class RuntimeComparisonTests(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
