"""Filesystem admission controls only; these are not native suite evidence."""

import ast
from scripts.tests.workspace_runner_support import runner_body
from collections import namedtuple
import importlib.util
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

SOURCE = Path(__file__).resolve().parents[1] / 'workspace_test_storage.py'
spec = importlib.util.spec_from_file_location('workspace_test_storage', SOURCE)
storage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(storage)
DiskUsage = namedtuple('DiskUsage', 'total used free')
GIB = 1024**3


class StorageAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.private = tempfile.TemporaryDirectory(dir=SOURCE.parents[1])
        self.addCleanup(self.private.cleanup)
        self.workspace = Path(self.private.name)
        self.temp = Path(tempfile.gettempdir()).resolve()
        self.target = self.workspace / 'target'
        self.output = self.workspace / 'receipt'
        self.cache = self.workspace / 'cache'
        for directory in (self.target, self.output, self.cache):
            directory.mkdir()
        self.environment = {'TMPDIR': str(self.temp),
                            'SCRIPTW_CACHE_DIR': str(self.cache)}

    def snapshot(self, *, workspace_free=10 * GIB, temporary_free=10 * GIB,
                 processes=1, building=False):
        temp_device = self.temp.stat().st_dev
        workspace_device = self.workspace.stat().st_dev
        if temp_device == workspace_device:
            self.skipTest('separate writable test filesystems are required')
        def disk_usage(path):
            free = temporary_free if path.stat().st_dev == temp_device else workspace_free
            return DiskUsage(20 * GIB, 20 * GIB - free, free)
        with patch.object(storage.shutil, 'disk_usage', side_effect=disk_usage):
            return storage.storage_snapshot(self.target, self.output,
                self.environment, native_processes=processes, building=building)

    def test_full_temp_blocks_a_cached_native_image_despite_room_for_links(self):
        tier = self.cache / 'wasmtime-modules-v1'
        tier.symlink_to(self.temp, target_is_directory=True)
        result = self.snapshot(temporary_free=20 * 1024**2)
        self.assertFalse(result['sufficient'])
        temporary = next(row for row in result['filesystems']
                         if row['device'] == self.temp.stat().st_dev)
        paths = {row['role']: row for row in temporary['paths']}
        self.assertEqual(paths['cache-wasmtime-modules-v1']['logical_path'], str(tier))
        self.assertEqual(paths['cache-wasmtime-modules-v1']['physical_path'], str(self.temp))
        self.assertEqual(result['build_reserve_bytes'], 0)

    def test_aliases_share_one_margin_on_each_actual_device(self):
        for tier in storage.CACHE_TIERS:
            (self.cache / tier).symlink_to(self.temp, target_is_directory=True)
        result = self.snapshot(workspace_free=storage.NATIVE_SCRATCH_BYTES,
                               temporary_free=storage.NATIVE_SCRATCH_BYTES)
        self.assertTrue(result['sufficient'])
        self.assertEqual(len(result['filesystems']), 2)
        self.assertTrue(all(row['required_bytes'] == storage.NATIVE_SCRATCH_BYTES
                            for row in result['filesystems']))
        self.assertEqual(sum(len(row['paths']) for row in result['filesystems']), 8)

    def test_active_and_scheduled_processes_need_concurrent_scratch_headroom(self):
        self.assertFalse(self.snapshot(temporary_free=900 * 1024**2,
                                       processes=4)['sufficient'])
        self.assertTrue(self.snapshot(temporary_free=900 * 1024**2,
                                      processes=3)['sufficient'])

    def test_build_reserve_remains_two_gib_including_exact_boundary(self):
        self.assertFalse(self.snapshot(workspace_free=2 * GIB - 1,
                                       building=True)['sufficient'])
        self.assertTrue(self.snapshot(workspace_free=2 * GIB,
                                      building=True)['sufficient'])
        self.assertTrue(self.snapshot(workspace_free=2 * GIB - 1,
                                      building=False)['sufficient'])

    def test_missing_cache_leaves_use_the_real_parent_without_creation(self):
        result = self.snapshot()
        self.assertTrue(result['sufficient'])
        for filesystem in result['filesystems']:
            for row in filesystem['paths']:
                if row['role'].startswith('cache-'):
                    self.assertEqual(row['probed_ancestor'], str(self.cache))
                    self.assertFalse(Path(row['physical_path']).exists())

    def test_cache_defaults_and_python_rust_temp_disagreement_remain_visible(self):
        with patch.object(storage.tempfile, 'gettempdir', return_value='/python-temp'):
            paths = dict(storage.writable_paths('/target', '/output', {
                'TMPDIR': '/rust-temp', 'XDG_CACHE_HOME': '/xdg'}))
        self.assertEqual(paths['python-temporary'], Path('/python-temp'))
        self.assertEqual(paths['rust-temporary'], Path('/rust-temp'))
        self.assertEqual(paths['cache-program-wasm-v1'], Path('/xdg/scriptw/program-wasm-v1'))
        defaults = dict(storage.writable_paths('/target', '/output', {}))
        self.assertEqual(defaults['rust-temporary'], Path('/tmp'))
        self.assertEqual(defaults['cache-program-wasm-v1'], Path('/tmp/scriptw/program-wasm-v1'))

    def test_dangling_cache_symlinks_probe_their_physical_destination(self):
        tier = self.cache / 'wasmtime-modules-v1'
        missing = self.temp / 'not-created-by-storage-admission-test' / 'modules'
        self.assertFalse(missing.exists())
        tier.symlink_to(missing, target_is_directory=True)
        result = self.snapshot(temporary_free=20 * 1024**2)
        self.assertFalse(result['sufficient'])
        row = next(path for fs in result['filesystems'] for path in fs['paths']
                   if path['role'] == 'cache-wasmtime-modules-v1')
        self.assertEqual(row['physical_path'], str(missing))
        self.assertEqual(row['probed_ancestor'], str(self.temp))
        self.assertFalse(missing.exists())

    def test_guard_requires_a_positive_scheduled_process_count(self):
        for count in (0, -1):
            with self.assertRaises(ValueError):
                storage.storage_snapshot(self.target, self.output,
                    self.environment, native_processes=count)


