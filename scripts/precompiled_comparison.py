"""Admit and publish the separate two-runtime, compiler-free execution cohort.

This module never launches a compiler, native image, or JavaScript workload.
Image checksums bind captured producer/consumer evidence; they never authorize
native deserialization. Actual native-origin authority belongs to the Rust API.
"""
import argparse
from datetime import datetime
import gzip
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import statistics
import tarfile
import tempfile

SCHEMA = 'scriptw-precompiled-comparison-v1'
RUNTIMES = ('scriptw', 'scriptc')
SAMPLES, WARMUPS, ROUNDS = 7, 10, 2
FIXTURES = 'crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths'
ADAPTER = 'function print(value) { console.log(String(value)); }\n'
STEM = 'precompiled-comparison'
LABELS = {'scriptw': 'Scriptw / Wasmtime', 'scriptc': 'Scriptc / Wasmtime (WASI)'}
COLORS = {'scriptw': '#3378b5', 'scriptc': '#36835b'}
CHARTS = {
    'execution': dict(field='ms', title='Precompiled execution time', unit='ms', decimals=3,
                      filename='precompiled-execution.svg'),
    'memory': dict(field='rss_mib', title='Precompiled process memory after execution', unit='MiB', decimals=1,
                   filename='precompiled-memory.svg'),
}
MAX_REPORT, MAX_ARCHIVE = 16 * 1024**2, 32 * 1024**2
MAX_MEMBER, MAX_EXPANDED, MAX_MEMBERS = 64 * 1024**2, 256 * 1024**2, 32768
MEMORY_FIELDS = ('rss_bytes', 'pss_bytes', 'private_dirty_bytes')
SNAPSHOTS = ('startup', 'after_engine', 'after_image_read', 'after_admission', 'after_transport_drop')
PROCESS_PHASES = ('before_store', 'after_instantiate', 'after_execution', 'after_store_drop')
TIME_PHASES = ('state_linker', 'register', 'instantiate', 'execution', 'total')
MEMORY_SCOPE = 'linux-process-smaps-rollup-endpoint-not-peak-or-live-js'
LOAD_SCOPES = {
    'scriptw': 'native-P-and-Cargo-R-admission-process-RSS-sampled-maximum-not-peak-or-live-js',
    'scriptc': 'native-WASI-P-admission-process-RSS-sampled-maximum-not-peak-or-live-js',
}
SHARED_CONFIGURATION = dict(native_compilation_mode='size-optimized', register_allocation='backtracking',
    target='generic', wasm_backtrace_details='disable', wasmtime_version='47.0.0',
    runtime_policy='scriptw-required-wasmtime-experimental-v1', consumer_compiler_enabled=False,
    collector='copying', gc_initial_bytes=16 * 1024**2, worker_stack_bytes=64 * 1024**2,
    max_wasm_stack_bytes=32 * 1024**2, store_memory_limit_bytes=1024**3, epoch_deadline=2**63 - 1)
FAILURES = ('source-failed', 'precompile-failed', 'run-failed', 'checksum-mismatch')


def require(condition, message):
    if not condition:
        raise ValueError('precompiled comparison: ' + message)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def _sha(value):
    return hashlib.sha256(value).hexdigest()


def _json(value):
    def pairs(items):
        result = {}
        for key, item in items:
            require(key not in result, 'duplicate JSON key: ' + key)
            result[key] = item
        return result

    def invalid(value):
        raise ValueError('precompiled comparison: nonfinite JSON value: ' + value)
    return json.loads(value, object_pairs_hook=pairs, parse_constant=invalid)


def safe_name(value):
    require(isinstance(value, str) and bool(value), 'retained path must be text')
    path = PurePosixPath(value)
    require(not path.is_absolute() and '..' not in path.parts and '\\' not in value
            and ':' not in value and str(path) == value and value != '.', 'unsafe retained path')
    return value


def _uint(value):
    return type(value) is int and 0 <= value < 2**64


def _hex(value, length=64):
    return isinstance(value, str) and re.fullmatch('[0-9a-f]{' + str(length) + '}', value) is not None


def _instant(value):
    require(isinstance(value, str), 'timestamp must be text')
    instant = datetime.fromisoformat(value.replace('Z', '+00:00'))
    require(instant.utcoffset() is not None, 'timestamp has no timezone')
    return instant


def _path_argument(value, relative):
    return value == relative or value.endswith('/' + relative)


def _resources(report):
    resources = report['resources']
    require(resources['cpu_affinity'] == report['machine']['measurement_affinity'], 'resource CPU affinity differs')
    counters = []
    for name in ('before', 'after'):
        row = resources[name]
        require(all(isinstance(row.get(key), str) for key in ('memory.max', 'memory.swap.max',
                    'memory.oom.group', 'memory.peak', 'memory.events')), 'kernel resource observations must be text')
        limit = int(row['memory.max'])
        require(0 < limit <= 4 * 1024**3 and row['memory.swap.max'] == '0'
                and row['memory.oom.group'] == '1' and 0 <= int(row['memory.peak']),
                'finite 4-GiB/no-swap/grouped-OOM budget is missing')
        events = {}
        for line in row['memory.events'].splitlines():
            words = line.split()
            require(len(words) == 2 and words[0] not in events and words[1].isdigit(), 'invalid kernel memory events')
            events[words[0]] = int(words[1])
        require({'oom', 'oom_kill', 'oom_group_kill'} <= set(events), 'kernel OOM counters are missing')
        counters.append(events)
    require(set(counters[0]) == set(counters[1])
            and all(counters[1][key] >= value for key, value in counters[0].items())
            and all(counters[1][key] == counters[0][key] for key in ('oom', 'oom_kill', 'oom_group_kill')),
            'kernel OOM event occurred during the campaign')


