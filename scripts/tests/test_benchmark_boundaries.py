"""Acceptance of exact raw timing boundaries and distinct runtime cohorts."""
import copy
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from benchmark_boundaries import run, validate_cache_identity, validate_calls, validate_child


def calls():
    markers = ['scriptw:call:ready']
    for index in range(7):
        markers.extend((f'scriptw:call:start:{index}', f'scriptw:call:end:{index}'))
    callbacks = [dict(marker=marker, entry_ns=position * 100, exit_ns=position * 100 + 10)
                 for position, marker in enumerate(markers)]
    return dict(callbacks=callbacks, samples_ns=[90] * 7)


class BoundaryEvidenceTests(unittest.TestCase):
    def test_boundary_child_environment_pins_the_declared_cache_algorithm(self):
        with tempfile.TemporaryDirectory() as temporary:
            missing = Path(temporary) / 'missing-runtime'
            for requested in [None, 'sha256', 'blake3']:
                environment = {}
                if requested is not None:
                    environment['SCRIPTW_COMPILER_CACHE_IMAGE_HASH'] = requested
                with (self.subTest(requested=requested),
                      patch('benchmark_boundaries.require_kernel_budget'),
                      patch('benchmark_boundaries.os.sched_getaffinity', return_value={0}),
                      patch('benchmark_boundaries.limited_environment', return_value=environment)):
                    with self.assertRaisesRegex(ValueError, 'built runner and Node executable'):
                        run(SimpleNamespace(runner=missing, node=missing))
                    expected = 'sha256'
                    if requested is not None:
                        expected = requested
                    self.assertEqual(environment['SCRIPTW_COMPILER_CACHE_IMAGE_HASH'], expected)

    def test_source_scoreboard_requires_compilation_and_v8_without_cached_data(self):
        node_prepared = dict(node='vX', v8='vY', exec_argv=[])
        node = dict(node_prepared, schema='scriptw-v8-boundaries-child-v1', mode='source',
            cached_data_supplied=False, cached_data_rejected=None, source_sha256='source', stdout='42\n')
        validate_child(node, runtime='node', mode='source', expected='42\n', source_sha='source',
                       prepared=node_prepared, samples=7)
        for field, value in [('cached_data_supplied', True), ('cached_data_rejected', False), ('mode', 'fresh')]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_child(dict(node, **{field: value}), runtime='node', mode='source', expected='42\n',
                    source_sha='source', prepared=node_prepared, samples=7)
        prepared = dict(source_fingerprint='compiler', source_commit='commit', executable_sha256='exe',
            artifact=dict(program_sha256='P', linked_runtime=dict(key='R')), source_bytes=12,
            requested_compiler_cache_image_hash='sha256',
            compiler_cache_identity=dict(image_hash_algorithm='sha256', image_digest='exe', fingerprint_domain='v4'))
        scriptw = dict(prepared, schema='scriptw-portable-boundaries-child-v1', mode='source',
            source_sha256='source', source_read_and_compilation_ns=10, source_to_result_ns=20,
            compilation_ns=dict(preparation=1, lowering=2, emission=3, validation=1),
            execution=dict(backend='wasm-aot', completion='normal', stdout='42\n'))
        validate_child(scriptw, runtime='scriptw', mode='source', expected='42\n', source_sha='source',
                       prepared=prepared, samples=7)
        for spans in ({}, None, dict(preparation=True, lowering=2, emission=3, validation=1)):
            with self.subTest(spans=spans), self.assertRaises(ValueError):
                validate_child(dict(scriptw, compilation_ns=spans), runtime='scriptw', mode='source',
                    expected='42\n', source_sha='source', prepared=prepared, samples=7)

    def test_compiler_image_hash_modes_have_distinct_domains_and_reject_mismatched_cohorts(self):
        sha = 'a' * 64
        default = dict(image_hash_algorithm='sha256', image_digest=sha,
            fingerprint_domain='scriptw-program-cache-compiler-v4')
        experimental = dict(image_hash_algorithm='blake3', image_digest='b' * 64,
            fingerprint_domain='scriptw-program-cache-compiler-v5-blake3')
        validate_cache_identity(default, 'sha256', sha)
        validate_cache_identity(experimental, 'blake3', sha)
        for identity, requested in [(default, 'blake3'), (experimental, 'sha256'),
            (dict(experimental, fingerprint_domain=default['fingerprint_domain']), 'blake3'),
            (dict(default, image_digest='b' * 64), 'sha256'), (None, 'sha256')]:
            with self.subTest(identity=identity, requested=requested), self.assertRaises(ValueError):
                validate_cache_identity(identity, requested, sha)

    def test_call_samples_are_bound_to_ordered_raw_callbacks(self):
        validate_calls(calls(), 7)
        for mutation in ('missing', 'reordered', 'duration', 'clock', 'boolean'):
            data = calls()
            if mutation == 'missing': data['callbacks'].pop()
            if mutation == 'reordered': data['callbacks'][1]['marker'] = 'scriptw:call:end:0'
            if mutation == 'duration': data['samples_ns'][1] += 1
            if mutation == 'clock': data['callbacks'][2]['entry_ns'] = 0
            if mutation == 'boolean': data['samples_ns'][0] = True
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                validate_calls(data, 7)

    def test_v8_bytecode_rejection_wrong_source_and_guest_warmup_count_are_errors(self):
        prepared = dict(node='vX', v8='vY', exec_argv=[], cached_data_sha256='cache', cached_data_bytes=16)
        row = dict(prepared, schema='scriptw-v8-boundaries-child-v1', mode='calls',
            cached_data_rejected=False, source_sha256='source', stdout='checked\n',
            guest_warmups=10, calls=calls())
        def validate(value):
            validate_child(value, runtime='node', mode='calls', expected='checked\n',
                           source_sha='source', prepared=prepared, samples=7)
        validate(row)
        for field, value in [('cached_data_rejected', True), ('cached_data_rejected', None),
                             ('source_sha256', 'other'), ('stdout', 'wrong\n'),
                             ('guest_warmups', 9), ('mode', 'fresh'), ('exec_argv', ['--jitless'])]:
            altered = copy.deepcopy(row)
            altered[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                validate(altered)

    def test_scriptw_loaded_artifact_runtime_and_compiler_proof_must_match_preparation(self):
        prepared = dict(source_fingerprint='compiler', source_commit='commit', executable_sha256='exe',
            artifact=dict(program_sha256='P', linked_runtime=dict(key='R')), bundle_sha256='bundle', bundle_bytes=128,
            requested_compiler_cache_image_hash='sha256',
            compiler_cache_identity=dict(image_hash_algorithm='sha256', image_digest='exe', fingerprint_domain='v4'))
        row = dict(prepared, schema='scriptw-portable-boundaries-child-v1', mode='fresh',
            bundle_read_and_admission_ns=10, bundle_load_and_execute_ns=20,
            execution=dict(backend='wasm-aot', completion='normal', stdout='42\n'))
        def validate(value):
            validate_child(value, runtime='scriptw', mode='fresh', expected='42\n',
                           source_sha='source', prepared=prepared, samples=7)
        validate(row)
        for mutate in ('runtime', 'compiler', 'bundle', 'backend', 'throw', 'cache_algorithm'):
            altered = copy.deepcopy(row)
            if mutate == 'runtime': altered['artifact']['linked_runtime']['key'] = 'other'
            if mutate == 'compiler': altered['source_fingerprint'] = 'other'
            if mutate == 'bundle': altered['bundle_sha256'] = 'other'
            if mutate == 'backend': altered['execution']['backend'] = 'spec-exec'
            if mutate == 'throw': altered['execution']['completion'] = 'throw'
            if mutate == 'cache_algorithm': altered['compiler_cache_identity']['image_hash_algorithm'] = 'blake3'
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                validate(altered)
