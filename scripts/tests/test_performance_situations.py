"""Cheap structural and independent checksum checks; V8 validation is opt-in."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import performance_situations as situations

ROOT = Path(__file__).resolve().parents[2]


class SituationTests(unittest.TestCase):
    def test_inventory_has_bounded_independently_climbable_families(self):
        entries = situations.workloads()
        self.assertEqual(len(entries), 13)
        self.assertEqual(len({entry['file'] for entry in entries}), 13)
        self.assertEqual({entry['category'] for entry in entries}, {
            'control', 'global lookup', 'frame size', 'arguments observation',
            'property mutation history'})
        for entry in entries:
            with self.subTest(file=entry['file']):
                self.assertLess(len(entry['source'].encode()), 40000)
                self.assertGreater(entry['operations'], 0)
                self.assertLessEqual(entry['operations'], 8192)
                self.assertEqual([item['input'] for item in entry['invocations']], [3, 5, 7, 11])
                self.assertEqual(entry['stdout'], entry['invocations'][0]['stdout'])
                self.assertEqual(entry['source'].count('export function prepare('), 1)
                self.assertEqual(entry['source'].count('export function run('), 1)
                self.assertNotIn('?', entry['source'])
                self.assertGreater(len({item['stdout'] for item in entry['invocations']}), 1)

    def test_known_checksums_and_equivalent_pairs(self):
        # Pin the reference calculation independently of manifest serialization.
        self.assertEqual([situations.frame_checksum(seed) for seed in situations.SEEDS],
                         [1495040, 1527808, 1552384, 1593344])
        self.assertEqual([situations.arguments_checksum(seed) for seed in situations.SEEDS],
                         [516096, 512000, 516096, 516096])
        self.assertEqual([situations.property_checksum(seed) for seed in situations.SEEDS],
                         [438272, 471040, 503808, 569344])
        entries = {entry['file']: entry for entry in situations.workloads()}
        for family in (['frame-compact', 'frame-branches32', 'frame-branches128', 'frame-branches512'],
                       ['args-unused', 'args-observed'], ['property-stable', 'property-mutated'],
                       ['global-shallow', 'global-deep8', 'global-bound-deep']):
            gold = entries[family[0] + '.js']['invocations']
            self.assertTrue(all(entries[name + '.js']['invocations'] == gold for name in family))
        for ordinary, getter in zip(entries['global-deep8.js']['invocations'],
                                    entries['global-getter-deep.js']['invocations']):
            self.assertEqual(int(getter['stdout']) - int(ordinary['stdout']), 1024)

    def test_generator_is_deterministic_without_overwriting(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / 'a', Path(directory) / 'b'
            a = situations.write_corpus(first)
            b = situations.write_corpus(second)
            self.assertEqual(a, b)
            self.assertEqual(a['schema'], 'scriptw-retained-situations-v1')
            self.assertEqual(a['source_format'], 'module')
            self.assertEqual({path.name: path.read_bytes() for path in first.iterdir()},
                             {path.name: path.read_bytes() for path in second.iterdir()})
            self.assertEqual(json.loads((first / 'manifest.json').read_text()), a)
            self.assertTrue(all('source' not in entry for entry in a['execution']))
            with self.assertRaises(FileExistsError):
                situations.write_corpus(first)

    def test_source_retains_semantic_controls(self):
        entries = {entry['file']: entry['source'] for entry in situations.workloads()}
        deep = entries['global-deep8.js']
        self.assertEqual(deep.count('return (function ()'), 7)
        for level in range(1, 9):
            self.assertIn(f'var p{level} = input + {level};', deep)
        self.assertIn('getterCount++', entries['global-getter-deep.js'])
        self.assertIn('if (input & 2) mapValue = alternateMap;', entries['global-getter-deep.js'])
        self.assertIn('arguments[0]', entries['args-observed.js'])
        self.assertNotIn('arguments[', entries['args-unused.js'])
        self.assertIn('delete row.x;', entries['property-mutated.js'])
        self.assertNotIn('delete row.x;', entries['property-stable.js'])
        with self.assertRaises(ValueError):
            situations.frame_source(3)

    def test_global_comparison_references_are_nearest_worker_captures(self):
        names = ('Map', 'Set', 'Uint8Array', 'DataView', 'Date', 'Number')
        for kind in ('shallow', 'deep', 'bound-deep', 'getter-deep'):
            with self.subTest(kind=kind):
                source = situations.global_source(kind)
                factory, worker = source.split('return function () {', 1)
                worker = worker.split('return sum;', 1)[0]
                self.assertNotIn('saved', worker)
                self.assertNotIn('expectedMap', worker)
                self.assertIn('var eMap = expectedMap;', factory)
                for name in names:
                    actual = name
                    if kind == 'bound-deep':
                        actual = 'b' + name
                        self.assertIn(f'var {actual} = e{name};', factory)
                    self.assertIn(f'if ({actual} === e{name}) sum++;', worker)
                # No intervening function puts expected references in a farther scope.
                capture_tail = factory[factory.rindex('var eMap = expectedMap;'):]
                self.assertNotIn('function', capture_tail)

    def test_frame_fanout_exercises_every_site_without_reducing_entries(self):
        entries = [entry for entry in situations.workloads() if entry['category'] == 'frame size']
        self.assertEqual([entry['source_sites'] for entry in entries], [1, 32, 128, 512])
        for entry in entries:
            with self.subTest(file=entry['file']):
                sites = entry['source_sites']
                self.assertEqual(entry['operations'], 8192)
                self.assertEqual(entry['frame_entries'], 8192)
                self.assertIn('sum += frame(i, rows[i & 63]);', entry['source'])
                self.assertIn('call < 16', entry['source'])
                self.assertIn('i < 512', entry['source'])
                branches = re.findall(r'case (\d+): return steps\[([01])\]\(row\);', entry['source'])
                if sites == 1:
                    self.assertFalse(branches)
                    self.assertIn('return steps[index & 1](row);', entry['source'])
                else:
                    self.assertEqual([(int(site), int(step)) for site, step in branches],
                                     [(site, site & 1) for site in range(sites)])
                    self.assertEqual({index & (sites - 1) for index in range(512)}, set(range(sites)))
                    self.assertIn(f'switch (index & {sites - 1})', entry['source'])

    @unittest.skipUnless(os.environ.get('SCRIPTW_SITUATIONS_NODE'),
                         'set SCRIPTW_SITUATIONS_NODE to pinned Node v24.12.0 for V8 checks')
    def test_all_seed_checksums_on_pinned_node(self):
        node = os.environ['SCRIPTW_SITUATIONS_NODE']
        adapter = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_baseline/node-situations.cjs'
        with tempfile.TemporaryDirectory() as directory:
            fixtures = Path(directory) / 'situations'
            plan = situations.write_corpus(fixtures)
            for entry in plan['execution']:
                with self.subTest(file=entry['file']):
                    result = subprocess.run([node, '--experimental-vm-modules', str(adapter),
                                             str(fixtures), entry['file'], '4', '4'],
                                            text=True, capture_output=True, timeout=30)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    report = json.loads(result.stdout)
                    self.assertEqual(len(report['observations']), 8)
                    self.assertTrue(all(row['matched'] for row in report['observations']))
                    self.assertEqual([row['input'] for row in report['observations']],
                                     list(situations.SEEDS) * 2)


if __name__ == '__main__':
    unittest.main()