def measurement_plan(workloads):
    """Exact serial consumer order: reverse cases, alternate the first runtime."""
    names = [case['file'] for case in workloads]
    return [dict(round=round_index, file=name, runtime=runtime)
            for round_index in range(ROUNDS)
            for case_index, name in enumerate(names if round_index == 0 else reversed(names))
            for runtime in (RUNTIMES if (case_index + round_index) % 2 == 0 else RUNTIMES[::-1])]


def _execution_registry(raw):
    """Check SW02's bounded canonical declarations, without native authority."""
    require(0 < len(raw) <= 1024**2, 'execution registry exceeds its bound')
    try:
        registry = json.loads(raw.decode('utf-8'))
        canonical = json.dumps(registry, ensure_ascii=False, sort_keys=True,
                               separators=(',', ':'), allow_nan=False).encode('utf-8')
    except (ValueError, UnicodeError) as error:
        raise ValueError('invalid execution registry') from error
    require(canonical == raw and isinstance(registry, dict)
            and set(registry) == {'abi_version', 'error_tag', 'modules'}
            and type(registry['abi_version']) is int and registry['abi_version'] == 2,
            'execution registry is unsupported or noncanonical')
    require(registry['error_tag'] is None or registry['error_tag'] ==
            {'module': 'scriptw_foreign', 'name': 'error_v1', 'payload': ['i32']},
            'execution error tag differs')
    modules = registry['modules']
    require(isinstance(modules, list), 'execution modules must be a list')
    previous_module, imports = None, set()
    def text(value):
        return isinstance(value, str) and value != '' and '\0' not in value
    for module in modules:
        require(isinstance(module, dict) and set(module) == {'specifier', 'functions'}
                and text(module['specifier'])
                and (previous_module is None or previous_module < module['specifier'])
                and isinstance(module['functions'], list) and len(module['functions']) > 0,
                'invalid execution module declaration')
        previous_module, previous_export = module['specifier'], None
        for function in module['functions']:
            require(isinstance(function, dict) and set(function) ==
                    {'export', 'module', 'name', 'mode', 'parameters', 'result'}
                    and all(text(function[key]) for key in ('export', 'module', 'name'))
                    and function['module'] not in ('scriptw_runtime', 'scriptw_foreign', 'scriptw_host')
                    and (previous_export is None or previous_export < function['export'])
                    and function['mode'] in ('sync', 'async')
                    and function['result'] in ('number', 'utf16', 'bytes', 'void')
                    and isinstance(function['parameters'], list)
                    and all(value in ('number', 'utf16', 'bytes') for value in function['parameters']),
                    'invalid execution function declaration')
            identity = (function['module'], function['name'])
            require(identity not in imports, 'duplicate execution foreign import')
            imports.add(identity)
            previous_export = function['export']


def image_metadata(raw, runtime, source_fingerprint):
    """Inspect inert framing only; never load or execute its native payload.

    Archived SW01 remains readable evidence; product recovery accepts SW02 only.
    """
    require(runtime in RUNTIMES and _hex(source_fingerprint), 'unknown image runtime/source')
    magic = raw[:8]
    require(magic in ((b'SCRWEP01', b'SCRWEP02') if runtime == 'scriptw' else (b'SCRWWA01',)),
            'image framing/source/checksum differs')
    minimum = (126 if magic == b'SCRWEP02' else 122) if runtime == 'scriptw' else 88
    require(minimum < len(raw) <= MAX_EXPANDED
            and raw[8:40].hex() == source_fingerprint
            and _sha(raw[:-32]) == raw[-32:].hex(), 'image framing/source/checksum differs')
    key, facts = None, {}
    if runtime == 'scriptw':
        linked, key_bytes, mode = raw[40], raw[41:73], raw[73]
        require(linked in (0, 1) and (linked == 1 or key_bytes == bytes(32)) and mode == 1,
                'Scriptw image runtime key/common native policy differs')
        key = key_bytes.hex() if linked else None
        size_offset, start = 74, 90
        if magic == b'SCRWEP02':
            registry_length = int.from_bytes(raw[90:94], 'little')
            require(0 < registry_length <= 1024**2 and 126 + registry_length < len(raw),
                    'execution registry length differs')
            registry = raw[94:94 + registry_length]
            _execution_registry(registry)
            facts = dict(execution_facts_bytes=registry_length, execution_facts_sha256=_sha(registry))
            start, minimum = 94 + registry_length, 126 + registry_length
    else:
        size_offset, start = 40, 56
    program_length = int.from_bytes(raw[size_offset:size_offset + 8], 'little')
    native_length = int.from_bytes(raw[size_offset + 8:size_offset + 16], 'little')
    require(program_length > 0 and native_length > 0
            and minimum + program_length + native_length == len(raw), 'image lengths differ')
    program = raw[start:start + program_length]
    require(program[:8] == b'\0asm\x01\0\0\0', 'image raw program is not Wasm')
    native = raw[start + program_length:-32]
    return dict(sha256=_sha(raw), bytes=len(raw), magic=magic.decode(),
                source_fingerprint=source_fingerprint, program_wasm_sha256=_sha(program),
                program_wasm_bytes=program_length, native_serialized_bytes=native_length,
                native_sha256=_sha(native), runtime_key=key, **facts)


