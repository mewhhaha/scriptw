"""Integrity and publication regressions for measured README performance history."""
import copy
import gzip
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import benchmark_metrics as metrics
import performance_charts as charts
SPEC = importlib.util.spec_from_file_location('performance_publisher', ROOT / 'scripts/publish-performance.py')
PUBLISH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PUBLISH)
MANIFEST = json.loads((ROOT / PUBLISH.HISTORY).read_text())


class PerformanceHistoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name)
        self.root = self.work / 'repo'
        self.root.mkdir()
        (self.root / 'README.md').write_text('Latest execution graphic\n'
                                            '\n## Tests\nPreserve this status.\n')

    def report(self, index=-1, name='input'):
        directory = self.work / name
        directory.mkdir()
        entry = MANIFEST['entries'][index]
        with tarfile.open(ROOT / PUBLISH.DOCS / entry['archive'], 'r:gz') as archive:
            archive.extractall(directory, filter='data')
        path = directory / 'report.json'
        return path, json.loads(path.read_text())

    def save(self, path, report):
        path.write_text(json.dumps(report, indent=2) + '\n')

    def replace_log(self, path, report, label, raw):
        log = path.parent / 'logs' / (label + '.stdout')
        log.write_text(json.dumps(raw))
        for command in report['commands']:
            if command['label'] == label:
                command['stdout_sha256'] = metrics.digest(log)
        for item in report['retained_files']:
            if item['path'] == 'logs/' + label + '.stdout':
                item['sha256'] = metrics.digest(log)

    def bytes(self):
        return {str(p.relative_to(self.root)): p.read_bytes() for p in self.root.rglob('*') if p.is_file()}

    def test_real_reports_are_revalidated_sorted_and_keep_absolute_scores(self):
        last, report = self.report()
        first, _ = self.report(0, 'first')
        PUBLISH.record(self.root, last)
        PUBLISH.record(self.root, first)
        _, points = PUBLISH.load_history(self.root)
        self.assertEqual([p['source_commit'] for p in points],
                         [json.loads(first.read_text())['source_commit'], report['source_commit']])
        self.assertEqual(len({p['cohort_id'] for p in points}), 1)
        row = next(row for row in points[-1]['metrics'] if row['id'] == 'objects')
        self.assertEqual(row['scriptw_ns'], report['summaries']['execution']['wasmtime']['objects.js']['execution_ns']['median'])
        self.assertEqual(row['v8_ns'], report['summaries']['execution']['v8']['objects.js']['execution_ns']['median'])
        text = (self.root / 'README.md').read_text()
        self.assertTrue(text.endswith('## Tests\nPreserve this status.\n'))
        self.assertTrue(PUBLISH.render(self.root, check=True)['checked'])
        svg = (self.root / PUBLISH.DOCS / 'assets/performance-memory.svg').read_text()
        self.assertIn(report['source_commit'][:9], svg)
        self.assertIn('07:05 UTC', svg)
        ET.fromstring(svg)

    def test_record_is_idempotent_and_archives_are_deterministic(self):
        path, report = self.report()
        raw = path.read_bytes()
        self.assertEqual(PUBLISH.pack(path, report, raw), PUBLISH.pack(path, report, raw))
        self.assertTrue(PUBLISH.record(self.root, path)['recorded'])
        before = self.bytes()
        self.assertFalse(PUBLISH.record(self.root, path)['recorded'])
        self.assertEqual(self.bytes(), before)

    def test_publication_keeps_every_retained_file_including_real_wasm(self):
        path, report = self.report()
        data = PUBLISH.pack(path, report, path.read_bytes())
        with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
            self.assertEqual(set(archive.getnames()), {r['path'] for r in report['retained_files']} | {'report.json'})
            for name in ('integer.js', 'objects.js', 'retained-objects.js'):
                self.assertEqual(archive.extractfile('wasm/' + name + '.wasm').read(),
                                 (path.parent / 'wasm' / (name + '.wasm')).read_bytes())

    def test_missing_or_changed_wasm_rejects_without_publication(self):
        path, _ = self.report()
        before = self.bytes()
        (path.parent / 'wasm/objects.js.wasm').write_bytes(b'changed')
        with self.assertRaises(ValueError):
            PUBLISH.record(self.root, path)
        self.assertEqual(self.bytes(), before)

    def test_oversized_retained_file_rejects_before_reading_its_payload(self):
        path, _ = self.report()
        with (path.parent / 'wasm/objects.js.wasm').open('r+b') as stream:
            stream.truncate(PUBLISH.MAX_MEMBER_BYTES + 1)
        with self.assertRaisesRegex(ValueError, 'publication budget'):
            PUBLISH.record(self.root, path)
        self.assertFalse((self.root / PUBLISH.HISTORY).exists())

    def test_record_does_not_overwrite_an_unregistered_evidence_file(self):
        path, _ = self.report()
        existing = self.root / PUBLISH.DOCS / 'performance-history' / (metrics.digest(path) + '.tar.gz')
        existing.parent.mkdir(parents=True)
        existing.write_bytes(b'preserve existing file')
        before = self.bytes()
        with self.assertRaisesRegex(ValueError, 'overwrite existing'):
            PUBLISH.record(self.root, path)
        self.assertEqual(self.bytes(), before)

    def test_forged_median_and_partial_reports_cannot_enter_history(self):
        path, original = self.report()
        changes = [dict(complete=False), dict(failure='interrupted'), dict(schema=metrics.POLICY_SCHEMA)]
        for change in changes:
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.save(path, dict(original, **change))
                PUBLISH.record(self.root, path)
        report = copy.deepcopy(original)
        report['summaries']['startup']['cold']['v8']['elapsed_ns']['median'] += 1
        self.save(path, report)
        with self.assertRaises(ValueError):
            PUBLISH.record(self.root, path)
        self.assertFalse((self.root / PUBLISH.HISTORY).exists())

    def test_selected_or_experimental_boundaries_are_rejected(self):
        path, original = self.report()
        for change in ('startup', 'case', 'policy', 'boundary'):
            report = copy.deepcopy(original)
            if change == 'startup': report['configuration']['startup'] = False
            if change == 'case': report['workloads'].pop()
            if change == 'policy': report['configuration']['native_policy'] = {'native_mode': 'speed'}
            if change == 'boundary': report['workloads'][1]['execution_mode'] = 'same-realm-batched'
            self.save(path, report)
            with self.subTest(change=change), self.assertRaises(ValueError):
                PUBLISH.record(self.root, path)

    def test_relabeling_source_or_cli_does_not_relabel_old_observations(self):
        path, original = self.report()
        for key, value in [('source_commit', 'a' * 40), ('cli_sha256', 'b' * 64),
                           ('runtime_runner_sha256', 'c' * 64)]:
            report = dict(original, **{key: value})
            self.save(path, report)
            metrics.load_report(path)  # Existing low-level admission lacks this cross-binding.
            with self.subTest(key=key), self.assertRaises(ValueError):
                PUBLISH.record(self.root, path)

    def test_wasmtime_child_identity_is_bound_even_if_log_hashes_are_resigned(self):
        path, report = self.report()
        label = 'execution-wasmtime-integer.js'
        raw = json.loads((path.parent / 'logs' / (label + '.stdout')).read_text())
        raw['source_commit'] = 'a' * 40
        self.replace_log(path, report, label, raw)
        self.save(path, report)
        metrics.load_report(path)
        with self.assertRaisesRegex(ValueError, 'Wasmtime output'):
            PUBLISH.record(self.root, path)

    def test_measurement_dates_need_timezone_and_order(self):
        path, original = self.report()
        for change in [dict(finished_at='2026-10-24T07:05:18'),
                       dict(started_at=original['finished_at'], finished_at=original['started_at']),
                       dict(started_at='not a date')]:
            self.save(path, dict(original, **change))
            with self.subTest(change=change), self.assertRaises(ValueError):
                PUBLISH.record(self.root, path)

    def test_zero_v8_median_is_not_divided_into_a_graph(self):
        path, report = self.report()
        label = 'execution-v8-integer.js'
        case = next(row for row in report['execution']['v8']['cases'] if row['file'] == 'integer.js')
        for observation in case['observations']: observation['execution_ns'] = 0
        raw = json.loads((path.parent / 'logs' / (label + '.stdout')).read_text())
        raw['cases'] = [{key: value for key, value in case.items() if key != 'command_label'}]
        self.replace_log(path, report, label, raw)
        report['summaries'] = metrics.summarize(report)
        self.save(path, report)
        metrics.load_report(path)
        with self.assertRaisesRegex(ValueError, 'positive'):
            PUBLISH.record(self.root, path)

    def test_new_machine_keeps_separate_evidence_and_displays_latest_scores(self):
        first, _ = self.report(0, 'first')
        last, report = self.report()
        report['machine']['cpu_model'] = 'different CPU'
        self.save(last, report)
        PUBLISH.record(self.root, first)
        PUBLISH.record(self.root, last)
        _, points = PUBLISH.load_history(self.root)
        self.assertEqual(len({point['cohort_id'] for point in points}), 2)
        svg = (self.root / PUBLISH.DOCS / 'assets/performance-memory.svg').read_text()
        self.assertIn('different CPU', svg)
        self.assertIn(report['source_commit'][:9], svg)
        self.assertNotIn(points[0]['source_commit'][:9], svg)

    def test_check_is_read_only_and_render_repairs_stale_outputs(self):
        path, _ = self.report()
        PUBLISH.record(self.root, path)
        svg = self.root / PUBLISH.DOCS / 'assets/performance-memory.svg'
        svg.write_text('stale')
        before = self.bytes()
        with self.assertRaisesRegex(ValueError, 'stale performance output'):
            PUBLISH.render(self.root, check=True)
        self.assertEqual(self.bytes(), before)
        PUBLISH.render(self.root)
        self.assertTrue(PUBLISH.render(self.root, check=True)['checked'])

    def test_archive_corruption_or_duplicate_history_is_rejected(self):
        path, _ = self.report()
        PUBLISH.record(self.root, path)
        manifest = json.loads((self.root / PUBLISH.HISTORY).read_text())
        archive = self.root / PUBLISH.DOCS / manifest['entries'][0]['archive']
        original = archive.read_bytes();archive.write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError, 'checksum'):
            PUBLISH.render(self.root, check=True)
        archive.write_bytes(original)
        manifest['entries'].append(manifest['entries'][0])
        (self.root / PUBLISH.HISTORY).write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, 'duplicate measured report'):
            PUBLISH.render(self.root, check=True)

    def test_archive_extraction_rejects_traversal_links_duplicates_and_oversized_files(self):
        for role in ('traversal', 'backslash', 'drive', 'link', 'duplicate', 'oversized'):
            data = io.BytesIO()
            with tarfile.open(fileobj=data, mode='w:gz') as archive:
                member = tarfile.TarInfo('../outside' if role == 'traversal' else 'report.json')
                if role == 'backslash': member.name = '..\\outside'
                if role == 'drive': member.name = 'C:/outside'
                if role == 'link': member.type = tarfile.SYMTYPE;member.linkname = '../outside'
                archive.addfile(member)
                if role == 'duplicate': archive.addfile(member)
            if role == 'oversized':
                # Only the header is needed: the budget must reject it before
                # trying to read or allocate its advertised payload.
                member.size = PUBLISH.MAX_MEMBER_BYTES + 1
                data = io.BytesIO(gzip.compress(member.tobuf() + b'\0' * 1024))
            destination = self.work / role;destination.mkdir()
            with self.subTest(role=role), self.assertRaises(ValueError):
                PUBLISH.unpack(data.getvalue(), destination)
            self.assertFalse((self.work / 'outside').exists())

    def test_historical_publication_preserves_the_current_readme(self):
        path, _ = self.report()
        readme = self.root / 'README.md'
        before = readme.read_bytes()
        PUBLISH.record(self.root, path)
        PUBLISH.render(self.root)
        self.assertEqual(readme.read_bytes(), before)

    def test_invalid_svg_text_cannot_partially_publish_history(self):
        path, report = self.report()
        report['machine']['cpu_model'] = 'invalid\0XML'
        self.save(path, report)
        before = self.bytes()
        with self.assertRaisesRegex(ValueError, 'invalid generated SVG'):
            PUBLISH.record(self.root, path)
        self.assertEqual(self.bytes(), before)

    def test_renderer_keeps_faster_than_v8_timings_and_escapes_labels(self):
        _, points = PUBLISH.load_history(ROOT)
        point = copy.deepcopy(points[-1])
        for row in point['metrics']:
            row.update(scriptw_ns=1_000_000, v8_ns=2_000_000)
        point['metrics'][0]['label'] = '<untrusted & label>'
        svg = charts.render_current(point)
        root = ET.fromstring(svg)
        cold_bars = [bar for bar in root.iter('{http://www.w3.org/2000/svg}rect')
                     if bar.get('data-metric') == 'cold']
        bars = {bar.get('data-engine'): bar for bar in cold_bars[:2]}
        self.assertEqual(set(bars), {'Scriptw', 'V8'})
        self.assertEqual(bars['Scriptw'].get('x'), bars['V8'].get('x'))
        self.assertAlmostEqual(float(bars['Scriptw'].get('width')) / float(bars['V8'].get('width')), .5)
        self.assertIn('1 ms', svg)
        self.assertIn('2 ms', svg)
        self.assertIn('&lt;untrusted &amp; label&gt;', svg)
        self.assertNotIn('<untrusted', svg)

    def test_cli_check_is_offline_and_preserves_conformance_status(self):
        path, _ = self.report()
        PUBLISH.record(self.root, path)
        before = self.bytes()
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/publish-performance.py'),
                                 'render', '--root', str(self.root), '--check'],
                                cwd=self.work, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.bytes(), before)


if __name__ == '__main__':
    unittest.main()
