"""Inert metadata and owned Python-child controls; no guest image execution."""
import importlib.util
import io
import json
import os
import socket
from pathlib import Path
import sys
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'scripts'))
spec = importlib.util.spec_from_file_location('mapping_diagnostic', ROOT/'scripts/diagnose-precompiled-mappings.py')
d = importlib.util.module_from_spec(spec)
spec.loader.exec_module(d)
from scripts.tests import test_precompiled_comparison as fixtures


class MappingDiagnosticTests(unittest.TestCase):
    def test_final_inventory_failure_cannot_publish_successful_case_captures(self):
        # Exercise run's final persistence boundary without launching a consumer
        # or trusting synthetic bytes as a native image.
        for failure in (None, 'write', 'bound'):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                campaign = root/'campaign'
                campaign.mkdir()
                (campaign/'report.json').write_text('{}')
                host = campaign/'consumer'
                host.write_bytes(b'inert host; never executed')
                metadata = dict(sha256=d.p.digest(host), native_sha256='b'*64, runtime_key=None)
                report = dict(source_fingerprint='a'*64, shared_configuration={}, samples=7, warmups=10,
                              workloads=[], attempts=[], commands=[])
                for name in ('integer.js', 'retained-objects.js'):
                    report['workloads'].append(dict(file=name))
                    for runtime in ('scriptw', 'scriptc'):
                        image = campaign/(runtime+'-'+name)
                        image.write_bytes(host.read_bytes())
                        identifier = runtime+'-'+name
                        report['attempts'].append(dict(file=name, runtime=runtime, round=0, status='ok',
                            image=dict(metadata, file=image.name), consumer_id=identifier))
                        report['commands'].append(dict(id=identifier, argv=[str(host), 'unused', str(image)]))
                def captured(_host, _argv, destination, _remaining, _present, index):
                    destination.mkdir()
                    (destination/'stdout').write_text('{}')
                    return dict(status='barriers_complete', nonce='a'*32, observation_index=index, snapshots=[])
                original_write = Path.write_bytes
                def write(path, data):
                    if failure == 'write' and path.name == 'host-elf-symbols.json':
                        raise OSError('injected host inventory write failure')
                    return original_write(path, data)
                inventory = dict(sha256=metadata['sha256'], functions=[],
                                 sections='x'*1024 if failure == 'bound' else [])
                output = root/'diagnostic'
                with patch.object(d, 'kernel_budget', return_value={}), \
                     patch.object(d.p, 'load_report', return_value=(report, [])), \
                     patch.object(d.subprocess, 'check_output', return_value='a'*40), \
                     patch.object(d, 'live_campaign', return_value=host), \
                     patch.object(d.p, 'image_metadata', return_value=metadata), \
                     patch.object(d, 'capture', side_effect=captured), \
                     patch.object(d, 'validate_endpoint', return_value={}), \
                     patch.object(d.module_images, 'validate_module_images', return_value={}), \
                     patch.object(d.module_images, 'host_symbol_inventory', return_value=inventory), \
                     patch.object(d.module_images, 'attribute_host', return_value=dict(symbols=[], interpretation='inert')), \
                     patch.object(d, 'MAX_SYMBOL_METADATA_BYTES', 1024), \
                     patch.object(Path, 'write_bytes', write), patch('builtins.print'):
                    code = d.run(SimpleNamespace(campaign=campaign, output=output, present_bits=True))
                result = json.loads((output/'report.json').read_text())
                self.assertEqual(len(result['cases']), 4)
                self.assertTrue(all(case['status'] == 'captured' and case['module_attribution_validated']
                                    and case['host_symbol_attribution_validated'] for case in result['cases']))
                if failure is None:
                    self.assertEqual((code, result['status'], result['host_symbol_inventory_status']),
                                     (0, 'captured', 'validated'))
                    self.assertTrue((output/'host-elf-symbols.json').is_file())
                else:
                    self.assertEqual((code, result['status'], result['host_symbol_inventory_status']),
                                     (1, 'incomplete', 'unavailable'))
                    self.assertIn('write failure' if failure == 'write' else 'byte bound exceeded',
                                  result['host_symbol_inventory_error'])
                    self.assertNotIn('host_symbol_inventory', result)
                    self.assertFalse((output/'host-elf-symbols.json').exists())

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
            d.validate_endpoint(json.dumps(value).encode(), report, item['image'], case, item['runtime'], 'a'*32, 16)
        raw.update(phase_barriers=dict(schema='scriptw-precompiled-phase-barriers-v1', nonce='a'*32,
                   observation_index=16, completed_phases=list(d.PHASES)), excluded_from_scored_cohorts=True,
                   diagnostic_total_timings_include_barrier_waits=True)
        validate(raw)
        self.assertIn('phase_barriers', raw, 'ordinary admission must receive only a transient copy')
        for marker in (None, {}, dict(raw['phase_barriers'], nonce='b'*32),
                       dict(raw['phase_barriers'], observation_index=15),
                       dict(raw['phase_barriers'], completed_phases=list(d.PHASES[:-1]))):
            with self.subTest(marker=marker), self.assertRaises(ValueError):
                validate(dict(raw, phase_barriers=marker))
        with self.assertRaises(ValueError):
            validate(dict(raw, excluded_from_scored_cohorts=False))
        for timing_flag in (False, None, 1, 'true'):
            with self.subTest(timing_flag=timing_flag), self.assertRaises(ValueError):
                validate(dict(raw, diagnostic_total_timings_include_barrier_waits=timing_flag))
        missing_timing_flag = dict(raw)
        del missing_timing_flag['diagnostic_total_timings_include_barrier_waits']
        with self.assertRaises(ValueError):
            validate(missing_timing_flag)
        with self.assertRaises(ValueError):
            d.validate_endpoint(b'[]', report, item['image'], case, item['runtime'], 'a'*32, 16)
        for field, value in [('compiler_enabled', True), ('source_fingerprint', 'f'*64),
                             ('samples', 6), ('shared_configuration', {}), ('image_sha256', '0'*64)]:
            with self.subTest(field=field):
                changed = dict(raw, **{field:value})
                with self.assertRaises(ValueError):
                    validate(changed)
        changed = dict(raw)
        changed['observations'] = [dict(row) for row in raw['observations']]
        changed['observations'][-1]['stdout'] = 'wrong checksum\n'
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
            d.drain_owned(process, outputs, deadline=10)
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
            d.drain_owned(process, outputs, deadline=10)
        self.assertEqual(sum(len(stream.getvalue()) for stream in outputs.values()), d.MAX_READ)
        self.assertIsNotNone(process.returncode)
        self.assertTrue(process.stdout.closed and process.stderr.closed)

    def test_deadline_reaps_only_owned_group_and_closes_pipes(self):
        process = self.child("import time; print('started',flush=True); time.sleep(60)")
        outputs = dict(stdout=io.BytesIO(), stderr=io.BytesIO())
        with self.assertRaisesRegex(ValueError, 'deadline exceeded'):
            d.drain_owned(process, outputs, deadline=0.2)
        self.assertIsNotNone(process.returncode)
        self.assertTrue(process.stdout.closed and process.stderr.closed)