class RunnerAdmissionControls(unittest.TestCase):
    def runner_functions(self, active, available=False):
        """Run only actual admission functions, without importing the driver."""
        source = SOURCE.with_name('run_workspace_tests.py')
        functions = [node for node in runner_body(source)
                     if isinstance(node, ast.FunctionDef)
                     and node.name in ('admit_writable_space', 'admit_build_space')]
        calls = []
        saves = []
        def snapshot(*args, **kwargs):
            calls.append(kwargs)
            return {'sufficient': available or not active,
                    'filesystems': [], 'native_processes': kwargs['native_processes']}
        def reap():
            active.clear()
        namespace = {'active': active, 'storage_snapshot': snapshot,
            'cargo_target_dir': Path('/target'), 'out': Path('/output'),
            'build_env': {}, 'time': SimpleNamespace(monotonic=lambda: 0,
                                                     sleep=lambda _: None),
            'json': __import__('json'), 'save': lambda: saves.append(True),
            'print': lambda *args, **kwargs: None, 'reap': reap,
            'forward_progress': lambda: None,
            'shutil': SimpleNamespace(disk_usage=lambda _: DiskUsage(10 * GIB, 0, 10 * GIB))}
        exec(compile(ast.Module(body=functions, type_ignores=[]), str(source), 'exec'), namespace)
        return namespace, calls, saves

    def test_cached_native_admission_drains_then_rechecks_without_a_link(self):
        active = {0: object(), 1: object()}
        namespace, calls, saves = self.runner_functions(active)
        case = {}
        self.assertTrue(namespace['admit_build_space'](case, 'cached', cached_native=True))
        self.assertEqual(calls, [{'native_processes': 3, 'building': False},
                                 {'native_processes': 1, 'building': False}])
        self.assertEqual(active, {})
        self.assertNotIn('state', case)
        self.assertEqual(saves, [])

    def test_insufficient_scratch_is_deferred_without_a_native_verdict(self):
        namespace, calls, saves = self.runner_functions({}, available=True)
        namespace['storage_snapshot'] = lambda *args, **kwargs: {
            'sufficient': False, 'filesystems': [{'physical_path': '/tmp'}]}
        case = {}
        self.assertFalse(namespace['admit_writable_space'](case, 'native-execution', native_processes=4))
        self.assertEqual(case['state'], 'storage-deferred')
        self.assertNotIn('exit_code', case)
        self.assertNotIn('outcomes', case)
        self.assertEqual(saves, [True])
        self.assertEqual(case['storage_preflights'][0]['label'], 'native-execution')


if __name__ == '__main__':
    unittest.main()
