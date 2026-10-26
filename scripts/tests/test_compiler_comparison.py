"""Tooling admission controls; synthetic observations are never benchmark evidence."""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from compiler_comparison import ADAPTER, FIXTURES, RUNTIMES, SCHEMA, digest, load_report, publish


class ComparisonAdmissionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        for name in ('sources', 'adapted', 'expected', 'raw', 'wasm'):
            (self.directory / name).mkdir()
        corpus = ROOT / FIXTURES
        plan = json.loads((corpus / 'manifest.json').read_text())
        shutil.copyfile(corpus / 'manifest.json', self.directory / 'sources/manifest.json')
        self.report = dict(schema=SCHEMA, complete=True, source_commit='1' * 40,
            samples=3, warmups=1, rounds=2, machine=dict(measurement_affinity=[2]),
            finished_at='2026-10-26T00:00:00+00:00', workloads=plan['execution'], attempts=[], commands=[],
            tools=dict(scriptc=dict(version='scriptc 0.4.5'), zig=dict(version='0.16.0'),
                       node=dict(version='v24.12.0', v8='fixture-v8'), runtime_baseline=dict(sha256='2' * 64)),
            scriptw_source_fingerprint='3' * 64, scriptw_runtime_policy={'fixture': True})
        for case in plan['execution']:
            name = case['file']
            original = (corpus / name).read_bytes()
            (self.directory / 'sources' / name).write_bytes(original)
            (self.directory / 'adapted' / name).write_bytes(ADAPTER.encode() + original)
            (self.directory / 'expected' / name).write_text(case['stdout'])
            for round_index in range(2):
                for runtime in RUNTIMES:
                    stem = f'{round_index}-{runtime}-{name}'
                    wasm = 'wasm/' + stem
                    (self.directory / wasm).write_bytes(b'fixture Wasm identity')
                    observations = [dict(warmup=index == 0, stdout=case['stdout'], execution_ns=10_000_000 + index,
                        rss_after_bytes=1024**2, matched=True, completion='normal', stderr='',
                        runtime_policy={'fixture': True},
                        memory=dict(process_rss=dict(final_rss_bytes=2 * 1024**2), after=dict(rss=3 * 1024**2)))
                        for index in range(4)]
                    if runtime == 'scriptc':
                        raw = dict(schema='scriptw-wasi-baseline-v1', wasmtime_version='47.0.0',
                            wasm_sha256=digest(self.directory / wasm), observations=observations, samples=3, warmups=1)
                    else:
                        raw = dict(schema='scriptw-wasmtime-baseline-v2' if runtime == 'scriptw' else 'scriptw-v8-baseline-v2',
                            node='v24.12.0', v8='fixture-v8', requested_native_policy=None, source_fingerprint='3' * 64,
                            source_commit=self.report['source_commit'], executable_sha256='2' * 64, samples=3, warmups=1,
                            cases=[dict(file=name, source_sha256=hashlib.sha256(original).hexdigest(),
                                wasm_sha256=digest(self.directory / wasm), observations=observations)])
                    raw_path = 'raw/' + stem + '.json'
                    (self.directory / raw_path).write_text(json.dumps(raw))
                    self.report['attempts'].append(dict(round=round_index, runtime=runtime, file=name,
                        status='ok', logs=[raw_path], raw=raw_path, wasm=wasm))
                    self.report['commands'].append(dict(stdout=raw_path, returncode=0))
        self.path = self.directory / 'report.json'

    def save(self):
        self.report['retained_files'] = [dict(path=str(path.relative_to(self.directory)),
            bytes=path.stat().st_size, sha256=digest(path)) for path in sorted(self.directory.rglob('*'))
            if path.is_file() and path != self.path]
        self.path.write_text(json.dumps(self.report))
        return self.path

    def test_medians_use_measured_observations_and_keep_all_original_cases(self):
        _, rows = load_report(self.save())
        self.assertEqual(len(rows), 17)
        self.assertEqual(rows[0]['runtimes']['scriptc']['ms'], 10.000002)
        self.assertEqual(rows[0]['runtimes']['v8']['rss_mib'], 3)

    def test_missing_runtime_or_case_cannot_publish(self):
        self.report['attempts'].pop()
        with self.assertRaisesRegex(ValueError, 'inventory differs'):
            load_report(self.save())

    def test_changed_stdout_is_rejected_even_with_updated_file_hash(self):
        attempt = self.report['attempts'][0]
        path = self.directory / attempt['raw']
        raw = json.loads(path.read_text())
        raw['cases'][0]['observations'][1]['stdout'] = 'wrong\n'
        path.write_text(json.dumps(raw))
        with self.assertRaisesRegex(ValueError, 'checksum differs'):
            load_report(self.save())

    def test_relabelling_success_as_failure_cannot_hide_a_measurement(self):
        attempt = next(row for row in self.report['attempts'] if row['runtime'] == 'scriptc')
        attempt['status'] = 'compile-failed'
        with self.assertRaisesRegex(ValueError, 'no failed command'):
            load_report(self.save())

    def test_real_checksum_mismatch_keeps_row_without_any_performance_bar(self):
        attempt = next(row for row in self.report['attempts'] if row['runtime'] == 'scriptc')
        attempt['status'] = 'checksum-mismatch'
        path = self.directory / attempt['raw']
        raw = json.loads(path.read_text())
        raw['observations'][0].update(stdout='wrong\n', matched=False)
        path.write_text(json.dumps(raw))
        _, rows = load_report(self.save())
        self.assertEqual(rows[0]['runtimes']['scriptc'], dict(status='checksum-mismatch', ms=None, rss_mib=None))

    def test_original_and_adapted_source_cannot_be_rewritten(self):
        (self.directory / 'adapted' / self.report['workloads'][0]['file']).write_text('print(0);\n')
        with self.assertRaisesRegex(ValueError, 'adaptation differs'):
            load_report(self.save())

    def test_corrupt_retained_bytes_fail_before_summary(self):
        self.save()
        path = self.directory / self.report['attempts'][0]['raw']
        path.write_text('{}')
        with self.assertRaisesRegex(ValueError, 'evidence changed'):
            load_report(self.path)

    def test_published_archive_round_trips_without_original_measurement_directory(self):
        destination = self.directory / 'checkout'
        (destination / 'docs/rust-rewrite/assets').mkdir(parents=True)
        (destination / 'README.md').write_text('# Scriptw\n\n## Benchmarks\nold\n\n## Tests\nkeep me\n')
        self.save()
        # This control owns archive admission and README publication. The real
        # workflow installs matplotlib and separately verifies the actual SVG.
        with patch('compiler_comparison.render', side_effect=lambda report, rows, metric: metric.encode()):
            publish(self.path, destination)
        path = destination / 'docs/rust-rewrite/compiler-comparison.json'
        _, rows = load_report(path, path.with_suffix('.tar.gz'))
        self.assertEqual(len(rows), 17)
        self.assertIn('keep me', (destination / 'README.md').read_text())
        self.assertIn('17/17', (destination / 'README.md').read_text())
        self.assertEqual((destination / 'docs/rust-rewrite/assets/execution-performance.svg').read_bytes(), b'execution')
        self.assertEqual((destination / 'docs/rust-rewrite/assets/execution-memory.svg').read_bytes(), b'memory')
        self.assertIn('assets/execution-performance.svg', (destination / 'README.md').read_text())
        self.assertIn('assets/execution-memory.svg', (destination / 'README.md').read_text())


if __name__ == '__main__':
    unittest.main()