class PhaseProtocolTests(unittest.TestCase):
    def event(self, phase, seq, nonce='a'*32, index=16):
        return f'SWPHASE1 {nonce} {seq} {phase} {index}\n'.encode()

    def test_exact_events_acknowledge_only_after_capture(self):
        protocol = d.PhaseProtocol('a'*32, 16)
        seen = []
        for seq, phase in enumerate(d.PHASES, 1):
            event = self.event(phase, seq)
            self.assertEqual(protocol.feed(event[:7], lambda *args: self.fail('partial event captured')), b'')
            ack = protocol.feed(event[7:], lambda *args: seen.append(args))
            self.assertEqual(seen[-1], (phase, seq, 16))
            self.assertEqual(ack, event.replace(b'SWPHASE1', b'SWACK1'))
        protocol.eof()
        with self.assertRaisesRegex(ValueError, 'duplicate|extra'):
            protocol.feed(self.event(d.PHASES[-1], 4), lambda *args: self.fail('extra capture'))

    def test_wrong_nonce_index_sequence_order_and_pipelining_never_capture(self):
        for event in (self.event(d.PHASES[0], 1, nonce='b'*32),
                      self.event(d.PHASES[0], 1, index=15),
                      self.event(d.PHASES[0], 2), self.event(d.PHASES[1], 1),
                      self.event(d.PHASES[0], 1) + self.event(d.PHASES[1], 2),
                      b'x'*257):
            with self.subTest(event=event), self.assertRaises(ValueError):
                d.PhaseProtocol('a'*32, 16).feed(event, lambda *args: self.fail('invalid capture'))
        protocol = d.PhaseProtocol('a'*32, 16)
        protocol.feed(self.event(d.PHASES[0], 1), lambda *args: None)
        with self.assertRaises(ValueError):
            protocol.feed(self.event(d.PHASES[0], 1), lambda *args: self.fail('duplicate capture'))

    def test_incomplete_eof_and_capture_failure_have_no_acknowledgment(self):
        for prefix in (b'', b'SWPHASE1'):
            protocol = d.PhaseProtocol('a'*32, 16)
            protocol.feed(prefix, lambda *args: None)
            with self.assertRaisesRegex(ValueError, 'incomplete'):
                protocol.eof()
        protocol = d.PhaseProtocol('a'*32, 16)
        def failed_capture(*args):
            raise ValueError('identity failed')
        with self.assertRaisesRegex(ValueError, 'identity failed'):
            protocol.feed(self.event(d.PHASES[0], 1), failed_capture)
        self.assertEqual(protocol.completed, [])
        partial = d.PhaseProtocol('a'*32, 16)
        partial.feed(self.event(d.PHASES[0], 1), lambda *args: None)
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            partial.eof()

    def test_proc_exe_requires_exact_path_and_inode(self):
        host = Path('/trusted/consumer')
        info = type('Info', (), {'st_ino': 12})()
        for path, inode in (('/usr/bin/python3', 12), (str(host), 13)):
            with patch.object(os, 'readlink', return_value=path), patch.object(Path, 'stat', return_value=type('Info', (), {'st_ino': inode})()):
                with self.assertRaisesRegex(ValueError, 'executable'):
                    d.verify_executable(123, host, info)


