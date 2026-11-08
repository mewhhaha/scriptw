import copy
import importlib.util
import json
import sys
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
SPEC = importlib.util.spec_from_file_location('rsc_charts', ROOT / 'scripts/render-application-benchmarks.py')
CHARTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHARTS)


def report():
    identity = {'source_fingerprint': 'a' * 64, 'executable_sha256': 'b' * 64,
                'program_wasm_sha256': 'c' * 64, 'runtime': {'wasm_sha256': 'd' * 64},
                'inputs': {'source_sha256': 'e' * 64},
                'configuration': {'cranelift_opt_level': 'None', 'register_allocation': 'SinglePass'}}
    phase = dict(front_and_lower=100, wasm_emission=200, native_preparation=300)
    engine = {'label': 'fixture', 'cases': {'rsc-sync': list(range(100, 108)),
                                         'rsc-async': list(range(200, 208))},
              'phase_processes': [dict(phase) for _ in range(8)], 'identity': identity}
    engines = {name: copy.deepcopy(engine) for name in ('baseline', 'candidate', 'candidate-speed', 'v8')}
    engines['baseline']['label'] = 'HISTORICAL CONTROL MUST NOT BE A BAR'
    engines['candidate-speed']['identity']['configuration'] = {
        'cranelift_opt_level': 'Speed', 'register_allocation': 'Backtracking'}
    return {'schema': 'scriptw-rsc-current-v2', 'date': '2026-11-08',
            'measurement': dict(blocks=2, processes_per_engine=8, warmups=3, samples=7, clock='fixture'),
            'inputs': {'fixture': 'f' * 64}, 'engines': engines}


class RscCurrentCharts(unittest.TestCase):
    def admitted(self, value):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'report.json'
            path.write_text(json.dumps(value))
            return CHARTS.rsc_report(path)

    def test_policy_comparison_requires_the_identical_program_runtime_and_host(self):
        for field in ('source_fingerprint', 'executable_sha256', 'program_wasm_sha256', 'runtime', 'inputs'):
            value = report()
            value['engines']['candidate-speed']['identity'][field] = 'different'
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.admitted(value)

    def test_partial_or_mislabeled_speed_cohorts_are_rejected(self):
        value = report()
        del value['engines']['candidate-speed']
        with self.assertRaises(ValueError):
            self.admitted(value)
        value = report()
        value['engines']['candidate-speed']['phase_processes'].pop()
        with self.assertRaises(ValueError):
            self.admitted(value)
        value = report()
        value['engines']['candidate-speed']['identity']['configuration']['cranelift_opt_level'] = 'None'
        with self.assertRaises(ValueError):
            self.admitted(value)

    def test_current_policy_bars_exclude_historical_control(self):
        value = self.admitted(report())
        for image in (CHARTS.rsc_runtime_chart(value), CHARTS.rsc_compile_chart(value)):
            self.assertIn(b'Scriptw Fast', image)
            self.assertIn(b'Scriptw Speed', image)
            self.assertNotIn(b'HISTORICAL CONTROL MUST NOT BE A BAR', image)
        self.assertIn(b'Node / V8', CHARTS.rsc_runtime_chart(value))

    def test_historical_three_actor_reports_remain_admissible(self):
        value = report()
        value['schema'] = 'scriptw-rsc-current-v1'
        del value['engines']['candidate-speed']
        self.admitted(value)


if __name__ == '__main__':
    unittest.main()