def _retained(report_path, report, archive):
    files, expanded = {}, 0
    for row in report['retained_files']:
        name = safe_name(row['path'])
        require(name not in files and _uint(row['bytes']) and row['bytes'] <= MAX_MEMBER
                and _hex(row['sha256']), 'invalid/repeated retained identity')
        require(not name.endswith(('.swpc', '.cwasm', '.wasmtime', '.so', '.rlib', '.a', '.exe'))
                and 'cache' not in PurePosixPath(name).parts, 'native image/binary/cache is not retained evidence')
        expanded += row['bytes']
        files[name] = row
    require(0 < len(files) <= MAX_MEMBERS and expanded <= MAX_EXPANDED, 'retained inventory exceeds budget')
    retained = {}
    if archive is not None:
        archive = Path(archive)
        authority = report['archive']
        require(authority.get('file') == STEM + '.tar.gz'
                and archive.is_file() and not archive.is_symlink() and archive.stat().st_size <= MAX_ARCHIVE
                and archive.stat().st_size == authority['bytes'] and digest(archive) == authority['sha256'],
                'archive length/hash differs')
        with tarfile.open(archive, 'r|gz') as bundle:
            for member in bundle:
                name = safe_name(member.name)
                require(member.isfile() and name in files and name not in retained
                        and member.size == files[name]['bytes'], 'unexpected archive member')
                retained[name] = bundle.extractfile(member).read()
        require(set(retained) == set(files) and authority['member_count'] == len(files)
                and authority['expanded_bytes'] == expanded, 'archive inventory differs')
    else:
        directory = report_path.parent.resolve()
        for name in files:
            path = directory / name
            require(path.is_file() and not path.is_symlink() and directory in path.resolve().parents
                    and path.stat().st_size == files[name]['bytes'], 'missing/symlinked/escaping retained file')
            retained[name] = path.read_bytes()
    for name, raw in retained.items():
        require(len(raw) == files[name]['bytes'] and _sha(raw) == files[name]['sha256'],
                'retained evidence changed: ' + name)
        require(not raw.startswith((b'\x7fELF', b'MZ', b'SCRWEP01', b'SCRWEP02', b'SCRWWA01')),
                'native binary/image payload cannot enter the evidence archive')
    return retained


def _identity(raw, report):
    require(raw.get('source_commit') == report['source_commit']
            and raw.get('source_fingerprint') == report['source_fingerprint']
            and raw.get('compiler_identity_scheme') == report['source_fingerprint_scheme']
            and raw.get('executable_sha256') == report['tools']['consumer']['sha256'],
            'actual shared executable/compiler identity differs')
    require(raw.get('comparison_cohort') == SCHEMA and raw.get('excluded_from_ordinary_cohorts') is True
            and 'excluded_from_scored_cohorts' not in raw, 'raw comparison cohort declaration differs')


def _memory(value):
    require(isinstance(value, dict) and value.get('scope') == MEMORY_SCOPE
            and all(_uint(value.get(key)) for key in MEMORY_FIELDS)
            and 0 < value['pss_bytes'] <= value['rss_bytes']
            and value['private_dirty_bytes'] <= value['rss_bytes'], 'invalid process memory endpoint')


def _consumer(raw, report, image, case, runtime):
    expected_schema = ('scriptw-precompiled-execution-v1' if runtime == 'scriptw'
                       else 'scriptw-precompiled-wasi-execution-v1')
    require(raw.get('schema') == expected_schema and raw.get('compiler_enabled') is False
            and raw.get('samples') == SAMPLES and raw.get('warmups') == WARMUPS
            and raw.get('shared_configuration') == report['shared_configuration']
            and raw.get('adapter') == ('scriptw' if runtime == 'scriptw' else 'wasi'),
            'consumer compiler/configuration/sample policy differs')
    _identity(raw, report)
    require(raw.get('image_sha256') == image['sha256'] and raw.get('image_bytes') == image['bytes']
            and raw.get('program_wasm_sha256') == image['program_wasm_sha256']
            and (runtime != 'scriptw' or raw.get('runtime_key') == image['runtime_key'])
            and (runtime != 'scriptc' or raw.get('wasm_sha256') == image['program_wasm_sha256']
                 and raw.get('native_serialized_bytes') == image['native_serialized_bytes']),
            'consumer image/raw Wasm correspondence differs')
    if image.get('magic') == 'SCRWEP02':
        require(all(raw.get(key) == image[key] for key in
                    ('execution_facts_bytes', 'execution_facts_sha256')),
                'consumer execution facts identity differs')
    load = raw.get('load_phases_ns')
    phases = ('engine', 'image_read_and_envelope_admission', 'native_admission_and_deserialization', 'process_total')
    require(isinstance(load, dict) and set(load) == set(phases)
            and all(_uint(load[key]) for key in phases)
            and load['process_total'] >= sum(load[key] for key in phases[:-1]), 'invalid load timings')
    sampled = raw.get('native_load_process_memory')
    require(isinstance(sampled, dict) and sampled.get('status') == 'sampled'
            and sampled.get('scope') == LOAD_SCOPES[runtime] and sampled.get('interval_ns') == 10_000_000
            and all(_uint(sampled.get(key)) for key in ('initial_rss_bytes', 'final_rss_bytes',
                    'maximum_rss_bytes', 'sample_count', 'elapsed_ns'))
            and sampled['sample_count'] >= 2 and 0 < sampled['initial_rss_bytes']
            and 0 < sampled['final_rss_bytes']
            and max(sampled['initial_rss_bytes'], sampled['final_rss_bytes']) <= sampled['maximum_rss_bytes']
            and sampled['elapsed_ns'] >= load['native_admission_and_deserialization'],
            'invalid native-load sampled RSS')
    endpoints = raw.get('process_snapshots')
    require(isinstance(endpoints, dict) and set(endpoints) == set(SNAPSHOTS), 'missing transport/load endpoints')
    for value in endpoints.values():
        _memory(value)
    observations = raw.get('observations')
    require(isinstance(observations, list) and len(observations) == WARMUPS + SAMPLES,
            'every warmup and measured observation is required')
    matched = []
    for index, observation in enumerate(observations):
        require(type(observation.get('index')) is int and observation['index'] == index
                and observation.get('warmup') is (index < WARMUPS), 'observation order/warmup differs')
        require(all(isinstance(observation.get(key), str) for key in ('stdout', 'stderr', 'completion')),
                'missing raw output/completion')
        correct = (observation['stdout'] == case['stdout'] and observation['stderr'] == ''
                   and observation['completion'] == 'normal')
        require(observation.get('matched') is correct, 'checksum status differs from actual output/completion')
        matched.append(correct)
        timing = observation.get('phases_ns')
        require(isinstance(timing, dict) and set(timing) == set(TIME_PHASES)
                and all(_uint(timing.get(key)) for key in TIME_PHASES) and timing['execution'] > 0
                and timing['total'] >= sum(timing[key] for key in TIME_PHASES[:-1]), 'invalid execution phases')
        memory = observation.get('process_snapshots')
        require(isinstance(memory, dict) and set(memory) == set(PROCESS_PHASES), 'missing Store lifecycle endpoint')
        for value in memory.values():
            _memory(value)
        require(_uint(observation.get('gc_capacity_before_bytes'))
                and _uint(observation.get('gc_capacity_after_bytes')), 'missing GC capacity endpoints')
        if runtime == 'scriptc':
            require(all(key in observation and (observation[key] is None or _uint(observation[key]))
                        for key in ('exported_linear_memory_before_bytes', 'exported_linear_memory_after_bytes')),
                    'missing/invalid WASI linear memory endpoints')
    return observations, all(matched)


