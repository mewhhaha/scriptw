#!/usr/bin/env python3
"""Render the Porffor runtime-kernel comparison chart from inert report JSON."""
import argparse
import hashlib
import io
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = 'scriptw-wasm-peer-comparison-v1'
PEERS = (('porffor.wasmtime', 'vs Porffor on Wasmtime', '#3378b5'),
         ('porffor.v8', 'vs Porffor on Node/V8 Wasm', '#de8b36'))
FAILURE_LABELS = {'wrong-output': 'Porffor prints wrong output',
                  'runtime-failed': 'Porffor throws'}


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def rows(report):
    """Per kernel: Scriptw median over each peer's median, or the peer's failure."""
    require(report.get('schema') == SCHEMA, 'unexpected report schema')
    failures = {}
    for failure in report['failures']:
        failures.setdefault((failure['case'], failure['actor']), set()).add(failure['status'])
    output = []
    for row in report['runtime']:
        actors = row['actors']
        scriptw = actors['scriptw']['median_ns']
        require(scriptw > 0, 'Scriptw median must be positive: ' + row['case'])
        ratios = {}
        peer_failures = set()
        for peer, _, _ in PEERS:
            if peer in actors:
                ratios[peer] = scriptw / actors[peer]['median_ns']
            else:
                statuses = failures.get((row['case'], peer), set())
                require(bool(statuses), 'peer missing without a failure: ' + row['case'])
                peer_failures.update(statuses)
        require(len(peer_failures) <= 1, 'one failure status per kernel: ' + row['case'])
        output.append(dict(case=row['case'], ratios=ratios,
                           failure=next(iter(peer_failures)) if peer_failures else None))
    for peer, _, _ in PEERS:
        measured = [row['ratios'][peer] for row in output if peer in row['ratios']]
        comparison = report['comparisons'][peer]
        require(len(measured) == comparison['kernels'], 'kernel count differs for ' + peer)
        geomean = math.exp(sum(map(math.log, measured)) / len(measured))
        require(abs(geomean - comparison['scriptw_time_ratio_geomean']) < 1e-9,
                'geometric mean differs from the report for ' + peer)
    measured = sorted((row for row in output if not row['failure']),
                      key=lambda row: row['ratios']['porffor.wasmtime'])
    failed = sorted((row for row in output if row['failure']), key=lambda row: row['case'])
    return measured + failed


def render(report, report_sha, preview=None):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from matplotlib.ticker import FixedLocator, NullLocator

    kernels = rows(report)
    version = report['tools']['peer']['version']
    with plt.rc_context({'font.family': 'DejaVu Sans', 'font.size': 10,
                         'text.color': '#24292f', 'text.usetex': False, 'text.parse_math': False,
                         'figure.facecolor': 'white', 'axes.facecolor': 'white',
                         'svg.fonttype': 'none', 'svg.hashsalt': 'scriptw-porffor-comparison-v1'}):
        width, top, bottom = 7.2, 1.25, .55
        fig, ax = plt.subplots(figsize=(width, top + bottom + .34 * len(kernels)))
        try:
            figure_height = fig.get_figheight()
            fig.subplots_adjust(left=.30, right=.97, top=1 - top / figure_height,
                                bottom=bottom / figure_height)
            fig.suptitle(f'Scriptw vs Porffor {version} · runtime kernels',
                         x=.02, y=1 - .14 / figure_height, ha='left', va='top',
                         fontsize=13, fontweight='bold')
            fig.text(.02, 1 - .50 / figure_height,
                     'Scriptw time ÷ Porffor time, median of 8 processes; '
                     'left of 1× is Scriptw faster. One CPU per actor.',
                     ha='left', va='top', fontsize=9, color='#57606a')
            handles = [plt.Rectangle((0, 0), 1, 1, color=color) for _, _, color in PEERS]
            fig.legend(handles, [label for _, label, _ in PEERS], loc='upper left',
                       bbox_to_anchor=(.01, 1 - .78 / figure_height), ncol=2,
                       frameon=False, fontsize=9)
            low, high = .01, 10
            height = .36
            for index, row in enumerate(kernels):
                if row['failure']:
                    ax.text(low * 1.15, index, FAILURE_LABELS[row['failure']],
                            va='center', fontsize=9, color='#8c959f', style='italic')
                    continue
                for offset, (peer, _, color) in zip((-height / 2, height / 2), PEERS):
                    value = row['ratios'][peer]
                    ax.barh(index + offset, value - low, height=height, left=low, color=color)
                    ax.text(value * 1.06, index + offset, f'{value:.2f}×', va='center',
                            fontsize=7.5, color='#24292f')
            ax.axvline(1, color='#24292f', linewidth=.9)
            ax.set_xscale('log')
            ax.set_xlim(low, high)
            ax.xaxis.set_major_locator(FixedLocator([.01, .1, 1, 10]))
            ax.xaxis.set_minor_locator(NullLocator())
            ax.set_xticklabels(['0.01×', '0.1×', '1×', '10×'])
            ax.set_yticks(range(len(kernels)), [row['case'] for row in kernels], fontsize=9)
            ax.set_ylim(len(kernels) - .5, -.5)
            ax.grid(axis='x', color='#d0d7de', linewidth=.6)
            ax.set_axisbelow(True)
            for side in ('top', 'right'):
                ax.spines[side].set_visible(False)
            metadata = dict(schema=report['schema'], report_sha256=report_sha,
                            date=report['date'],
                            source_commit=report['scriptw']['source_commit'],
                            source_fingerprint=report['scriptw']['source_fingerprint'],
                            porffor=version, matplotlib=matplotlib.__version__)
            output = io.BytesIO()
            fig.savefig(output, format='svg', facecolor='white', metadata={
                'Date': None, 'Title': f'Scriptw vs Porffor {version} runtime kernels',
                'Creator': 'Scriptw Porffor comparison chart',
                'Description': json.dumps(metadata, sort_keys=True, ensure_ascii=False)})
            if preview:
                fig.savefig(preview, dpi=110, facecolor='white')
        finally:
            plt.close(fig)
    return b'\n'.join(line.rstrip() for line in output.getvalue().splitlines()) + b'\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path,
                        default=ROOT / 'docs/rust-rewrite/wasm-peer-current.json')
    parser.add_argument('--output', type=Path,
                        default=ROOT / 'docs/rust-rewrite/assets/porffor-current/runtime.svg')
    parser.add_argument('--preview', type=Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    report_path = args.report.resolve(strict=True)
    report_bytes = report_path.read_bytes()
    content = render(json.loads(report_bytes), hashlib.sha256(report_bytes).hexdigest(),
                     args.preview)
    if args.check:
        require(args.output.is_file() and args.output.read_bytes() == content,
                'generated output differs: ' + str(args.output))
        print('Verified the Porffor comparison chart')
        return
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(content)
    print(f'Wrote {args.output}')


if __name__ == '__main__':
    main()