class PhaseSocketDrainTests(unittest.TestCase):
    """Trusted Python socket peers; no native images, Wasm or JS execution."""
    def launch(self, code):
        parent, child = socket.socketpair()
        process = subprocess.Popen([sys.executable, '-I', '-c', code, str(child.fileno())],
            pass_fds=(child.fileno(),), stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        child.close()
        self.addCleanup(parent.close)
        def cleanup():
            if process.poll() is None:
                d.kill_owned_group(process)
                process.wait(timeout=2)
            for pipe in (process.stdout, process.stderr):
                pipe.close()
        self.addCleanup(cleanup)
        return process, parent

    def test_four_barriers_drain_both_streams_before_and_between_acks(self):
        code = r"""
import os,socket,sys
s=socket.socket(fileno=int(sys.argv[1]))
for seq,phase in enumerate(('after_transport_drop','after_instantiate','after_execution','after_store_drop'),1):
 os.write(1,b'a'*70000);os.write(2,b'b'*70000)
 event=f'SWPHASE1 {"a"*32} {seq} {phase} 16\n'.encode()
 s.sendall(event)
 ack=bytearray()
 while not ack.endswith(b'\n'):
  data=s.recv(256)
  assert data
  ack.extend(data)
 assert ack==event.replace(b'SWPHASE1',b'SWACK1')
s.close()
"""
        process, control = self.launch(code)
        outputs = {name: io.BytesIO() for name in ('stdout', 'stderr', 'control')}
        captured = []
        protocol = d.PhaseProtocol('a'*32, 16)
        d.drain_owned(process, outputs, control, protocol, lambda *args: captured.append(args), deadline=10)
        self.assertEqual(process.returncode, 0)
        self.assertEqual([row[0] for row in captured], list(d.PHASES))
        self.assertEqual(outputs['stdout'].getvalue(), b'a'*(4*70000))
        self.assertEqual(outputs['stderr'].getvalue(), b'b'*(4*70000))
        self.assertTrue(process.stdout.closed and process.stderr.closed)

    def test_control_eof_and_missing_event_deadline_preserve_raw_and_reap(self):
        for code, error in (("import socket,sys; s=socket.socket(fileno=int(sys.argv[1]));print('raw',flush=True);s.close()", 'incomplete'),
                            ("import time;print('raw',flush=True);time.sleep(60)", 'deadline')):
            with self.subTest(error=error):
                process, control = self.launch(code)
                outputs = {name: io.BytesIO() for name in ('stdout', 'stderr', 'control')}
                with self.assertRaisesRegex(ValueError, error):
                    d.drain_owned(process, outputs, control, d.PhaseProtocol('a'*32, 16), lambda *args: self.fail('no event'), deadline=0.3)
                self.assertIn(b'raw', outputs['stdout'].getvalue())
                self.assertIsNotNone(process.returncode)
                self.assertTrue(process.stdout.closed and process.stderr.closed)