def _commands(report, retained):
    commands, previous_end = {}, None
    for row in report['commands']:
        identifier = row['id']
        require(isinstance(identifier, str) and re.fullmatch('[a-zA-Z0-9_.-]+', identifier)
                and identifier not in commands, 'duplicate/invalid command identity')
        require(row['kind'] in ('tool-version', 'source-produce', 'wasm-precompile', 'consume')
                and row['runtime'] in (*RUNTIMES, 'shared')
                and row['round'] in (None, 0, 1)
                and row['file'] in (None, *[case['file'] for case in report['workloads']]), 'unknown typed command role')
        require(isinstance(row['argv'], list) and bool(row['argv'])
                and all(isinstance(argument, str) and argument for argument in row['argv'])
                and isinstance(row['cwd'], str) and _hex(row['executable_sha256'])
                and type(row['returncode']) is int and type(row['timed_out']) is bool,
                'incomplete actual command observation')
        start, end = _instant(row['started_at']), _instant(row['finished_at'])
        require(start <= end and (previous_end is None or previous_end <= start),
                'commands overlap or appear out of serial order')
        previous_end = end
        require(row['stdout'] != row['stderr'] and row['stdout'] in retained and row['stderr'] in retained,
                'missing actual command streams')
        require(not any(row[field] == prior[other] for prior in commands.values()
                        for field in ('stdout', 'stderr') for other in ('stdout', 'stderr')),
                'command streams are reused')
        if row['kind'] in ('wasm-precompile', 'consume') or (row['kind'] == 'source-produce' and row['runtime'] == 'scriptw'):
            require(row['executable_sha256'] == report['tools']['consumer']['sha256']
                    and row['argv'][0] == report['tools']['consumer']['path'], 'shared consumer/producer executable differs')
        require(row['kind'] == 'consume' or row['round'] is None, 'producer/tool command carries a measurement round')
        commands[identifier] = row
    require(commands and _instant(report['started_at']) <= _instant(next(iter(commands.values()))['started_at'])
            and previous_end <= _instant(report['finished_at']), 'report does not bracket actual commands')
    return commands


def _producer_case(command, retained, runtime, case, report, wasm_path):
    require(command['returncode'] == 0 and command['timed_out'] is False
            and retained[command['stderr']] == b'', 'successful producer has failed command/diagnostics')
    raw = _json(retained[command['stdout']])
    _identity(raw, report)
    require(raw.get('shared_configuration') == report['shared_configuration']
            and raw.get('compiler_enabled') is True
            and raw.get('adapter') == ('scriptw' if runtime == 'scriptw' else 'wasi'),
            'producer native configuration/authority differs')
    if runtime == 'scriptw':
        require(raw.get('schema') == 'scriptw-precompiled-producer-v1'
                and raw.get('manifest_sha256') == _sha(retained['sources/manifest.json'])
                and [row['file'] for row in raw.get('cases', [])] == [row['file'] for row in report['workloads']],
                'Scriptw producer lost the complete original corpus')
        row = next(row for row in raw['cases'] if row['file'] == case['file'])
        require(row.get('source_sha256') == _sha(retained['sources/' + case['file']])
                and row.get('stdout') == case['stdout'] and row.get('source_compilation_profile') == 'portable'
                and row.get('intl_profile') == 'Minimal', 'Scriptw producer source/output/profile differs')
        require(isinstance(row.get('phases_ns'), dict)
                and set(row['phases_ns']) == {'source_compilation', 'native_preparation'}
                and all(_uint(value) for value in row['phases_ns'].values()), 'Scriptw producer phase timings differ')
    else:
        require(raw.get('schema') == 'scriptw-precompiled-wasi-producer-v1', 'unexpected WASI producer schema')
        candidates = raw.get('cases', [raw])
        rows = [row for row in candidates if row.get('file') == Path(wasm_path).name]
        require(len(rows) == 1, 'WASI producer case correspondence differs')
        row = rows[0]
        phases = row.get('phases_ns')
        require(isinstance(phases, dict) and set(phases) == {'engine', 'native_compilation', 'native_serialization', 'total'}
                and all(_uint(value) for value in phases.values())
                and phases['total'] >= sum(phases[key] for key in phases if key != 'total'),
                'WASI producer phase timings differ')
    return row


