#!/usr/bin/env python3
"""Render the verified execution checkpoint without running JavaScript.

Requires matplotlib. The older startup/history publisher retains its own
corpus and evidence; this graphic uses the complete execution checkpoint.
"""
import argparse
from datetime import datetime, timezone
import io
import json
import math
from pathlib import Path
import sys

from execution_chart_data import load_chart_data

ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / 'docs/rust-rewrite/execution-performance-20261025.json'
OUTPUT = ROOT / 'docs/rust-rewrite/assets/execution-performance.svg'
BEFORE = '#6b7280'
CURRENT = '#0969da'
V8 = '#d97706'
INK = '#24292f'
MUTED = '#57606a'
GRID = '#e5e7eb'


def render(data, preview=None):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from matplotlib.lines import Line2D
    from matplotlib.ticker import FuncFormatter, LogLocator, NullLocator

    rows = data['rows']
    moment = datetime.fromisoformat(data['measurement_end'].replace('Z', '+00:00'))
    measured = moment.astimezone(timezone.utc).strftime('%Y-%m-%d %H:%M UTC')
    description = (
        f'Fresh Store/Realm execution, all {len(rows)} checked workloads. '
        f'Measured {measured}; Scriptw {data["source_commit"]}; '
        f'baseline {data["baseline_commit"]} with its archived patch. '
        'Lower is better. Time uses a logarithmic axis and paired current-round V8 samples. '
        'Memory compares Scriptw sampled process RSS using a linear axis; '
        'it includes runtime/setup and retained warmup effects, not live JavaScript bytes '
        'or a whole-child peak. Both panels use medians of 14 observations per product. '
        'Existing-Realm calls are a separate cohort. Shared-host observations do not '
        'establish statistical significance or V8 parity. '
        f'Source report SHA-256: {data["source_report_sha256"]}. '
        f'Archive SHA-256: {data["archive_sha256"]}. '
    )
    for row in rows:
        description += (
            f'{row["file"]}: before {row["before_ms"]:.3f} ms, '
            f'now {row["after_ms"]:.3f} ms, V8 {row["v8_ms"]:.3f} ms; '
            f'Scriptw RSS before {row["before_rss_mib"]:.1f} MiB, '
            f'now {row["after_rss_mib"]:.1f} MiB. '
        )

    settings = {
        'font.family': 'DejaVu Sans', 'font.size': 10, 'text.color': INK,
        'axes.labelcolor': MUTED, 'xtick.color': MUTED, 'ytick.color': INK,
        'svg.fonttype': 'none', 'svg.hashsalt': 'scriptw-execution-checkpoint-v1',
        'figure.facecolor': 'white', 'savefig.facecolor': 'white',
    }
    with plt.rc_context(settings):
        fig = plt.figure(figsize=(14, 17.4))
        try:
            fig.text(.045, .978, 'Scriptw execution vs V8', fontsize=22, weight='bold')
            fig.text(.045, .955, 'Fresh Store / Realm · all 17 checked workloads · lower is better',
                     fontsize=12, color=MUTED)
            fig.text(.045, .936, f'{measured} · current {data["source_commit"][:9]} · '
                     f'before {data["baseline_commit"][:9]} + archived patch',
                     fontsize=10, color=MUTED)
            handles = [
                Line2D([], [], marker='o', markerfacecolor='white', color=BEFORE,
                       linestyle='none', label='Scriptw before'),
                Line2D([], [], marker='o', color=CURRENT, linestyle='none', label='Scriptw now'),
                Line2D([], [], marker='D', color=V8, linestyle='none', label='Node / V8'),
            ]
            fig.legend(handles=handles, loc='upper left', bbox_to_anchor=(.243, .923),
                       frameon=False, ncol=3, columnspacing=2.0, fontsize=10)

            def panel(rect, title, headings):
                ax = fig.add_axes(rect)
                ax.set_ylim(len(rows) - .5, -.5)
                ax.set_yticks(range(len(rows)), [r['file'] for r in rows], fontsize=10)
                ax.tick_params(axis='y', length=0, pad=12)
                ax.tick_params(axis='x', length=0, pad=9)
                for spine in ax.spines.values():
                    spine.set_visible(False)
                ax.set_axisbelow(True)
                ax.grid(axis='x', color=GRID, linewidth=.8)
                for i in range(len(rows)):
                    if i % 2 == 0:
                        ax.axhspan(i - .5, i + .5, color='#f6f8fa', zorder=-1)
                fig.text(.045, rect[1] + rect[3] + .016, title,
                         fontsize=11, weight='bold', color=MUTED)
                for x, heading in zip((1.13, 1.40, 1.67), headings):
                    ax.text(x, 1.045, heading, transform=ax.transAxes, ha='right',
                            fontsize=10, weight='bold', color=MUTED)
                return ax

            timing = panel([.25, .565, .39, .320], 'EXECUTION TIME · ms',
                           ('Before', 'Now', 'V8'))
            timing.set_xscale('log')
            values = [row[key] for row in rows for key in ('before_ms', 'after_ms', 'v8_ms')]
            timing.set_xlim(10 ** math.floor(math.log10(min(values))),
                            10 ** math.ceil(math.log10(max(values))))
            timing.xaxis.set_major_locator(LogLocator(base=10))
            timing.xaxis.set_minor_locator(NullLocator())
            timing.xaxis.set_major_formatter(FuncFormatter(lambda value, _: f'{value:g}'))
            timing.set_xlabel('Elapsed milliseconds · logarithmic axis', labelpad=13, fontsize=10)
            for i, row in enumerate(rows):
                timing.plot([row['before_ms'], row['after_ms']], [i, i],
                            color='#9ca3af', linewidth=1.5)
                timing.scatter(row['before_ms'], i, s=38, facecolors='white',
                               edgecolors=BEFORE, linewidths=1.5, zorder=3)
                timing.scatter(row['after_ms'], i, s=24, color=CURRENT, zorder=4)
                timing.scatter(row['v8_ms'], i, s=24, marker='D', color=V8, zorder=4)
                for x, key, color in zip((1.13, 1.40, 1.67),
                                         ('before_ms', 'after_ms', 'v8_ms'),
                                         (BEFORE, CURRENT, V8)):
                    timing.text(x, i, f'{row[key]:.3f}', color=color, fontsize=10,
                                ha='right', va='center', transform=timing.get_yaxis_transform())
            fig.text(.045, .511,
                     'Construction, checksums and completion included; setup and compilation recorded separately.',
                     fontsize=10, color=MUTED)
            integer = next(row for row in rows if row['file'] == 'integer.js')
            numeric = next(row for row in rows if row['file'] == 'numeric-loop-50000.js')
            fig.text(.045, .491,
                     f'Costs retained: integer control {integer["change_percent"]:+.1f}% elapsed; '
                     f'50k numeric loop {numeric["rss_change_percent"]:+.1f}% sampled RSS.',
                     fontsize=10, color=MUTED)

            memory = panel([.25, .115, .39, .335], 'SCRIPTW SAMPLED PROCESS RSS · MiB',
                           ('Before', 'Now', 'Change'))
            maximum = max(r[key] for r in rows for key in ('before_rss_mib', 'after_rss_mib'))
            memory.set_xlim(0, math.ceil(maximum / 50) * 50)
            memory.set_xlabel('Sampled process RSS · linear axis', labelpad=12, fontsize=10)
            for i, row in enumerate(rows):
                memory.plot([row['before_rss_mib'], row['after_rss_mib']], [i, i],
                            color='#9ca3af', linewidth=1.5)
                memory.scatter(row['before_rss_mib'], i, s=38, facecolors='white',
                               edgecolors=BEFORE, linewidths=1.5, zorder=3)
                memory.scatter(row['after_rss_mib'], i, s=24, color=CURRENT, zorder=4)
                for x, value, color in (
                    (1.13, f'{row["before_rss_mib"]:.1f}', BEFORE),
                    (1.40, f'{row["after_rss_mib"]:.1f}', CURRENT),
                    (1.67, f'{row["rss_change_percent"]:+.1f}%', MUTED),
                ):
                    memory.text(x, i, value, color=color, fontsize=10, ha='right', va='center',
                                transform=memory.get_yaxis_transform())
            fig.text(.045, .062,
                     'RSS includes runtime/setup and retained warmup effects; not live JavaScript bytes or whole-child peak.',
                     fontsize=10, color=MUTED)
            fig.text(.045, .043,
                     '14 observations per product · two rounds of seven after ten warmups · ABBA · one CPU · 4 GiB cap',
                     fontsize=10, color=MUTED)
            fig.text(.045, .024,
                     'Shared-host measurements are descriptive. Large gaps to V8 remain; call-cohort results are separate.',
                     fontsize=10, color=MUTED)
            svg = io.BytesIO()
            fig.savefig(svg, format='svg', metadata={
                'Title': 'Scriptw versus V8: verified fresh-Realm execution',
                'Description': description, 'Date': None,
                'Creator': 'scripts/render-execution-performance.py',
            })
            if preview:
                Path(preview).parent.mkdir(parents=True, exist_ok=True)
                fig.savefig(preview, format='png', dpi=100)
            # Matplotlib leaves spaces at the ends of multiline SVG paths.
            # Newlines already separate their coordinates; keep the artifact
            # clean under Git's whitespace check without changing geometry.
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
