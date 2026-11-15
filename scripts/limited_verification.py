#!/usr/bin/env python3
"""Run verification under a confirmed cgroup-v2 process-tree memory cap.

Local scopes use at most 4096 MiB, no swap and grouped OOM termination.
An explicit --cloud invocation uses the managed machine's inherited finite
cap instead, read from cgroup v2 and from the cgroup-v1 memory and CPU
hierarchies of a hybrid machine. Cargo builds and test execution remain
serial. Cloud callers may opt into CPU-quota-bound native compilation with
--cloud-cpus auto, or give explicit payload workers a quota-bound CPU set with
--cloud-worker-cpus auto while native compilation stays serial. Payload worker
counts are preserved.
"""

import argparse
import os
from pathlib import Path

from resource_numbers import positive_decimal
import shutil
import sys


MIB = 1024 * 1024
DEFAULT_MEMORY_MIB = 4096
MAX_RETAINED_MODULES = 1
MAX_RETAINED_MODULE_IMAGE_BYTES = 64 * MIB
CLOUD_MAX_RETAINED_MODULES = 512
CLOUD_MODULE_ENTRY_BUDGET_BYTES = 64 * MIB
CLOUD_MAX_RETAINED_MODULE_IMAGE_BYTES = 2 * 1024 * MIB
CLOUD_MODULE_IMAGE_BUDGET_DIVISOR = 8


def positive_mib(value: str) -> int:
    try:
        amount = positive_decimal(value, "memory MiB")
        if amount > DEFAULT_MEMORY_MIB:
            raise ValueError("memory cap exceeds local policy")
        return amount
    except ValueError as error:
        raise argparse.ArgumentTypeError(
            "memory MiB must be an integer from 1 to 4096"
        ) from error


def inherited_limit(environment: dict, name: str, ceiling: int) -> int:
    raw = environment.get(name)
    if raw is None:
        return ceiling
    return min(positive_decimal(raw.strip(), name), ceiling)