def load_report(path, archive=None):
    """Validate all source/producer/consumer observations and rederive both bars."""
    path = Path(path)
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= MAX_REPORT, 'missing/oversized report')
    report = _json(path.read_bytes())
    require(report.get('schema') == SCHEMA and report.get('complete') is True
            and _hex(report.get('source_commit'), 40) and _hex(report.get('source_fingerprint'))
            and report.get('source_fingerprint_scheme') == 'scriptw-program-cache-compiler-v3',
            'incomplete report or invalid source identity')
    require(type(report.get('samples')) is int and report['samples'] == SAMPLES
            and type(report.get('warmups')) is int and report['warmups'] == WARMUPS
            and type(report.get('rounds')) is int and report['rounds'] == ROUNDS, 'sampling plan differs')
    require(report['policy']['consumer_compiler_enabled'] is False
            and report['policy']['producers_outside_consumers'] is True, 'compiler-free consumer policy differs')
    config = report['shared_configuration']
    require(isinstance(config, dict) and set(config) == set(SHARED_CONFIGURATION) | {'target_triple'}
            and all(type(config[key]) is type(value) and config[key] == value
                    for key, value in SHARED_CONFIGURATION.items())
            and isinstance(config['target_triple'], str)
            and re.fullmatch('[A-Za-z0-9_.-]+', config['target_triple']) is not None,
            'shared Wasmtime configuration differs from the common required policy')
    affinity = report['machine']['measurement_affinity']
    require(isinstance(affinity, list) and len(affinity) == 1 and _uint(affinity[0]), 'measurement must use one CPU')
    _resources(report)
    require(_hex(report['tools']['consumer']['sha256']) and isinstance(report['tools']['consumer']['path'], str)
            and report['tools']['scriptc']['version'] in ('0.4.5', 'scriptc 0.4.5')
            and report['tools']['zig']['version'] == '0.16.0', 'tool pins/shared consumer differ')
    retained = _retained(path, report, archive)
    corpus = Path(__file__).resolve().parents[1] / FIXTURES
    canonical = _json((corpus / 'manifest.json').read_bytes())
    plan = _json(retained['sources/manifest.json'])
    require(plan == canonical and len(plan['execution']) == 17 and report['workloads'] == plan['execution'],
            'complete canonical workload inventory required')
    for case in report['workloads']:
        name = safe_name(case['file'])
        require(Path(name).name == name and retained['sources/' + name] == (corpus / name).read_bytes(),
                'canonical source differs')
        require(retained['adapted/' + name] == ADAPTER.encode() + retained['sources/' + name],
                'Scriptc adaptation differs')
        require(retained['expected/' + name] == case['stdout'].encode(), 'original checksum differs')
    commands = _commands(report, retained)
    tools = report['tools']
    identity_command = commands[tools['consumer']['identity_command']]
    require(identity_command['kind'] == 'tool-version' and identity_command['runtime'] == 'shared'
            and identity_command['file'] is None and identity_command['round'] is None
            and identity_command['returncode'] == 0 and identity_command['timed_out'] is False
            and identity_command['argv'] == [tools['consumer']['path'], 'identity']
            and identity_command['executable_sha256'] == tools['consumer']['sha256']
            and retained[identity_command['stderr']] == b'', 'shared-host identity command differs')
    host = _json(retained[identity_command['stdout']])
    _identity(host, report)
    require(host.get('schema') == 'scriptw-precompiled-shared-host-v1' and host.get('adapter') == 'shared'
            and host.get('shared_configuration') == config, 'shared-host identity/configuration differs')
    for tool in ('scriptc', 'zig', 'rustc'):
        entry = tools[tool]
        command = commands[entry['version_command']]
        require(command['kind'] == 'tool-version' and command['runtime'] == 'shared'
                and command['file'] is None and command['returncode'] == 0 and command['timed_out'] is False
                and command['executable_sha256'] == entry['sha256'] and _hex(entry['sha256'])
                and retained[command['stdout']].decode().strip() == entry['version']
                and retained[command['stderr']] == b'', 'tool version authority differs')
    require({row['id'] for row in report['commands'] if row['kind'] == 'tool-version'}
            == {tools['consumer']['identity_command'], *(tools[name]['version_command'] for name in ('scriptc', 'zig', 'rustc'))},
            'unaccounted/repeated tool-version command')
    cargo = [_json(line) for line in retained[tools['consumer']['cargo_log']].splitlines()]
    producer = tools['consumer']['cargo_producer']
    require(cargo and cargo[-1] == dict(reason='build-finished', success=True) and producer in cargo
            and sum(row.get('reason') == 'build-finished' for row in cargo) == 1
            and producer.get('reason') == 'compiler-artifact' and producer['target']['kind'] == ['example']
            and producer['target']['name'] == 'compiler_precompiled_baseline' and producer['features'] == []
            and producer['profile']['opt_level'] == '3' and producer['profile']['test'] is False,
            'actual complete default release Cargo producer differs')
    expected = measurement_plan(report['workloads'])
    attempts = report['attempts']
    require(len(attempts) == len(expected) == 68
            and [{key: row[key] for key in ('round', 'file', 'runtime')} for row in attempts] == expected,
            'exact two-runtime/two-round serial inventory differs')
    consumed, used_producers, by_key = [], set(), {}
    for attempt in attempts:
        round_index, name, runtime = (attempt[key] for key in ('round', 'file', 'runtime'))
        require(type(round_index) is int, 'round must be an integer')
        case = next(case for case in report['workloads'] if case['file'] == name)
        require(attempt['status'] in ('ok', *FAILURES), 'unknown attempt status')
        ids = attempt['producer_ids']
        require(isinstance(ids, list) and bool(ids) and len(set(ids)) == len(ids)
                and all(identifier in commands for identifier in ids), 'missing/repeated producer link')
        producers = [commands[identifier] for identifier in ids]
        require(all(row['runtime'] == runtime and row['file'] in (None, name)
                    and row['kind'] in ('source-produce', 'wasm-precompile') for row in producers),
                'producer runtime/case/role differs')
        require(all(_instant(first['finished_at']) <= _instant(second['started_at'])
                    for first, second in zip(producers, producers[1:])), 'native producer precedes source compilation')
        kinds = [row['kind'] for row in producers]
        require(kinds == (['source-produce'] if runtime == 'scriptw'
                          else ['source-produce', 'wasm-precompile'])
                or runtime == 'scriptc' and attempt['status'] == 'source-failed' and kinds == ['source-produce'],
                'producer chain differs from the source-to-Wasm-to-native path')
        if runtime == 'scriptc':
            source = producers[0]
            require(source['file'] == name and source['argv'][0] == report['tools']['scriptc']['path']
                    and source['executable_sha256'] == report['tools']['scriptc']['sha256']
                    and 'build' in source['argv']
                    and any(_path_argument(argument, 'adapted/' + name)
                            for argument in source['argv']), 'Scriptc command did not compile the exact adapted source')
        used_producers.update(ids)
        failed = [row for row in producers if row['returncode'] != 0 or row['timed_out']]
        if attempt['status'] in ('source-failed', 'precompile-failed'):
            kind = 'source-produce' if attempt['status'] == 'source-failed' else 'wasm-precompile'
            require(any(row['kind'] == kind for row in failed) and attempt['consumer_id'] is None,
                    'producer failure has no failed command or hides an executed consumer')
            by_key[round_index, name, runtime] = (attempt['status'], None)
            continue
        require(not failed and len([row for row in producers if row['kind'] == ('source-produce' if runtime == 'scriptw' else 'wasm-precompile')]) == 1,
                'native image has no unique successful producer')
        image = attempt['image']
        image_file = safe_name(image['file'])
        require(image_file.startswith('images/') and image_file not in retained, 'native image transport was retained/misplaced')
        if image.get('magic') == 'SCRWEP02':
            require(runtime == 'scriptw' and _uint(image.get('execution_facts_bytes'))
                    and 0 < image['execution_facts_bytes'] <= 1024**2
                    and _hex(image.get('execution_facts_sha256')),
                    'invalid execution facts framing identity')
            frame_overhead = 126 + image['execution_facts_bytes']
        else:
            frame_overhead = 122 if runtime == 'scriptw' else 88
        require(_hex(image.get('sha256')) and _hex(image.get('native_sha256'))
                and image.get('source_fingerprint') == report['source_fingerprint']
                and image.get('magic') in (('SCRWEP01', 'SCRWEP02') if runtime == 'scriptw' else ('SCRWWA01',))
                and all(_uint(image.get(key)) and image[key] > 0 for key in ('bytes', 'program_wasm_bytes', 'native_serialized_bytes'))
                and image['bytes'] <= MAX_EXPANDED
                and image['bytes'] == image['program_wasm_bytes'] + image['native_serialized_bytes'] + frame_overhead
                and (image['runtime_key'] is None or runtime == 'scriptw' and _hex(image['runtime_key'])),
                'invalid retained image framing identity')
        wasm = retained[attempt['wasm']]
        require(wasm[:8] == b'\0asm\x01\0\0\0' and len(wasm) == image['program_wasm_bytes']
                and _sha(wasm) == image['program_wasm_sha256'], 'native image/raw Wasm correspondence differs')
        actual_producer = next(row for row in producers if row['kind'] == ('source-produce' if runtime == 'scriptw' else 'wasm-precompile'))
        if runtime == 'scriptc':
            source_args, native_args = producers[0]['argv'], actual_producer['argv']
            require('-o' in source_args and source_args.index('-o') + 1 < len(source_args)
                    and _path_argument(source_args[source_args.index('-o') + 1], attempt['wasm'])
                    and len(native_args) == 4 and native_args[1] == 'prepare-wasi'
                    and _path_argument(native_args[2], attempt['wasm'])
                    and _path_argument(native_args[3], image_file), 'source/Wasm/native command paths differ')
        else:
            require(len(actual_producer['argv']) == 4 and actual_producer['argv'][1] == 'prepare-scriptw'
                    and _path_argument(actual_producer['argv'][2], 'sources'), 'Scriptw source producer command differs')
        produced = _producer_case(actual_producer, retained, runtime, case, report, attempt['wasm'])
        require(produced.get('image_sha256') == image['sha256'] and produced.get('image_bytes') == image['bytes']
                and produced.get('program_wasm_sha256') == image['program_wasm_sha256']
                and produced.get('program_wasm_bytes') == image['program_wasm_bytes']
                and produced.get('native_serialized_bytes') == image['native_serialized_bytes']
                and produced.get('wasm') == Path(attempt['wasm']).name
                and produced.get('image') == Path(image_file).name
                and (runtime != 'scriptw' or produced.get('runtime_key') == image['runtime_key']),
                'producer/image hashes or framing differ')
        if image.get('magic') == 'SCRWEP02':
            require(all(produced.get(key) == image[key] for key in
                        ('execution_facts_bytes', 'execution_facts_sha256')),
                    'producer execution facts identity differs')
        consumer = commands[attempt['consumer_id']]
        require(consumer['kind'] == 'consume' and consumer['runtime'] == runtime and consumer['file'] == name
                and type(consumer['round']) is int and consumer['round'] == round_index
                and all(_instant(row['finished_at']) <= _instant(consumer['started_at']) for row in producers),
                'consumer role/case/round or producer order differs')
        argv = consumer['argv']
        require(len(argv) == 6 and argv[1] == ('execute-scriptw' if runtime == 'scriptw' else 'execute-wasi')
                and _path_argument(argv[2], image_file) and argv[3:] == [case['stdout'], str(SAMPLES), str(WARMUPS)],
                'consumer command image/checksum/sample arguments differ')
        consumed.append(consumer['id'])
        if attempt['status'] == 'run-failed':
            require(consumer['returncode'] != 0 or consumer['timed_out'], 'runtime failure has no failed command')
            by_key[round_index, name, runtime] = (attempt['status'], None)
            continue
        require(consumer['returncode'] == 0 and consumer['timed_out'] is False
                and retained[consumer['stderr']] == b'', 'successful consumer has failed command/diagnostics')
        raw = _json(retained[consumer['stdout']])
        observations, matched = _consumer(raw, report, image, case, runtime)
        require(matched is (attempt['status'] == 'ok'), 'attempt status hides output failure or success')
        by_key[round_index, name, runtime] = (attempt['status'], raw)
    require(len(consumed) == len(set(consumed))
            and consumed == [row['id'] for row in report['commands'] if row['kind'] == 'consume'],
            'consumer commands are omitted/reused/out of order')
    require(used_producers == {row['id'] for row in report['commands']
                              if row['kind'] in ('source-produce', 'wasm-precompile')}, 'unaccounted producer command')
    if consumed:
        require(all(_instant(row['finished_at']) <= _instant(commands[consumed[0]]['started_at'])
                    for row in report['commands'] if row['kind'] != 'consume'),
                'producer/tool work entered the consumer cohort')
    table = []
    for case in report['workloads']:
        runtimes = {}
        for runtime in RUNTIMES:
            pairs = [by_key[round_index, case['file'], runtime] for round_index in range(ROUNDS)]
            failures = sorted({status for status, _ in pairs if status != 'ok'})
            if failures:
                runtimes[runtime] = dict(status=', '.join(failures), ms=None, rss_mib=None,
                    pss_mib=None, private_dirty_mib=None, rounds=[status for status, _ in pairs])
                continue
            raw = [raw for _, raw in pairs]
            values = [observation for value in raw for observation in value['observations'][WARMUPS:]]
            median = lambda items: statistics.median(items)
            runtimes[runtime] = dict(status='ok', ms=median([row['phases_ns']['execution'] for row in values]) / 1e6,
                **{field.removesuffix('_bytes') + '_mib':
                   median([row['process_snapshots']['after_execution'][field] for row in values]) / 2**20
                   for field in MEMORY_FIELDS},
                measured_samples=len(values), rounds=['ok', 'ok'],
                load_phases_ns={key: [value['load_phases_ns'][key] for value in raw] for key in raw[0]['load_phases_ns']},
                load_sampled_max_rss_bytes=[value['native_load_process_memory']['maximum_rss_bytes'] for value in raw],
                after_transport_drop=[value['process_snapshots']['after_transport_drop'] for value in raw],
                first_warmups=[value['observations'][0] for value in raw],
                process_medians={phase: {field: median([row['process_snapshots'][phase][field] for row in values])
                                       for field in MEMORY_FIELDS} for phase in PROCESS_PHASES})
        table.append(dict(file=case['file'], runtimes=runtimes))
    if 'table' in report:
        require(report['table'] == table, 'published summaries differ from actual observations')
    return report, table


