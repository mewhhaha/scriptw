"""Reject incomplete or mixed full-run evidence before changing the README chart."""
import copy
import base64
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tarfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import test262_chart_evidence as evidence


class FullChartEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.counts = dict(Success=1, Bug=1, Crash=0, NotImplemented=0)
        self.real = dict(backend='wasm-aot', manifest_hash=99, total=2, passed=1,
            compiler_identity={'source': 'exact'}, pinned_revisions={'test262': 'pin'},
            snapshot_paths={'json': '/native/run-aggregate-99.json'},
            counts_per_outcome=[dict(label=k, count=v) for k, v in self.counts.items()])
        identity = dict(snapshot_version=8, producer='scriptw', execution_backend='wasm-aot',
            compiler_identity=self.real['compiler_identity'], pinned_revisions=self.real['pinned_revisions'],
            matrix_strategy_version=3, total=2, passed=1, counts_per_outcome=self.counts)
        self.entry = dict(node_id='language/a', node_kind='filter-leaf', manifest_hash=7,
            matrix_path=['language', 'a'], total=2, passed=1, counts_per_outcome=self.counts)
        self.aggregate = dict(identity, manifest_hash=99, run_kind='aggregate-matrix',
            matrix_path=['top-level'], completed_nodes=['language/a'], aggregate_entries=[self.entry])
        self.leaf = dict(identity, manifest_hash=7, run_kind='matrix-filter-leaf',
            matrix_path=['language', 'a'], completed_test_ids=['strict-script:a.js', 'strict-script:b.js'],
            failures=[dict(test_id='strict-script:b.js', outcome='Bug')])
        self.snapshots = {'run-aggregate-99.json': self.aggregate, 'run-language_a-7.json': self.leaf}

    def check(self):
        return evidence.checked_full_outcomes(self.real, self.aggregate, self.snapshots)

    def test_exact_native_execution_census_is_recovered(self):
        self.assertEqual(self.check(), {'strict-script:a.js': 'Success', 'strict-script:b.js': 'Bug'})

    def test_identity_and_result_mismatches_are_rejected(self):
        for change in [dict(snapshot_version=7), dict(producer='other'), dict(execution_backend='spec-exec'),
                       dict(compiler_identity={'source': 'different'}), dict(pinned_revisions={}),
                       dict(manifest_hash=8), dict(matrix_path=['other']), dict(run_kind='full'),
                       dict(matrix_strategy_version=2), dict(passed=True)]:
            with self.subTest(change=change):
                original = self.snapshots['run-language_a-7.json']
                self.snapshots['run-language_a-7.json'] = dict(self.leaf, **change)
                with self.assertRaises(ValueError): self.check()
                self.snapshots['run-language_a-7.json'] = original

    def test_missing_duplicate_and_unmeasured_leaf_results_are_rejected(self):
        for change in [dict(completed_test_ids=['strict-script:a.js']),
                       dict(completed_test_ids=['strict-script:a.js'] * 2),
                       dict(failures=[]), dict(failures=self.leaf['failures'] * 2),
                       dict(failures=[dict(test_id='strict-script:missing.js', outcome='Bug')]),
                       dict(failures=[dict(test_id='strict-script:b.js', outcome='Crash')])]:
            with self.subTest(change=change):
                self.snapshots['run-language_a-7.json'] = dict(self.leaf, **change)
                with self.assertRaises(ValueError): self.check()

    def test_aggregate_and_file_inventory_cannot_hide_missing_or_extra_nodes(self):
        for change in [dict(completed_nodes=[]), dict(completed_nodes=['language/a'] * 2),
                       dict(aggregate_entries=[self.entry] * 2), dict(manifest_hash=100), dict(total=True)]:
            with self.subTest(change=change), self.assertRaises(ValueError):
                evidence.checked_full_outcomes(self.real, dict(self.aggregate, **change), self.snapshots)
        for snapshots in [{}, dict(self.snapshots, extra=self.leaf)]:
            with self.assertRaisesRegex(ValueError, 'inventory'):
                evidence.checked_full_outcomes(self.real, self.aggregate, snapshots)

    def test_overlapping_ids_across_distinct_leaves_are_rejected(self):
        other = dict(self.entry, node_id='language/b', manifest_hash=8, matrix_path=['language', 'b'])
        self.aggregate.update(completed_nodes=['language/a', 'language/b'], aggregate_entries=[self.entry, other])
        self.snapshots['run-language_b-8.json'] = dict(self.leaf, manifest_hash=8, matrix_path=['language', 'b'])
        with self.assertRaisesRegex(ValueError, 'overlapping'): self.check()

    def test_actual_archived_full_native_run_validates_without_failure_replay(self):
        # Immutable complete source20 baseline, not a new execution campaign.
        with tarfile.open(ROOT / 'test262/snapshots/current-test262-20261020.tar.gz', 'r:gz') as archive:
            snapshots = {Path(m.name).name: json.load(archive.extractfile(m)) for m in archive
                if m.isfile() and m.name.startswith('snapshots/scriptw-current-20261020-') and m.name.endswith('.json')}
            publication = json.load(archive.extractfile('snapshots/published-status-wasm-aot.json'))
        aggregate, = [s for s in snapshots.values() if s['run_kind'] == 'aggregate-matrix']
        # Keep this regression fixture stable when a later status is published.
        real = copy.deepcopy(publication['real_suite'])
        real.update(manifest_hash=aggregate['manifest_hash'], compiler_identity=aggregate['compiler_identity'],
            pinned_revisions=aggregate['pinned_revisions'], total=aggregate['total'], passed=aggregate['passed'],
            snapshot_paths={'json': f"scriptw-current-20261020-aggregate-{aggregate['manifest_hash']}.json"},
            counts_per_outcome=[dict(label=k, count=v) for k, v in aggregate['counts_per_outcome'].items()])
        outcomes = evidence.checked_full_outcomes(real, aggregate, snapshots)
        self.assertEqual(len(outcomes), 102994)
        self.assertEqual(sum(v != 'Success' for v in outcomes.values()), 16)

    def test_fresh_chart_caption_does_not_mix_in_historical_replay(self):
        spec = importlib.util.spec_from_file_location('full_chart', ROOT / 'scripts/generate-test-graph.py')
        graph = importlib.util.module_from_spec(spec); spec.loader.exec_module(graph)
        real = dict(self.real, refresh_date='2026-10-22',
            compiler_identity={'source_revision': {'kind': 'git-commit', 'commit': 'b' * 40}})
        record = dict(date=real['refresh_date'], full_current_source_rerun=True,
            scoped_chart_counts=dict(Success=0, Bug=1, Crash=0, NotImplemented=0, OutOfScope=1))
        text = graph.render(dict(real_suite=real), reconciliation=record)
        svg = graph.render_svg(dict(real_suite=real), record)
        self.assertIn('Published full run', text)
        self.assertIn('compiler `bbbbbbbbb`', text)
        self.assertNotIn('2026-10-20', text)
        self.assertNotIn('not a fresh full rerun', svg)
        self.assertIn('Fresh complete pinned-suite run', svg)
        self.assertIn('failures remain counted', svg)

    def test_portable_full_record_roundtrips_and_rejects_corruption(self):
        published_path = 'test262/snapshots/current-test262-20261020.tar.gz'
        retained = []
        with tarfile.open(ROOT / published_path, 'r:gz') as archive:
            publication = json.load(archive.extractfile('snapshots/published-status-wasm-aot.json'))
            for member in archive:
                if not member.isfile() or not member.name.startswith('snapshots/scriptw-current-20261020-') or not member.name.endswith('.json'):
                    continue
                raw = archive.extractfile(member).read()
                packed = gzip.compress(raw, mtime=0)
                retained.append(dict(name=Path(member.name).name, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest(),
                    encoding='gzip+base64', gzip_sha256=hashlib.sha256(packed).hexdigest(), data=base64.b64encode(packed).decode()))
                snapshot = json.loads(raw)
                if snapshot['run_kind'] == 'aggregate-matrix': aggregate = snapshot
        publication = copy.deepcopy(publication)
        publication['real_suite'].update(refresh_date='2026-10-20', total=aggregate['total'], passed=aggregate['passed'],
            manifest_hash=aggregate['manifest_hash'], compiler_identity=aggregate['compiler_identity'],
            pinned_revisions=aggregate['pinned_revisions'],
            snapshot_paths={'json': f"scriptw-current-20261020-aggregate-{aggregate['manifest_hash']}.json"},
            counts_per_outcome=[dict(label=k, count=v) for k, v in aggregate['counts_per_outcome'].items()])
        with patch.object(evidence, 'load_publication', return_value=(publication, published_path)):
            record = evidence.build_full(ROOT, retained)
            self.assertTrue(record['full_current_source_rerun'])
            self.assertEqual(record['scoped_chart_counts'], dict(Success=102549, Bug=16, Crash=0, NotImplemented=0, OutOfScope=429))
            self.assertEqual(record, evidence.build_full(ROOT, record['native_snapshots']))
            broken = copy.deepcopy(retained)
            broken[-1]['sha256'] = '0' * 64
            with self.assertRaisesRegex(ValueError, 'payload digest'):
                evidence.build_full(ROOT, broken)


if __name__ == '__main__':
    unittest.main()
