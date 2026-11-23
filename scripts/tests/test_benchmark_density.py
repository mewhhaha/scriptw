"""The process-density benchmark holds live processes equivalently and derives capacity from summed PSS."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
spec = importlib.util.spec_from_file_location('process_density_benchmark', ROOT / 'scripts/benchmark-density.py')
density = importlib.util.module_from_spec(spec)
spec.loader.exec_module(density)

ROLLUP = """55a0c0000000-7ffd00000000 ---p 00000000 00:00 0                          [rollup]
Rss:                2088 kB
Pss:                 549 kB
Pss_Dirty:           312 kB
Pss_Anon:            248 kB
Pss_File:            301 kB
Pss_Shmem:             0 kB
Shared_Clean:       1752 kB
Shared_Dirty:          0 kB
Private_Clean:        28 kB
Private_Dirty:       308 kB
Referenced:         2088 kB
Anonymous:           248 kB
Swap:                  0 kB
"""


class DensityTests(unittest.TestCase):
    def test_rollup_reports_bytes_and_uss_and_rejects_swapped_or_malformed_readings(self):
        self.assertEqual(density.parse_rollup(ROLLUP), dict(rss=2088 * 1024, pss=549 * 1024, uss=336 * 1024,
                                                            private_dirty=308 * 1024, pss_anon=248 * 1024))
        with self.assertRaises(ValueError):
            density.parse_rollup(ROLLUP.replace('Swap:                  0 kB', 'Swap:                  4 kB'))
        with self.assertRaises(ValueError):
            density.parse_rollup(ROLLUP.replace('Pss:                 549 kB', 'Pss:                 549 MB'))

    def test_capacity_is_linear_in_summed_pss_and_never_uses_rss(self):
        mib = 1 << 20
        result = density.capacity({1: 16 * mib, 8: 16 * mib + 7 * 5 * mib, 32: 16 * mib + 31 * 5 * mib})
        self.assertEqual((result['marginal_bytes'], result['fixed_bytes']), (5 * mib, 11 * mib))
        self.assertEqual(result['per_gib'], (1024 - 11) // 5)
        self.assertEqual(result['per_16_gib'], (16 * 1024 - 11) // 5)
        self.assertEqual(result['marginal_check_bytes'], dict(count=8, bytes=5 * mib))
        self.assertIsNone(density.capacity({8: 10, 32: 20}))
        self.assertIsNone(density.capacity({1: 20, 32: 10}))

    def test_admission_keeps_the_reserve_free_for_unshared_rss(self):
        gib = density.GIB
        self.assertTrue(density.admission(32, 50 << 20, 10 * gib)['admitted'])
        self.assertFalse(density.admission(32, 50 << 20, 6 * gib + (1 << 30))['admitted'])

    def test_plans_balance_actors_and_skip_failed_attempts(self):
        admitted = {program: list(density.WALL_ACTORS) for program in density.PROGRAMS}
        admitted['objects.js'].remove(density.PEER)
        plan = density.density_plan(density.PROGRAMS, density.COUNTS, density.GROUPS, admitted)
        for program in density.PROGRAMS:
            for actor in density.DENSITY_ACTORS:
                rows = [row for row in plan if row['program'] == program and row['actor'] == actor]
                expected = 0 if (program, actor) == ('objects.js', density.PEER) else len(density.COUNTS) * density.GROUPS
                self.assertEqual(len(rows), expected)
        steady = density.steady_plan(density.PROGRAMS, admitted)
        for program in density.PROGRAMS:
            for actor in density.STEADY_ACTORS:
                blocks = [row['block'] for row in steady if row['program'] == program and row['actor'] == actor]
                self.assertEqual(sorted(blocks), [0] * (density.STEADY_PROCESSES // 2) + [1] * (density.STEADY_PROCESSES // 2))

    def test_public_json_round_trips_with_one_line_per_raw_row(self):
        summary = dict(schema='s', density=[dict(a=1)], raw=dict(fields=['rss', 'pss'], density=[dict(samples=[[1, 2], [3, 4]])],
                                                                 host_identity_ns=[5, 6]))
        text = density.public_json(summary)
        self.assertEqual(json.loads(text), summary)
        self.assertIn('\n   {"samples":[[1,2],[3,4]]}\n', text)

    def test_wall_outputs_are_checked_per_actor(self):
        self.assertEqual(density.check_wall_output('node', b'42\n', '42\n'), ({}, ''))
        self.assertIsNone(density.check_wall_output('node', b'41\n', '42\n')[0])
        self.assertEqual(density.check_wall_output('scriptw.run', b'42\nrun outcome: done\n', '42\n'), ({}, ''))
        self.assertIsNone(density.check_wall_output('scriptw.run', b'42\n42\nrun outcome: done\n', '42\n')[0])
        self.assertIsNone(density.check_wall_output('scriptw', b'not json', '42\n')[0])


class HoldTests(unittest.TestCase):
    """The hold mechanisms themselves, on real short-lived processes."""

    def setUp(self):
        self.folder = Path(tempfile.mkdtemp())
        self.env = {key: value for key, value in os.environ.items() if not key.startswith('NODE_')}

    def tearDown(self):
        shutil.rmtree(self.folder)

    def streams(self):
        return (self.folder / 'out').open('xb'), (self.folder / 'err').open('xb')

    def test_ptrace_exit_stop_holds_the_address_space_until_release(self):
        stdout, stderr = self.streams()
        with stdout, stderr:
            member = density.start_traced(['/bin/sh', '-c', 'echo held'], self.env, stdout, stderr)
            try:
                density.wait_traced([member], release=False)
                self.assertTrue(member['held'])
                sample = density.read_process(member['pid'])
                self.assertGreater(sample[density.FIELDS.index('pss')], 0)
                density.release_all([member])
            except BaseException:
                density.kill_all([member])
                raise
        self.assertEqual(member['exit_code'], 0)
        self.assertEqual((self.folder / 'out').read_text(), 'held\n')

    @unittest.skipUnless(shutil.which('node'), 'Node is not installed')
    def test_node_preload_speaks_the_host_barrier_protocol(self):
        program = self.folder / 'program.js'
        program.write_text('console.log("done");\n')
        stdout, stderr = self.streams()
        with stdout, stderr:
            argv = [shutil.which('node'), '--require', str(density.HOLD_HARNESS), str(program)]
            member = density.start_barrier(argv, self.env, stdout, stderr, scriptw=False)
            try:
                density.serve_barriers([member], release=False)
                self.assertTrue(member['held'].endswith(' after_execution 0\n'))
                # The program has already printed: the hold is after execution, before exit.
                self.assertEqual((self.folder / 'out').read_text(), 'done\n')
                self.assertGreater(density.read_process(member['pid'])[density.FIELDS.index('pss')], 0)
                density.release_all([member])
            except BaseException:
                density.kill_all([member])
                raise
        self.assertEqual(member['exit_code'], 0)


if __name__ == '__main__':
    unittest.main()
