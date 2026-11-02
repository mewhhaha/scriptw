"""Small admission and reduction helpers for synchronous application benchmarks."""
import json
import re
from pathlib import Path
import statistics

from benchmark_metrics import digest

SCHEMA = 'scriptw-application-benchmarks-v1'
ACTORS = ('scriptw', 'scriptc', 'v8')
SAMPLES, WARMUPS = 7, 3
LETTERS = 'ABCBCACABACB'


def require(condition, message):
    if not condition:
        raise ValueError('application benchmark: ' + message)


def checked_product_stdout(actual, expected):
    """The product CLI appends its real backend outcome after guest output."""
    require(actual.startswith(expected), 'product guest output differs')
    outcome = actual[len(expected):]
    require(re.fullmatch(r'run outcome: RunOutcome \{ backend_used: WasmAot, note: "(?:[^"\\]|\\.)*" \}\n?', outcome)
            is not None, 'missing normal Wasm-AOT CLI outcome or extra output')
    return True


def validate_manifest(manifest):
    cases = manifest.get('execution')
    require(isinstance(cases, list) and bool(cases), 'nonempty execution inventory required')
    names = set()
    for case in cases:
        name = case.get('file')
        require(isinstance(name, str) and Path(name).name == name and name.endswith('.js')
                and '\\' not in name and ':' not in name and name not in names, 'unsafe or duplicate case filename')
        names.add(name)
        require(isinstance(case.get('stdout'), str), 'expected stdout required')
        for field in ('label', 'category', 'scope'):
            require(isinstance(case.get(field), str) and bool(case[field].strip()), field + ' required')
        require(type(case.get('operations')) is int and case['operations'] > 0, 'positive operations required')
    return cases


def process_plan(names, available):
    require(bool(names) and len(names) == len(set(names)), 'distinct cases required')
    require(set(available) == set(names), 'availability inventory differs')
    rows = []
    for block in range(2):
        ordered, letters = list(enumerate(names)), LETTERS
        if block == 1:
            ordered.reverse()
            letters = letters[::-1]
        for index, name in ordered:
            require(len(available[name]) == len(set(available[name]))
                    and set(available[name]) <= set(ACTORS), 'unknown or repeated actor')
            offset = index % len(ACTORS)
            actors = ACTORS[offset:] + ACTORS[:offset]
            position = 0
            for letter in letters:
                actor = actors['ABC'.index(letter)]
                if actor in available[name]:
                    rows.append(dict(block=block, case=name, position=position, actor=actor))
                    position += 1
    return rows


def checked_identity(report, identity):
    for field in ('source_fingerprint', 'executable_sha256', 'shared_configuration'):
        require(report.get(field) == identity[field], 'consumer identity differs: ' + field)
    require(report.get('comparison_cohort') == 'scriptw-precompiled-comparison-v1'
            and report.get('excluded_from_ordinary_cohorts') is True, 'comparison cohort differs')


def checked_observations(report, actor, case, source_sha, identity, image, node):
    require(actor in ACTORS, 'unknown actor')
    require(report.get('samples') == SAMPLES and report.get('warmups') == WARMUPS, 'sample counts differ')
    if actor == 'v8':
        require(report.get('schema') == 'scriptw-v8-baseline-v2'
                and report.get('node') == 'v' + node['versions']['node']
                and report.get('v8') == node['versions']['v8'], 'V8 identity differs')
        require(len(report.get('cases', [])) == 1, 'one V8 case required')
        value = report['cases'][0]
        require(value.get('file') == case['file'] and value.get('source_sha256') == source_sha,
                'V8 source identity differs')
        require(type(value.get('compilation_ns')) is int and value['compilation_ns'] >= 0,
                'missing V8 source compilation span')
        observations = value['observations']
    else:
        checked_identity(report, identity)
        schema = 'scriptw-precompiled-execution-v1'
        wasm_key = 'program_wasm_sha256'
        if actor == 'scriptc':
            schema = 'scriptw-precompiled-wasi-execution-v1'
            wasm_key = 'wasm_sha256'
        require(report.get('schema') == schema and report.get('compiler_enabled') is False,
                'consumer enabled compiler or schema differs')
        require(report.get('image_sha256') == image['image_sha256']
                and report.get(wasm_key) == image[wasm_key], 'consumer image differs')
        if actor == 'scriptw':
            require(report.get('runtime_key') == image['runtime_key'], 'runtime key differs')
        observations = report['observations']
    require(len(observations) == SAMPLES + WARMUPS, 'missing observations')
    require([row.get('warmup') for row in observations] == [True] * WARMUPS + [False] * SAMPLES,
            'warmup order differs')
    wall = []
    context = []
    for row in observations:
        require(row.get('stdout') == case['stdout'], 'output differs, including warmup')
        if actor == 'v8':
            elapsed = row.get('execution_ns')
            require(type(row.get('context_ns')) is int and row['context_ns'] > 0, 'missing context span')
        else:
            require(row.get('matched') is True and row.get('completion') == 'normal'
                    and row.get('stderr') == '', 'failed consumer completion')
            elapsed = row['phases_ns'].get('execution')
        require(type(elapsed) is int and elapsed > 0, 'missing entry wall span')
        if not row['warmup']:
            wall.append(elapsed)
            if actor == 'v8':
                context.append(row['context_ns'])
    result = dict(entry_wall_ns=wall)
    if actor == 'v8':
        result.update(context_ns=context, source_compilation_ns=value['compilation_ns'])
    else:
        result['load_phases_ns'] = report['load_phases_ns']
    return result