def cloud_module_cache_limits(cloud_budget: int) -> tuple[int, int]:
    """Bound retained modules to a small, measured share of the machine cap."""
    entries = min(
        CLOUD_MAX_RETAINED_MODULES,
        max(1, cloud_budget // CLOUD_MODULE_ENTRY_BUDGET_BYTES),
    )
    image_bytes = min(
        CLOUD_MAX_RETAINED_MODULE_IMAGE_BYTES,
        max(1, cloud_budget // CLOUD_MODULE_IMAGE_BUDGET_DIVISOR),
    )
    return entries, image_bytes


def limited_environment(
    *, cloud_budget: int | None = None, native_jobs: int | None = None
) -> dict:
    environment = dict(os.environ)
    environment.update(
        CARGO_BUILD_JOBS="1",
        SCRIPTW_JOBS="1",
        RUST_TEST_THREADS="1",
        RAYON_NUM_THREADS="1",
    )
    if native_jobs is not None:
        if native_jobs < 1:
            raise ValueError("native compilation jobs must be positive")
        for name in ("SCRIPTW_JOBS", "RAYON_NUM_THREADS"):
            environment[name] = str(inherited_limit(os.environ, name, native_jobs))
        # run-watched invokes capped.sh, whose local default halves affinity.
        # The cloud quota already bounded this set; retain an explicit share.
        environment.setdefault("SCRIPTW_CPU_PERCENT", "100")
    # R's native image exceeds the local 64-MiB retention ceiling. Scale the
    # cloud cache with its inherited finite cap so repeated dynamic programs
    # remain hot without allowing retained images above one eighth of the
    # machine budget. The entry limit reserves no memory by itself.
    if cloud_budget is None:
        entries, image_bytes = MAX_RETAINED_MODULES, MAX_RETAINED_MODULE_IMAGE_BYTES
    else:
        entries, image_bytes = cloud_module_cache_limits(cloud_budget)
    for name, ceiling in (
        ("SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES", entries),
        ("SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES", image_bytes),
    ):
        environment[name] = str(inherited_limit(environment, name, ceiling))
    return environment


def kernel_group() -> Path:
    entries = Path("/proc/self/cgroup").read_text().splitlines()
    unified = [entry[3:] for entry in entries if entry.startswith("0::")]
    if len(unified) != 1 or not unified[0].startswith("/"):
        raise ValueError("a unified cgroup-v2 membership is required")
    relative = Path(unified[0].lstrip("/"))
    if ".." in relative.parts:
        raise ValueError("invalid cgroup-v2 membership")
    return Path("/sys/fs/cgroup") / relative


def serial_affinity() -> None:
    inherited = os.sched_getaffinity(0)
    if not inherited:
        raise ValueError("the inherited CPU set is empty")
    os.sched_setaffinity(0, {min(inherited)})
    if len(os.sched_getaffinity(0)) != 1:
        raise ValueError("serial inherited CPU affinity could not be established")


def require_kernel_budget(requested_bytes: int) -> None:
    group = kernel_group()
    limit = (group / "memory.max").read_text().strip()
    if (
        not limit.isascii()
        or not limit.isdecimal()
        or not 0 < int(limit) <= requested_bytes
    ):
        raise ValueError("the kernel memory.max does not enforce the requested budget")
    if (group / "memory.swap.max").read_text().strip() != "0":
        raise ValueError("the kernel memory.swap.max must be zero")
    if (group / "memory.oom.group").read_text().strip() != "1":
        raise ValueError(
            "the kernel must terminate the whole group on memory exhaustion"
        )
    serial_affinity()


def cgroup_ancestors():
    root = Path("/sys/fs/cgroup")
    group = kernel_group()
    if not group.is_relative_to(root):
        raise ValueError("cgroup membership escaped its controller root")
    while True:
        yield group
        if group == root:
            break
        group = group.parent


def legacy_ancestors(controller: str):
    """Yield the cgroup-v1 groups of one controller, innermost first.

    A hybrid machine keeps a unified membership with no memory or CPU
    controller and enforces its limits in the v1 hierarchies. Nothing is
    yielded when the controller has no v1 membership.
    """
    memberships = []
    for entry in Path("/proc/self/cgroup").read_text().splitlines():
        fields = entry.split(":", 2)
        if len(fields) == 3 and controller in fields[1].split(","):
            memberships.append(fields[2])
    if not memberships:
        return
    if len(memberships) != 1 or not memberships[0].startswith("/"):
        raise ValueError(f"invalid cgroup-v1 {controller} membership")
    relative = Path(memberships[0].lstrip("/"))
    if ".." in relative.parts:
        raise ValueError(f"invalid cgroup-v1 {controller} membership")
    root = Path("/sys/fs/cgroup") / controller
    group = root / relative
    while True:
        yield group
        if group == root:
            break
        group = group.parent


def legacy_value(group: Path, name: str) -> int | None:
    """Read one cgroup-v1 integer. Only the controller root may lack the file."""
    try:
        raw = (group / name).read_text().strip()
    except FileNotFoundError:
        if group.parent == Path("/sys/fs/cgroup"):
            return None
        raise
    digits = raw[1:] if raw.startswith("-") else raw
    if not digits.isascii() or not digits.isdecimal():
        raise ValueError(f"invalid inherited cloud {name}")
    return int(raw)


def legacy_unlimited_bytes() -> int:
    """The v1 memory limit that means no limit: LONG_MAX rounded to a page."""
    page = os.sysconf("SC_PAGE_SIZE")
    return (2**63 - 1) // page * page


def require_cloud_budget(*, serial: bool = True) -> int:
    """Inspect the actual inherited cap, including tighter visible ancestors."""
    limits = []
    for group in cgroup_ancestors():
        try:
            limit = (group / "memory.max").read_text().strip()
        except FileNotFoundError:
            # The host controller root has no limit file. A namespaced root
            # can have one; read it whenever present. Missing delegated scope
            # files still fail, and some visible finite limit remains required.
            if group == Path("/sys/fs/cgroup"):
                continue
            raise
        if limit != "max":
            if not limit.isascii() or not limit.isdecimal() or int(limit) <= 0:
                raise ValueError("invalid inherited cloud memory.max")
            limits.append(int(limit))
    for group in legacy_ancestors("memory"):
        limit = legacy_value(group, "memory.limit_in_bytes")
        if limit is None:
            continue
        if limit <= 0:
            raise ValueError("invalid inherited cloud memory.limit_in_bytes")
        if limit < legacy_unlimited_bytes():
            limits.append(limit)
    if not limits:
        raise ValueError("a finite inherited cloud memory.max is required")
    if serial:
        serial_affinity()
    return min(limits)


def cloud_affinity(*, native_limits: bool) -> int:
    """Use whole CPUs within the inherited affinity and tightest CPU quota."""
    inherited = os.sched_getaffinity(0)
    if not inherited:
        raise ValueError("the inherited CPU set is empty")
    capacity = len(inherited)
    for group in cgroup_ancestors():
        try:
            fields = (group / "cpu.max").read_text().split()
        except FileNotFoundError:
            if group == Path("/sys/fs/cgroup"):
                continue
            raise
        if len(fields) != 2:
            raise ValueError("invalid inherited cloud cpu.max")
        quota, period = fields
        if not period.isascii() or not period.isdecimal() or int(period) <= 0:
            raise ValueError("invalid inherited cloud cpu.max period")
        if quota != "max":
            if not quota.isascii() or not quota.isdecimal() or int(quota) <= 0:
                raise ValueError("invalid inherited cloud cpu.max quota")
            capacity = min(capacity, max(1, int(quota) // int(period)))
    for group in legacy_ancestors("cpu"):
        quota = legacy_value(group, "cpu.cfs_quota_us")
        period = legacy_value(group, "cpu.cfs_period_us")
        if quota is None and period is None:
            continue
        if quota is None or period is None or period <= 0 or quota == 0 or quota < -1:
            raise ValueError("invalid inherited cloud cpu.cfs_quota_us")
        if quota != -1:
            capacity = min(capacity, max(1, quota // period))
    if native_limits:
        for name in ("SCRIPTW_JOBS", "RAYON_NUM_THREADS"):
            capacity = inherited_limit(os.environ, name, capacity)
    selected = set(sorted(inherited)[:capacity])
    os.sched_setaffinity(0, selected)
    if os.sched_getaffinity(0) != selected:
        raise ValueError("cloud CPU affinity could not be established")
    return len(selected)


def cloud_native_affinity() -> int:
    return cloud_affinity(native_limits=True)


def cloud_worker_affinity() -> int:
    return cloud_affinity(native_limits=False)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--memory-mib",
        type=positive_mib,
        help="local scope budget in MiB (default: 4096)",
    )
    parser.add_argument(
        "--cloud",
        action="store_true",
        help="use the managed cloud machine's inherited finite memory cap",
    )
    cloud_cpus = parser.add_mutually_exclusive_group()
    cloud_cpus.add_argument(
        "--cloud-cpus",
        choices=("auto",),
        help="opt into quota-bound cloud native compilation; tests and Cargo builds stay serial",
    )
    cloud_cpus.add_argument(
        "--cloud-worker-cpus",
        choices=("auto",),
        help="give explicit payload workers quota-bound cloud CPUs; native compilation, tests and Cargo builds stay serial",
    )
    parser.add_argument("--in-scope", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    if not args.command or args.command[0] != "--" or len(args.command) == 1:
        parser.error("a command is required after --")
    command = args.command[1:]
    if args.cloud and (args.memory_mib is not None or args.in_scope):
        parser.error("--cloud cannot be combined with local scope options")
    if args.cloud_cpus is not None and not args.cloud:
        parser.error("--cloud-cpus requires --cloud")
    if args.cloud_worker_cpus is not None and not args.cloud:
        parser.error("--cloud-worker-cpus requires --cloud")
    memory_mib = args.memory_mib or DEFAULT_MEMORY_MIB
    budget = memory_mib * MIB
    try:
        if not sys.platform.startswith("linux"):
            raise ValueError(
                "Linux cgroup-v2 memory control is required; verification was not started"
            )
        if args.cloud:
            cap = require_cloud_budget(
                serial=args.cloud_cpus is None and args.cloud_worker_cpus is None
            )
            cpus = 1
            native_jobs = None
            if args.cloud_cpus is not None:
                cpus = native_jobs = cloud_native_affinity()
            elif args.cloud_worker_cpus is not None:
                cpus = cloud_worker_affinity()
                native_jobs = 1
            environment = limited_environment(cloud_budget=cap, native_jobs=native_jobs)
            print(
                f"limited-verification: confirmed inherited cloud memory cap {cap} bytes, "
                f"{cpus} CPU(s), serial Cargo/test defaults, "
                f"native compilation defaults up to {environment['SCRIPTW_JOBS']} jobs; retained module cache "
                f"{environment['SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES']} entries / "
                f"{environment['SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES']} image bytes",
                file=sys.stderr,
                flush=True,
            )
            os.execvpe(command[0], command, environment)
        environment = limited_environment()
        if args.in_scope:
            require_kernel_budget(budget)
            print(
                f"limited-verification: confirmed kernel memory cap {memory_mib} MiB, "
                "swap 0, one CPU and serial worker defaults; retained module cache "
                f"{environment['SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES']} entry / "
                f"{environment['SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES']} image bytes",
                file=sys.stderr,
                flush=True,
            )
            os.execvpe(command[0], command, environment)
        manager = shutil.which("systemd-run")
        if manager is None:
            raise ValueError("systemd-run is required; verification was not started")
        invocation = [
            manager,
            "--user",
            "--scope",
            "--collect",
            "--quiet",
            "--no-ask-password",
            "--expand-environment=no",
            f"--property=MemoryMax={budget}",
            "--property=MemorySwapMax=0",
            "--property=OOMPolicy=kill",
            "--",
            sys.executable,
            str(Path(__file__).resolve()),
            "--in-scope",
            "--memory-mib",
            str(memory_mib),
            "--",
            *command,
        ]
        os.execvpe(manager, invocation, environment)
    except (OSError, ValueError) as error:
        print(f"limited-verification: {error}; no uncapped fallback", file=sys.stderr)
        return 2
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
