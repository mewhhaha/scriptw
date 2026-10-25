"""Admission regressions against the committed, byte-bound execution evidence."""
import errno
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from execution_chart_data import load_chart_data

SOURCE = ROOT / 'docs/rust-rewrite/execution-performance-20261025.json'


class ExecutionChartDataTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.report = json.loads(SOURCE.read_text())
        # Immutable archive is shared, never written by these tests.
        name = self.report['archive']['file']
        try:
            os.link(SOURCE.parent / name, self.directory / name)
        except OSError as error:
            if error.errno != errno.EXDEV:
                raise
            shutil.copyfile(SOURCE.parent / name, self.directory / name)
        self.path = self.directory / SOURCE.name

    def save(self):
        self.path.write_text(json.dumps(self.report))
        return self.path

    def test_committed_authority_keeps_all_controls_and_observed_costs(self):
        rows = load_chart_data(self.save())
        self.assertEqual(rows['source_commit'], 'ab97459e54f9e0cd3ed983d098c46ccacac8fec6')
        self.assertEqual((rows['samples'], rows['warmups'], len(rows['rows'])), (14, 10, 17))
        mapped = {row['file']: row for row in rows['rows']}
        self.assertAlmostEqual(mapped['integer.js']['change_percent'], 15.1785, places=3)
        numeric = mapped['numeric-loop-50000.js']
        self.assertGreater(numeric['rss_change_percent'], 7)
        self.assertGreater(numeric['after_rss_mib'], 354)
        first = mapped['sort-reverse-128.js']
        self.assertEqual(first['scriptw_rss_mib'], 335.6875)
        self.assertEqual(first['v8_rss_mib'], 52.65234375)
        self.assertIn('join-empty-1024.js', mapped)
        self.assertTrue(rows['measurement_end'].startswith('2026-10-25T01:50:53'))

    def test_changed_published_summary_is_rejected(self):
        self.report['performance']['table'][0]['candidate_ms'] *= 0.5
        with self.assertRaisesRegex(ValueError, 'published timing table differs'):
            load_chart_data(self.save())

    def test_changed_candidate_identity_is_rejected(self):
        self.report['source_commit'] = '0' * 40
        with self.assertRaisesRegex(ValueError, 'candidate source identity differs'):
            load_chart_data(self.save())

    def test_changed_raw_analysis_is_rejected_even_with_matching_local_summary(self):
        case = self.report['performance']['balanced_observations']['cases'][0]
        case['pooled']['candidate']['wasmtime']['execution_ns'][0] = 1
        with self.assertRaisesRegex(ValueError, 'archived authority'):
            load_chart_data(self.save())

    def test_changed_current_rss_endpoint_is_rejected(self):
        case = self.report['performance']['balanced_observations']['cases'][0]
        case['pooled']['candidate']['v8']['memory_observations']['rss_after_execution_bytes']['median'] = 1
        with self.assertRaisesRegex(ValueError, 'archived authority'):
            load_chart_data(self.save())

    def test_changed_archive_bytes_are_rejected_without_touching_shared_archive(self):
        name = self.report['archive']['file']
        path = self.directory / name
        path.unlink()  # Remove only this temporary hardlink before writing.
        path.write_bytes(b'changed archive')
        self.report['archive']['bytes'] = path.stat().st_size
        with self.assertRaisesRegex(ValueError, 'archive SHA-256 differs'):
            load_chart_data(self.save())

    def test_archive_paths_cannot_escape_the_report_directory(self):
        self.report['archive']['file'] = '../' + self.report['archive']['file']
        with self.assertRaisesRegex(ValueError, 'unsafe archive path'):
            load_chart_data(self.save())


if __name__ == '__main__':
    unittest.main()