def successful_process(resources):
    return resources.get('exit_code') == 0 and resources.get('timed_out') is False


def summarize(report):
    names = [case['file'] for case in report['workloads']]
    available = report['plan']['available']
    require(set(report['preparations']) == set(names), 'preparation case inventory differs')
    for name in names:
        require(set(report['preparations'][name]) == set(ACTORS), 'full three-actor matrix required')
        expected = []
        for actor in ACTORS:
            preparation = report['preparations'][name][actor]
            require(preparation['status'] in ('prepared', 'failed'), 'unknown preparation status')
            if preparation['status'] == 'prepared':
                expected.append(actor)
            else:
                require(isinstance(preparation.get('reason'), str) and bool(preparation['reason'])
                        and isinstance(preparation.get('stage'), str) and bool(preparation['stage']),
                        'failed preparation needs stage and reason')
        require(available[name] == expected, 'available actors differ from preparation matrix')
    plan = process_plan(names, available)
    rows = report['rows']
    require([{key: row[key] for key in ('block', 'case', 'position', 'actor')} for row in rows] == plan,
            'process inventory or order differs')
    result = []
    for name in names:
        actors = {}
        for actor in ACTORS:
            preparation = report['preparations'][name][actor]
            group = [row for row in rows if row['case'] == name and row['actor'] == actor]
            failures = [row for row in group if row['status'] != 'sampled']
            for row in failures:
                require(row['status'] == 'failed' and bool(row.get('reason'))
                        and row.get('stage') == 'execution', 'execution failure needs stage and reason')
            if preparation['status'] != 'prepared' or failures:
                reason = preparation.get('reason')
                stage = preparation.get('stage')
                if failures:
                    reason, stage = failures[0]['reason'], failures[0]['stage']
                actors[actor] = dict(status='failed', stage=stage, reason=reason,
                                     attempted_processes=len(group), successful_processes=len(group) - len(failures))
                continue
            require(len(group) == 8, 'eight fresh processes required per available actor')
            for row in group:
                require(successful_process(row['resources']), 'failed process labeled sampled')
                require(len(row['entry_wall_ns']) == SAMPLES
                        and all(type(n) is int and n > 0 for n in row['entry_wall_ns']), 'entry samples differ')
                require(type(row['resources'].get('peak_rss_bytes')) is int
                        and row['resources']['peak_rss_bytes'] > 0, 'missing process RSS')
            actors[actor] = dict(status='sampled', processes=8,
                entry_wall_ns=statistics.median(statistics.median(row['entry_wall_ns']) for row in group),
                process_wall_ns=statistics.median(row['resources']['elapsed_ns'] for row in group),
                process_peak_rss_bytes=statistics.median(row['resources']['peak_rss_bytes'] for row in group),
                blocks=[dict(block=block,
                    entry_wall_ns=statistics.median(statistics.median(row['entry_wall_ns']) for row in group if row['block'] == block),
                    process_peak_rss_bytes=statistics.median(row['resources']['peak_rss_bytes'] for row in group if row['block'] == block))
                    for block in range(2)])
        result.append(dict(file=name, actors=actors))
    return result


def load_report(path):
    path = Path(path)
    report = json.loads(path.read_text())
    require(report.get('schema') == SCHEMA and report.get('status') == 'complete', 'incomplete report')
    validate_manifest(dict(execution=report['workloads']))
    require(report['summary'] == summarize(report), 'summary differs from raw samples')
    require(report['inputs_before'] == report['inputs_after'], 'source or tools changed')
    retained = set()
    for item in report['retained_files']:
        require(item['path'] not in retained, 'duplicate retained evidence')
        retained.add(item['path'])
        member = (path.parent / item['path']).resolve()
        require(member.is_relative_to(path.parent.resolve()) and member.is_file()
                and member.stat().st_size == item['bytes'] and digest(member) == item['sha256'],
                'retained evidence changed or unsafe path')
    by_name = {case['file']: case for case in report['workloads']}
    for row in report['rows']:
        require(row['stdout'] in retained and row['stderr'] in retained, 'raw execution logs missing')
        require(digest(path.parent / row['stdout']) == row['stdout_sha256']
                and digest(path.parent / row['stderr']) == row['stderr_sha256'], 'raw execution identity differs')
        if row['status'] != 'sampled':
            continue
        case, actor = by_name[row['case']], row['actor']
        source = path.parent / 'sources' / case['file']
        image = report['preparations'][case['file']][actor].get('image')
        raw = json.loads((path.parent / row['stdout']).read_text())
        values = checked_observations(raw, actor, case, digest(source), report['consumer_identity'],
                                      image, report['node_identity'])
        require(all(row.get(key) == value for key, value in values.items()), 'derived execution data differs from raw log')
        require(not (path.parent / row['stderr']).read_bytes(), 'sampled execution has stderr')
    return report