def render(report, rows, metric, preview=None):
    """Paired absolute bars on one linear zero scale; failed cases have no bar."""
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from matplotlib.patches import Patch
    chart = CHARTS[metric]
    with plt.rc_context({'font.family': 'DejaVu Sans', 'svg.fonttype': 'none',
                         'svg.hashsalt': 'scriptw-precompiled-comparison-' + metric + '-v1'}):
        fig, ax = plt.subplots(figsize=(15, 11.5))
        try:
            fig.subplots_adjust(left=.24, right=.96, top=.86, bottom=.18)
            fig.suptitle(chart['title'], fontsize=23, x=.04, ha='left', y=.975)
            fig.text(.04, .925, f"Measured {report['finished_at'][:10]} · source {report['source_commit'][:9]} · "
                     '14 samples · 10 warmups per round', color='#57606a')
            fig.legend(handles=[Patch(color=COLORS[runtime], label=LABELS[runtime]) for runtime in RUNTIMES],
                       loc='upper right', bbox_to_anchor=(.97, .987), frameon=False)
            ax.set_title('Compiler disabled in both consumers · lower is better · median', loc='left', pad=16)
            maximum = max([row['runtimes'][runtime][chart['field']] or 0 for row in rows for runtime in RUNTIMES] + [1e-9])
            ax.set_xlim(0, maximum * 1.32)
            ax.set_ylim(len(rows) - .5, -.5)
            ax.set_yticks(range(len(rows)), [row['file'] for row in rows])
            ax.set_axisbelow(True)
            ax.grid(axis='x', color='#eaeef2')
            for spine in ax.spines.values():
                spine.set_visible(False)
            for index, row in enumerate(rows):
                for ordinal, runtime in enumerate(RUNTIMES):
                    item, y = row['runtimes'][runtime], index + (ordinal - .5) * .32
                    if item['status'] == 'ok':
                        value = item[chart['field']]
                        ax.barh(y, value, height=.27, color=COLORS[runtime])
                        ax.annotate(f"{value:.{chart['decimals']}f}", (value, y), xytext=(4, 0),
                                    textcoords='offset points', va='center', fontsize=8, color=COLORS[runtime])
                    else:
                        ax.text(maximum * .02, y, item['status'], color='#b42318', fontsize=8, va='center')
            ax.set_xlabel(chart['unit'])
            notes = ['Same consumer executable and Wasmtime configuration; fresh Store per invocation.',
                     'Source and Wasm compilation run in separate producer processes; loading/setup costs are recorded separately.',
                     'Post-execution process RSS includes resident code/setup/warmup history, not live JS bytes or peak RSS.',
                     'All 17 cases retained; failures have labels without bars. Shared-host observations are descriptive.']
            for y, note in zip((.105, .08, .055, .03), notes):
                fig.text(.04, y, note, fontsize=10)
            output = io.BytesIO()
            fig.savefig(output, format='svg', metadata={'Date': None,
                'Title': 'Compiler-free Scriptw versus Scriptc: ' + chart['title'],
                'Description': json.dumps(dict(source_commit=report['source_commit'], compiler_enabled=False,
                    executable_sha256=report['tools']['consumer']['sha256'], metric=metric, rows=rows), sort_keys=True)})
            if preview:
                Path(preview).parent.mkdir(parents=True, exist_ok=True)
                fig.savefig(preview, dpi=90)
            return b'\n'.join(line.rstrip() for line in output.getvalue().splitlines()) + b'\n'
        finally:
            plt.close(fig)


