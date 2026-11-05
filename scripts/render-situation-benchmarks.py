#!/usr/bin/env python3
"""Render retained-situation benchmark cards from inert report JSON."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = 'scriptw-situation-benchmarks-v1'
ACTORS = ('scriptw-fast', 'scriptw-speed', 'v8')
ACTOR_LABELS = {'scriptw-fast': 'Scriptw Fast', 'scriptw-speed': 'Scriptw Speed',
                'v8': 'Node / V8'}
ACTOR_COLORS = {'scriptw-fast': '#3478a8', 'scriptw-speed': '#4a9a79',
                'v8': '#d17b36'}
CATEGORIES = (
    ('strict equality', 'Dynamic strict equality', 'Retained public call · ms', ('strict-equality-numbers.js', 'strict-equality-mixed.js')),
    ('enumeration', 'For-in enumeration', 'Retained public call · ms', ('for-in-wide.js',)),
    ('type tests', 'Type classification', 'Retained public call · ms', ('typeof-dispatch.js', 'singleton-dispatch.js')),
    ('control', 'Control', 'Retained public call · ms', ('control-empty.js',)),
    ('global lookup', 'Global lookup', 'Retained public call · ms',
     ('global-shallow.js', 'global-deep8.js', 'global-bound-deep.js', 'global-getter-deep.js')),
    ('frame size', 'Control-flow / call-site scaling', 'Retained public call · ms',
     ('frame-compact.js', 'frame-branches32.js', 'frame-branches128.js', 'frame-branches512.js')),
    ('arguments observation', 'Arguments observation', 'Retained public call · ms',
     ('args-unused.js', 'args-observed.js')),
    ('property mutation history', 'Property mutation history', 'Retained public call · ms',
     ('property-stable.js', 'property-mutated.js')),
    ('function properties', 'Function property reads', 'Retained public call · ms',
     ('metadata-ordinary.js', 'metadata-function.js', 'metadata-intrinsic.js')),
)
SHORT_LABELS = {
    'strict-equality-numbers.js': 'Number pairs',
    'strict-equality-mixed.js': 'Mixed value pairs',
    'for-in-wide.js': '64 own keys · 16 passes',
    'typeof-dispatch.js': 'Mixed typeof dispatch',
    'singleton-dispatch.js': 'Strict null / undefined',
    'control-empty.js': 'Empty call',
    'global-shallow.js': 'Shallow', 'global-deep8.js': 'Deep · 8 captures',
    'global-bound-deep.js': 'Bound deep', 'global-getter-deep.js': 'Getter deep',
    'frame-compact.js': '1 source site', 'frame-branches32.js': '32 source sites',
    'frame-branches128.js': '128 source sites', 'frame-branches512.js': '512 source sites',
    'args-unused.js': 'Arguments unused', 'args-observed.js': 'Arguments observed',
    'property-stable.js': 'Stable properties', 'property-mutated.js': 'After mutation',
    'metadata-ordinary.js': 'Ordinary records', 'metadata-function.js': 'JS functions',
    'metadata-intrinsic.js': 'Native constructors',
}
FRAME_CASES = ('frame-compact.js', 'frame-branches32.js', 'frame-branches128.js', 'frame-branches512.js')


def require(condition, message):
    if not condition:
        raise ValueError('situation charts: ' + message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(report):
    require(report.get('schema') == SCHEMA, 'foreign situation report')
    require(report.get('status') in ('complete', 'failed', 'incomplete'), 'unknown report status')
    policy = report.get('policy', {})
    require(policy.get('samples') == 21 and policy.get('warmups') == 100
            and policy.get('blocks') == 2 and policy.get('processes_per_actor_per_block') == 4,
            'measurement cohort must be 21 samples, 100 warmups and 8 processes')
    require(policy.get('actors') == list(ACTORS), 'three-actor inventory differs')
    cases = report.get('cases', [])
    require(isinstance(cases, list), 'case inventory is not a list')
    names = [case.get('file') for case in cases]
    require(len(names) == len(set(names)), 'duplicate case')
    expected = {name for _, _, _, group in CATEGORIES for name in group}
    require(set(names) <= expected, 'unknown case in situation inventory')
    for case in cases:
        require(case.get('category') in {row[0] for row in CATEGORIES}, 'unknown case category')
        runtimes = case.get('runtimes', {})
        require(isinstance(runtimes, dict) and set(runtimes) == set(ACTORS),
                'case must retain all three actor results')
        for actor in ACTORS:
            runtime = runtimes.get(actor)
            if runtime is None:
                continue
            require(runtime.get('status') in ('ok', 'complete', 'failed', 'incomplete', 'unavailable'),
                    'unknown runtime status')
            if runtime['status'] in ('ok', 'complete'):
                medians = runtime.get('process_medians_ns')
                require(isinstance(medians, list) and len(medians) == 8
                        and all(type(value) is int and value > 0 for value in medians),
                        'successful runtime needs eight positive process medians')
                require(runtime.get('range_ns') == [min(medians), max(medians)]
                        and runtime.get('median_ns') == statistics.median(medians),
                        'runtime median or process range differs')
    return cases


def chart_context():
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    return matplotlib, plt


def svg_bytes(fig, title, report, report_sha, source_hashes):
    import io
    matplotlib, _ = chart_context()
    metadata = dict(schema=SCHEMA, report_sha256=report_sha,
                    source_commit=report.get('source_identity', {}).get('source_commit'),
                    source_fingerprint=report.get('source_identity', {}).get('source_fingerprint'),
                    source_sha256=source_hashes, report_status=report['status'],
                    samples=report['policy']['samples'], warmups=report['policy']['warmups'],
                    matplotlib=matplotlib.__version__)
    output = io.BytesIO()
    fig.savefig(output, format='svg', facecolor='white', metadata={
        'Date': None, 'Title': title, 'Creator': 'Scriptw situation charts',
        'Description': json.dumps(metadata, sort_keys=True, ensure_ascii=False)})
    return b'\n'.join(line.rstrip() for line in output.getvalue().splitlines()) + b'\n'


def apply_style(plt, salt):
    return plt.rc_context({'font.family': 'DejaVu Sans', 'font.size': 10,
                           'svg.fonttype': 'none', 'svg.hashsalt': salt,
                           'text.usetex': False})


def runtime_chart(report, report_sha, cases, category, title, ylabel, preview=None):
    matplotlib, plt = chart_context()
    lookup = {case['file']: case for case in cases if case.get('category') == category}
    ordered = [lookup[name] for name in next(group for key, _, _, group in CATEGORIES if key == category)
               if name in lookup]
    height = max(3.8, 1.2 + .82 * len(ordered))
    with apply_style(plt, 'scriptw-situations-' + category.replace(' ', '-')):
        fig, ax = plt.subplots(figsize=(9.2, height))
        try:
            width = .22
            centers = list(range(len(ordered)))
            highs = []
            for actor_index, actor in enumerate(ACTORS):
                positions = [center + (actor_index - 1) * width for center in centers]
                for case, x in zip(ordered, positions, strict=True):
                    runtime = case.get('runtimes', {}).get(actor)
                    if runtime and runtime.get('status') in ('ok', 'complete'):
                        value = runtime['median_ns'] / 1e6
                        low, high = (bound / 1e6 for bound in runtime['range_ns'])
                        ax.bar(x, value, width, color=ACTOR_COLORS[actor], zorder=3)
                        ax.errorbar(x, value, yerr=[[value - low], [high - value]], fmt='none',
                                    ecolor='#444b52', elinewidth=1, capsize=3, zorder=4)
                        ax.annotate(f'{value:.3g}', (x, high), xytext=(0, 4),
                                    textcoords='offset points', ha='center', va='bottom', fontsize=8)
                        highs.append(high)
                    else:
                        status = runtime.get('status', 'unavailable') if runtime else 'unavailable'
                        ax.text(x, 0, status, ha='center', va='bottom', rotation=90,
                                fontsize=7, color='#666b70', clip_on=False)
            ax.set_xticks(centers, [SHORT_LABELS.get(case['file'], case['label']) for case in ordered])
            ax.set_ylabel(ylabel)
            prefix = 'INCOMPLETE · ' if report['status'] != 'complete' else ''
            ax.set_title(prefix + title, loc='left', fontweight='bold', pad=13)
            ax.set_axisbelow(True)
            ax.grid(axis='y', color='#eaeef2')
            handles = [plt.Rectangle((0, 0), 1, 1, color=ACTOR_COLORS[actor]) for actor in ACTORS]
            ax.legend(handles, [ACTOR_LABELS[actor] for actor in ACTORS], frameon=False,
                      ncol=3, loc='upper left')
            ax.set_ylim(bottom=0, top=(max(highs) * 1.3 if highs else 1.0))
            for side in ('top', 'right'):
                ax.spines[side].set_visible(False)
            ax.tick_params(length=0, pad=7)
            fig.subplots_adjust(left=.12, right=.98, top=.84, bottom=.16)
            if preview:
                preview.parent.mkdir(parents=True, exist_ok=True)
                fig.savefig(preview, dpi=110, facecolor='white')
            source_hashes = {case['file']: case.get('source_sha256') for case in ordered}
            return svg_bytes(fig, title, report, report_sha, source_hashes)
        finally:
            plt.close(fig)


def frame_image_chart(report, report_sha, preview=None):
    matplotlib, plt = chart_context()
    receipts = {}
    for preparation in report.get('preparations', []):
        actor = preparation.get('actor')
        if actor not in ('scriptw-fast', 'scriptw-speed'):
            continue
        for case in preparation.get('cases', []):
            if case.get('file') in FRAME_CASES and case.get('status', 'ok') == 'ok':
                receipts.setdefault(case['file'], {})[actor] = case
    labels = ('Fast native P image', 'Speed native P image', 'Portable Wasm P')
    colors = (ACTOR_COLORS['scriptw-fast'], ACTOR_COLORS['scriptw-speed'], '#9aa4ad')
    with apply_style(plt, 'scriptw-situations-frame-native-image'):
        fig, ax = plt.subplots(figsize=(9.2, 4.6))
        try:
            centers = list(range(len(FRAME_CASES)))
            width = .22
            fields = ('native_serialized_bytes', 'native_serialized_bytes', 'program_wasm_bytes')
            actors = ('scriptw-fast', 'scriptw-speed', 'scriptw-fast')
            highs = []
            for index, (label, color, field, actor) in enumerate(zip(labels, colors, fields, actors, strict=True)):
                positions = [center + (index - 1) * width for center in centers]
                for name, x in zip(FRAME_CASES, positions, strict=True):
                    receipt = receipts.get(name, {}).get(actor)
                    value = receipt.get(field) if receipt else None
                    if type(value) is int and value > 0:
                        kib = value / 1024
                        ax.bar(x, kib, width, color=color, zorder=3)
                        ax.annotate(f'{kib:.0f}', (x, kib), xytext=(0, 4),
                                    textcoords='offset points', ha='center', va='bottom', fontsize=8)
                        highs.append(kib)
                    else:
                        ax.text(x, 0, 'unavailable', ha='center', va='bottom', rotation=90,
                                fontsize=7, color='#666b70', clip_on=False)
            handles = [plt.Rectangle((0, 0), 1, 1, color=color) for color in colors]
            ax.legend(handles, labels, frameon=False, ncol=3, loc='upper left')
            ax.set_xticks(centers, [SHORT_LABELS[name] for name in FRAME_CASES])
            ax.set_ylabel('Serialized bytes · KiB')
            prefix = 'INCOMPLETE · ' if report['status'] != 'complete' else ''
            ax.set_title(prefix + 'Call-site scaling · native P image sizes', loc='left',
                         fontweight='bold', pad=13)
            ax.text(0, 1.015, 'Native P image includes metadata and data; Wasm P is the portable program.',
                    transform=ax.transAxes, ha='left', va='bottom', fontsize=8, color='#555b61')
            ax.set_axisbelow(True)
            ax.grid(axis='y', color='#eaeef2')
            ax.set_ylim(bottom=0, top=max(highs) * 1.3 if highs else 1.0)
            for side in ('top', 'right'):
                ax.spines[side].set_visible(False)
            ax.tick_params(length=0, pad=7)
            fig.subplots_adjust(left=.12, right=.98, top=.82, bottom=.16)
            if preview:
                preview.parent.mkdir(parents=True, exist_ok=True)
                fig.savefig(preview, dpi=110, facecolor='white')
            hashes = {case['file']: case.get('source_sha256') for case in report.get('cases', [])
                      if case.get('file') in FRAME_CASES}
            return svg_bytes(fig, 'Frame case native P image sizes', report, report_sha, hashes)
        finally:
            plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', required=True, type=Path)
    parser.add_argument('--output-dir', required=True, type=Path)
    parser.add_argument('--preview-dir', type=Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    report_path = args.report.resolve(strict=True)
    report = json.loads(report_path.read_text())
    cases = validate(report)
    report_sha = digest(report_path)
    outputs = {}
    for category, title, ylabel, _ in CATEGORIES:
        name = category.replace(' ', '-') + '.svg'
        preview = args.preview_dir / (name[:-4] + '.png') if args.preview_dir else None
        outputs[args.output_dir / name] = runtime_chart(
            report, report_sha, cases, category, title, ylabel, preview)
    preview = args.preview_dir / 'frame-native-image.png' if args.preview_dir else None
    outputs[args.output_dir / 'frame-native-image.svg'] = frame_image_chart(report, report_sha, preview)
    for path, content in outputs.items():
        if args.check:
            require(path.is_file() and path.read_bytes() == content,
                    'generated output differs: ' + str(path))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
    if args.check:
        print(f'Verified {len(outputs)} situation benchmark charts')
        return
    print(f'Wrote {len(outputs)} situation benchmark charts to {args.output_dir}')


if __name__ == '__main__':
    main()
