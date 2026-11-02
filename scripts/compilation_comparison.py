"""Admission and charting for fresh-process JavaScript-to-portable-Wasm builds."""
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import statistics
import tarfile

from benchmark_metrics import digest
from compiler_comparison import ADAPTER

SCHEMA = 'scriptw-portable-wasm-compilation-v1'
ACTORS = ('scriptw', 'scriptc')
ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'crates/scriptw-cli/tests/fixtures/v8_execution_hotpaths'
SCRIPTC_COMPANIONS = ('ts7', 'tsc_ts', 'llvm_package', 'runtime_sources', 'declarations', 'comptime')
POLICY = dict(
    timing='Parent wall around GNU-time-supervised fresh compiler process; includes startup, source read, parse/lower/codegen/link and Wasm output I/O. No Wasm-to-native compilation, instantiation or JavaScript execution.',
    artifacts='Scriptw build wasm emits P and its Cargo-built Portable shared R sidecar; R extraction/admission/copy/write costs included each process, Rust/Cargo R production excluded. Scriptc links its prebuilt runtime into standalone WASI Wasm; runtime-pack production excluded.',
    cache='Fresh per-process work/source/application cache/tmp/Zig-local directories; Scriptc SCRIPTC_NO_CACHE=1. OS page caches and existing Zig global toolchain cache retained, never flushed. This is cold application/compiler-process startup, not a cold machine/toolchain installation.',
    memory='GNU time compiler-child lifetime high-water RSS, including accounted descendants; not simultaneous aggregate process-tree RSS, runtime RSS or live heap.',
    source='Original17 exact source/expected bytes retained; Scriptc alone receives the canonical print-to-console.log adapter.',
    compiler_identity='Scriptw source_commit is the embedded base Git revision; source_fingerprint and executable SHA identify the measured working-tree compiler. The performance evidence separately binds its source patch and publication.',
    statistics='Two blocks, ABBA per case with alternating first actor; second block reverses cases and actors. Four fresh builds per compiler/case; pooled and both block medians. Descriptive observations, no significance/conformance claim.',
    resources='Sequential children, one CPU, at most4GiB cgroup, no swap, grouped OOM; native/module caches unused by build wasm.',
    validation='Successful compilation and portable Wasm magic checked, emitted bytes retained; no output execution/checksum verification in this compilation-only measurement.')


def require(condition, message):
    if not condition:
        raise ValueError('compilation comparison: ' + message)


def scriptc_companions(compiler, manifest):
    require(manifest.get('schema') == 'scriptc.native-toolchain.v1'
            and manifest.get('compiler_version') == '0.4.5', 'Scriptc toolchain manifest differs')
    paths = {}
    for field in SCRIPTC_COMPANIONS:
        value = manifest.get(field)
        require(isinstance(value, str) and bool(value), 'missing Scriptc companion selection: ' + field)
        paths[field] = os.path.normpath(Path(compiler).parent / value)
    return paths


def process_environment(work, runtime_pack, zig_global_cache):
    work = Path(work)
    return dict(SCRIPTW_JOBS='1', SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES='1',
                SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES=str(64 * 1024**2),
                SCRIPTC_TARGET='wasm32-wasi', SCRIPTC_RUNTIME_PACK=str(runtime_pack), SCRIPTC_NO_CACHE='1',
                NODE_DISABLE_COMPILE_CACHE='1', ZIG_GLOBAL_CACHE_DIR=str(zig_global_cache),
                TMPDIR=str(work / 'tmp'), SCRIPTW_CACHE_DIR=str(work / 'cache'),
                SCRIPTC_CACHE_DIR=str(work / 'cache'), ZIG_LOCAL_CACHE_DIR=str(work / 'zig-local'))


def is_digest(value):
    return isinstance(value, str) and len(value) == 64 and set(value) <= set('0123456789abcdef')


def require_tool_identity(tool):
    require(isinstance(tool, dict) and isinstance(tool.get('path'), str) and Path(tool['path']).is_absolute()
            and type(tool.get('bytes')) is int and tool['bytes'] > 0 and is_digest(tool.get('sha256')),
            'missing tool binary identity')


def process_plan(names):
    require(len(names) == len(set(names)) == 17, 'complete original17 corpus required')
    result = []
    for block in range(2):
        ordered = list(enumerate(names))
        if block == 1:
            ordered.reverse()
        for index, name in ordered:
            actors = ACTORS
            if (index + block) % 2:
                actors = actors[::-1]
            for position, actor in enumerate((actors[0], actors[1], actors[1], actors[0])):
                result.append(dict(block=block, case=name, position=position, actor=actor))
    return result