def markdown(report, rows):
    lines = ['# Compiler-free Scriptw / Scriptc comparison', '',
        f"Measured {report['started_at']} through {report['finished_at']}; source `{report['source_commit']}`.", '',
        'Both runtimes use the same executable and shared Wasmtime configuration with its compiler disabled. '
        'Both application modules use size-optimized native compilation with backtracking register allocation '
        'and the same generic target/backtrace policy. '
        'Separate producer processes compile the original JavaScript to Wasm and serialize native images. '
        'Scriptc receives only the recorded print adapter; the original checksums and every workload remain unchanged.', '',
        'Each of two rounds has ten checked warmups and seven measured fresh-Store invocations per runtime. '
        'Round two reverses workload order and the first runtime alternates. Execution times the compiled entry and '
        'output capture, excluding image loading, Store/instance setup and post-timer checksum validation. Adapter-specific '
        'completion representations differ. Phase totals include the process-memory probes and represent '
        'instrumented wall time, not pure Store-to-entry latency. Complete timings and memory endpoints '
        'remain in the raw evidence. This cohort is separate from the ordinary three-runtime comparison.', '',
        'RSS, PSS and private-dirty values below are pooled medians of 14 post-execution process endpoints while each '
        'Store remains alive. They include native code, host/setup and retained warmup history; they are not live '
        'JavaScript bytes, synchronized physical ownership measurements or whole-child high-water memory. Loading has '
        'its own 10 ms sampled RSS maximum, which can miss shorter transients. No direct causal subtraction is claimed.', '',
        '| Workload | Scriptw ms | Scriptc ms | Scriptw RSS / PSS / private dirty MiB | Scriptc RSS / PSS / private dirty MiB |',
        '| --- | ---: | ---: | ---: | ---: |']
    for row in rows:
        def value(runtime, memory=False):
            item = row['runtimes'][runtime]
            if item['status'] != 'ok':
                return item['status']
            return (' / '.join(f'{item[key]:.3f}' for key in ('rss_mib', 'pss_mib', 'private_dirty_mib'))
                    if memory else f"{item['ms']:.3f}")
        lines.append(f"| {row['file']} | {value('scriptw')} | {value('scriptc')} | {value('scriptw', True)} | {value('scriptc', True)} |")
    lines += ['', 'A failure in either round removes that runtime’s bars for the case; all original failure streams remain. '
        'Checksums, warmup order, shared executable/configuration, raw Wasm and producer/consumer image identities are '
        'readmitted before rendering. Checksums do not grant native-code trust: externally supplied native images retain '
        'the Rust API’s explicit unsafe provenance requirement.', '',
        '[Execution time](assets/precompiled-execution.svg) · [Process memory](assets/precompiled-memory.svg)', '',
        '[Checked report](precompiled-comparison.json) · [Raw evidence](precompiled-comparison.tar.gz). '
        'The archive excludes executables, serialized native images and caches, retaining their identities, inert '
        'framing metadata, sources, raw Wasm, complete command streams, tool versions and Cargo producer log.', '',
        'Validate or regenerate without running native code:', '', '```sh',
        'python3 scripts/precompiled_comparison.py check docs/rust-rewrite/precompiled-comparison.json',
        'python3 scripts/precompiled_comparison.py render docs/rust-rewrite/precompiled-comparison.json', '```', '']
    return '\n'.join(lines)


