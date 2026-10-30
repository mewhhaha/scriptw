"""Admission controls with inert synthetic data, never benchmark evidence.

No native serialization below is executed or passed to Wasmtime. These tests
exercise provenance, full inventories, measurement boundaries and archive reads.
"""
from datetime import datetime, timedelta, timezone
import hashlib
import io
import json
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import precompiled_comparison as comparison


def binary_registry(registry):
    """Independent fixture encoder; never native evidence or product authority."""
    word = lambda value: value.to_bytes(4, 'little')
    def text(value):
        raw = value.encode('utf-8')
        return word(len(raw)) + raw
    raw = word(registry['abi_version']) + bytes([registry['error_tag'] is not None])
    raw += word(len(registry['modules']))
    for module in registry['modules']:
        raw += text(module['specifier']) + word(len(module['functions']))
        for function in module['functions']:
            raw += b''.join(text(function[key]) for key in ('export', 'module', 'name'))
            raw += bytes([('sync', 'async').index(function['mode'])])
            raw += word(len(function['parameters']))
            raw += bytes(('number', 'utf16', 'bytes').index(value) for value in function['parameters'])
            raw += bytes([('number', 'utf16', 'bytes', 'void').index(function['result'])])
    return raw


class PrecompiledComparisonTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.path = self.directory / 'report.json'
        self.files = set()
        self.clock = datetime(2026, 10, 26, tzinfo=timezone.utc)
        self.config = dict(comparison.SHARED_CONFIGURATION, target_triple='x86_64-unknown-linux-gnu')
        self.consumer = dict(path='/inert/consumer', sha256='5' * 64)
        corpus = ROOT / comparison.FIXTURES
        plan = json.loads((corpus / 'manifest.json').read_bytes())
        self.report = dict(schema=comparison.SCHEMA, complete=True, source_commit='1' * 40,
            source_fingerprint='2' * 64, source_fingerprint_scheme='scriptw-program-cache-compiler-v3',
            started_at=self.clock.isoformat(), samples=7, warmups=10, rounds=2,
            machine=dict(measurement_affinity=[0]), shared_configuration=self.config,
            policy=dict(consumer_compiler_enabled=False, producers_outside_consumers=True),
            workloads=plan['execution'], tools=dict(consumer=self.consumer), attempts=[], commands=[])
        resource = {'memory.max':str(4 * 1024**3), 'memory.swap.max':'0', 'memory.oom.group':'1',
                    'memory.peak':'1024', 'memory.events':'low 0\nhigh 0\nmax 0\noom 0\noom_kill 0\noom_group_kill 0\n'}
        self.report['resources'] = dict(before=dict(resource), after=dict(resource), cpu_affinity=[0])
        self.write('sources/manifest.json', (corpus / 'manifest.json').read_bytes())
        for case in self.report['workloads']:
            original = (corpus / case['file']).read_bytes()
            self.write('sources/' + case['file'], original)
            self.write('adapted/' + case['file'], comparison.ADAPTER.encode() + original)
            self.write('expected/' + case['file'], case['stdout'].encode())
        for ordinal, (tool, version) in enumerate([('scriptc', 'scriptc 0.4.5'), ('zig', '0.16.0'), ('rustc', 'rustc fixture')]):
            value = dict(path='/inert/' + tool, version=version, sha256=str(ordinal + 6) * 64)
            event = self.command('version-' + tool, 'tool-version', 'shared', None,
                                 [value['path'], '--version'], sha=value['sha256'])
            value['version_command'] = event['id']
            self.report['tools'][tool] = value
            self.write(event['stdout'], (version + '\n').encode())
        producer = dict(reason='compiler-artifact', target=dict(kind=['example'], name='compiler_precompiled_baseline'),
                        features=[], profile=dict(opt_level='3', test=False))
        self.write('build/cargo.jsonl', ('\n'.join(json.dumps(row) for row in [producer,
            dict(reason='build-finished', success=True)]) + '\n').encode())
        self.consumer.update(cargo_log='build/cargo.jsonl', cargo_producer=producer)
        identity = self.command('shared-identity', 'tool-version', 'shared', None,
                                [self.consumer['path'], 'identity'])
        self.consumer['identity_command'] = identity['id']
        self.write(identity['stdout'], self.raw_json(dict(self.identity(),
            schema='scriptw-precompiled-shared-host-v1', adapter='shared', shared_configuration=self.config)))
        batch = self.command('scriptw-source', 'source-produce', 'scriptw', None,
                             [self.consumer['path'], 'prepare-scriptw', '/inert/sources', '/inert/images/scriptw'])
        self.artifacts, batch_cases = {}, []
        for case in self.report['workloads']:
            for runtime in comparison.RUNTIMES:
                # A valid empty Wasm module is sufficient for inert byte/hash
                # admission; no fake native payload is ever deserialized.
                wasm = b'\0asm\x01\0\0\0'
                native = b'INERT UNIT TEST BYTES; NEVER DESERIALIZE'
                fingerprint = bytes.fromhex(self.report['source_fingerprint'])
                if runtime == 'scriptw':
                    body = b'SCRWEP03' + fingerprint + b'\x01' + b'\x09' * 32 + b'\x01'
                else:
                    body = b'SCRWWA01' + fingerprint
                body += len(wasm).to_bytes(8, 'little') + len(native).to_bytes(8, 'little')
                if runtime == 'scriptw':
                    registry = binary_registry(dict(abi_version=2, error_tag=None, modules=[]))
                    body += len(registry).to_bytes(4, 'little') + registry
                body += wasm + native
                image = comparison.image_metadata(body + hashlib.sha256(body).digest(), runtime,
                                                  self.report['source_fingerprint'])
                image['file'] = 'images/' + runtime + '/' + case['file'] + '.cwasm'
                wasm_path = 'wasm/' + runtime + '/' + case['file'] + '.wasm'
                self.write(wasm_path, wasm)
                produced = dict(file=case['file'], stdout=case['stdout'],
                    source_sha256=hashlib.sha256((corpus / case['file']).read_bytes()).hexdigest(),
                    image=Path(image['file']).name, image_sha256=image['sha256'], image_bytes=image['bytes'],
                    program_wasm_sha256=image['program_wasm_sha256'], program_wasm_bytes=image['program_wasm_bytes'],
                    native_serialized_bytes=image['native_serialized_bytes'], runtime_key=image['runtime_key'],
                    source_compilation_profile='portable', intl_profile='Minimal', wasm=case['file'] + '.wasm',
                    phases_ns=dict(source_compilation=1, native_preparation=2))
                if runtime == 'scriptw':
                    produced.update({key: image[key] for key in
                                     ('execution_facts_bytes', 'execution_facts_sha256')})
                    batch_cases.append(produced)
                    producer_ids = [batch['id']]
                else:
                    produced.update(file=Path(wasm_path).name, wasm=Path(wasm_path).name,
                        phases_ns=dict(engine=1, native_compilation=2, native_serialization=3, total=6))
                    source = self.command('scriptc-source-' + case['file'], 'source-produce', 'scriptc', case['file'],
                        [self.report['tools']['scriptc']['path'], 'build', '/inert/adapted/' + case['file'],
                         '-o', '/inert/' + wasm_path], sha=self.report['tools']['scriptc']['sha256'])
                    native_event = self.command('scriptc-native-' + case['file'], 'wasm-precompile', 'scriptc', case['file'],
                        [self.consumer['path'], 'prepare-wasi', '/inert/' + wasm_path, '/inert/' + image['file']])
                    self.write(native_event['stdout'], self.raw_json(dict(self.identity(), **produced,
                        schema='scriptw-precompiled-wasi-producer-v1', shared_configuration=self.config,
                        compiler_enabled=True, adapter='wasi')))
                    producer_ids = [source['id'], native_event['id']]
                self.artifacts[case['file'], runtime] = dict(wasm=wasm_path, image=image, producer_ids=producer_ids)
        self.write(batch['stdout'], self.raw_json(dict(self.identity(), schema='scriptw-precompiled-producer-v1',
            shared_configuration=self.config, manifest_sha256=hashlib.sha256((corpus / 'manifest.json').read_bytes()).hexdigest(),
            cases=batch_cases, compiler_enabled=True, adapter='scriptw')))
        for item in comparison.measurement_plan(self.report['workloads']):
            case = next(case for case in self.report['workloads'] if case['file'] == item['file'])
            artifact = self.artifacts[item['file'], item['runtime']]
            event = self.command(f"consume-{item['round']}-{item['runtime']}-{item['file']}", 'consume',
                item['runtime'], item['file'], [self.consumer['path'], 'execute-scriptw' if item['runtime'] == 'scriptw' else 'execute-wasi',
                '/inert/' + artifact['image']['file'], case['stdout'], '7', '10'], round_index=item['round'])
            raw = self.consumer_raw(case, item['runtime'], artifact['image'], item['round'])
            self.write(event['stdout'], self.raw_json(raw))
            self.report['attempts'].append(dict(**item, **artifact, status='ok', consumer_id=event['id']))
        self.report['finished_at'] = (self.clock + timedelta(seconds=1)).isoformat()

    def identity(self):
        return dict(source_commit=self.report['source_commit'], source_fingerprint=self.report['source_fingerprint'],
            compiler_identity_scheme=self.report['source_fingerprint_scheme'], executable_sha256=self.consumer['sha256'],
            comparison_cohort=comparison.SCHEMA, excluded_from_ordinary_cohorts=True)

    @staticmethod
    def raw_json(value):
        return json.dumps(value).encode()

    def write(self, name, value):
        path = self.directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value)
        self.files.add(name)

    def command(self, identifier, kind, runtime, name, argv, *, sha=None, round_index=None):
        start = self.clock
        self.clock += timedelta(seconds=1)
        event = dict(id=identifier, kind=kind, runtime=runtime, file=name, round=round_index,
            argv=argv, cwd='/inert', executable_sha256=sha or self.consumer['sha256'],
            returncode=0, timed_out=False, started_at=start.isoformat(), finished_at=self.clock.isoformat(),
            stdout='raw/' + identifier + '.stdout', stderr='raw/' + identifier + '.stderr')
        self.report['commands'].append(event)
        self.write(event['stdout'], b'')
        self.write(event['stderr'], b'')
        return event

    @staticmethod
    def memory(mib):
        return dict(rss_bytes=mib * 1024**2, pss_bytes=mib * 1024**2 * 3 // 4,
                    private_dirty_bytes=mib * 1024**2 // 2, scope=comparison.MEMORY_SCOPE)

    def consumer_raw(self, case, runtime, image, round_index):
        observations = []
        for index in range(17):
            warmup = index < 10
            execution = 10**12 if warmup else index * 1000 + round_index * 100
            memory = self.memory(100 if warmup else index + 1 + round_index * 2)
            observations.append(dict(index=index, warmup=warmup, stdout=case['stdout'], stderr='',
                completion='normal', matched=True,
                phases_ns=dict(state_linker=1, register=2, instantiate=3, execution=execution, total=execution + 6),
                gc_capacity_before_bytes=16 * 1024**2, gc_capacity_after_bytes=16 * 1024**2,
                exported_linear_memory_before_bytes=65536, exported_linear_memory_after_bytes=65536,
                process_snapshots={key: dict(memory) for key in comparison.PROCESS_PHASES}))
        return dict(self.identity(),
            schema='scriptw-precompiled-execution-v1' if runtime == 'scriptw' else 'scriptw-precompiled-wasi-execution-v1',
            compiler_enabled=False, shared_configuration=self.config, adapter='scriptw' if runtime == 'scriptw' else 'wasi',
            samples=7, warmups=10, image_sha256=image['sha256'], image_bytes=image['bytes'],
            program_wasm_sha256=image['program_wasm_sha256'], wasm_sha256=image['program_wasm_sha256'],
            **({key: image[key] for key in ('execution_facts_bytes', 'execution_facts_sha256')}
               if image.get('magic') in ('SCRWEP02', 'SCRWEP03') else {}),
            runtime_key=image['runtime_key'], load_phases_ns=dict(engine=100, image_read_and_envelope_admission=200,
                native_admission_and_deserialization=300, process_total=1000),
            native_serialized_bytes=image['native_serialized_bytes'],
            native_load_process_memory=dict(status='sampled', interval_ns=10_000_000,
                initial_rss_bytes=4 * 1024**2, final_rss_bytes=8 * 1024**2, maximum_rss_bytes=12 * 1024**2,
                sample_count=2, elapsed_ns=400, scope=comparison.LOAD_SCOPES[runtime]),
            process_snapshots={key: self.memory(10) for key in comparison.SNAPSHOTS}, observations=observations)

    def save(self):
        self.report['retained_files'] = [dict(path=name, bytes=(self.directory / name).stat().st_size,
            sha256=comparison.digest(self.directory / name)) for name in sorted(self.files)]
        self.path.write_text(json.dumps(self.report))
        return self.path

    def raw_attempt(self, index=0):
        attempt = self.report['attempts'][index]
        event = next(row for row in self.report['commands'] if row['id'] == attempt['consumer_id'])
        return attempt, event, json.loads((self.directory / event['stdout']).read_bytes())

    def mutate_raw(self, function, index=0):
        attempt, event, raw = self.raw_attempt(index)
        function(raw)
        self.write(event['stdout'], self.raw_json(raw))
        return attempt

    def test_exact_reversed_alternating_plan_and_four_complete_sample_groups(self):
        _, rows = comparison.load_report(self.save())
        self.assertEqual(len(rows), 17)
        for runtime in comparison.RUNTIMES:
            item = rows[0]['runtimes'][runtime]
            self.assertEqual(item['measured_samples'], 14)
            self.assertAlmostEqual(item['ms'], .01305)
            self.assertEqual(item['rss_mib'], 15)
            self.assertEqual(item['pss_mib'], 11.25)
            self.assertEqual(item['private_dirty_mib'], 7.5)
            self.assertEqual(len(item['after_transport_drop']), 2)
        plan = comparison.measurement_plan(self.report['workloads'])
        self.assertEqual(plan[0]['runtime'], 'scriptw')
        self.assertEqual(plan[34]['file'], self.report['workloads'][-1]['file'])
        self.assertEqual(plan[34]['runtime'], 'scriptc')

    def test_module_image_diagnostic_feature_host_admits_complete_scored_report(self):
        producer = self.consumer['cargo_producer']
        producer['features'] = ['benchmark-module-images']
        self.write(self.consumer['cargo_log'], ('\n'.join(json.dumps(row) for row in [producer,
            dict(reason='build-finished', success=True)]) + '\n').encode())
        admitted, rows = comparison.load_report(self.save())
        self.assertEqual(admitted['tools']['consumer']['cargo_producer']['features'],
                         ['benchmark-module-images'])
        self.assertEqual(len(rows), 17)
        self.assertTrue(all(row['runtimes'][runtime]['status'] == 'ok'
                            for row in rows for runtime in comparison.RUNTIMES))

    def test_cargo_host_feature_admission_is_closed_and_bound_to_actual_transcript(self):
        producer = self.consumer['cargo_producer']
        for features in (['unknown'], ['differential-oracle'],
                         ['benchmark-module-images', 'unknown'],
                         ['unknown', 'benchmark-module-images'],
                         ['benchmark-module-images', 'benchmark-module-images'],
                         None, 'benchmark-module-images', {}, False):
            with self.subTest(features=features):
                producer['features'] = features
                self.write(self.consumer['cargo_log'], ('\n'.join(json.dumps(row) for row in [producer,
                    dict(reason='build-finished', success=True)]) + '\n').encode())
                with self.assertRaisesRegex(ValueError, 'release Cargo producer'):
                    comparison.load_report(self.save())
        # A permitted feature claim cannot replace the actual producer record.
        producer['features'] = ['benchmark-module-images']
        other_producer = dict(producer, features=[])
        self.write(self.consumer['cargo_log'], ('\n'.join(json.dumps(row) for row in [other_producer,
            dict(reason='build-finished', success=True)]) + '\n').encode())
        with self.assertRaisesRegex(ValueError, 'release Cargo producer'):
            comparison.load_report(self.save())

    def test_missing_or_reordered_attempt_is_rejected(self):
        self.report['attempts'][0], self.report['attempts'][1] = self.report['attempts'][1], self.report['attempts'][0]
        with self.assertRaisesRegex(ValueError, 'serial inventory'):
            comparison.load_report(self.save())

    def test_warmup_checksum_failure_is_not_dropped_or_barred_as_success(self):
        self.mutate_raw(lambda raw: raw['observations'][0].update(stdout='wrong\n', matched=False))
        with self.assertRaisesRegex(ValueError, 'status hides'):
            comparison.load_report(self.save())
        self.report['attempts'][0]['status'] = 'checksum-mismatch'
        _, rows = comparison.load_report(self.save())
        self.assertEqual(rows[0]['runtimes']['scriptw']['status'], 'checksum-mismatch')
        self.assertIsNone(rows[0]['runtimes']['scriptw']['ms'])
        self.assertIsNone(rows[0]['runtimes']['scriptw']['rss_mib'])
        self.assertEqual(rows[0]['runtimes']['scriptc']['status'], 'ok')

    def test_wrong_matched_flag_cannot_hide_output(self):
        self.mutate_raw(lambda raw: raw['observations'][10].update(stdout='wrong\n'))
        with self.assertRaisesRegex(ValueError, 'checksum status'):
            comparison.load_report(self.save())

    def test_missing_measured_or_warmup_observation_fails(self):
        self.mutate_raw(lambda raw: raw['observations'].pop())
        with self.assertRaisesRegex(ValueError, 'every warmup'):
            comparison.load_report(self.save())

    def test_enabled_compiler_is_rejected(self):
        self.mutate_raw(lambda raw: raw.update(compiler_enabled=True))
        with self.assertRaisesRegex(ValueError, 'compiler/configuration'):
            comparison.load_report(self.save())

    def test_phase_barrier_reports_cannot_be_relabelled_as_scores(self):
        for runtime in comparison.RUNTIMES:
            index = next(i for i, row in enumerate(self.report['attempts'])
                         if row['runtime'] == runtime and row['status'] == 'ok')
            _, event, original = self.raw_attempt(index)
            for marker in (None, False, {'schema': 'scriptw-precompiled-phase-barriers-v1'}):
                with self.subTest(runtime=runtime, marker=marker):
                    raw = dict(original, phase_barriers=marker)
                    # Even deleting the explicit unscored flag cannot make the
                    # paused timing and memory history a canonical observation.
                    raw.pop('excluded_from_scored_cohorts', None)
                    self.write(event['stdout'], self.raw_json(raw))
                    with self.assertRaisesRegex(ValueError, 'phase-barrier diagnostics'):
                        comparison.load_report(self.save())
            self.write(event['stdout'], self.raw_json(original))

            for marker in (None, False, {'schema': 'scriptw-precompiled-execution-windows-v1'}):
                with self.subTest(runtime=runtime, execution_windows=marker):
                    raw = dict(original, execution_windows=marker)
                    raw.pop('excluded_from_scored_cohorts', None)
                    self.write(event['stdout'], self.raw_json(raw))
                    with self.assertRaisesRegex(ValueError, 'phase-barrier diagnostics'):
                        comparison.load_report(self.save())
            self.write(event['stdout'], self.raw_json(original))

            for flag in (None, False, True):
                with self.subTest(runtime=runtime, timing_label=flag):
                    raw = dict(original, diagnostic_total_timings_include_barrier_waits=flag)
                    self.write(event['stdout'], self.raw_json(raw))
                    with self.assertRaisesRegex(ValueError, 'phase-barrier diagnostics'):
                        comparison.load_report(self.save())
            self.write(event['stdout'], self.raw_json(original))

    def test_different_consumer_binary_is_rejected(self):
        self.mutate_raw(lambda raw: raw.update(executable_sha256='a' * 64))
        with self.assertRaisesRegex(ValueError, 'shared executable'):
            comparison.load_report(self.save())

    def test_different_config_or_native_tier_is_rejected(self):
        self.mutate_raw(lambda raw: raw.update(shared_configuration=dict(raw['shared_configuration'], register_allocation='single-pass')))
        with self.assertRaisesRegex(ValueError, 'compiler/configuration'):
            comparison.load_report(self.save())

    def test_native_image_and_raw_wasm_correspondence_is_checked(self):
        self.write(self.report['attempts'][0]['wasm'], b'\0asm\x01\0\0\0changed')
        with self.assertRaisesRegex(ValueError, 'raw Wasm correspondence'):
            comparison.load_report(self.save())

    def test_invalid_phase_or_sampler_max_is_rejected(self):
        self.mutate_raw(lambda raw: raw['native_load_process_memory'].update(maximum_rss_bytes=1))
        with self.assertRaisesRegex(ValueError, 'sampled RSS'):
            comparison.load_report(self.save())

    def test_execution_total_cannot_omit_a_phase(self):
        self.mutate_raw(lambda raw: raw['observations'][10]['phases_ns'].update(total=1))
        with self.assertRaisesRegex(ValueError, 'execution phases'):
            comparison.load_report(self.save())

    def test_transport_drop_endpoint_is_required(self):
        self.mutate_raw(lambda raw: raw['process_snapshots'].pop('after_transport_drop'))
        with self.assertRaisesRegex(ValueError, 'transport/load'):
            comparison.load_report(self.save())

    def test_relabelled_failure_requires_an_actual_failed_command(self):
        self.report['attempts'][0]['status'] = 'run-failed'
        with self.assertRaisesRegex(ValueError, 'no failed command'):
            comparison.load_report(self.save())

    def test_actual_failed_consumer_retains_case_without_a_bar(self):
        attempt, command, _ = self.raw_attempt()
        attempt['status'] = 'run-failed'
        command['returncode'] = 1
        self.write(command['stderr'], b'actual fixture failure\n')
        _, rows = comparison.load_report(self.save())
        self.assertEqual(rows[0]['runtimes']['scriptw']['status'], 'run-failed')
        self.assertIsNone(rows[0]['runtimes']['scriptw']['ms'])
        self.assertEqual(len(rows), 17)

    def test_runtime_key_relabelling_cannot_pair_with_another_runtime(self):
        self.report['attempts'][0]['image']['runtime_key'] = 'f' * 64
        with self.assertRaisesRegex(ValueError, 'producer/image'):
            comparison.load_report(self.save())

    def test_sw03_facts_cannot_be_relabelled_independently_of_the_producer(self):
        self.report['attempts'][0]['image']['execution_facts_sha256'] = 'f' * 64
        with self.assertRaisesRegex(ValueError, 'producer execution facts'):
            comparison.load_report(self.save())

    def test_sw03_consumer_must_echo_exact_execution_facts(self):
        self.mutate_raw(lambda raw: raw.update(execution_facts_sha256='f' * 64))
        with self.assertRaisesRegex(ValueError, 'consumer execution facts'):
            comparison.load_report(self.save())

    def test_shared_host_identity_cannot_claim_another_policy(self):
        command = next(row for row in self.report['commands'] if row['id'] == 'shared-identity')
        raw = json.loads((self.directory / command['stdout']).read_bytes())
        raw['shared_configuration']['collector'] = 'different'
        self.write(command['stdout'], self.raw_json(raw))
        with self.assertRaisesRegex(ValueError, 'identity/configuration'):
            comparison.load_report(self.save())

    def test_producers_cannot_run_between_consumers(self):
        commands = self.report['commands']
        position = next(index for index, row in enumerate(commands) if row['kind'] == 'consume')
        producer = commands.pop(position - 1)
        commands.insert(position + 1, producer)
        with self.assertRaisesRegex(ValueError, 'serial order'):
            comparison.load_report(self.save())

    def test_adapter_cannot_change_even_with_new_retained_hash(self):
        self.write('adapted/' + self.report['workloads'][0]['file'], b'print(0);\n')
        with self.assertRaisesRegex(ValueError, 'adaptation differs'):
            comparison.load_report(self.save())

    def test_native_binaries_cannot_enter_archive(self):
        self.write('hidden/native', b'\x7fELFunit-test')
        with self.assertRaisesRegex(ValueError, 'native binary/image'):
            comparison.load_report(self.save())

    def test_kernel_oom_event_cannot_be_published_as_resource_clean(self):
        self.report['resources']['after']['memory.events'] = 'low 0\nhigh 0\nmax 0\noom 1\noom_kill 1\noom_group_kill 1\n'
        with self.assertRaisesRegex(ValueError, 'kernel OOM event'):
            comparison.load_report(self.save())

    def test_wrong_source_compile_output_path_cannot_be_relabelled(self):
        command = next(row for row in self.report['commands'] if row['id'].startswith('scriptc-source-'))
        command['argv'][-1] = '/inert/different.wasm'
        with self.assertRaisesRegex(ValueError, 'command paths differ'):
            comparison.load_report(self.save())

    def test_archive_roundtrip_is_independent_and_never_edits_existing_graphs_or_readme(self):
        destination = self.directory / 'checkout'
        assets = destination / 'docs/rust-rewrite/assets'
        assets.mkdir(parents=True)
        (destination / 'README.md').write_text('unchanged ordinary comparison')
        (assets / 'unrelated.svg').write_bytes(b'ordinary graph')
        self.save()
        with patch('precompiled_comparison.render', side_effect=lambda report, row: row['file'].encode()) as renderer:
            comparison.publish(self.path, destination)
        target = destination / 'docs/rust-rewrite/precompiled-comparison.json'
        _, rows = comparison.load_report(target, target.with_suffix('.tar.gz'))
        self.assertEqual(len(rows), 17)
        self.assertEqual((destination / 'README.md').read_text(), 'unchanged ordinary comparison')
        self.assertEqual((assets / 'unrelated.svg').read_bytes(), b'ordinary graph')
        self.assertEqual(renderer.call_count, 17)
        cards = comparison.chart_files(rows)
        self.assertEqual({path.relative_to(assets) for path in (assets / 'precompiled').glob('*.svg')},
                         {path for path, _ in cards})
        for path, row in cards:
            self.assertEqual((assets / path).read_bytes(), row['file'].encode())
        with self.assertRaisesRegex(ValueError, 'already exists'):
            comparison.publish(self.path, destination)
        # The published raw evidence remains independently admissible after
        # every original stream is removed from the measurement directory.
        shutil.rmtree(self.directory / 'raw')
        comparison.load_report(target, target.with_suffix('.tar.gz'))

    def test_inert_frame_corruption_lengths_and_policy_are_checked_without_deserialization(self):
        runtime = 'scriptw'
        wasm, native = b'\0asm\x01\0\0\0', b'INERT'
        body = b'SCRWEP01' + b'\x02' * 32 + b'\x00' + bytes(32) + b'\x01'
        body += len(wasm).to_bytes(8, 'little') + len(native).to_bytes(8, 'little') + wasm + native
        valid = body + hashlib.sha256(body).digest()
        comparison.image_metadata(valid, runtime, '02' * 32)
        for value in [valid[:-1], valid + b'\0', b'broken', valid[:74] + b'\xff' * 8 + valid[82:]]:
            with self.assertRaises(ValueError):
                comparison.image_metadata(value, runtime, '02' * 32)
        wrong_mode = bytearray(body)
        wrong_mode[73] = 0
        with self.assertRaisesRegex(ValueError, 'common native policy'):
            comparison.image_metadata(bytes(wrong_mode) + hashlib.sha256(wrong_mode).digest(), runtime, '02' * 32)

    def test_sw02_execution_facts_are_bounded_canonical_and_keep_historical_sw01_readable(self):
        wasm, native = b'\0asm\x01\0\0\0', b'INERT; NEVER DESERIALIZE'
        prefix = b'SCRWEP02' + b'\x02' * 32 + b'\x00' + bytes(32) + b'\x01'
        prefix += len(wasm).to_bytes(8, 'little') + len(native).to_bytes(8, 'little')
        def frame(registry):
            body = prefix + len(registry).to_bytes(4, 'little') + registry + wasm + native
            return body + hashlib.sha256(body).digest()
        canonical = b'{"abi_version":2,"error_tag":null,"modules":[]}'
        metadata = comparison.image_metadata(frame(canonical), 'scriptw', '02' * 32)
        self.assertEqual(metadata['magic'], 'SCRWEP02')
        self.assertEqual(metadata['program_wasm_sha256'], hashlib.sha256(wasm).hexdigest())
        self.assertEqual(metadata['native_sha256'], hashlib.sha256(native).hexdigest())
        for registry in [canonical + b' ', canonical.replace(b':2', b':3'),
                         b'{"abi_version":2,"abi_version":2,"error_tag":null,"modules":[]}',
                         canonical.replace(b'null', b'false')]:
            with self.assertRaises(ValueError):
                comparison.image_metadata(frame(registry), 'scriptw', '02' * 32)
        excessive = bytearray(frame(canonical)[:-32])
        excessive[90:94] = (2**32 - 1).to_bytes(4, 'little')
        with self.assertRaises(ValueError):
            comparison.image_metadata(bytes(excessive) + hashlib.sha256(excessive).digest(),
                                      'scriptw', '02' * 32)
        old_body = b'SCRWEP01' + prefix[8:] + wasm + native
        historical = comparison.image_metadata(old_body + hashlib.sha256(old_body).digest(),
                                               'scriptw', '02' * 32)
        self.assertEqual(historical['magic'], 'SCRWEP01')
        self.assertEqual(historical['native_sha256'], metadata['native_sha256'])

    def test_sw02_foreign_declarations_reject_conflicts_and_reserved_namespaces(self):
        def encoded(registry):
            return json.dumps(registry, sort_keys=True, separators=(',', ':')).encode()
        function = dict(export='call', module='fixture', name='call', mode='async',
                        parameters=['number', 'bytes'], result='utf16')
        registry = dict(abi_version=2, error_tag=dict(module='scriptw_foreign',
                        name='error_v1', payload=['i32']), modules=[dict(specifier='fixture:host',
                        functions=[function])])
        comparison._execution_registry(encoded(registry))
        import copy
        invalid = copy.deepcopy(registry)
        invalid['modules'][0]['functions'][0]['module'] = 'scriptw_host'
        with self.assertRaises(ValueError):
            comparison._execution_registry(encoded(invalid))
        invalid = copy.deepcopy(registry)
        invalid['modules'][0]['functions'].append(dict(function, export='other'))
        with self.assertRaises(ValueError):
            comparison._execution_registry(encoded(invalid))
        invalid = copy.deepcopy(registry)
        invalid['modules'][0]['functions'][0]['mode'] = 'unsupported'
        with self.assertRaises(ValueError):
            comparison._execution_registry(encoded(invalid))

    def test_sw03_binary_facts_preserve_every_type_mode_result_and_error_tag(self):
        registry = dict(abi_version=2, error_tag=dict(module='scriptw_foreign', name='error_v1', payload=['i32']),
            modules=[dict(specifier='fixture:é', functions=[dict(export=export, module='fixture', name=export,
                mode=mode, parameters=['number', 'utf16', 'bytes'], result=result)
                for export, mode, result in [('a', 'sync', 'number'), ('b', 'async', 'utf16'),
                    ('c', 'sync', 'bytes'), ('d', 'async', 'void')]])])
        self.assertEqual(comparison._binary_execution_registry(binary_registry(registry)), registry)

    def test_sw03_resigned_frames_reject_malformed_binary_facts(self):
        registry = dict(abi_version=2, error_tag=None, modules=[dict(specifier='m', functions=[
            dict(export='f', module='h', name='n', mode='sync', parameters=['number', 'utf16', 'bytes'], result='void')])])
        canonical = binary_registry(registry)
        wasm, native = b'\0asm\x01\0\0\0', b'INERT; NEVER DESERIALIZE'
        prefix = b'SCRWEP03' + b'\x02' * 32 + b'\x00' + bytes(32) + b'\x01'
        prefix += len(wasm).to_bytes(8, 'little') + len(native).to_bytes(8, 'little')
        def frame(facts):
            body = prefix + len(facts).to_bytes(4, 'little') + facts + wasm + native
            return body + hashlib.sha256(body).digest()
        admitted = comparison.image_metadata(frame(canonical), 'scriptw', '02' * 32)
        self.assertEqual(admitted['magic'], 'SCRWEP03')
        self.assertEqual(admitted['execution_facts_sha256'], hashlib.sha256(canonical).hexdigest())
        invalid = [canonical[:end] for end in range(len(canonical))]
        invalid += [canonical + b'\0', b'{"abi_version":2,"error_tag":null,"modules":[]}', bytes(1024**2 + 1)]
        for offset, value in [(0, b'\x03'), (4, b'\x02'), (5, b'\xff' * 4), (9, b'\xff' * 4),
                (13, b'\xff'), (13, b'\0'), (14, b'\xff' * 4), (33, b'\x02'),
                (34, b'\xff' * 4), (38, b'\x03'), (41, b'\x04')]:
            changed = bytearray(canonical)
            changed[offset:offset + len(value)] = value
            invalid.append(bytes(changed))
        for facts in invalid:
            with self.subTest(facts=facts[:48], length=len(facts)):
                with self.assertRaises(ValueError):
                    comparison.image_metadata(frame(facts), 'scriptw', '02' * 32)

    def test_sw03_binary_facts_reject_noncanonical_owners_and_import_conflicts(self):
        import copy
        function = dict(export='a', module='fixture', name='call', mode='async', parameters=[], result='void')
        original = dict(abi_version=2, error_tag=None,
            modules=[dict(specifier='a', functions=[function]),
                     dict(specifier='b', functions=[dict(function, name='other')])])
        comparison._binary_execution_registry(binary_registry(original))
        invalid = []
        for namespace in ('scriptw_host', 'scriptw_runtime', 'scriptw_foreign'):
            registry = copy.deepcopy(original)
            registry['modules'][0]['functions'][0]['module'] = namespace
            invalid.append(registry)
        registry = copy.deepcopy(original)
        registry['modules'].reverse()
        invalid.append(registry)
        registry = copy.deepcopy(original)
        registry['modules'][1]['specifier'] = 'a'
        invalid.append(registry)
        registry = copy.deepcopy(original)
        registry['modules'][1]['functions'][0]['name'] = 'call'
        invalid.append(registry)
        for exports in (['a', 'a'], ['b', 'a']):
            registry = copy.deepcopy(original)
            registry['modules'][0]['functions'] = [dict(function, export=name, name=str(index))
                for index, name in enumerate(exports)]
            invalid.append(registry)
        for registry in invalid:
            with self.subTest(registry=registry):
                with self.assertRaises(ValueError):
                    comparison._binary_execution_registry(binary_registry(registry))


if __name__ == '__main__':
    unittest.main()
