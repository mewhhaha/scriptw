import copy
import importlib.util
import contextlib
import io
import json
import sys
from pathlib import Path
import tempfile
import unittest
from unittest import mock

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


def isolated_report(warmups=100, samples=101):
    value = report()
    value['schema'] = 'scriptw-rsc-current-v3'
    value['measurement'].update(processes_per_engine_per_case=8, processes_per_engine=16,
                                warmups=warmups, samples=samples,
                                case_isolation='one case per fresh process')
    for name, engine in value['engines'].items():
        engine['phase_processes'] *= 2
        if name == 'v8':
            engine['phase_processes'] = [dict(realm_creation_ns=10, source_module_parse_ns=20,
                                             module_link_and_initialization_ns=30) for _ in range(16)]
        engine['warmup_curves_ns'] = {
            case: [[(1000 + index * 20 + process) * 1000 for index in range(warmups)]
                   for process in range(8)] for case in ('rsc-sync', 'rsc-async')}
    return value


class RscCurrentCharts(unittest.TestCase):
    def admitted(self, value):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'report.json'
            path.write_text(json.dumps(value))
            return CHARTS.rsc_report(path)

    def test_policy_comparison_requires_the_identical_program_runtime_and_host(self):
        for factory in (report, isolated_report):
            for field in ('source_fingerprint', 'executable_sha256', 'program_wasm_sha256', 'runtime', 'inputs'):
                value = factory()
                value['engines']['candidate-speed']['identity'][field] = 'different'
                with self.subTest(schema=value['schema'], field=field), self.assertRaises(ValueError):
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


    def test_isolated_report_requires_case_and_complete_cold_process_inventories(self):
        for change in ('missing isolation', 'both cases', 'eight cold processes', 'missing V8 cold process'):
            value = isolated_report()
            if change == 'missing isolation':
                del value['measurement']['case_isolation']
            elif change == 'both cases':
                value['measurement']['case_isolation'] = 'both cases in one retained realm'
            elif change == 'eight cold processes':
                value['engines']['candidate']['phase_processes'] = value['engines']['candidate']['phase_processes'][:8]
            else:
                value['engines']['v8']['phase_processes'].pop()
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.admitted(value)

    def test_isolated_call_counts_are_bounded_without_fixing_one_warmup_policy(self):
        self.admitted(isolated_report(warmups=9, samples=17))
        even_samples = isolated_report(warmups=9, samples=18)
        even_samples['engines']['v8']['cases']['rsc-sync'][0] = 100.5
        self.admitted(even_samples)
        for field, count in [('warmups', 0), ('samples', 0), ('warmups', -1),
                             ('samples', True), ('samples', 901)]:
            value = isolated_report()
            value['measurement'][field] = count
            with self.subTest(field=field, count=count), self.assertRaises(ValueError):
                self.admitted(value)

    def test_isolated_reports_reject_missing_or_invalid_warmup_curves(self):
        for change in ('missing', 'missing case', 'missing process', 'missing call', 'zero', 'bool'):
            for engine in ('baseline', 'candidate', 'candidate-speed', 'v8'):
                value = isolated_report()
                curves = value['engines'][engine]['warmup_curves_ns']
                if change == 'missing':
                    del value['engines'][engine]['warmup_curves_ns']
                elif change == 'missing case':
                    del curves['rsc-sync']
                elif change == 'missing process':
                    curves['rsc-sync'].pop()
                elif change == 'missing call':
                    curves['rsc-sync'][0].pop()
                elif change == 'zero':
                    curves['rsc-sync'][0][0] = 0
                else:
                    curves['rsc-sync'][0][0] = True
                with self.subTest(change=change, engine=engine), self.assertRaises(ValueError):
                    self.admitted(value)

    def test_isolated_policies_may_only_change_optimization_and_register_allocation(self):
        value = isolated_report()
        for engine in ('candidate', 'candidate-speed'):
            value['engines'][engine]['identity']['configuration']['gc_initial_bytes'] = 16777216
        self.admitted(value)
        value['engines']['candidate-speed']['identity']['configuration']['gc_initial_bytes'] *= 2
        with self.assertRaisesRegex(ValueError, 'outside optimization'):
            self.admitted(value)
        value = isolated_report()
        value['engines']['candidate-speed']['identity']['configuration']['register_allocation'] = 'SinglePass'
        with self.assertRaisesRegex(ValueError, 'policy identity'):
            self.admitted(value)

    def test_warmup_chart_uses_per_call_medians_for_current_engines(self):
        import matplotlib.axes
        original = matplotlib.axes.Axes.plot
        plotted = []
        def capture(axes, x, y, *args, **kwargs):
            plotted.append((list(x), list(y), kwargs['label']))
            return original(axes, x, y, *args, **kwargs)
        value = self.admitted(isolated_report(warmups=9, samples=17))
        with mock.patch.object(matplotlib.axes.Axes, 'plot', capture):
            image = CHARTS.rsc_warmup_chart(value)
        self.assertEqual(len(plotted), 6)
        self.assertEqual(plotted[0][0], list(range(1, 10)))
        self.assertEqual(plotted[0][1], [(1003.5 + index * 20) / 1000 for index in range(9)])
        self.assertEqual([row[2] for row in plotted], ['Scriptw Fast', 'Scriptw Speed', 'Node / V8'] * 2)
        for label in (b'Sync Flight', b'Async Flight', b'no confidence interval', b'log scale'):
            self.assertIn(label, image)
        self.assertNotIn(b'HISTORICAL CONTROL MUST NOT BE A BAR', image)

    def test_cli_renders_and_checks_schema_specific_chart_inventory(self):
        for fixture, expected in [(report(), 2), (isolated_report(warmups=9, samples=17), 3)]:
            with self.subTest(schema=fixture['schema']), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / 'report.json'
                source.write_text(json.dumps(fixture))
                assets = root / 'assets'
                argv = ['charts', '--rsc-current', str(source)]
                with mock.patch.object(CHARTS, 'ROOT', root), mock.patch.object(CHARTS, 'RSC_ASSETS', assets), \
                     mock.patch.object(sys, 'argv', argv), contextlib.redirect_stdout(io.StringIO()):
                    CHARTS.main()
                    self.assertEqual(len(list(assets.glob('*.svg'))), expected)
                    with mock.patch.object(sys, 'argv', argv + ['--check']):
                        CHARTS.main()
                    (assets / 'runtime.svg').write_bytes(b'outdated')
                    with mock.patch.object(sys, 'argv', argv + ['--check']), self.assertRaises(ValueError):
                        CHARTS.main()


if __name__ == '__main__':
    unittest.main()
