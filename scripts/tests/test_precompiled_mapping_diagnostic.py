"""Inert metadata and owned Python-child controls; no guest image execution."""
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'scripts'))
spec = importlib.util.spec_from_file_location('mapping_diagnostic', ROOT/'scripts/diagnose-precompiled-mappings.py')
d = importlib.util.module_from_spec(spec)
spec.loader.exec_module(d)
from scripts.tests import test_precompiled_comparison as fixtures


class MappingDiagnosticTests(unittest.TestCase):
    def test_fork_before_exec_and_wrong_inode_cannot_enter_samples(self):
        host = Path('/live/bin/consumer')
        info = type('Info', (), {'st_ino': 12})()
        raw = b'1000-2000 r-xp 00000000 00:21 12 /usr/bin/python3\nRss: 4 kB\n'
        self.assertFalse(d.exact_executable(d.mappings(raw), host, info))
        self.assertFalse(d.exact_executable(d.mappings(raw.replace(b'/usr/bin/python3', str(host).encode()).replace(b' 12 ', b' 13 ')), host, info))
        # A proven main executable may have a device alias; retain the maps device.
        rows = d.mappings(raw.replace(b'/usr/bin/python3', str(host).encode()))
        self.assertTrue(d.exact_executable(rows, host, info))
        self.assertEqual(rows[0]['device'], [0, 33])
        self.assertFalse(d.exact_executable(d.mappings(raw.replace(b'r-xp', b'r--p')), host, info))

    def test_truncated_repeated_and_oversized_inputs_are_rejected(self):
        with self.assertRaises(ValueError):
            d.mappings(b'1000-2000 r-xp 00000000 00:21 12 /live/consumer\n')
        with self.assertRaises(ValueError):
            d.mappings(b'1000-2000 r-xp 00000000 00:21 12 /live/consumer\nRss: 4 kB\nRss: 8 kB\n')
        with self.assertRaises(ValueError):
            d.unique_json(b'{"compiler_enabled":false,"compiler_enabled":true}')
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)/'oversized'
            path.write_bytes(b'12345')
            with self.assertRaises(ValueError):
                d.bounded_read(path, 4)

    def test_live_authority_rejects_archive_paths_and_source_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            campaign = Path(temporary)
            host = campaign/'bin/consumer'
            host.parent.mkdir()
            host.write_bytes(b'inert; never executed')
            report = dict(source_commit='a'*40, policy=dict(consumer_compiler_enabled=False),
                          tools=dict(consumer=dict(path=str(host), sha256=d.p.digest(host), identity_command='identity')),
                          commands=[dict(id='identity', kind='tool-version', argv=[str(host), 'identity'])])
            self.assertEqual(d.live_campaign(campaign, report, 'a'*40), host)
            with self.assertRaises(ValueError):
                d.live_campaign(campaign, report, 'b'*40)
            report['commands'][0]['argv'][0] = '/downloaded/archive/consumer'
            with self.assertRaises(ValueError):
                d.live_campaign(campaign, report, 'a'*40)
            report['policy']['consumer_compiler_enabled'] = True
            with self.assertRaises(ValueError):
                d.live_campaign(campaign, report, 'a'*40)

    def test_endpoint_rejects_compiler_identity_checksum_and_missing_metadata(self):
        fixture = fixtures.PrecompiledComparisonTests()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        report = fixture.report
        item = next(row for row in report['attempts'] if row['status'] == 'ok')
        event = next(row for row in report['commands'] if row['id'] == item['consumer_id'])
        raw = json.loads((fixture.directory/event['stdout']).read_bytes())
        case = next(row for row in report['workloads'] if row['file'] == item['file'])
        def validate(value):
            d.validate_endpoint(json.dumps(value).encode(), report, item['image'], case, item['runtime'])
        validate(raw)
        for field, value in [('compiler_enabled', True), ('source_fingerprint', 'f'*64),
                             ('samples', 6), ('shared_configuration', {}), ('image_sha256', '0'*64)]:
            with self.subTest(field=field):
                changed = dict(raw, **{field:value})
                with self.assertRaises(ValueError):
                    validate(changed)
        changed = dict(raw)
        del changed['process_snapshots']
        with self.assertRaises(ValueError):
            validate(changed)

    def test_presence_retains_only_present_bits_and_limits_pages(self):
        host = Path('/live/consumer')
        page = os.sysconf('SC_PAGE_SIZE')
        rows = [dict(path=str(host), permissions='r--p', start=page, end=page*3, file_offset=0)]
        # PFNs and all non-present bits must disappear from the representation.
        data = ((1 << 63) | 0x123456789).to_bytes(8, 'little') + ((1 << 62) | 0xabcdef).to_bytes(8, 'little')
        with patch.object(Path, 'open', unittest.mock.mock_open()), patch.object(os, 'pread', return_value=data):
            result = d.presence(123, rows, host)
        self.assertEqual(result[0]['present_bits_lsb_hex'], '01')
        rows[0]['end'] = page*65539
        with self.assertRaises(ValueError):
            d.presence(123, rows, host)


