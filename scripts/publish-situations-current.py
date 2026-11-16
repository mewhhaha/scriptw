#!/usr/bin/env python3
"""Reduce one complete focused-situation campaign to the published current report.

`publish` reads the report.json that benchmark-situations.py wrote for the actors
scriptw-fast, scriptw-speed and v8 over the whole generated inventory, re-checks
its process reductions, and writes the document that
render-situation-benchmarks.py --report draws. It drops the per-command log,
raw per-invocation rows and retained-file listing, which stay in the campaign
directory. `tables` prints the Markdown that benchmarks.md quotes from a
published report. Nothing is measured, compiled or executed here.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import statistics

SCHEMA = 'scriptw-situation-benchmarks-v1'
ACTORS = ['scriptw-fast', 'scriptw-speed', 'v8']
SAMPLES, WARMUPS, PROCESSES = 21, 100, 8
FRAME_CASES = ('frame-compact.js', 'frame-branches32.js', 'frame-branches128.js', 'frame-branches512.js')
DROPPED = ('rows', 'commands', 'retained_files')


def require(condition, message):
    if not condition:
        raise ValueError('situation publication: ' + message)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def check_runtime(runtime, label):
    require(runtime.get('status') == 'complete', 'incomplete runtime: ' + label)
    processes = runtime['processes']
    require(len(processes) == PROCESSES, 'eight processes required: ' + label)
    medians = [row['median_ns'] for row in processes]
    require(all(len(row['samples_ns']) == SAMPLES and row['median_ns'] == statistics.median(row['samples_ns'])
                for row in processes), 'process median differs: ' + label)
    require(runtime['process_medians_ns'] == medians and runtime['median_ns'] == statistics.median(medians)
            and runtime['range_ns'] == [min(medians), max(medians)], 'runtime reduction differs: ' + label)


def publish(args):
    require(re.fullmatch(r'\d{4}-\d{2}-\d{2}', args.date) is not None, 'date must be YYYY-MM-DD')
    root = args.campaign.resolve(strict=True)
    report_path = root / 'report.json'
    report = json.loads(report_path.read_text())
    require(report.get('schema') == SCHEMA and report.get('status') == 'complete'
            and report.get('complete') is True, 'a complete situation campaign is required')
    policy = report['policy']
    require(policy['actors'] == ACTORS and policy['samples'] == SAMPLES and policy['warmups'] == WARMUPS
            and policy['blocks'] * policy['processes_per_actor_per_block'] == PROCESSES,
            'unexpected campaign shape')
    manifest = json.loads((root / 'sources/manifest.json').read_text())
    inventory = [case['file'] for case in manifest['execution']]
    require([case['file'] for case in report['cases']] == inventory
            and [case['file'] for case in report['workloads']] == inventory,
            'the published panel needs every generated program, not a --case selection')
    for case in report['cases']:
        for actor in ACTORS:
            check_runtime(case['runtimes'][actor], actor + '/' + case['file'])
    rows = report['rows']
    require(len(rows) == len(inventory) * len(ACTORS) * PROCESSES
            and all(row['status'] == 'ok' for row in rows), 'consumer process inventory differs')
    require(all(item['status'] == 'ok' for item in report['preparations'])
            and sorted(item['actor'] for item in report['preparations']) == ACTORS[:2],
            'one successful whole-inventory preparation per native policy required')
    current = {key: value for key, value in report.items() if key not in DROPPED}
    current['publication'] = dict(
        status='published', date=args.date, cpu_policy=args.cpu_policy,
        campaign=dict(path=str(report_path.relative_to(Path.cwd().resolve()) if report_path.is_relative_to(Path.cwd().resolve())
                               else report_path), sha256=digest(report_path)),
        omitted_from_publication=dict(fields=list(DROPPED),
            statement='per-command log, raw per-invocation observations and retained-file listing remain in the campaign directory'),
        consumer_processes=len(rows), checked_invocations=len(rows) * (SAMPLES + WARMUPS),
        width_panel='for-in-width-current.json is a separate historical diagnostic and was not rerun by this campaign')
    args.output.write_text(json.dumps(current, indent=1) + '\n')


def ms(value_ns):
    return f'{value_ns / 1e6:g}'


def tables(args):
    report = json.loads(args.report.read_text())
    require(report.get('schema') == SCHEMA and report.get('status') == 'complete', 'complete published report required')
    started = report['started_at'][:10]
    publication = report.get('publication', {})
    date = publication.get('date', started)
    source = report['source_identity']['source_fingerprint'][:12]
    lines = [f'Measured **{date}** on source `{source}`. All {len(report["cases"])} programs use two balanced '
             'blocks and eight fresh processes per actor. Each program compares Fast, Speed and V8.', '',
             '| Program · retained call | Fast · ms (median; min–max) | Speed · ms (median; min–max) '
             '| V8 · ms (median; min–max) |', '| --- | ---: | ---: | ---: |']
    for case in report['cases']:
        cells = []
        for actor in ACTORS:
            runtime = case['runtimes'][actor]
            low, high = runtime['range_ns']
            cells.append(f'{ms(runtime["median_ns"])} ({ms(low)}–{ms(high)})')
        lines.append(f'| `{case["file"]}` | ' + ' | '.join(cells) + ' |')
    receipts = {(item['actor'], case['file']): case for item in report['preparations'] for case in item['cases']}
    lines += ['', '| Retained program | Fast native P · KiB | Speed native P · KiB | Portable Wasm P · KiB |',
              '| --- | ---: | ---: | ---: |']
    for name in FRAME_CASES:
        fast, speed = receipts[('scriptw-fast', name)], receipts[('scriptw-speed', name)]
        lines.append(f'| `{name}` | {fast["native_serialized_bytes"] / 1024:.1f} | '
                     f'{speed["native_serialized_bytes"] / 1024:.1f} | {fast["program_wasm_bytes"] / 1024:.1f} |')
    widest = 'frame-branches512.js'
    fast, speed = receipts[('scriptw-fast', widest)], receipts[('scriptw-speed', widest)]
    mib = 1024 * 1024
    processes = sum(len(runtime['processes']) for case in report['cases'] for runtime in case['runtimes'].values())
    lines += ['', f'The current 512-site native P image is **{fast["native_serialized_bytes"] / mib:.2f} MiB Fast / '
              f'{speed["native_serialized_bytes"] / mib:.2f} MiB Speed**; portable P is '
              f'**{fast["program_wasm_bytes"] / mib:.2f} MiB**.', '',
              f'The {date} situation cohort has {processes} process observations and '
              f'{processes * (SAMPLES + WARMUPS):,} exact calls.']
    print('\n'.join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    command = commands.add_parser('publish', help='reduce a complete campaign to the published report')
    command.add_argument('--campaign', type=Path, required=True, help='benchmark-situations.py output directory')
    command.add_argument('--date', required=True, help='measurement date, YYYY-MM-DD')
    command.add_argument('--cpu-policy', required=True, help='one sentence on CPU, machine and quota scope')
    command.add_argument('--output', type=Path, required=True)
    command.set_defaults(run=publish)
    command = commands.add_parser('tables', help='print the benchmarks.md tables of a published report')
    command.add_argument('--report', type=Path, required=True)
    command.set_defaults(run=tables)
    args = parser.parse_args()
    args.run(args)


if __name__ == '__main__':
    main()
