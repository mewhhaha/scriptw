#!/usr/bin/env python3
"""Render the verified execution checkpoint without running JavaScript.

Requires matplotlib. The older startup/history publisher retains its own
corpus and evidence; this graphic uses the complete execution checkpoint.
"""
import argparse
from datetime import datetime, timezone
import io
import json
from pathlib import Path
import sys

from execution_chart_data import load_chart_data

ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / 'docs/rust-rewrite/execution-memory-admission-20261025.json'
OUTPUT = ROOT / 'docs/rust-rewrite/assets/execution-performance.svg'
CURRENT = '#0969da'
V8 = '#d97706'
INK = '#24292f'
MUTED = '#57606a'
GRID = '#e5e7eb'


def render(data, preview=None):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from matplotlib.patches import Patch

    rows = data['rows']
    moment = datetime.fromisoformat(data['measurement_end'].replace('Z', '+00:00'))
    measured = moment.astimezone(timezone.utc).strftime('%Y-%m-%d %H:%M UTC')
    description = (
        f'Latest verified Scriptw versus V8, all {len(rows)} fresh Store/Realm workloads. '
        f'Measured {measured}; compiler {data["source_commit"]}. '
        'Lower is better. Paired bars use a shared zero-based linear scale within each section: '
        'median elapsed milliseconds and median post-execution process RSS in MiB. '
        'RSS includes host/runtime, setup and retained warmup effects; it is not live '
        'JavaScript heap usage or a peak. Each runtime has 14 measured observations '
        'from two rounds of seven after ten warmups. Existing-Realm calls are separate. '
        f'Source report SHA-256: {data["source_report_sha256"]}. '
        f'Archive SHA-256: {data["archive_sha256"]}. '
    )
    for row in rows:
        description += (
            f'{row["file"]}: V8 {row["v8_ms"]:.3f} ms, '
            f'Scriptw {row["after_ms"]:.3f} ms; '
            f'V8 RSS {row["v8_rss_mib"]:.1f} MiB, '
            f'Scriptw RSS {row["scriptw_rss_mib"]:.1f} MiB. '
        )

    settings = {
        'font.family': 'DejaVu Sans', 'font.size': 10, 'text.color': INK,
        'axes.labelcolor': MUTED, 'xtick.color': MUTED, 'ytick.color': INK,
        'svg.fonttype': 'none', 'svg.hashsalt': 'scriptw-latest-v8-bars-v1',
        'figure.facecolor': 'white', 'savefig.facecolor': 'white',
    }
    with plt.rc_context(settings):
        fig = plt.figure(figsize=(14, 19))
        try:
            fig.text(.045, .978, 'Scriptw vs V8', fontsize=22, weight='bold')
            fig.text(.045, .953, 'Lower is better · shared linear scale within each section',
                     fontsize=12, color=MUTED)
            fig.text(.045, .934, f'{measured} · compiler {data["source_commit"][:9]} · '
                     '14 samples · 10 execution warmups per round', fontsize=10, color=MUTED)
            fig.legend(handles=[Patch(color=V8, label='Node / V8'),
                                Patch(color=CURRENT, label='Scriptw / Wasmtime')],
                       loc='upper right', bbox_to_anchor=(.96, .991), frameon=False,
                       ncol=2, columnspacing=2, fontsize=11)

            def panel(rect, title, fields, unit, decimals):
                ax = fig.add_axes(rect)
                ax.set_ylim(len(rows) - .5, -.5)
                ax.set_yticks(range(len(rows)), [r['file'] for r in rows], fontsize=10)
                ax.tick_params(axis='y', length=0, pad=12)
                ax.tick_params(axis='x', length=0, pad=8)
                for spine in ax.spines.values():
                    spine.set_visible(False)
                ax.set_axisbelow(True)
                ax.grid(axis='x', color=GRID, linewidth=.8)
                maximum = max(row[field] for row in rows for field in fields)
                ax.set_xlim(0, maximum * 1.22)
                ax.axvline(0, color=GRID, linewidth=1)
                fig.text(.045, rect[1] + rect[3] + .017, title,
                         fontsize=11, weight='bold', color=MUTED)
                for i, row in enumerate(rows):
                    for field, color, offset in ((fields[0], V8, -.20),
                                                  (fields[1], CURRENT, .20)):
                        value = row[field]
                        ax.barh(i + offset, value, height=.28, color=color, linewidth=0)
                        ax.annotate(f'{value:.{decimals}f} {unit}', (value, i + offset),
                                    xytext=(5, 0), textcoords='offset points', va='center',
                                    fontsize=9, color=color)
                ax.set_xlabel(unit, labelpad=11, fontsize=10)

            panel([.25, .55, .67, .345], 'ELAPSED TIME · ms · median',
                  ('v8_ms', 'after_ms'), 'ms', 3)
            fig.text(.045, .504,
                     'Execution: fresh Store / V8 context; construction, checksums and completion included.',
                     fontsize=10, color=MUTED)
            panel([.25, .115, .67, .345], 'PROCESS MEMORY AFTER EXECUTION · MiB · median RSS',
                  ('v8_rss_mib', 'scriptw_rss_mib'), 'MiB', 1)
            fig.text(.045, .067,
                     'RSS includes host/runtime, setup and retained warmup effects; it is not live JavaScript heap or peak RSS.',
                     fontsize=10, color=MUTED)
            fig.text(.045, .047,
                     'Two rounds of seven measurements after ten warmups per runtime · one CPU · 4 GiB cap',
                     fontsize=10, color=MUTED)
            fig.text(.045, .027,
                     'Shared-host measurements are descriptive; setup/compilation and existing-Realm calls are recorded separately.',
                     fontsize=10, color=MUTED)
            svg = io.BytesIO()
            fig.savefig(svg, format='svg', metadata={
                'Title': 'Latest Scriptw versus V8: elapsed time and process memory',
                'Description': description, 'Date': None,
                'Creator': 'scripts/render-execution-performance.py',
            })
            if preview:
                Path(preview).parent.mkdir(parents=True, exist_ok=True)
                fig.savefig(preview, format='png', dpi=100)
            # Newlines already separate Matplotlib's multiline path coordinates.
            return b'\n'.join(line.rstrip() for line in svg.getvalue().splitlines()) + b'\n'
        finally:
            plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, default=REPORT)
    parser.add_argument('--output', type=Path, default=OUTPUT)
    parser.add_argument('--preview', type=Path, help='optional local PNG for visual inspection')
    parser.add_argument('--check', action='store_true', help='validate data and reject a stale SVG')
    args = parser.parse_args()
    try:
        data = load_chart_data(args.report)
        svg = render(data, None if args.check else args.preview)
        if args.check:
            if not args.output.is_file() or args.output.read_bytes() != svg:
                raise ValueError('execution graph is stale; regenerate it from the verified checkpoint')
        else:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_bytes(svg)
        print(json.dumps({'checked': args.check, 'source_commit': data['source_commit'],
                          'workloads': len(data['rows']), 'output': str(args.output)}))
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(f'execution chart: {error}', file=sys.stderr)
        raise SystemExit(1)


if __name__ == '__main__':
    main()
