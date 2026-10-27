#!/usr/bin/env python3
"""Probe-only exact native controls for the ordered bootstrap batch."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import time

from limited_verification import kernel_group, require_kernel_budget
from native_test_protocol import result_summaries

ROOT = Path(__file__).resolve().parents[1]
MIB = 1024**2
AOT_IDS = [
    'data::pooled_literal_domain::tests::intrinsic_installation_keys_reject_program_literals_and_retain_symbol_identity',
    'functions::function_object_allocation::constructor_allocation::tests::shared_constructor_tail_requires_exact_native_entry_and_uniform_captures',
    'tests::ordered_bootstrap_allocation::ordered_realm_bootstrap_shares_real_recipe_and_constructor_allocation',
    'tests::bootstrap_compact_metadata::native_bootstrap_allocates_compact_metadata_without_eager_property_records',
]
REALM_IDS = [
    'aot_created_realm_array_iterator::ordered_deferred_installation_keeps_alias_descriptors_independent_across_gc',
    'aot_created_realm_array_iterator::shared_constructor_allocation_keeps_eager_realm_links_and_bootstrap_exceptions',
]


def require(value, message):
    if not value:
        raise ValueError(message)


def stamp():
    return datetime.now(timezone.utc).isoformat()


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def source(expected):
    require(re.fullmatch('[0-9a-f]{40}', expected) is not None, 'expected source must be an exact Git commit')
    actual = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(actual == expected, 'checkout differs from expected source')
    require(not subprocess.check_output(['git', 'diff', '--binary', 'HEAD', '--'], cwd=ROOT), 'tracked checkout changes are not admitted')


def budget():
    group = kernel_group()
    return {key: (group / key).read_text().strip() for key in
            ('memory.max', 'memory.swap.max', 'memory.oom.group', 'memory.peak', 'memory.events')}


def scope():
    require_kernel_budget(4 * 1024**3)
    require(sorted(os.sched_getaffinity(0)) == [0], 'probe requires CPU0')
    require(os.environ.get('CARGO_BUILD_JOBS') == os.environ.get('RUST_TEST_THREADS') == '1', 'Cargo/libtest must remain serial')
    require(os.environ.get('SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES') == '1'
            and os.environ.get('SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES') == str(64 * MIB), 'retained module cache exceeds the admitted policy')


def no_oom(before, after):
    events = [dict(line.split() for line in row['memory.events'].splitlines()) for row in (before, after)]
    require(all(events[0][key] == events[1][key] for key in ('oom', 'oom_kill', 'oom_group_kill')), 'kernel OOM event occurred')


def save(output, report):
    temporary = output / 'report.pending.json'
    temporary.write_text(json.dumps(report, indent=2) + '\n')
    temporary.replace(output / 'report.json')


def run_command(row, output, *, timeout, stdout_limit=32 * MIB, stderr_limit=32 * MIB):
    """Bound both pipes and kill only this helper's owned child group on failure."""
    started = time.monotonic()
    row.update(started_at=stamp(), timeout_seconds=timeout, stdout_limit_bytes=stdout_limit,
               stderr_limit_bytes=stderr_limit, timed_out=False, output_limit_exceeded=False)
    paths = {name: output / row[name] for name in ('stdout', 'stderr')}
    process = None
    try:
        with paths['stdout'].open('xb') as stdout, paths['stderr'].open('xb') as stderr:
            streams = {'stdout': stdout, 'stderr': stderr}
            limits = {'stdout': stdout_limit, 'stderr': stderr_limit}
            retained = dict(stdout=0, stderr=0)
            process = subprocess.Popen(row['command'], cwd=ROOT, stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
            row['owned_child_pid'] = process.pid
            deadline = started + timeout
            stopping = False
            with selectors.DefaultSelector() as selector:
                for name, pipe in [('stdout', process.stdout), ('stderr', process.stderr)]:
                    os.set_blocking(pipe.fileno(), False)
                    selector.register(pipe, selectors.EVENT_READ, name)
                while selector.get_map() or process.poll() is None:
                    if time.monotonic() >= deadline:
                        if stopping:
                            row['pipe_cleanup_incomplete'] = True
                            break
                        row['timed_out'] = True
                        stopping = True
                        deadline = time.monotonic() + 5
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    for key, _ in selector.select(0.1):
                        try:
                            raw = os.read(key.fileobj.fileno(), 65536)
                        except BlockingIOError:
                            continue
                        if not raw:
                            selector.unregister(key.fileobj)
                            continue
                        name = key.data
                        keep = min(len(raw), limits[name] - retained[name])
                        streams[name].write(raw[:keep])
                        retained[name] += keep
                        if keep != len(raw) and not stopping:
                            row['output_limit_exceeded'] = True
                            stopping = True
                            deadline = time.monotonic() + 5
                            try:
                                os.killpg(process.pid, signal.SIGKILL)
                            except ProcessLookupError:
                                pass
                if process.poll() is None:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                row['exit_code'] = process.wait(timeout=5)
    finally:
        if process is not None:
            if process.poll() is None:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait(timeout=5)
            for pipe in (process.stdout, process.stderr):
                pipe.close()
            row.setdefault('exit_code', process.returncode)
        row.update(seconds=time.monotonic() - started, finished_at=stamp())
        for name, path in paths.items():
            if path.exists():
                row[name + '_sha256'] = sha(path)
                row[name + '_bytes'] = path.stat().st_size


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--expected-source', required=True)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = dict(schema='scriptw-ordered-bootstrap-focused-ci-v1', source_commit=args.expected_source,
                  complete=False, started_at=stamp(), commands=[], scopes=[],
                  resource_scope='This invocation includes the two serial Cargo commands and six exact native controls only; it is not full conformance verification.')
    save(output, report)
    try:
        scope()
        report.update(cpu_affinity=[0], before=budget())
        source(args.expected_source)
        save(output, report)
        for label, target, ids in [
            ('aot-exact-four', ['-p', 'scriptw-aot-wasm', '--lib'], AOT_IDS),
            ('realm-exact-two', ['-p', 'scriptw-engine', '--features', 'benchmark-module-images', '--test', 'aot_realm_modules'], REALM_IDS),
        ]:
            source(args.expected_source)
            row = dict(label=label, command=['cargo', 'test', '--locked', '--release', '--jobs', '1', *target,
                                           '--', '--exact', '--include-ignored', '--test-threads=1', '--color=never', *ids],
                       stdout=label + '.stdout', stderr=label + '.stderr', expected_test_ids=list(ids))
            report['commands'].append(row)
            save(output, report)
            try:
                run_command(row, output, timeout=10800)
            finally:
                save(output, report)
            require(row.get('exit_code') == 0 and not row['timed_out'] and not row['output_limit_exceeded']
                    and not row.get('pipe_cleanup_incomplete'), label + ' failed; raw streams retained')
            text = (output / row['stdout']).read_text()
            names = re.findall(r'^test ([^\r\n]+) \.\.\. ok$', text, re.M)
            summaries = result_summaries(text)
            require(len(names) == len(set(names)) == len(ids) and set(names) == set(ids), 'exact native passing-name inventory differs')
            require(len(summaries) == 1 and summaries[0][:5] == ('ok', str(len(ids)), '0', '0', '0'), 'positive native result summary differs')
            report['scopes'].append(dict(label=label, target_arguments=target, expected_test_ids=list(ids),
                                         passed_test_ids=names, passed=len(names), summary=summaries[0], complete=True))
            save(output, report)
        source(args.expected_source)
        after = budget()
        report['after'] = after
        no_oom(report['before'], after)
        require([scope['passed'] for scope in report['scopes']] == [4, 2], 'all six focused native controls are required')
        report.update(complete=True, passed=6, finished_at=stamp())
        return 0
    except Exception as error:
        report.update(error=str(error), error_type=type(error).__name__, finished_at=stamp())
        return 1
    finally:
        if 'before' in report and 'after' not in report:
            try:
                report['after'] = budget()
            except (OSError, ValueError) as error:
                report['resource_after_error'] = str(error)
        save(output, report)
        print(json.dumps(dict(complete=report['complete'], passed=report.get('passed'), error=report.get('error'), output=str(output))), flush=True)


if __name__ == '__main__':
    raise SystemExit(main())