class TrustedPipeDrainTests(unittest.TestCase):
    """Only trusted Python children; never JS, Wasm or serialized native images."""

    def child(self, code, *arguments):
        process = subprocess.Popen([sys.executable, '-I', '-c', code, *map(str, arguments)],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        def cleanup():
            if process.poll() is None:
                d.kill_owned_group(process)
                process.wait(timeout=2)
            for pipe in (process.stdout, process.stderr):
                pipe.close()
        self.addCleanup(cleanup)
        return process

    def test_private_file_above_stream_cap_and_both_pipes_drain_to_eof(self):
        with tempfile.TemporaryDirectory() as temporary:
            backing = Path(temporary)/'private-backing'
            process = self.child("import pathlib,sys; pathlib.Path(sys.argv[1]).write_bytes(b'x'*(3*1024**2)); "
                                 "sys.stdout.buffer.write(b'a'*70000); sys.stderr.buffer.write(b'b'*70000)", backing)
            outputs = dict(stdout=io.BytesIO(), stderr=io.BytesIO())
            d.drain_owned(process, outputs, lambda: None, deadline=10)
            self.assertEqual(process.returncode, 0)
            self.assertEqual(backing.stat().st_size, 3*1024**2)
            self.assertEqual(outputs['stdout'].getvalue(), b'a'*70000)
            self.assertEqual(outputs['stderr'].getvalue(), b'b'*70000)
            self.assertTrue(process.stdout.closed and process.stderr.closed)

    def test_stream_overflow_retains_bounded_prefix_and_reaps_child(self):
        process = self.child("import os,time; os.write(2,b'raw diagnostic failure\\n'); "
                             "os.write(1,b'x'*(4*1024**2)); time.sleep(60)")
        outputs = dict(stdout=io.BytesIO(), stderr=io.BytesIO())
        with self.assertRaisesRegex(ValueError, 'stream byte bound exceeded'):
            d.drain_owned(process, outputs, lambda: None, deadline=10)
        self.assertEqual(sum(len(stream.getvalue()) for stream in outputs.values()), d.MAX_READ)
        self.assertIsNotNone(process.returncode)
        self.assertTrue(process.stdout.closed and process.stderr.closed)

    def test_deadline_reaps_only_owned_group_and_closes_pipes(self):
        process = self.child("import time; print('started',flush=True); time.sleep(60)")
        outputs = dict(stdout=io.BytesIO(), stderr=io.BytesIO())
        with self.assertRaisesRegex(ValueError, 'deadline exceeded'):
            d.drain_owned(process, outputs, lambda: None, deadline=0.2)
        self.assertIsNotNone(process.returncode)
        self.assertTrue(process.stdout.closed and process.stderr.closed)
