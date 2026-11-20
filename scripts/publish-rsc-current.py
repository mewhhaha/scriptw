#!/usr/bin/env python3
"""Reduce one complete retained RSC campaign to the published current report.

Reads the report that benchmark-rsc.py wrote for the actors `baseline`,
`candidate` (Fast), `candidate-speed` (the same host under the Speed policy)
and `v8`, and writes the `scriptw-rsc-current-v3` document that
render-application-benchmarks.py --rsc-json draws. Nothing is measured,
compiled or executed here.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import statistics

ACTORS = ('baseline', 'candidate', 'candidate-speed', 'v8')
CASES = ('rsc-sync', 'rsc-async')
POLICIES = {'baseline': 'fast', 'candidate': 'fast', 'candidate-speed': 'speed'}
# As benchmark-rsc.py: the two high-water marks, then resident memory after warmups and at the end.
MEMORY_FIELDS = dict(front_and_emit_high_water='vm_hwm_bytes', compile_high_water='vm_hwm_bytes',
                     post_compile='vm_rss_bytes', steady='vm_rss_bytes')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def summary(values):
    ordered = sorted(values)
    return dict(mean=statistics.mean(values), median=statistics.median(values),
                p95=ordered[math.ceil(len(values) * .95) - 1], minimum=ordered[0], maximum=ordered[-1])


def engine(report, root, actor, label):
    rows = sorted((row for row in report['processes'] if row['actor'] == actor),
                  key=lambda row: (row['block'], row['ordinal']))
    require(len(rows) == 16, 'sixteen processes per actor required: ' + actor)
    raw = json.loads((root / rows[0]['report']).read_text())
    identity = dict(node=rows[0]['node'], inputs=raw['inputs'])
    if actor != 'v8':
        host = report['tools']['hosts'][actor]
        require(report['policy']['actor_native_modes'][actor] == POLICIES[actor], 'unexpected native policy: ' + actor)
        identity = dict(rows[0]['source_identity'], executable_sha256=host['sha256'], native_mode=POLICIES[actor],
                        program_wasm_bytes=raw.get('program_wasm_bytes'), inputs=raw['inputs'])
    identity.update(request_clock=rows[0]['request_clock'], gc_policy=rows[0]['gc_policy'])
    result = dict(label=label, cases={}, warmup_curves_ns={}, identity=identity, case_statistics={},
                  phase_processes=[dict(case=row['case'], block=row['block'], repetition=row['repetition'],
                                        ordinal=row['ordinal'], **row['phases_ns']) for row in rows],
                  whole_process_peak_rss_bytes={}, memory_bytes={})
    for case in CASES:
        selected = [row for row in rows if row['case'] == case]
        require(len(selected) == 8, 'eight processes per case required: ' + actor + '/' + case)
        calls = [value for row in selected for value in row['request_ns']['samples']]
        result['cases'][case] = [row['request_ns']['median'] for row in selected]
        result['warmup_curves_ns'][case] = [row['warmup_curve_ns'] for row in selected]
        result['case_statistics'][case] = dict(
            process_medians_ns=result['cases'][case],
            process_means_ns=[row['request_ns']['mean'] for row in selected],
            process_p95_ns=[row['request_ns']['p95'] for row in selected],
            timed_calls=len(calls), all_timed_calls_summary_ns_dependent=summary(calls),
            raw_calls='Full ordered times and per-call checks retained in the bound raw campaign',
            block_medians_ns={str(block): statistics.median(row['request_ns']['median'] for row in selected
                                                            if row['block'] == block) for block in (0, 1)})
        result['whole_process_peak_rss_bytes'][case] = summary(
            [row['accounting']['peak_rss_bytes'] for row in selected])
        result['memory_bytes'][case] = memory(selected)
    return result


def memory(rows):
    """Scriptw stage memory, or None for V8 and hosts built before the memory split."""
    if any(row.get('memory') is None for row in rows):
        return None
    for row in rows:
        require(row['memory']['compile_high_water']['vm_hwm_bytes'] <= row['accounting']['peak_rss_bytes'],
                'compile high water exceeds the whole-process peak')
    return {stage + '_bytes': summary([row['memory'][stage][field] for row in rows])
            for stage, field in MEMORY_FIELDS.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--campaign', type=Path, required=True, help='benchmark-rsc.py output directory')
    parser.add_argument('--date', required=True, help='measurement date, YYYY-MM-DD')
    parser.add_argument('--baseline-label', required=True)
    parser.add_argument('--cpu-policy', required=True, help='one sentence on CPU, machine and quota scope')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    require(re.fullmatch(r'\d{4}-\d{2}-\d{2}', args.date) is not None, 'date must be YYYY-MM-DD')
    root = args.campaign.resolve(strict=True)
    report_path = root / 'report.json'
    report = json.loads(report_path.read_text())
    require(report['schema'] == 'scriptw-retained-rsc-campaign-v1' and report['status'] == 'complete'
            and report['complete'] is True, 'a complete retained RSC campaign is required')
    policy = report['policy']
    require(policy['blocks'] == 2 and policy['processes_per_block'] == 4
            and policy['case_isolation'] == 'one case per fresh process', 'unexpected campaign shape')
    require({row['actor'] for row in report['processes']} == set(ACTORS), 'unexpected actor inventory')
    samples, warmups = policy['samples'], policy['warmups']
    labels = {'baseline': args.baseline_label, 'candidate': 'Scriptw Fast', 'candidate-speed': 'Scriptw Speed',
              'v8': 'Node / V8'}
    inputs = {name: value for name, value in report['inputs'].items() if name.endswith('_sha256')}
    inputs['campaign_report_sha256'] = digest(report_path)
    for name, value in report['file_sha256'].items():
        if name.startswith('tooling/'):
            inputs[re.sub(r'[^a-z0-9]+', '_', name.removeprefix('tooling/').lower()) + '_sha256'] = value
    model = sorted({line.partition(':')[2].strip() for line in report['machine']['cpuinfo'].splitlines()
                    if line.startswith('model name')})
    current = dict(
        schema='scriptw-rsc-current-v3', date=args.date,
        campaign=dict(path=str(report_path.relative_to(Path.cwd().resolve())), sha256=inputs['campaign_report_sha256'],
                      started_at=report['started_at'], finished_at=report['finished_at']),
        measurement=dict(blocks=2, processes_per_engine_per_case=8, processes_per_engine=16,
            processes_per_block_per_case=4, warmups=warmups, samples=samples, calls_per_process=warmups + samples,
            checked_calls_total=len(report['processes']) * (warmups + samples),
            measured_calls_per_engine_case=8 * samples, warmup_calls_per_engine_case=8 * warmups,
            case_isolation='one case per fresh process', cpu_policy=args.cpu_policy,
            clock='per-request monotonic clock recorded by each retained host'),
        machine=dict(kernel=report['machine']['kernel'], architecture=report['machine']['architecture'],
                     cpu_models=model, resources={key: value for key, value in report['resources'].items()
                                                  if key in ('cloud', 'cgroup', 'affinity')}),
        inputs=inputs,
        engines={actor: engine(report, root, actor, labels[actor]) for actor in ACTORS},
        limitations=dict(
            all_timed_calls_ns='dependent serial calls within fresh processes; process medians are the independent units',
            profiling='no profiling or artifact dumps admitted; phase timings are cold/setup phases',
            memory='Child high-water RSS includes compilation, setup and warmup; neither request-only RSS nor live Wasm heap. '
                   'memory_bytes splits Scriptw processes: VmHWM after front end and emission, VmHWM after native '
                   'preparation, VmRSS after warmups (post_compile) and after the last request (steady); resident '
                   'memory in a compiling process includes compiler memory the allocator retains.',
            producers='Cargo producer logs are retained by the campaign; feature graphs are not compared.'),
        status='complete')
    args.output.write_text(json.dumps(current, indent=1) + '\n')


if __name__ == '__main__':
    main()
