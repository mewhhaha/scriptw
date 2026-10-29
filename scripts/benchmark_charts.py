"""Compact per-workload charts for already validated benchmark summaries."""
from html import escape
import io
import json
from pathlib import Path, PurePosixPath
from textwrap import fill


LABELS = {'v8': 'V8', 'scriptw': 'Scriptw', 'scriptc': 'Scriptc'}
COLORS = {'v8': '#de8b36', 'scriptw': '#3378b5', 'scriptc': '#36835b'}
PANELS = (('ms', 'Time · ms', 3), ('rss_mib', 'RSS · MiB', 1))


def render(report, row, runtimes, *, preview=None):
    """Render one validated case with independent linear scales from zero."""
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from matplotlib.ticker import FuncFormatter, MaxNLocator

    runtimes = tuple(runtimes)
    filename = PurePosixPath(row['file']).name
    metadata = {
        'schema': report['schema'],
        'source_commit': report['source_commit'],
        'measured_date': report['finished_at'][:10],
        'started_at': report.get('started_at'),
        'finished_at': report['finished_at'],
        'policy': report.get('policy'),
        'shared_configuration': report.get('shared_configuration'),
        'runtimes': runtimes,
        'row': row,
    }
    with plt.rc_context({
        'font.family': 'DejaVu Sans', 'font.size': 10,
        'text.color': '#24292f', 'text.usetex': False, 'text.parse_math': False,
        'figure.facecolor': 'white', 'axes.facecolor': 'white',
        'svg.fonttype': 'none', 'svg.hashsalt': 'scriptw-benchmark-card-v1',
    }):
        fig, axes = plt.subplots(1, 2, figsize=(6.1, 3.2))
        try:
            fig.subplots_adjust(left=.09, right=.97, top=.77, bottom=.17, wspace=.42)
            fig.suptitle(filename, x=.055, y=.95, ha='left', fontsize=15, fontweight='bold')
            for ax, (field, title, decimals) in zip(axes, PANELS):
                values = [row['runtimes'][runtime][field] for runtime in runtimes
                          if row['runtimes'][runtime]['status'] == 'ok']
                maximum = max(values, default=0) or 1
                ax.set_ylim(0, maximum * 1.30)
                ax.set_xlim(-.6, len(runtimes) - .4)
                ax.set_title(title, loc='left', fontsize=11, color='#57606a', pad=11)
                ax.set_xticks(range(len(runtimes)), [LABELS[runtime] for runtime in runtimes])
                ax.tick_params(axis='x', length=0, pad=8, labelsize=10, colors='#24292f')
                ax.tick_params(axis='y', length=0, labelsize=8, colors='#57606a')
                ax.yaxis.set_major_locator(MaxNLocator(nbins=3, min_n_ticks=2))
                ax.yaxis.set_major_formatter(FuncFormatter(lambda value, _: f'{value:g}'))
                ax.set_axisbelow(True)
                ax.grid(axis='y', color='#eaeef2', linewidth=.8)
                for side in ('top', 'right', 'left'):
                    ax.spines[side].set_visible(False)
                ax.spines['bottom'].set_color('#d0d7de')
                for index, runtime in enumerate(runtimes):
                    item = row['runtimes'][runtime]
                    if item['status'] == 'ok':
                        value = item[field]
                        ax.bar(index, value, width=.68, color=COLORS[runtime], linewidth=0)
                        ax.annotate(f'{value:.{decimals}f}', (index, value), xytext=(0, 5),
                                    textcoords='offset points', ha='center', va='bottom',
                                    fontsize=11, color='#24292f')
                    else:
                        ax.text(index, .06, fill('Failed: ' + item['status'], width=13),
                                transform=ax.get_xaxis_transform(), ha='center', va='bottom',
                                fontsize=8, color='#b42318', linespacing=1.25)
            output = io.BytesIO()
            fig.savefig(output, format='svg', facecolor='white', metadata={
                'Date': None, 'Title': filename, 'Creator': 'Scriptw benchmark charts',
                'Description': json.dumps(metadata, sort_keys=True, ensure_ascii=False),
            })
            if preview is not None:
                Path(preview).parent.mkdir(parents=True, exist_ok=True)
                fig.savefig(preview, dpi=100)
            return b'\n'.join(line.rstrip() for line in output.getvalue().splitlines()) + b'\n'
        finally:
            plt.close(fig)


def gallery(rows, directory, prefix='assets'):
    """Return a two-column HTML gallery using the actual fixture basenames."""
    rows = list(rows)
    lines = ['<table>']
    for offset in range(0, len(rows), 2):
        lines.append('<tr>')
        for row in rows[offset:offset + 2]:
            filename = PurePosixPath(row['file'])
            src = str(PurePosixPath(prefix) / directory / (filename.stem + '.svg'))
            descriptions = [filename.name]
            for runtime, item in row['runtimes'].items():
                if item['status'] == 'ok':
                    descriptions.append(f"{LABELS[runtime]}: {item['ms']:.3f} ms, {item['rss_mib']:.1f} MiB RSS")
                else:
                    descriptions.append(f"{LABELS[runtime]}: failed ({item['status']})")
            alt = escape('; '.join(descriptions), quote=True)
            lines.append(f'<td><img src="{escape(src, quote=True)}" width="440" alt="{alt}"></td>')
        if offset + 1 == len(rows):
            lines.append('<td></td>')
        lines.append('</tr>')
    lines.append('</table>')
    return '\n'.join(lines)
