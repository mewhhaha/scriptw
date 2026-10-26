#!/usr/bin/env python3
"""Measure Scriptw and Scriptc in independent, compiler-disabled Wasmtime hosts."""
import argparse
from datetime import datetime, timezone
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys

from precompiled_comparison import (
    ADAPTER, FIXTURES, ROUNDS, SAMPLES, SCHEMA, WARMUPS, digest,
    image_metadata, load_report, measurement_plan, require,
)

ROOT = Path(__file__).resolve().parents[1]


def stamp():
    return datetime.now(timezone.utc).isoformat()


def budget():
    from limited_verification import require_kernel_budget
    require_kernel_budget(4 * 1024**3)
    require(len(os.sched_getaffinity(0)) == 1, 'use the serial resource scope')
    values = {}
    for line in Path('/proc/self/cgroup').read_text().splitlines():
        if line.startswith('0::'):
            group = Path('/sys/fs/cgroup') / line[3:].lstrip('/')
            values = {key: (group/key).read_text().strip() for key in
                      ('memory.max', 'memory.swap.max', 'memory.oom.group', 'memory.peak', 'memory.events')}
    require(values and values['memory.oom.group'] == '1', 'grouped OOM is required')
    return values


def measure(args):
    require(sys.platform == 'linux', 'Linux memory endpoints are required')
    before = budget()
    # Unrelated user workspaces do not participate in this benchmark's authority.
    inputs = ['crates', 'vendor', 'scripts', '.github/workflows', 'Cargo.toml', 'Cargo.lock']
    require(not subprocess.check_output(['git','diff','--binary','HEAD','--',*inputs], cwd=ROOT)
            and not subprocess.check_output(['git','ls-files','--others','--exclude-standard','--',*inputs], cwd=ROOT),
            'commit the complete benchmark and product inputs before measuring')
    require(not os.environ.get('NODE_OPTIONS'), 'NODE_OPTIONS must be unset')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    for name in ('sources', 'adapted', 'expected', 'logs', 'wasm/scriptw', 'wasm/scriptc', 'build', 'bin', 'images/scriptc'):
        (output/name).mkdir(parents=True)
    environment = dict(os.environ, NODE_DISABLE_COMPILE_CACHE='1', SCRIPTC_TARGET='wasm32-wasi')
    environment.pop('NODE_COMPILE_CACHE', None)
    instrumentation = ('SCRIPTW_PERF_MAP_DIR','SCRIPTW_WASM_TRACE','SCRIPTW_WASM_TRACE_DUMP',
        'SCRIPTW_EMIT_SIZE_REPORT','SCRIPTW_ROW_PERF_CONTROL','SCRIPTW_ROW_PERF_ACK',
        'SCRIPTW_ROW_PERF_REGION','SCRIPTW_BOUNDARY_READY_FD','SCRIPTW_BOUNDARY_READY_NONCE')
    for key in instrumentation:
        environment.pop(key,None)
    commands = []
    host = output/'bin/consumer'
    shutil.copy2(args.host.resolve(), host)
    require(host.stat().st_nlink == 1 and digest(host) == digest(args.host), 'shared host freeze differs')
    host_sha = digest(host)
    plan_bytes = (ROOT/FIXTURES/'manifest.json').read_bytes()
    plan = json.loads(plan_bytes)
    require(len(plan['execution']) == 17, 'the complete original corpus is required')
    (output/'sources/manifest.json').write_bytes(plan_bytes)
    for case in plan['execution']:
        name = case['file']
        original = (ROOT/FIXTURES/name).read_bytes()
        (output/'sources'/name).write_bytes(original)
        (output/'adapted'/name).write_bytes(ADAPTER.encode()+original)
        (output/'expected'/name).write_text(case['stdout'])
    report = dict(schema=SCHEMA, complete=False, started_at=stamp(),
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip(),
        github_run_url=os.environ.get('BENCHMARK_RUN_URL'), samples=SAMPLES, warmups=WARMUPS,
        rounds=ROUNDS, workloads=plan['execution'], attempts=[], commands=commands,
        policy=dict(consumer_compiler_enabled=False, producers_outside_consumers=True,
            execution='fresh Store/Realm; instance setup excluded; guest/runtime startup and output/completion included',
            memory='post-execution RSS/PSS/private-dirty endpoints with Store alive; load maximum sampled separately',
            limitations='same-host descriptive comparison, not live JS heap, peak RSS, statistical significance or conformance'),
        environment={key:environment.get(key) for key in (*instrumentation,'NODE_OPTIONS',
            'NODE_COMPILE_CACHE','NODE_DISABLE_COMPILE_CACHE','SCRIPTC_TARGET',
            'CARGO_BUILD_JOBS','SCRIPTW_JOBS','RUST_TEST_THREADS','RAYON_NUM_THREADS',
            'SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES','SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES')})

    def checkpoint():
        (output/'incomplete-report.json').write_text(json.dumps(report, indent=2)+'\n')

    def invoke(identifier, kind, runtime, argv, *, file=None, round_index=None, timeout=900):
        argv = list(map(str, argv))
        executable = Path(shutil.which(argv[0]) or argv[0]).resolve()
        event = dict(id=identifier, kind=kind, runtime=runtime, file=file, round=round_index,
            argv=argv, cwd=str(ROOT), executable_sha256=digest(executable), started_at=stamp(),
            stdout='logs/'+identifier+'.stdout', stderr='logs/'+identifier+'.stderr')
        commands.append(event)
        print(identifier, flush=True)
        with (output/event['stdout']).open('xb') as out, (output/event['stderr']).open('xb') as err:
            process = subprocess.Popen(argv, cwd=ROOT, env=environment, stdout=out, stderr=err,
                                       start_new_session=True)
            try:
                event['returncode'] = process.wait(timeout=timeout)
                event['timed_out'] = False
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                event.update(returncode=124, timed_out=True, timeout_seconds=timeout)
        event['finished_at'] = stamp()
        checkpoint()
        return event

    def value(event):
        require(event['returncode'] == 0 and not (output/event['stderr']).read_bytes(),
                'shared host/tool failed; retain original streams: '+event['id'])
        return json.loads((output/event['stdout']).read_text())

    identity_event = invoke('host-identity','tool-version','shared',[host,'identity'])
    identity = value(identity_event)
    require(identity['schema'] == 'scriptw-precompiled-shared-host-v1'
            and identity['source_commit'] == report['source_commit']
            and identity['executable_sha256'] == host_sha, 'actual shared host source differs')
    report.update(source_fingerprint=identity['source_fingerprint'],
        source_fingerprint_scheme=identity['compiler_identity_scheme'],
        shared_configuration=identity['shared_configuration'])
    spec = importlib.util.spec_from_file_location('precompiled_machine', ROOT/'scripts/benchmark-v8.py')
    machine = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(machine)
    report['machine'] = machine.machine()
    report['machine']['measurement_affinity'] = sorted(os.sched_getaffinity(0))
    shutil.copyfile(args.build_log, output/'build/cargo.jsonl')
    cargo = [json.loads(line) for line in (output/'build/cargo.jsonl').read_text().splitlines()]
    producers = [row for row in cargo if row.get('reason') == 'compiler-artifact'
        and row.get('executable') and row['target']['name'] == 'compiler_precompiled_baseline']
    require(len(producers) == 1 and Path(producers[0]['executable']).resolve() == args.host.resolve(),
            'actual Cargo host producer is required')
    report['tools'] = dict(consumer=dict(path=str(host),sha256=host_sha,
        cargo_log='build/cargo.jsonl',cargo_producer=producers[0],identity_command=identity_event['id']))
    for name, command in [('scriptc',[args.scriptc,'--version']),('zig',[args.zig,'version']),('rustc',['rustc','--version'])]:
        event = invoke('version-'+name,'tool-version','shared',command)
        require(event['returncode'] == 0 and not (output/event['stderr']).read_bytes(), 'tool version query failed')
        report['tools'][name] = dict(path=event['argv'][0],sha256=event['executable_sha256'],
            version=(output/event['stdout']).read_text().strip(),version_command=event['id'])
    require(report['tools']['scriptc']['version'] in ('0.4.5', 'scriptc 0.4.5')
            and report['tools']['zig']['version'] == '0.16.0', 'external tool pins differ')
    if args.package_lock:
        shutil.copyfile(args.package_lock, output/'build/package-lock.json')
    # Keep the driver's executable resolution identical to the pinned Zig query.
    environment['PATH'] = str(Path(shutil.which(str(args.zig)) or args.zig).resolve().parent)+os.pathsep+environment['PATH']
    scriptw = invoke('prepare-scriptw','source-produce','scriptw',
                     [host,'prepare-scriptw',output/'sources',output/'images/scriptw'])
    scriptw_value = value(scriptw)
    require(len(scriptw_value['cases']) == 17, 'Scriptw must produce the complete corpus')
    prepared = {}
    for case in scriptw_value['cases']:
        name = case['file']
        image = output/'images/scriptw'/case['image']
        metadata = image_metadata(image.read_bytes(),'scriptw',report['source_fingerprint'])
        metadata['file'] = str(image.relative_to(output))
        raw_wasm = output/'images/scriptw'/case['wasm']
        require(digest(raw_wasm) == metadata['program_wasm_sha256'], 'Scriptw raw P export differs')
        retained = output/'wasm/scriptw'/(name+'.wasm')
        shutil.copyfile(raw_wasm,retained)
        prepared['scriptw',name] = dict(status='ok',producer_ids=[scriptw['id']],
            wasm=str(retained.relative_to(output)),image=metadata)
    for case in plan['execution']:
        name = case['file']
        wasm = output/'wasm/scriptc'/(name+'.wasm')
        source = invoke('source-scriptc-'+name,'source-produce','scriptc',
                        [args.scriptc,'build',output/'adapted'/name,'-o',wasm],file=name)
        item = dict(status='source-failed',producer_ids=[source['id']])
        if source['returncode'] == 0:
            item['wasm'] = str(wasm.relative_to(output))
            image = output/'images/scriptc'/(name+'.cwasm')
            native = invoke('native-scriptc-'+name,'wasm-precompile','scriptc',
                            [host,'prepare-wasi',wasm,image],file=name)
            item['producer_ids'].append(native['id'])
            item['status'] = 'precompile-failed'
            if native['returncode'] == 0:
                value(native)
                metadata = image_metadata(image.read_bytes(),'scriptc',report['source_fingerprint'])
                metadata['file'] = str(image.relative_to(output))
                item.update(status='ok',wasm=str(wasm.relative_to(output)),image=metadata)
        prepared['scriptc',name] = item
    for task in measurement_plan(plan['execution']):
        item = dict(task,**prepared[task['runtime'],task['file']])
        item['consumer_id'] = None
        report['attempts'].append(item)
        if item['status'] != 'ok':
            checkpoint()
            continue
        case = next(case for case in plan['execution'] if case['file'] == task['file'])
        image = output/item['image']['file']
        require(image_metadata(image.read_bytes(),task['runtime'],report['source_fingerprint']) ==
                {key: value for key,value in item['image'].items() if key!='file'}, 'producer image changed before consumption')
        event = invoke(f"round-{task['round']}-{task['runtime']}-{task['file']}",'consume',task['runtime'],
            [host,'execute-scriptw' if task['runtime']=='scriptw' else 'execute-wasi',
             image,case['stdout'],str(SAMPLES),str(WARMUPS)],file=task['file'],round_index=task['round'])
        item['consumer_id'] = event['id']
        if event['returncode'] != 0:
            item['status'] = 'run-failed'
        else:
            raw = value(event)
            if any(not row['matched'] for row in raw['observations']):
                item['status'] = 'checksum-mismatch'
        checkpoint()
    require(digest(host) == host_sha, 'frozen shared executable changed')
    report['resources'] = dict(before=before,after=budget(),cpu_affinity=sorted(os.sched_getaffinity(0)))
    report.update(complete=True,finished_at=stamp())
    retained = {'sources/manifest.json','build/cargo.jsonl'}
    for case in plan['execution']:
        retained.update(directory+'/'+case['file'] for directory in ('sources','adapted','expected'))
    retained.update(event[key] for event in commands for key in ('stdout','stderr'))
    retained.update(item['wasm'] for item in prepared.values() if 'wasm' in item)
    if args.package_lock:
        retained.add('build/package-lock.json')
    report['retained_files'] = [dict(path=name,bytes=(output/name).stat().st_size,sha256=digest(output/name))
        for name in sorted(retained)]
    target = output/'report.json'
    target.write_text(json.dumps(report,indent=2)+'\n')
    load_report(target)
    if args.evidence_output:
        evidence = args.evidence_output.resolve()
        evidence.mkdir(parents=True,exist_ok=False)
        for item in report['retained_files']:
            destination = evidence/item['path']
            destination.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(output/item['path'],destination)
        shutil.copyfile(target,evidence/'report.json')
        load_report(evidence/'report.json')
        print('Portable evidence:',evidence/'report.json')
    print(target)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host',required=True,type=Path)
    parser.add_argument('--scriptc',required=True,type=Path)
    parser.add_argument('--zig',default='zig')
    parser.add_argument('--output',required=True,type=Path)
    parser.add_argument('--build-log',required=True,type=Path)
    parser.add_argument('--package-lock',type=Path)
    parser.add_argument('--evidence-output',type=Path,help='new directory containing only admitted portable evidence')
    measure(parser.parse_args())


if __name__ == '__main__':
    try:
        main()
    except (ValueError,KeyError,TypeError,OSError,subprocess.SubprocessError) as error:
        raise SystemExit('Precompiled measurement refused: '+str(error))
