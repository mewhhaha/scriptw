"""Store-allocation A/B preparation and reduction contracts."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('benchmark_store_allocation', SCRIPTS / 'benchmark-store-allocation.py')
m = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(m)
from compilation_comparison import scriptc_wasi_preparation

FIXTURES = SCRIPTS.parent / 'crates/scriptw-cli/tests/fixtures'


class StoreAllocationTests(unittest.TestCase):
    def test_expected_stdout_comes_from_the_fixture_manifest(self):
        self.assertEqual(m.expected_stdout(FIXTURES / 'v8_execution_hotpaths/sort-reverse-128.js'), '707264\n')
        edge = json.loads((FIXTURES / 'edge_rendering/portable-manifest.json').read_text())
        router = next(case['stdout'] for case in edge['execution'] if case['file'] == 'edge-router.js')
        self.assertEqual(m.expected_stdout(FIXTURES / 'edge_rendering/edge-router.js'), router)
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'unlisted.js'
            source.write_text('1;')
            (Path(directory) / 'manifest.json').write_text(json.dumps(dict(execution=[dict(file='other.js', stdout='')])))
            with self.assertRaises(SystemExit):
                m.expected_stdout(source)

    def test_each_block_alternates_which_mode_runs_first(self):
        for block in (1, 2):
            orders = [m.alternating_modes(block, process) for process in range(1, 9)]
            self.assertEqual(orders.count(('pooled', 'on-demand')), 4)
            self.assertEqual(sorted(map(sorted, orders)), [sorted(m.MODES)] * 8)
            self.assertNotEqual(orders[0], orders[1])
        self.assertNotEqual(m.alternating_modes(1, 1), m.alternating_modes(2, 1))

    def test_summary_reports_faults_per_mode_and_entry_per_block(self):
        rows = [dict(actor='scriptc', fixture='integer.js', block=block, process=process, mode=mode,
                     execution_ns=(50_000 if mode == 'on-demand' else 25_000) + process,
                     minflt_execution=20 if mode == 'on-demand' else 2)
                for block in (1, 2) for process in range(1, 5) for mode in m.MODES]
        [item] = m.summarize_rows(rows)
        self.assertEqual(item['faults'], {'pooled': 2, 'on-demand': 20})
        self.assertEqual(sorted(item['blocks']), [1, 2])
        self.assertEqual(item['blocks'][1]['pooled']['median_ms'], 0.025)
        self.assertEqual(item['blocks'][1]['saved'], 0.5)

    def test_scriptc_preparation_is_the_application_benchmark_invocation(self):
        emit, native = scriptc_wasi_preparation('scriptc', 'host', 'a.js', 'a.wasm', 'a.swwa')
        self.assertEqual(emit, ['scriptc', 'build', 'a.js', '-o', 'a.wasm'])
        self.assertEqual(native, ['host', 'prepare-wasi', 'a.wasm', 'a.swwa'])


if __name__ == '__main__':
    unittest.main()
