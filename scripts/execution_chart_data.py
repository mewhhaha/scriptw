"""Read-only admission of the archived execution checkpoint for its README plot.

This is a separate fresh-Realm corpus, not an ordinary startup/history report.
Current-engine RSS comparisons use process-wide endpoints after execution,
including runtime/setup and retained warmup effects, not peak or live JS bytes.
Rendering never runs a compiler, benchmark or native test.
"""
from datetime import datetime
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re
import statistics
import tarfile


ROUNDS = ('baseline-round-a-data', 'candidate-round-b-data',
          'candidate-round-c-data', 'baseline-round-d-data')
MAX_REPORT = 16 * 1024**2
MAX_ARCHIVE = 32 * 1024**2
MAX_MEMBER = 64 * 1024**2
MAX_EXPANDED = 256 * 1024**2
# Complete Intl replay evidence includes more than 6,000 execution receipts.
MAX_MEMBERS = 32768


def _require(condition, message):
    if not condition:
        raise ValueError('execution chart: ' + message)


def _json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            _require(key not in result, 'duplicate JSON key: ' + key)
            result[key] = value
        return result

    def nonfinite(value):
        raise ValueError('execution chart: nonfinite JSON number: ' + value)

    return json.loads(data, object_pairs_hook=pairs, parse_constant=nonfinite)


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _path(value):
    _require(isinstance(value, str), 'archive path must be text')
    path = PurePosixPath(value)
    _require(not path.is_absolute() and '..' not in path.parts and '\\' not in value
             and not re.match(r'^[A-Za-z]:', value) and str(path) == value,
             'unsafe archive path')
    return value


def _instant(value):
    result = datetime.fromisoformat(value.replace('Z', '+00:00'))
    _require(result.utcoffset() is not None, 'measurement timestamp needs a timezone')
    return result


def _positive(value):
    _require(type(value) in (int, float) and math.isfinite(value) and value > 0,
             'timing or RSS value must be finite and positive')
    return value


def _archive(report_path, report):
    authority = report['archive']
    name = _path(authority['file'])
    _require(Path(name).name == name and name.endswith('.tar.gz'), 'invalid archive filename')
    path = report_path.parent / name
    _require(path.is_file() and not path.is_symlink() and path.stat().st_size <= MAX_ARCHIVE,
             'missing, symlinked or oversized archive')
    _require(path.stat().st_size == authority['bytes'], 'archive length differs')
    with path.open('rb') as stream:
        _require(hashlib.file_digest(stream, 'sha256').hexdigest() == authority['sha256'],
                 'archive SHA-256 differs')
    prefix = name.removesuffix('.tar.gz') + '/'
    selected = {prefix + 'analysis.json', prefix + 'archive-members.json'}
    selected.update(prefix + label + '/report.json' for label in ROUNDS)
    selected.update(prefix + label.removesuffix('-data') + '.json' for label in ROUNDS)
    retained, names, expanded = {}, set(), 0
    # Stream without extracting; unselected native evidence never enters memory.
    with tarfile.open(path, 'r|gz') as archive:
        for member in archive:
            member_name = _path(member.name)
            _require(member.isfile() and member_name.startswith(prefix) and member_name not in names,
                     'unexpected, duplicate or non-file archive member')
            names.add(member_name)
            expanded += member.size
            _require(0 <= member.size <= MAX_MEMBER and expanded <= MAX_EXPANDED and len(names) <= MAX_MEMBERS,
                     'archive exceeds reading budget')
            if member_name in selected:
                _require(member.size <= MAX_REPORT, 'chart evidence exceeds reading budget')
                retained[member_name] = archive.extractfile(member).read()
    _require(len(names) == authority['member_count'] and expanded == authority['expanded_bytes']
             and set(retained) == selected, 'archive inventory differs')
    manifest_name = prefix + 'archive-members.json'
    _require(_sha(retained[manifest_name]) == authority['manifest_sha256'], 'manifest SHA-256 differs')
    manifest = _json(retained[manifest_name])
    _require(manifest['schema'] == 'scriptw-execution-performance-archive-v1'
             and manifest['source_commit'] == report['source_commit'], 'archive source identity differs')
    declared = {}
    for row in manifest['members']:
        member_name = _path(row['path'])
        _require(member_name not in declared, 'duplicate manifest identity')
        declared[member_name] = row
    _require(set(declared) == names - {manifest_name}, 'manifest inventory differs')
    for member_name, data in retained.items():
        if member_name != manifest_name:
            _require(declared[member_name]['bytes'] == len(data)
                     and declared[member_name]['sha256'] == _sha(data), 'retained evidence hash differs')
    return {name.removeprefix(prefix): _json(data) for name, data in retained.items()
            if name != manifest_name}