def summarize(report):
    names = [case['file'] for case in report['workloads']]
    plan = process_plan(names)
    rows = report['rows']
    require([{key: row[key] for key in ('block', 'case', 'position', 'actor')} for row in rows] == plan,
            'fresh-process order or inventory differs')
    result = []
    for name in names:
        actors = {}
        for actor in ACTORS:
            group = [row for row in rows if row['case'] == name and row['actor'] == actor]
            for row in group:
                resource = row['resources']
                require(resource['exit_code'] == 0 and resource['timed_out'] is False, 'failed build')
                for field in ('elapsed_ns', 'peak_rss_bytes'):
                    require(type(resource[field]) is int and resource[field] > 0, 'invalid process measurement')
            actors[actor] = dict(
                ms=statistics.median(row['resources']['elapsed_ns'] for row in group) / 1e6,
                rss_mib=statistics.median(row['resources']['peak_rss_bytes'] for row in group) / 1024**2,
                processes=len(group),
                block_median_ms=[statistics.median(row['resources']['elapsed_ns'] for row in group
                                                   if row['block'] == block) / 1e6 for block in range(2)])
        result.append(dict(file=name, actors=actors))
    return result


def safe_path(name):
    path = PurePosixPath(name)
    require(bool(path.parts) and not path.is_absolute() and '..' not in path.parts
            and '\\' not in name and ':' not in name and str(path) == name, 'unsafe retained path')
    return name