def publish(report_path, root, *, replace=False):
    """Publish only fully admitted actual evidence; never edit the README."""
    report_path, root = Path(report_path), Path(root)
    report, rows = load_report(report_path)
    destination = root / 'docs/rust-rewrite'
    targets = [destination / (STEM + suffix) for suffix in ('.json', '.md', '.tar.gz')]
    targets += [destination / 'assets' / chart['filename'] for chart in CHARTS.values()]
    require(replace or not any(path.exists() for path in targets), 'publication already exists; explicit replacement required')
    destination.mkdir(parents=True, exist_ok=True)
    (destination / 'assets').mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.precompiled-comparison-', dir=destination) as temporary:
        temporary = Path(temporary)
        bundle = temporary / (STEM + '.tar.gz')
        with bundle.open('xb') as output, gzip.GzipFile(filename='', mode='wb', fileobj=output, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w|', format=tarfile.PAX_FORMAT) as archive:
                for row in sorted(report['retained_files'], key=lambda row: row['path']):
                    payload = (report_path.parent / row['path']).read_bytes()
                    require(len(payload) == row['bytes'] and _sha(payload) == row['sha256'], 'evidence changed during publication')
                    member = tarfile.TarInfo(row['path'])
                    member.size, member.mode = len(payload), 0o644
                    archive.addfile(member, io.BytesIO(payload))
        require(bundle.stat().st_size <= MAX_ARCHIVE, 'compressed evidence exceeds budget')
        report = dict(report, table=rows, archive=dict(file=bundle.name, sha256=digest(bundle),
            bytes=bundle.stat().st_size, member_count=len(report['retained_files']),
            expanded_bytes=sum(row['bytes'] for row in report['retained_files'])))
        candidate = temporary / (STEM + '.json')
        candidate.write_text(json.dumps(report, indent=2) + '\n')
        load_report(candidate, bundle)
        (temporary / (STEM + '.md')).write_text(markdown(report, rows))
        for metric, chart in CHARTS.items():
            (temporary / chart['filename']).write_bytes(render(report, rows, metric))
        for target in targets:
            (temporary / target.name).replace(target)
    return report, rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('validate', 'publish', 'render', 'check'))
    parser.add_argument('report', type=Path)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--replace', action='store_true')
    args = parser.parse_args()
    if args.command == 'publish':
        report, rows = publish(args.report, args.root, replace=args.replace)
    else:
        archive = args.report.with_suffix('.tar.gz') if args.command in ('render', 'check') else None
        report, rows = load_report(args.report, archive)
        if args.command in ('render', 'check'):
            for metric, chart in CHARTS.items():
                target = args.root / 'docs/rust-rewrite/assets' / chart['filename']
                content = render(report, rows, metric)
                if args.command == 'check':
                    require(target.read_bytes() == content, metric + ' image is stale')
                else:
                    target.write_bytes(content)
    print(json.dumps(dict(validated=True, workloads=len(rows), runtimes=list(RUNTIMES),
        source_commit=report['source_commit'], compiler_enabled=False)))


if __name__ == '__main__':
    main()