def _rounds(report, analysis, retained):
    checked = {}
    previous_end = None
    for index, label in enumerate(ROUNDS):
        product = 'baseline' if index in (0, 3) else 'candidate'
        meta = analysis['rounds'][label]
        raw = retained[label + '/report.json']
        receipt = retained[label.removesuffix('-data') + '.json']
        identity = report['products'][product]['compiler_identity']
        runner = report['products'][product]['runtime_runner_sha256']
        _require(meta['product'] == raw['product'] == product and raw['label'] == label
                 and raw['complete'] is True, 'round product or completion differs')
        _require(meta['compiler_identity'] == raw['compiler_identity'] == identity
                 and meta['runtime_runner_sha256'] == raw['runtime_runner_sha256'] == runner,
                 'round compiler or runner identity differs')
        _require(raw['samples'] == 7 and raw['warmups'] == 10 and raw['affinity'] == [0],
                 'round sample, warmup or CPU plan differs')
        _require(receipt['label'] == label.removesuffix('-data') and receipt['exit_code'] == 0
                 and receipt['source_head'] == report['source_commit']
                 and receipt['source_patch_sha256'] == _sha(b'')
                 and receipt['cpu_affinity'] == [0], 'round receipt or launcher identity differs')
        _require(_instant(meta['start']) <= _instant(meta['end'])
                 and meta['end'] == receipt['date'], 'measurement timestamps differ')
        _require(previous_end is None or previous_end <= _instant(meta['start']),
                 'ABBA rounds overlap or ran out of order')
        previous_end = _instant(meta['end'])
        if product == 'baseline':
            _require(meta['product_verification_patch_sha256']
                     == report['products']['baseline']['actual_source_patch_sha256'],
                     'baseline source patch differs')
        checked[label] = raw
    return checked