def load_report(path):
    path = Path(path)
    report = json.loads(path.read_text())
    require(report['schema'] == SCHEMA and report['status'] == 'complete', 'incomplete or foreign report')
    require(report['plan']['process_plan'] == process_plan([case['file'] for case in report['workloads']]),
            'declared sampling plan differs')
    require(report['summary'] == summarize(report), 'summary differs from raw processes')
    require(set(report['tools']) == {'scriptw', 'scriptc', 'zig', 'time'}, 'tool inventory differs')
    for tool in report['tools'].values():
        require_tool_identity(tool)
    require(report['tools']['scriptc']['version'] == '0.4.5'
            and report['tools']['zig']['version'] == '0.16.0', 'tool pins differ')
    identity = report['tools']['scriptw']['identity']
    require(is_digest(identity.get('source_fingerprint')), 'missing compiler source fingerprint')
    require(identity['executable_sha256'] == report['tools']['scriptw']['sha256'], 'CLI identity differs')
    require(identity['source_revision'] == dict(kind='git-commit', commit=report['source_commit']),
            'compiler source revision differs')
    machine = report['plan']['machine']
    resource = machine['resources']
    require(len(machine['affinity']) == 1 and 0 < int(resource['memory.max']) <= 4 * 1024**3
            and resource['memory.swap.max'] == '0' and resource['memory.oom.group'] == '1',
            'resource scope differs')
    files = {safe_path(row['path']): row for row in report['retained_files']}
    require(len(files) == len(report['retained_files']), 'duplicate retained path')
    retained = {}
    if 'archive' in report:
        archive_path = path.parent / safe_path(report['archive']['file'])
        require(digest(archive_path) == report['archive']['sha256'], 'archive digest differs')
        expanded = 0
        with tarfile.open(archive_path, 'r|gz') as archive:
            for member in archive:
                name = safe_path(member.name)
                expanded += member.size
                require(member.isfile() and name in files and name not in retained
                        and 0 <= member.size <= 64 * 1024**2 and expanded <= 256 * 1024**2,
                        'archive member or reading budget differs')
                retained[name] = archive.extractfile(member).read()
        require(set(retained) == set(files), 'archive inventory differs')
    else:
        for name in files:
            target = path.parent / name
            require(target.is_file() and not target.is_symlink(), 'missing retained evidence')
            retained[name] = target.read_bytes()
    for name, row in files.items():
        value = retained[name]
        require(len(value) == row['bytes'] and hashlib.sha256(value).hexdigest() == row['sha256'],
                'retained evidence changed: ' + name)
    manifest = json.loads(retained['sources/manifest.json'])
    require(manifest['execution'] == report['workloads']
            and manifest == json.loads((FIXTURES / 'manifest.json').read_text()), 'canonical manifest differs')
    require(json.loads(retained['plan.json']) == report['plan'], 'retained plan differs')
    require(report['plan']['tools'] == report['tools'] and report['plan']['policy'] == report['policy'] == POLICY,
            'declared tools or measurement policy differs')
    inputs = json.loads(retained['inputs-before.json'])
    require(inputs == json.loads(retained['inputs-after.json']),
            'inputs changed during measurement')
    for tool in report['tools'].values():
        require(inputs.get(tool['path']) == tool['sha256'], 'tool binary differs from measured inputs')
    scriptc = report['tools']['scriptc']
    runtime = scriptc.get('runtime_pack', {})
    require(isinstance(runtime.get('path'), str) and Path(runtime['path']).is_absolute()
            and type(runtime.get('files')) is int and runtime['files'] > 0,
            'missing Scriptc runtime-pack identity')
    runtime_inputs = json.loads(retained['tooling/runtime-pack-inputs.json'])
    pack = json.loads(retained['tooling/runtime-pack.json'])
    require(pack.get('version') == '0.4.5' and pack.get('target', {}).get('name') == 'wasm32-wasi'
            and files['tooling/runtime-pack.json']['sha256'] == runtime.get('manifest_sha256')
            and files['tooling/runtime-pack-inputs.json']['sha256'] == runtime.get('tree_sha256')
            and len(runtime_inputs) == runtime['files'] and runtime_inputs.get('runtime-pack.json') == runtime['manifest_sha256'],
            'Scriptc runtime-pack identity differs')
    for name, sha in runtime_inputs.items():
        require(is_digest(sha) and inputs.get(str(Path(runtime['path']) / safe_path(name))) == sha,
                'Scriptc runtime input differs from measured inputs')
    toolchain = scriptc.get('toolchain', {})
    manifest_identity = toolchain.get('manifest', {})
    require_tool_identity(manifest_identity)
    require(manifest_identity['path'] == scriptc['path'] + '.json'
            and files['tooling/scriptc-toolchain.json']['sha256'] == manifest_identity['sha256']
            and files['tooling/scriptc-toolchain.json']['bytes'] == manifest_identity['bytes']
            and files['tooling/scriptc-toolchain-inputs.json']['sha256'] == toolchain.get('inputs_sha256'),
            'Scriptc toolchain identity differs')
    companions = json.loads(retained['tooling/scriptc-toolchain-inputs.json'])
    selections = scriptc_companions(scriptc['path'], json.loads(retained['tooling/scriptc-toolchain.json']))
    require(len(companions) == toolchain.get('files') and companions.get(manifest_identity['path']) == manifest_identity['sha256'],
            'Scriptc companion inventory differs')
    for selected in selections.values():
        require(any(Path(name).is_relative_to(selected) for name in companions), 'missing selected Scriptc companion')
    for name, sha in companions.items():
        require(is_digest(sha) and inputs.get(name) == sha, 'Scriptc companion differs from measured inputs')
    require(json.loads(retained['logs/scriptw-identity.stdout']) == identity, 'retained CLI identity differs')
    require(retained['logs/scriptc-identity.stdout'].decode().strip().removeprefix('scriptc ').strip() == '0.4.5'
            and retained['logs/zig-identity.stdout'].decode().strip() == '0.16.0', 'retained tool versions differ')
    for case in report['workloads']:
        name = safe_path(case['file'])
        require(Path(name).name == name and retained['sources/' + name] == (FIXTURES / name).read_bytes(),
                'original source differs')
        require(retained['adapted/' + name] == ADAPTER.encode() + retained['sources/' + name], 'print adapter differs')
        require(retained['expected/' + name] == case['stdout'].encode(), 'expected output differs')
    runtime_artifacts = set()
    work_root = Path(report['plan'].get('work_root', ''))
    zig_cache = report['plan'].get('zig_global_cache_dir', '')
    require(work_root.is_absolute() and Path(zig_cache).is_absolute(), 'missing compilation cache directories')
    for number, row in enumerate(report['rows']):
        require(json.loads(retained[row['raw']]) == row, 'retained process measurement differs')
        work = work_root / f"{number:03d}-{row['actor']}-{row['case']}"
        argv = [report['tools'][row['actor']]['path'], 'build', str(work / row['case']), '-o', str(work / 'program.wasm')]
        environment = process_environment(work, runtime['path'], zig_cache)
        if row['actor'] == 'scriptw':
            argv = [report['tools']['scriptw']['path'], 'build', 'wasm', str(work / row['case'])]
            environment['SCRIPTW_WASM_DUMP'] = str(work / 'program.wasm')
        require(row.get('argv') == argv, 'compilation command boundary differs')
        require(row.get('environment') == environment, 'compilation cache or target environment differs')
        source = 'sources/' + row['case']
        if row['actor'] == 'scriptc':
            source = 'adapted/' + row['case']
        require(row['source_sha256'] == files[source]['sha256'], 'measured source differs')
        require(row['stdout'] in retained and row['stderr'] in retained, 'missing build diagnostics')
        expected_artifacts = {'program'}
        if row['actor'] == 'scriptw':
            expected_artifacts.add('runtime')
        require(set(row['artifacts']) == expected_artifacts, 'portable artifact inventory differs')
        for kind, artifact in row['artifacts'].items():
            require(files[artifact['path']]['sha256'] == artifact['sha256']
                    and files[artifact['path']]['bytes'] == artifact['bytes'], 'artifact identity differs')
            require(retained[artifact['path']][:8] == b'\0asm\1\0\0\0', 'output is not portable Wasm')
            if kind == 'runtime':
                runtime_artifacts.add(artifact['sha256'])
    require(len(runtime_artifacts) == 1, 'shared R artifact differs between processes')
    return report


