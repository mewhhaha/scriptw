"""Inert publication controls for the focused-situation panel; nothing is executed."""
import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
ROOT = SCRIPTS.parent
SPEC = importlib.util.spec_from_file_location('publish_situations_current', SCRIPTS / 'publish-situations-current.py')
m = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(m)
PUBLISHED = ROOT / 'docs/rust-rewrite/situations-current.json'


def campaign(directory, report):
    """Rebuild the campaign shape that benchmark-situations.py retains."""
    (directory / 'sources').mkdir(parents=True)
    manifest = dict(schema='scriptw-retained-situations-v1', source_format='module',
                    execution=[{key: value for key, value in case.items() if key != 'source_sha256'}
                               for case in report['workloads']])
    (directory / 'sources/manifest.json').write_text(json.dumps(manifest))
    (directory / 'report.json').write_text(json.dumps(report))


def args(**values):
    return type('Args', (), values)


class Publication(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        published = json.loads(PUBLISHED.read_text())
        cls.report = {key: value for key, value in published.items() if key in (
            'schema', 'status', 'complete', 'started_at', 'workloads', 'cases', 'preparations', 'inputs',
            'tools', 'source_identity', 'policy', 'resources', 'machine', 'tooling', 'finished_at')}
        cls.report['rows'] = [dict(file=case['file'], actor=actor, status='ok', raw={'observations': []})
                              for case in published['cases'] for actor in m.ACTORS for _ in range(8)]
        cls.report['commands'] = [dict(id='host-identity', status='ok')]
        cls.report['retained_files'] = []

    def publish(self, report):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / 'campaign'
            campaign(root, report)
            output = Path(directory) / 'current.json'
            m.publish(args(campaign=root, date='2026-11-16', cpu_policy='one pinned CPU', output=output))
            return json.loads(output.read_text())

    def test_publication_keeps_reductions_and_drops_raw_rows(self):
        current = self.publish(self.report)
        self.assertFalse(set(m.DROPPED) & set(current))
        self.assertEqual(current['cases'], self.report['cases'])
        self.assertEqual(current['publication']['consumer_processes'], 504)
        self.assertEqual(current['publication']['checked_invocations'], 60984)
        self.assertEqual(current['publication']['date'], '2026-11-16')

    def test_incomplete_partial_or_misreduced_campaigns_are_refused(self):
        changes = []
        report = copy.deepcopy(self.report)
        report['complete'] = False
        changes.append(report)
        report = copy.deepcopy(self.report)
        report['cases'].pop()
        changes.append(report)
        report = copy.deepcopy(self.report)
        report['cases'][0]['runtimes']['v8']['median_ns'] += 1
        changes.append(report)
        report = copy.deepcopy(self.report)
        report['rows'][0]['status'] = 'failed'
        changes.append(report)
        report = copy.deepcopy(self.report)
        report['policy']['actors'] = ['scriptw-fast', 'v8']
        changes.append(report)
        for index, report in enumerate(changes):
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.publish(report)

    def test_benchmarks_markdown_quotes_the_published_report(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            m.tables(args(report=PUBLISHED))
        document = (ROOT / 'docs/rust-rewrite/benchmarks.md').read_text()
        for line in output.getvalue().splitlines():
            with self.subTest(line=line):
                self.assertIn(line, document)


if __name__ == '__main__':
    unittest.main()