def load_chart_data(report_path):
    """Validate committed authority and return the 17 ordered plotting rows."""
    report_path = Path(report_path)
    _require(report_path.is_file() and not report_path.is_symlink()
             and report_path.stat().st_size <= MAX_REPORT, 'missing or oversized source report')
    data = report_path.read_bytes()
    report = _json(data)
    _require(report['schema'] == 'scriptw-execution-performance-checkpoint-v1'
             and report['complete'] is True and report['performance']['complete'] is True,
             'need a complete execution checkpoint')
    source = report['source_commit']
    _require(re.fullmatch('[0-9a-f]{40}', source) is not None
             and report['products']['candidate']['compiler_identity']['source_revision']
             == {'kind': 'git-commit', 'commit': source}, 'candidate source identity differs')
    retained = _archive(report_path, report)
    analysis = retained['analysis.json']
    _require(analysis == report['performance']['balanced_observations'],
             'published analysis differs from archived authority')
    _require(analysis['schema'] == 'scriptw-execution-performance-comparison-v1'
             and analysis['complete'] is True and analysis['case_count'] == 17
             and analysis['round_order'] == list(ROUNDS) and set(analysis['rounds']) == set(ROUNDS),
             'fresh-Realm ABBA plan differs')
    cases, table = analysis['cases'], report['performance']['table']
    names = [row['file'] for row in cases]
    _require(len(names) == len(set(names)) == 17 and [row['file'] for row in table] == names,
             'need every ordered workload, including controls')
    raw_rounds = _rounds(report, analysis, retained)
    _require(all([row['file'] for row in raw['case_results']] == names
                 for raw in raw_rounds.values()), 'round workload inventory differs')
    rows = []
    for case_index, (case, published) in enumerate(zip(cases, table)):
        samples = {product: {runtime: [] for runtime in ('wasmtime', 'v8')}
                   for product in ('baseline', 'candidate')}
        rss = {product: [] for product in samples}
        endpoint_rss = {runtime: [] for runtime in ('wasmtime', 'v8')}
        for label, raw in raw_rounds.items():
            product = raw['product']
            item = raw['case_results'][case_index]
            _require(item['source_sha256'] == case['source_sha256']
                     and item['expected_stdout'] == case['expected_stdout'], 'workload identity differs')
            for runtime in ('wasmtime', 'v8'):
                event = item['runtimes'][runtime]
                observations = event['data']
                schema = ('scriptw-wasmtime-baseline-v2' if runtime == 'wasmtime'
                          else 'scriptw-v8-baseline-v2')
                _require(event['exit_code'] == 0 and observations['samples'] == 7
                         and observations['schema'] == schema and observations['warmups'] == 10
                         and len(observations['cases']) == 1,
                         'runtime failed or changed its sampling plan')
                observed = observations['cases'][0]
                _require(observed['file'] == case['file'] and observed['source_sha256'] == case['source_sha256'],
                         'runtime source identity differs')
                values = observed['observations']
                _require(len(values) == 17 and [row['warmup'] for row in values] == [True]*10 + [False]*7
                         and all(row['stdout'] == case['expected_stdout'] for row in values),
                         'runtime outputs or warmup observations differ')
                measured = values[10:]
                actual = [_positive(row['execution_ns']) for row in measured]
                _require(actual == case['rounds'][label][runtime]['execution_ns'], 'round timings differ')
                samples[product][runtime].extend(actual)
                if runtime == 'wasmtime':
                    identity = raw['compiler_identity']
                    _require(observations['source_commit'] == identity['source_revision']['commit']
                             and observations['source_fingerprint'] == identity['source_fingerprint']
                             and observations['executable_sha256'] == raw['runtime_runner_sha256']
                             and observations['requested_native_policy'] is None,
                             'executed Wasmtime identity differs')
                    for value in measured:
                        memory = value['memory']['process_rss']
                        _require(memory['status'] == 'sampled'
                                 and memory['scope'] == 'runtime-invocation-process-rss', 'RSS scope differs')
                        rss[product].append(_positive(memory['maximum_rss_bytes']))
                        if product == 'candidate':
                            endpoint_rss[runtime].append(_positive(memory['final_rss_bytes']))
                else:
                    _require([observations['node'], observations['v8']]
                             == analysis['rounds'][label]['node_v8'], 'paired Node/V8 identity differs')
                    if product == 'candidate':
                        for value in measured:
                            memory = value['memory']
                            _require(memory['scope']
                                     == 'process-wide-endpoints;natural-gc;not-peak-or-live-workload-bytes',
                                     'V8 RSS endpoint scope differs')
                            endpoint_rss[runtime].append(_positive(memory['after']['rss']))
        for product in samples:
            for runtime, values in samples[product].items():
                pooled = case['pooled'][product][runtime]
                _require(len(values) == 14 and pooled['execution_ns'] == values
                         and pooled['median_execution_ns'] == statistics.median(values),
                         'pooled timing summary differs')
            memory = case['pooled'][product]['wasmtime']['memory_observations']['maximum_sampled_rss_bytes']
            _require(memory['values'] == rss[product] and memory['median'] == statistics.median(rss[product]),
                     'pooled RSS summary differs')
        for runtime, field in (('wasmtime', 'final_rss_bytes'), ('v8', 'rss_after_execution_bytes')):
            memory = case['pooled']['candidate'][runtime]['memory_observations'][field]
            _require(len(endpoint_rss[runtime]) == 14 and memory['values'] == endpoint_rss[runtime]
                     and memory['median'] == statistics.median(endpoint_rss[runtime]),
                     'current RSS endpoint summary differs')
        before, after = [statistics.median(samples[p]['wasmtime']) / 1e6 for p in ('baseline', 'candidate')]
        v8 = statistics.median(samples['candidate']['v8']) / 1e6
        change = 100 * (after / before - 1)
        _require(published['baseline_ms'] == before and published['candidate_ms'] == after
                 and published['paired_candidate_v8_ms'] == v8
                 and math.isclose(published['change_percent'], change, abs_tol=1e-12)
                 and math.isclose(published['candidate_over_paired_v8'], after / v8, abs_tol=1e-12),
                 'published timing table differs')
        before_rss, after_rss = [statistics.median(rss[p]) / 1024**2 for p in ('baseline', 'candidate')]
        rows.append(dict(file=case['file'], before_ms=before, after_ms=after, v8_ms=v8,
                         before_rss_mib=before_rss, after_rss_mib=after_rss,
                         scriptw_rss_mib=statistics.median(endpoint_rss['wasmtime']) / 1024**2,
                         v8_rss_mib=statistics.median(endpoint_rss['v8']) / 1024**2,
                         change_percent=change, rss_change_percent=100 * (after_rss / before_rss - 1)))
    return dict(source_commit=source,
                baseline_commit=report['products']['baseline']['compiler_identity']['source_revision']['commit'],
                refresh_date=report['refresh_date'],
                measurement_end=max(_instant(row['end']) for row in analysis['rounds'].values()).isoformat(),
                source_report_sha256=_sha(data), archive_sha256=report['archive']['sha256'],
                samples=14, warmups=10, rows=rows)