def render(report, *, preview=None):
    """One zero-based grouped chart, with both block medians retained in metadata."""
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from matplotlib.ticker import MaxNLocator
    rows = report['summary']
    with plt.rc_context({'font.family': 'DejaVu Sans', 'font.size': 10,
                         'text.color': '#24292f', 'text.usetex': False, 'text.parse_math': False,
                         'svg.fonttype': 'none', 'svg.hashsalt': SCHEMA}):
        fig, ax = plt.subplots(figsize=(10, 9))
        try:
            fig.subplots_adjust(left=.31, right=.97, top=.88, bottom=.17)
            fig.suptitle('JavaScript → portable Wasm', x=.04, y=.97, ha='left', fontsize=18, fontweight='bold')
            fig.text(.04, .93, 'Fresh compiler process · four builds per compiler/workload · median wall time', color='#57606a')
            colors = {'scriptw': '#3378b5', 'scriptc': '#36835b'}
            for actor, offset in (('scriptw', -.19), ('scriptc', .19)):
                values = [row['actors'][actor]['ms'] for row in rows]
                positions = [index + offset for index in range(len(rows))]
                ax.barh(positions, values, height=.34, color=colors[actor], label=actor.capitalize())
                for y, value in zip(positions, values):
                    ax.annotate(f'{value:.1f}', (value, y), xytext=(4, 0), textcoords='offset points',
                                va='center', fontsize=8)
            ax.set_yticks(range(len(rows)), [row['file'] for row in rows])
            ax.invert_yaxis()
            maximum = max(row['actors'][actor]['ms'] for row in rows for actor in ACTORS)
            ax.set_xlim(0, maximum * 1.15)
            ax.set_xlabel('Compilation wall time · ms', labelpad=10)
            ax.xaxis.set_major_locator(MaxNLocator(nbins=5))
            ax.set_axisbelow(True)
            ax.grid(axis='x', color='#eaeef2')
            ax.tick_params(axis='both', length=0)
            for side in ('top', 'right', 'left'):
                ax.spines[side].set_visible(False)
            ax.spines['bottom'].set_color('#d0d7de')
            ax.legend(frameon=False, loc='lower right')
            fig.text(.04, .045, 'Includes startup and output I/O: Scriptw P + shared R sidecar; Scriptc standalone WASI.\n'
                     'Fresh application caches; retained Zig toolchain cache. Excludes Wasm → native compilation and execution.',
                     fontsize=9, color='#57606a', linespacing=1.5)
            encoded = io.BytesIO()
            metadata = {key: report[key] for key in ('schema', 'source_commit', 'finished_at', 'policy', 'tools', 'summary')}
            metadata['renderer'] = dict(matplotlib=matplotlib.__version__, sha256=digest(Path(__file__)))
            fig.savefig(encoded, format='svg', metadata={'Date': None, 'Title': 'JavaScript to portable Wasm compilation',
                        'Creator': 'Scriptw compilation comparison', 'Description': json.dumps(metadata, sort_keys=True)})
            if preview is not None:
                fig.savefig(preview, dpi=120)
            return b'\n'.join(line.rstrip() for line in encoded.getvalue().splitlines()) + b'\n'
        finally:
            plt.close(fig)
