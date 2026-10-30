"""Run complete Cargo-selected native scopes with source-bound exact receipts.

Run from the repository root under limited_verification.py --cloud. Cargo stays
serial; up to four native targets use one CPU and one test thread each. Original
worker/campaign deadlines, publication, performance and rustdoc scopes remain
idle-exclusive. Explicitly isolated test roots use their own Cargo cohort while
retaining every selected target. --differential-native-cpus auto permits
quota-bound compiler/admission work only inside idle-exclusive differential
scopes, recording actual CPUs and jobs. --native-target-cpus auto plus exact
--native-cpu-target PACKAGE:TARGET opts selected complete independent scopes
and the whole fake-suite publication target into the same idle-exclusive
compiler CPU policy, keeping libtest serial.
--partition-cli inventories and executes every qualified CLI
test exactly once across disjoint processes. --partition-native-target names an
opt-in complete native scope, such as scriptw-engine:aot_intl; original native
watcher allowances and existing timing/campaign barriers remain intact.
"""

import argparse, hashlib, json, os, re, shutil, subprocess, sys, time, gzip
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from workspace_test_schedule import (
    execution_order,
    is_barrier,
    is_partitioned_cli,
    is_partitioned_native,
    validate_partition_native_targets,
    native_stall_seconds,
    differential_native_compilation,
    NATIVE_RESOURCE_EXEC,
    validate_native_cpu_targets,
    native_cpu_target,
    validate_native_resources,
)
from workspace_test_storage import storage_snapshot
from verification_identity import source_map
import workspace_feature_roots as feature_roots
from workspace_test_artifacts import (
    WorkspaceInventory,
    continuation_cases,
    build_script_artifact,
    doc_cohorts,
    isolated_test_packages,
    package_selection,
    selector_key,
    selected_cohort_selectors,
    selected_target_members,
    test_artifacts,
)


def parse_args(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", choices=["debug", "release"], default="debug")
    parser.add_argument("--plan-only", action="store_true")
    parser.add_argument("--packages", nargs="*")
    parser.add_argument("--isolate-test-package", action="append", default=[])
    parser.add_argument("--output", required=True)
    parser.add_argument("--all-features", action="store_true")
    parser.add_argument("--include-ignored", action="store_true")
    parser.add_argument("--test-processes", type=int, default=1)
    parser.add_argument("--shared-runtime-linker", type=Path)
    parser.add_argument("--shared-runtime-linker-contract", type=Path)
    parser.add_argument("--strip-test-debug", action="store_true")
    parser.add_argument("--no-test-incremental", action="store_true")
    parser.add_argument(
        "--preserve-test-artifacts",
        action="store_true",
        help="Keep verified completed and unselected Cargo test outputs for ordinary Cargo reuse",
    )
    parser.add_argument("--retry-from", type=Path)
    parser.add_argument("--reuse-library-build-from", type=Path)
    parser.add_argument("--restored-library-provenance", type=Path)
    parser.add_argument("--retry-interrupted", action="store_true")
    parser.add_argument("--only-targets", nargs="*")
    parser.add_argument("--native-filter", action="append", default=[])
    parser.add_argument("--stop-on-build-failure", action="store_true")
    parser.add_argument("--partition-cli", action="store_true")
    parser.add_argument("--cli-partition-count", type=int, choices=range(1, 5),
                        help="Optional sequential CLI chunks; live libtest processes stay bounded by --test-processes")
    parser.add_argument("--partition-native-target", action="append", default=[])
    parser.add_argument(
        "--continuation-phase", choices=["parallel", "barriers", "all"], default="all"
    )
    parser.add_argument(
        "--build-selected-together",
        action="store_true",
        help="Build the union of selected normal native selectors once; preserve every per-target execution and isolated package graph",
    )
    parser.add_argument(
        "--selected-feature-roots",
        action="store_true",
        help="Narrow native Cargo roots with an empty auxiliary and prove each selected root matches the ordinary workspace graph",
    )
    parser.add_argument(
        "--differential-native-cpus",
        choices=["auto"],
        help="explicit quota-bound native compilation in idle-exclusive differential scopes; libtest/cases remain serial",
    )
    parser.add_argument(
        "--idle-builds",
        action="store_true",
        help="Wait for owned native tests before each uncached Cargo build so LLVM starts with the full bounded CPU budget",
    )
    parser.add_argument(
        "--native-target-cpus",
        choices=["auto"],
        help="opt exact complete native targets into quota-bound compilation CPUs; libtest stays serial",
    )
    parser.add_argument(
        "--native-cpu-target",
        action="append",
        default=[],
        metavar="PACKAGE:TARGET",
        help="exact scope owning native compilation CPUs exclusively; repeat for multiple scopes",
    )
    return parser.parse_args(argv)


def validate_args(args):
    if not (bool(args.native_target_cpus) == bool(args.native_cpu_target)):
        raise ValueError(
            "Native compiler CPUs require both --native-target-cpus auto and exact --native-cpu-target scopes"
        )
    if args.selected_feature_roots:
        if not (
            not (
                args.all_features
                or args.packages
                or (set(args.isolate_test_package) - {"scriptw-intl"})
                or args.retry_from
                or args.reuse_library_build_from
                or args.restored_library_provenance
            )
        ):
            raise ValueError(
                "Feature-root mode currently requires the default workspace graph, default isolation and a fresh receipt"
            )
    native_filters = dict(row.split("=", 1) for row in args.native_filter)
    if args.cli_partition_count is not None and not args.partition_cli:
        raise ValueError("CLI partition count requires --partition-cli")
    if args.cli_partition_count is not None and args.cli_partition_count < args.test_processes:
        raise ValueError("CLI partition count cannot leave an admitted CPU unused")
    if args.partition_cli:
        if not (args.include_ignored):
            raise ValueError("Complete CLI partitions require --include-ignored")
        if not ("scriptw-cli:cli" not in native_filters):
            raise ValueError("Complete CLI partitions cannot use a native filter")
    return native_filters


def plan_inventory(metadata, args, native_filters):
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] in members]
    priority = [
        "scriptw-front",
        "scriptw-ir",
        "scriptw-aot-wasm",
        "scriptw-engine",
        "scriptw-cli",
    ]
    packages.sort(
        key=lambda p: (
            priority.index(p["name"]) if p["name"] in priority else len(priority),
            p["name"],
        )
    )
    if args.packages:
        if not (set(args.packages) <= {p["name"] for p in packages}):
            raise ValueError(
                "Verification condition failed: set(args.packages) <= {p['name'] for p in packages}"
            )
        packages = [p for p in packages if p["name"] in args.packages]
    isolated_packages = isolated_test_packages(packages, args.isolate_test_package)
    if not (set(isolated_packages) <= {p["name"] for p in packages}):
        raise ValueError("Isolated test packages must belong to the selected workspace")
    if not (not isolated_packages or not args.reuse_library_build_from):
        raise ValueError(
            "Broad historical library carry cannot cross an isolated Cargo graph"
        )
    if not (not (args.build_selected_together and args.reuse_library_build_from)):
        raise ValueError(
            "Historical library carry cannot cross a combined native build graph"
        )
    cases = []
    disabled_targets = []
    for p in packages:
        if args.selected_feature_roots and p["name"] == feature_roots.PACKAGE:
            continue
        enabled = set(p["features"]) if args.all_features else set()
        pending = ["default"] if not args.all_features else []
        while pending:
            feature = pending.pop()
            if feature not in enabled and feature in p["features"]:
                enabled.add(feature)
                pending.extend(p["features"][feature])
        for t in p["targets"]:
            kind = t["kind"][0]
            missing = set(t.get("required-features", [])) - enabled
            if missing:
                disabled_targets.append(
                    {
                        "package": p["name"],
                        "target": t["name"],
                        "missing_required_features": sorted(missing),
                    }
                )
                continue
            if t.get("test") and kind in ["lib", "bin", "test", "bench", "example"]:
                selector = ["--lib"] if kind == "lib" else ["--" + kind, t["name"]]
                cases.append(
                    {
                        "package": p["name"],
                        "target": t["name"],
                        "kind": kind,
                        "selector": selector,
                    }
                )
        if any(t.get("doctest") and "lib" in t["kind"] for t in p["targets"]):
            cases.append(
                {
                    "package": p["name"],
                    "target": "doc",
                    "kind": "doc",
                    "selector": ["--doc"],
                }
            )
    # Front-load the repaired cache and original-budget regressions before the
    # remaining inventory. This changes scheduling, never native case selection.
    first = [
        ("scriptw-aot-wasm", "scriptw_aot_wasm", "lib"),
        ("scriptw-test262", "differential_worker_execution", "test"),
        ("scriptw-test262", "differential_generated_campaign", "test"),
        ("scriptw-test262", "scriptw_test262", "lib"),
        ("scriptw-test262", "differential_corpus", "test"),
    ]

    def order(case):
        key = (case["package"], case["target"], case["kind"])
        return first.index(key) if key in first else len(first)

    cases.sort(key=order)
    full_inventory = list(cases)
    partition_native_targets = validate_partition_native_targets(
        full_inventory,
        args.partition_native_target,
        native_filters,
        args.include_ignored,
    )
    native_cpu_targets = validate_native_cpu_targets(
        full_inventory,
        args.native_cpu_target,
        native_filters,
        args.include_ignored,
        [
            *partition_native_targets,
            *(["scriptw-cli:cli"] if args.partition_cli else []),
        ],
    )
    if not (
        set(native_filters)
        <= {c["package"] + ":" + c["target"] for c in full_inventory}
    ):
        raise ValueError(
            "Verification condition failed: set(native_filters) <= {c['package'] + ':' + c['target'] for c in full_inventory}"
        )
    if args.only_targets:
        requested = set(args.only_targets)
        known = {c["package"] + ":" + c["target"] for c in full_inventory}
        if not (requested <= known):
            raise ValueError(requested - known)
        if not (set(partition_native_targets) <= requested):
            raise ValueError(
                "Explicit native partitions must be selected by --only-targets"
            )
        if not (set(native_cpu_targets) <= requested):
            raise ValueError(
                "Native compiler CPU targets must be selected by --only-targets"
            )
        cases = [c for c in cases if c["package"] + ":" + c["target"] in requested]

    combined_selectors = (
        selected_cohort_selectors(cases, isolated_packages)
        if args.build_selected_together
        else []
    )
    return WorkspaceInventory(
        packages,
        isolated_packages,
        cases,
        disabled_targets,
        full_inventory,
        partition_native_targets,
        native_cpu_targets,
        combined_selectors,
    )


def run(args, root):
    native_filters = validate_args(args)
    out = root / args.output
    out.mkdir(parents=True, exist_ok=False)

    source = source_map(root)
    (out / "source-start.json").write_text(json.dumps(source, sort_keys=True) + "\n")
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--locked",
                "--offline",
            ],
            text=True,
        )
    )
    (out / "metadata.json").write_text(json.dumps(metadata) + "\n")
    cargo_target_dir = Path(metadata["target_directory"]).resolve()
    profile_root = cargo_target_dir / args.profile
    profile_flags = ["--release"] if args.profile == "release" else []

    def evidence_path(path):
        path = Path(path).absolute()
        try:
            return str(path.relative_to(root))
        except ValueError:
            return str(path)

    members = set(metadata["workspace_members"])
    package_ids = {p["name"]: p["id"] for p in metadata["packages"]}
    (
        packages,
        isolated_packages,
        cases,
        disabled_targets,
        full_inventory,
        partition_native_targets,
        native_cpu_targets,
        combined_selectors,
    ) = plan_inventory(metadata, args, native_filters)
    result = {
        "verification_driver_sha256": hashlib.sha256(
            Path(__file__).read_bytes()
        ).hexdigest(),
        "source_map_sha256": hashlib.sha256(
            json.dumps(source, sort_keys=True).encode()
        ).hexdigest(),
        "head": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True
        ).strip(),
        "profile": args.profile,
        "target_directory": str(cargo_target_dir),
        "all_features": args.all_features,
        "include_ignored": args.include_ignored,
        "cases": cases,
        "targets_disabled_by_cargo_features": disabled_targets,
        "selected_workspace_packages": [p["name"] for p in packages],
        "isolated_test_packages": isolated_packages,
        "partition_cli": args.partition_cli,
        "cli_partition_count": args.cli_partition_count,
        "partition_native_targets": partition_native_targets,
        "preserve_test_artifacts": args.preserve_test_artifacts,
    }
    result["selected_feature_roots"] = args.selected_feature_roots
    result["differential_native_cpus"] = args.differential_native_cpus
    result["native_target_cpus"] = args.native_target_cpus
    result["native_cpu_targets"] = native_cpu_targets
    schedule_path = Path(__file__).with_name("workspace_test_schedule.py")
    result["workspace_schedule_sha256"] = hashlib.sha256(
        schedule_path.read_bytes()
    ).hexdigest()
    result["idle_builds"] = args.idle_builds
    result["build_selected_together"] = args.build_selected_together
    result["selected_build_selectors"] = combined_selectors
    result["selected_target_members"] = selected_target_members(cases)
    if args.retry_from:
        if not (args.continuation_phase in ("parallel", "barriers")):
            raise AssertionError(
                "Verification condition failed: args.continuation_phase in ('parallel', 'barriers')"
            )
        previous = json.loads(args.retry_from.read_text())
        if not (previous.get("idle_builds", False) == args.idle_builds):
            raise AssertionError("Continuation cannot change idle-build policy")
        if not (
            previous.get("differential_native_cpus") == args.differential_native_cpus
        ):
            raise AssertionError(
                "Continuation cannot change differential native CPU policy"
            )
        if not (
            previous.get("workspace_schedule_sha256")
            == result["workspace_schedule_sha256"]
        ):
            raise AssertionError("Continuation scheduler changed")
        if not (previous.get("native_target_cpus") == args.native_target_cpus):
            raise AssertionError("Continuation cannot change native target CPU policy")
        if not (previous.get("native_cpu_targets", []) == native_cpu_targets):
            raise AssertionError(
                "Continuation cannot change native CPU target ownership"
            )
        if not (previous["head"] == result["head"] and previous["source_unchanged"]):
            raise AssertionError(
                "Verification condition failed: previous['head'] == result['head'] and previous['source_unchanged']"
            )
        if not (previous.get("profile", "debug") == args.profile):
            raise AssertionError("Retry profile changed")
        if not (
            Path(previous.get("target_directory", root / "target")).resolve()
            == cargo_target_dir
        ):
            raise AssertionError("Retry Cargo target directory changed")
        if not (
            previous["all_features"] == args.all_features
            and previous["include_ignored"] == args.include_ignored
        ):
            raise AssertionError(
                "Verification condition failed: previous['all_features'] == args.all_features and previous['include_ignored'] == args.include_ignored"
            )
        if not (
            previous["selected_workspace_packages"]
            == result["selected_workspace_packages"]
        ):
            raise AssertionError(
                "Verification condition failed: previous['selected_workspace_packages'] == result['selected_workspace_packages']"
            )
        if not (previous.get("isolated_test_packages", []) == isolated_packages):
            raise AssertionError(
                "Verification condition failed: previous.get('isolated_test_packages', []) == isolated_packages"
            )
        if not (
            previous.get("partition_native_targets", []) == partition_native_targets
        ):
            raise AssertionError("Retry native partition policy changed")
        if not (
            previous.get("partition_cli", args.partition_cli) == args.partition_cli
        ):
            raise AssertionError("Retry CLI partition policy changed")
        if previous.get("cli_partition_count") != args.cli_partition_count:
            raise AssertionError("Retry CLI partition count changed")
        if not (
            previous.get("build_selected_together", False)
            == args.build_selected_together
        ):
            raise AssertionError("Retry combined build policy changed")
        if not (previous.get("selected_build_selectors", []) == combined_selectors):
            raise AssertionError("Retry combined build selector graph changed")
        if not (
            previous.get("selected_target_members") == result["selected_target_members"]
        ):
            raise AssertionError("Retry selected target inventory changed")
        if not (
            previous.get("preserve_test_artifacts", False)
            == args.preserve_test_artifacts
        ):
            raise AssertionError("Retry artifact retention policy changed")
        previous_source = json.loads(
            (args.retry_from.parent / "source-start.json").read_text()
        )
        if not (previous_source == source):
            raise AssertionError(
                "Verification condition failed: previous_source == source"
            )
        cases[:] = continuation_cases(
            cases,
            previous["cases"],
            retry_interrupted=args.retry_interrupted,
            barrier_phase=args.continuation_phase == "barriers",
            is_barrier=lambda case: is_barrier(case)
            or native_cpu_target(case, native_cpu_targets),
        )
        result["continuation_phase"] = args.continuation_phase
        result["retry_input"] = {
            "path": str(args.retry_from),
            "sha256": hashlib.sha256(args.retry_from.read_bytes()).hexdigest(),
            "policy": "exact previously unstarted, explicitly interrupted or storage-deferred whole targets from immutable same-source input; parallel excludes docs/timing; barriers externally idle; original outcomes preserved",
        }
    build_env = os.environ.copy()
    if not (
        not args.idle_builds
        or (
            args.no_test_incremental
            and build_env.get("SCRIPTW_TEST_CODEGEN_JOBS") == "auto"
        )
    ):
        raise AssertionError(
            "Idle builds require --no-test-incremental and SCRIPTW_TEST_CODEGEN_JOBS=auto"
        )
    if args.no_test_incremental:
        build_env["SCRIPTW_TEST_NO_INCREMENTAL"] = "1"
    if args.shared_runtime_linker:
        linker = args.shared_runtime_linker.resolve()
        if not (linker.name == "cc" and linker.is_file()):
            raise AssertionError(
                "Verification condition failed: linker.name == 'cc' and linker.is_file()"
            )
        build_env["PATH"] = str(linker.parent) + os.pathsep + build_env["PATH"]
        if args.strip_test_debug:
            build_env["SCRIPTW_TEST_STRIP_DEBUG"] = "1"
        if args.shared_runtime_linker_contract:
            contract_path = args.shared_runtime_linker_contract.resolve()
            contract = json.loads(contract_path.read_text())
            if not (contract["profile"] == args.profile):
                raise AssertionError("Shared linker profile differs")
            if not (Path(contract["target_directory"]).resolve() == cargo_target_dir):
                raise AssertionError("Shared linker target directory differs")
            if not (
                Path(contract["native_output_directory"]).resolve()
                == profile_root / "deps"
            ):
                raise AssertionError("Shared linker native output directory differs")
            if not (
                Path(contract["shared_runtime_directory"]).resolve()
                == cargo_target_dir / "shared-test-runtime-images"
            ):
                raise AssertionError("Shared linker image directory differs")
            files = [Path(row["path"]).resolve() for row in contract["files"]]
            if not (len(files) == len(set(files)) and linker in files):
                raise AssertionError(
                    "Verification condition failed: len(files) == len(set(files)) and linker in files"
                )
            if not (
                {p.name for p in files}
                == {
                    "cc",
                    "shared_image.py",
                    "strip_debug.py",
                }
            ):
                raise AssertionError(
                    "Verification condition failed: {p.name for p in files} == {'cc', 'shared_image.py', 'strip_debug.py'}"
                )
            if not (
                all(
                    hashlib.sha256(Path(row["path"]).read_bytes()).hexdigest()
                    == row["sha256"]
                    for row in contract["files"]
                )
            ):
                raise AssertionError("Shared linker contract bytes changed")
            files.append(contract_path)
        else:
            if not (args.profile == "debug"):
                raise AssertionError(
                    "An external release linker needs an explicit bound contract"
                )
            if not ((linker.parents[2] / "target").resolve() == cargo_target_dir):
                raise AssertionError(
                    "Adjacent debug linker targets a different Cargo directory"
                )
            files = [
                linker,
                linker.with_name("shared_image.py"),
                linker.with_name("strip_debug.py"),
            ]
        result["shared_runtime_linker"] = {
            "profile": args.profile,
            "target_directory": str(cargo_target_dir),
            "files": {
                evidence_path(p): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in files
            },
            "policy": "Only Rust libtest ELF outputs in the selected Cargo profile directory share immutable data; ordinary product and doc links retain static archive.",
        }
    else:
        if not (not args.shared_runtime_linker_contract):
            raise AssertionError("A linker contract requires its actual cc")
    if args.no_test_incremental:
        wrapper = (root / "scripts/test-shared-runtime-images/test-rustc").resolve()
        if not (not build_env.get("RUSTC_WRAPPER")):
            raise AssertionError("Cannot replace an existing compiler wrapper")
        build_env["RUSTC_WRAPPER"] = str(wrapper)
        result["test_incremental_policy"] = {
            "wrapper": evidence_path(wrapper),
            "sha256": hashlib.sha256(wrapper.read_bytes()).hexdigest(),
            "retained_normal_incremental_crates": (
                ["scriptw_intl"]
                if build_env.get("SCRIPTW_TEST_KEEP_INTL_INCREMENTAL") == "1"
                else []
            ),
            "policy": "Verification compiler units omit incremental scratch except explicitly retained normal Intl units; libtest scratch is always omitted, queries stay literal and compiler semantics, profiles and verdicts remain intact.",
        }

    def release_incremental(case):
        # No deletion needs no Cargo lock. In particular, library namespaces are
        # always preserved and --no-test-incremental usually leaves no matches.
        # Skipping an empty cleanup is safe if another producer creates a cache
        # after this probe: we do not touch it. Actual deletion still holds the
        # Cargo lock and discovers the current candidates again under that lock.
        prefix = case["target"].replace("-", "_") + "-"
        root = profile_root / "incremental"
        case["released_incremental"] = []
        if case["kind"] == "lib" or not any(p.is_dir() for p in root.glob(prefix + "*")):
            return
        with (profile_root / ".cargo-lock").open("a+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            for cache in root.glob(prefix + "*"):
                if cache.is_dir():
                    shutil.rmtree(cache)
                    case["released_incremental"].append(cache.name)

    def save():
        if not (
            hashlib.sha256(schedule_path.read_bytes()).hexdigest()
            == result["workspace_schedule_sha256"]
        ):
            raise AssertionError("Scheduler changed during verification")
        result["source_unchanged"] = source == source_map(root)
        result["shared_runtime_linker_unchanged"] = all(
            hashlib.sha256((root / path).read_bytes()).hexdigest() == digest
            for path, digest in result.get("shared_runtime_linker", {})
            .get("files", {})
            .items()
        )
        if not (result["shared_runtime_linker_unchanged"]):
            raise AssertionError(
                "Shared linker/helper contract changed during verification"
            )
        (out / "receipt.json").write_text(json.dumps(result, indent=2) + "\n")

    save()

    # Independent complete native targets can run concurrently, each under one CPU
    # and one libtest thread. Cargo only builds when a native CPU slot is free.
    import fcntl

    available = sorted(os.sched_getaffinity(0))
    if not (
        1
        <= args.test_processes
        <= min(4 if args.strip_test_debug else 2, len(available))
    ):
        raise AssertionError(
            "Verification condition failed: 1 <= args.test_processes <= min(4 if args.strip_test_debug else 2, len(available))"
        )
    if not (not args.strip_test_debug or args.shared_runtime_linker):
        raise AssertionError(
            "Verification condition failed: not args.strip_test_debug or args.shared_runtime_linker"
        )
    slots = available[: args.test_processes]
    result["artifact_selection_policy"] = (
        "Exact Cargo package identity, target name, kind and test profile; normal selectors exclude explicitly isolated test roots; each isolated package has its own exact Cargo cohort; all native IDs execute once."
    )
    result["scheduling"] = {
        "native_cpus": slots,
        "test_processes": args.test_processes,
        "test_threads_per_process": 1,
        "cargo_jobs": 1,
        "strip_test_debug": args.strip_test_debug,
        "idle_builds": args.idle_builds,
        "compilation_policy": (
            "uncached Cargo builds wait for owned native tests to finish, then start with the full quota-bound parent budget; cached native targets may still overlap"
            if args.idle_builds
            else "all quota-bound inherited parent CPUs when idle; one free CPU while native targets run; the build tree expands to the parent budget once every native wrapper has exited"
        ),
    }
    active = {}
    from workspace_build_cpus import expand_build_tree

    def group_key(case):
        if args.selected_feature_roots:
            selected = feature_roots.build_key(
                case, isolated_packages, args.build_selected_together
            )
            if selected:
                return selected
        return selector_key(case, isolated_packages, combined_selectors)

    def cargo_test_base(case):
        if args.selected_feature_roots and feature_roots.build_key(
            case, isolated_packages, args.build_selected_together
        ):
            return feature_roots.command(
                case,
                cases,
                isolated_packages,
                args.build_selected_together,
                profile_flags,
            )
        flags = ["--all-features"] if args.all_features else []
        selection = (
            ["-p", case["package"]]
            if case["kind"] == "doc" or case["package"] in isolated_packages
            else package_selection(args.packages, isolated_packages)
        )
        selectors = (
            [word for selector in combined_selectors for word in selector]
            if group_key(case) == ("selected-native", "*")
            else case["selector"]
        )
        return [
            "cargo",
            "test",
            "--locked",
            "--offline",
            *profile_flags,
            *selection,
            *flags,
            *selectors,
        ]

    if args.selected_feature_roots:
        result["selected_feature_root_groups"] = [
            {
                "group": list(group),
                "cargo_command": cargo_test_base(
                    next(c for c in cases if group_key(c) == group)
                ),
                "members": selected_target_members(
                    [c for c in cases if group_key(c) == group]
                ),
            }
            for group in sorted(
                {
                    group_key(c)
                    for c in cases
                    if c["kind"] != "doc" and c["package"] not in isolated_packages
                }
            )
        ]
        result["selected_build_selector_policy"] = (
            "Declared selected-selector union; actual fast builds use the exact per-owner groups above, not a workspace-wide selector cohort."
        )

    def profile_test_artifacts(messages, expected):
        return test_artifacts(messages, expected, profile_root=profile_root)

    if args.plan_only:
        if not (not args.reuse_library_build_from):
            raise AssertionError("Planning does not admit carried native outputs")
        result["prepared_only"] = True
        result["native_executed"] = False
        result["native_acceptance"] = False
        result["command_plan"] = [
            {
                "package": case["package"],
                "target": case["target"],
                "kind": case["kind"],
                "cargo_command": cargo_test_base(case),
                "build_command": cargo_test_base(case)
                + (
                    []
                    if case["kind"] == "doc"
                    else ["--no-run", "--message-format=json-render-diagnostics"]
                ),
                "inventory_arguments": ["--list"],
                "execution_arguments": ["--test-threads=1"]
                + (["--include-ignored"] if args.include_ignored else []),
                "barrier": is_barrier(case)
                or is_partitioned_native(
                    case, args.partition_cli, partition_native_targets
                )
                or native_cpu_target(case, native_cpu_targets),
                "partitioned_cli": is_partitioned_cli(case, args.partition_cli),
                "partitioned_native": is_partitioned_native(
                    case, args.partition_cli, partition_native_targets
                ),
                "build_group": (
                    ":".join(group_key(case)) if case["kind"] != "doc" else None
                ),
            }
            for _, case in execution_order(
                cases, args.partition_cli, partition_native_targets, native_cpu_targets
            )
        ]
        result["documentation_cohort_plan"] = [
            {
                "group": group,
                "packages": sorted(names),
                "inventory_command": [
                    "cargo",
                    "test",
                    "--locked",
                    "--offline",
                    *profile_flags,
                    *selection,
                    *(["--all-features"] if args.all_features else []),
                    "--doc",
                    "--no-fail-fast",
                    "--",
                    "--list",
                ],
                "execution_command": [
                    "cargo",
                    "test",
                    "--locked",
                    "--offline",
                    *profile_flags,
                    *selection,
                    *(["--all-features"] if args.all_features else []),
                    "--doc",
                    "--no-fail-fast",
                    "--",
                    "--test-threads=1",
                    *(["--include-ignored"] if args.include_ignored else []),
                ],
            }
            for group, selection, names in doc_cohorts(
                full_inventory, args.packages, isolated_packages
            )
        ]
        save()
        print(
            json.dumps(
                {
                    "prepared_only": True,
                    "native_executed": False,
                    "profile": args.profile,
                    "target_directory": str(cargo_target_dir),
                    "selected_targets": len(cases),
                },
                sort_keys=True,
            ),
            flush=True,
        )
        return 0
    artifact_cohorts = {}
    build_groups = {}
    result["build_groups"] = build_groups

    # A successful cold library cohort may have unexecuted siblings after a disk
    # deferral. Carry only those exact outputs, from this unchanged source, without
    # asking Cargo to regenerate already-retired unrelated libraries.
    if args.reuse_library_build_from:
        carry_path = args.reuse_library_build_from
        carry = json.loads(carry_path.read_text())
        if not (carry["source_unchanged"] and carry["head"] == result["head"]):
            raise AssertionError(
                "Verification condition failed: carry['source_unchanged'] and carry['head'] == result['head']"
            )
        if not (carry.get("profile", "debug") == args.profile):
            raise AssertionError("Library carry profile changed")
        if not (
            Path(carry.get("target_directory", root / "target")).resolve()
            == cargo_target_dir
        ):
            raise AssertionError("Library carry Cargo target directory changed")
        if not (
            carry["all_features"] == args.all_features
            and carry["include_ignored"] == args.include_ignored
        ):
            raise AssertionError(
                "Verification condition failed: carry['all_features'] == args.all_features and carry['include_ignored'] == args.include_ignored"
            )
        if not (
            carry["selected_workspace_packages"]
            == result["selected_workspace_packages"]
        ):
            raise AssertionError(
                "Verification condition failed: carry['selected_workspace_packages'] == result['selected_workspace_packages']"
            )
        if not (
            json.loads((carry_path.parent / "source-start.json").read_text()) == source
        ):
            raise AssertionError(
                "Verification condition failed: json.loads((carry_path.parent / 'source-start.json').read_text()) == source"
            )
        library_group = carry["build_groups"]["lib:*"]
        archive = next(
            row
            for row in library_group["sealed_build_logs"]
            if row["path"].endswith(".build.jsonl.gz")
        )
        packed = root / archive["path"]
        if not (
            hashlib.sha256(packed.read_bytes()).hexdigest() == archive["gzip_sha256"]
        ):
            raise AssertionError(
                "Verification condition failed: hashlib.sha256(packed.read_bytes()).hexdigest() == archive['gzip_sha256']"
            )
        raw = gzip.decompress(packed.read_bytes())
        if not (hashlib.sha256(raw).hexdigest() == archive["uncompressed_sha256"]):
            raise AssertionError(
                "Verification condition failed: hashlib.sha256(raw).hexdigest() == archive['uncompressed_sha256']"
            )
        messages = [json.loads(line) for line in raw.splitlines()]
        if not (
            any(
                m.get("reason") == "build-finished" and m.get("success") is True
                for m in messages
            )
        ):
            raise AssertionError(
                "Verification condition failed: any((m.get('reason') == 'build-finished' and m.get('success') is True for m in messages))"
            )
        expected = [
            (package_ids[c["package"]], c["target"], c["kind"])
            for c in full_inventory
            if c["kind"] == "lib"
        ]
        outputs = profile_test_artifacts(messages, expected)
        wanted = {
            (package_ids[c["package"]], c["target"], c["kind"])
            for c in cases
            if c["kind"] == "lib"
        }
        admitted = {}
        evidence = []
        restored_rows = {}
        if args.restored_library_provenance:
            restoration = json.loads(args.restored_library_provenance.read_text())
            archive_path = root / restoration["archive"]
            if not (
                hashlib.sha256(archive_path.read_bytes()).hexdigest()
                == restoration["archive_sha256"]
            ):
                raise AssertionError(
                    "Verification condition failed: hashlib.sha256(archive_path.read_bytes()).hexdigest() == restoration['archive_sha256']"
                )
            restored_rows = {row["path"]: row for row in restoration["rows"]}
            result["restored_library_provenance"] = {
                "path": str(args.restored_library_provenance),
                "sha256": hashlib.sha256(
                    args.restored_library_provenance.read_bytes()
                ).hexdigest(),
                "policy": "Only byte-identical restored ELF outputs subsequently selected as fresh by normal Cargo in the current-source completed library build. Cargo freshness metadata was never changed; all native cases execute anew.",
            }
        for key in sorted(wanted):
            path = outputs[key]
            matching = [
                m
                for m in messages
                if m.get("reason") == "compiler-artifact"
                and m.get("executable") == str(path)
                and m.get("profile", {}).get("test")
            ]
            if not (len(matching) == 1):
                raise AssertionError(
                    "Verification condition failed: len(matching) == 1"
                )
            if matching[0]["fresh"]:
                original = restored_rows[str(path)]
                if not (
                    original["sha256"] == hashlib.sha256(path.read_bytes()).hexdigest()
                ):
                    raise AssertionError(
                        "Verification condition failed: original['sha256'] == hashlib.sha256(path.read_bytes()).hexdigest()"
                    )
                if not (original["bytes"] == path.stat().st_size):
                    raise AssertionError(
                        "Verification condition failed: original['bytes'] == path.stat().st_size"
                    )
            if not (
                path.is_file()
                and path.parent.resolve() == (profile_root / "deps").resolve()
            ):
                raise AssertionError(
                    "Verification condition failed: path.is_file() and path.parent.resolve() == (profile_root / 'deps').resolve()"
                )
            previous_case = next(
                c
                for c in carry["cases"]
                if (package_ids[c["package"]], c["target"], c["kind"]) == key
            )
            if not (
                previous_case.get("state")
                in (
                    "storage-deferred",
                    None,
                )
            ):
                raise AssertionError("Never carry an executed or failed output")
            admitted[key] = path
            evidence.append(
                {
                    "package_id": key[0],
                    "target": key[1],
                    "kind": key[2],
                    "path": evidence_path(path),
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                    "bytes": path.stat().st_size,
                    "cargo_fresh": matching[0]["fresh"],
                }
            )
        if not (admitted):
            raise AssertionError("Verification condition failed: admitted")
        cases.sort(key=lambda case: 0 if case["kind"] == "lib" else 1)
        result["scheduling"][
            "cached_library_priority"
        ] = "Run never-executed validated library outputs first; their retirement releases space without another link. Exact whole-target inventory is unchanged."
        artifact_cohorts[("lib", "*")] = admitted
        build_groups["lib:*"] = {
            **library_group,
            "carried_unexecuted_outputs": evidence,
            "carry_input": {
                "path": str(carry_path),
                "sha256": hashlib.sha256(carry_path.read_bytes()).hexdigest(),
                "policy": "Same exact source and feature graph; successful sealed Cargo stream; only never-executed exact Cargo-selected library siblings; restored fresh outputs additionally checked byte-for-byte against restoration provenance; hashes sealed at this admission.",
            },
        }

    def build_with_completed_cache_retirement(command, stdout, stderr, case):
        # rustc receives the shared incremental root, not its opaque child cache.
        # Do not guess an active unit's child or delete caches during Cargo.
        case["completed_unit_cache_retirement"] = []
        run = subprocess.Popen(command, stdout=stdout, stderr=stderr, env=build_env)
        started = time.monotonic()
        next_check = started
        idle_budget_released = False
        events = case.setdefault("idle_build_cpu_events", [])
        while run.poll() is None:
            now = time.monotonic()
            if not idle_budget_released and now >= next_check:
                next_check = now + 1
                # Do not call complete() while Cargo owns .cargo-lock: native ELF
                # retirement remains after the build. poll() only observes exit.
                if all(entry["run"].poll() is not None for entry in active.values()):
                    try:
                        program = (
                            (Path("/proc") / str(run.pid) / "cmdline")
                            .read_bytes()
                            .split(b"\0")[0]
                        )
                    except FileNotFoundError:
                        program = b""
                    # Wait until taskset/limited_verification have exec'd Cargo;
                    # otherwise the launcher could reapply its serial restriction.
                    if Path(os.fsdecode(program)).name != "cargo":
                        time.sleep(0.1)
                        continue
                    changes = expand_build_tree(run.pid, available)
                    idle_budget_released = True
                    if changes:
                        events.append(
                            {
                                "elapsed_seconds": round(now - started, 3),
                                "native_wrappers_exited": True,
                                "changes": changes,
                            }
                        )
                        print(
                            "IDLE BUILD CPU RELEASE",
                            run.pid,
                            len(changes),
                            "task masks",
                            available,
                            flush=True,
                        )
            time.sleep(0.1)
        return run.returncode

    def seal_build_logs(case, paths):
        archives = []
        for path in paths:
            if not path.exists():
                continue
            original_sha = hashlib.sha256(path.read_bytes()).hexdigest()
            packed = path.with_suffix(path.suffix + ".gz")
            with path.open("rb") as src, gzip.open(
                packed, "wb", compresslevel=1
            ) as dst:
                shutil.copyfileobj(src, dst)
            archives.append(
                {
                    "path": evidence_path(packed),
                    "uncompressed_sha256": original_sha,
                    "gzip_sha256": hashlib.sha256(packed.read_bytes()).hexdigest(),
                    "uncompressed_bytes": path.stat().st_size,
                    "compressed_bytes": packed.stat().st_size,
                }
            )
            path.unlink()
        case["sealed_build_logs"] = archives
        if case.get("build_group") and archives:
            build_groups[case["build_group"]]["sealed_build_logs"] = archives

    last_progress_at = time.monotonic()
    last_progress_sizes = {}

    def forward_progress():
        nonlocal last_progress_at
        if time.monotonic() - last_progress_at < 20:
            return
        sizes = {
            entry["label"]: entry["watch"].stat().st_size
            for entry in active.values()
            if entry["watch"].exists()
        }
        changed = {
            label: size
            for label, size in sizes.items()
            if last_progress_sizes.get(label) != size
        }
        if changed:
            print("PROGRESS", json.dumps(changed, sort_keys=True), flush=True)
            last_progress_sizes.update(changed)
            last_progress_at = time.monotonic()

    def finish_unselected_artifacts(artifacts, wanted):
        retired = []
        preserved = []
        for key, path in artifacts.items():
            if key in wanted:
                continue
            directory = "examples" if key[2] == "example" else "deps"
            if path.parent.resolve() != (profile_root / directory).resolve():
                raise AssertionError(
                    "Unselected native artifact belongs to another profile or target directory"
                )
            row = {
                "package_id": key[0],
                "target": key[1],
                "kind": key[2],
                "path": evidence_path(path),
                "bytes": path.stat().st_size,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "reason": "Unselected Cargo output; no native execution or verdict claimed.",
            }
            if args.preserve_test_artifacts:
                preserved.append(row)
            else:
                path.unlink()
                retired.append(row)
        return retired, preserved

    def complete(cpu):
        entry = active[cpu]
        run = entry["run"]
        status = run.poll()
        if status is None:
            return False
        entry["stream"].close()
        case = entry["case"]
        watch = entry["watch"]
        log = entry["log"]
        transcript = watch.read_text() if watch.exists() else log.read_text()
        matches = re.findall(
            r"^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;",
            transcript,
            re.M,
        )
        outcomes = [
            dict(zip(["passed", "failed", "ignored"], map(int, row))) for row in matches
        ]
        observed = sum(sum(row.values()) for row in outcomes)
        ignored = sum(row["ignored"] for row in outcomes)
        failed = sum(row["failed"] for row in outcomes)
        matches_inventory = (
            bool(outcomes)
            and observed == case["inventoried_tests"]
            and ignored == 0
            and failed == 0
        )
        case.update(
            state="passed" if status == 0 and matches_inventory else "failed",
            exit_code=status,
            inventory_matches=matches_inventory,
            seconds=round(time.monotonic() - entry["start"], 3),
            outcomes=outcomes,
            transcript_log=evidence_path(watch),
        )
        if watch.exists():
            case["transcript_sha256"] = hashlib.sha256(watch.read_bytes()).hexdigest()
        if case.get("execution_resource_log"):
            resource_path = root / case["execution_resource_log"]
            if resource_path.exists():
                resource = json.loads(resource_path.read_text())
                case["execution_resources"] = resource
                case["execution_resource_sha256"] = hashlib.sha256(
                    resource_path.read_bytes()
                ).hexdigest()
                validate_native_resources(resource, case["command"], available)
            elif case["state"] == "passed":
                raise AssertionError(
                    "Native scope passed without its actual CPU/job receipt"
                )
        if entry.get("partition_receipt"):
            partition_path = entry["partition_receipt"]
            if partition_path.exists():
                partition_evidence = json.loads(partition_path.read_text())
                case["native_partitions"] = {
                    "path": evidence_path(partition_path),
                    "sha256": hashlib.sha256(partition_path.read_bytes()).hexdigest(),
                    "evidence": partition_evidence,
                }
                if case["state"] == "passed":
                    if not (
                        partition_evidence["inventory_matches"]
                        and partition_evidence["complete_test_count"]
                        == case["inventoried_tests"]
                    ):
                        raise AssertionError(
                            "Verification condition failed: partition_evidence['inventory_matches'] and partition_evidence['complete_test_count'] == case['inventoried_tests']"
                        )
                    if not (
                        partition_evidence["counts"] == outcomes[0]
                        and len(outcomes) == 1
                    ):
                        raise AssertionError(
                            "Verification condition failed: partition_evidence['counts'] == outcomes[0] and len(outcomes) == 1"
                        )
                    if not (
                        partition_evidence["executable_sha256"]
                        == case["executable_sha256"]
                    ):
                        raise AssertionError(
                            "Verification condition failed: partition_evidence['executable_sha256'] == case['executable_sha256']"
                        )
                    if not (
                        partition_evidence["qualified_target"]
                        == case["package"] + ":" + case["target"]
                    ):
                        raise AssertionError(
                            "Verification condition failed: partition_evidence['qualified_target'] == case['package'] + ':' + case['target']"
                        )
                    if not (
                        partition_evidence["stall_seconds"]
                        == native_stall_seconds(case["package"])
                    ):
                        raise AssertionError(
                            "Verification condition failed: partition_evidence['stall_seconds'] == native_stall_seconds(case['package'])"
                        )
                    if not (
                        partition_evidence["partition_count"] == case["partition_count"]
                        and len(partition_evidence["partitions"]) == case["partition_count"]
                        and partition_evidence["maximum_native_processes"] == len(case["test_cpus"])
                    ):
                        raise AssertionError("Native chunk count or live-process ceiling changed")
                    if not (partition_evidence["cpus"] == case["test_cpus"]):
                        raise AssertionError(
                            "Verification condition failed: partition_evidence['cpus'] == case['test_cpus']"
                        )
        executable = entry.get("executable")
        if executable is not None:
            directory = "examples" if case["kind"] == "example" else "deps"
            if executable.parent.resolve() != (profile_root / directory).resolve():
                raise AssertionError(
                    "Executed native artifact belongs to another profile or target directory"
                )
            if not (
                hashlib.sha256(executable.read_bytes()).hexdigest()
                == case["executable_sha256"]
            ):
                raise AssertionError("Native artifact changed during execution")
            if args.preserve_test_artifacts:
                case["executable_released"] = False
            else:
                executable.unlink()
                case["executable_released"] = True
            release_incremental(case)
        del active[cpu]
        save()
        print("END", entry["label"], case["state"], case["outcomes"], flush=True)
        if not result["source_unchanged"]:
            raise SystemExit("source changed during sweep")
        return True

    def reap(wait=False):
        while True:
            changed = False
            for cpu in list(active):
                changed = complete(cpu) or changed
            if changed or not wait or not active:
                return
            forward_progress()
            time.sleep(0.1)

    def split_doc_packages(text):
        starts = list(re.finditer(r"^\s*Doc-tests (\S+)\s*$", text, re.M))
        rows = {}
        for index, start in enumerate(starts):
            name = start.group(1)
            if not (name not in rows):
                raise AssertionError(name)
            end = starts[index + 1].start() if index + 1 < len(starts) else len(text)
            rows[name] = text[start.start() : end]
        return rows

    def admit_writable_space(case, label, *, building=False, native_processes=1):
        def snapshot():
            return storage_snapshot(
                cargo_target_dir,
                out,
                build_env,
                native_processes=len(active) + native_processes,
                building=building,
            )

        checked = snapshot()
        last_drain_at = None
        while not checked["sufficient"] and active:
            now = time.monotonic()
            if last_drain_at is None or now - last_drain_at >= 20:
                print(
                    "STORAGE DRAIN",
                    label,
                    json.dumps(checked, sort_keys=True),
                    flush=True,
                )
                last_drain_at = now
            reap()
            forward_progress()
            time.sleep(0.1)
            checked = snapshot()
        case.setdefault("storage_preflights", []).append({"label": label, **checked})
        if building:
            case["build_start_free_disk_bytes"] = shutil.disk_usage(
                cargo_target_dir
            ).free
        if not checked["sufficient"]:
            case.update(
                state="storage-deferred",
                reason="insufficient writable filesystem headroom; no new process admitted; active native scopes drained normally; inspect storage_preflights physical paths",
            )
            save()
            print(
                "STORAGE DEFERRED",
                label,
                json.dumps(checked, sort_keys=True),
                flush=True,
            )
            return False
        return True

    def admit_build_space(case, label, *, cached_native=False, native_processes=1):
        # Existing validated ELFs need no link reserve. Any actual relink, including
        # the static-to-shared fallback, must obtain the reserve before unlinking.
        # Every admission also checks physical temporary/cache/output filesystems.
        free_bytes = shutil.disk_usage(cargo_target_dir).free
        case["build_start_free_disk_bytes"] = free_bytes
        case["build_space_policy"] = (
            "No link admitted: use the exact source-bound validated existing native artifact"
            if cached_native
            else "Reserve two GiB before an actual Cargo/rustdoc build"
        )
        if not cached_native and free_bytes < 4 * 1024**3:
            profile_root.mkdir(parents=True, exist_ok=True)
            with (profile_root / ".cargo-lock").open("a+b") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX)
                removed = []
                for cache in (
                    (profile_root / "incremental").iterdir()
                    if (profile_root / "incremental").is_dir()
                    else []
                ):
                    if cache.is_dir():
                        removed.append(
                            {
                                "path": evidence_path(cache),
                                "allocated_bytes": sum(
                                    p.stat().st_blocks * 512
                                    for p in cache.rglob("*")
                                    if p.is_file()
                                ),
                            }
                        )
                        shutil.rmtree(cache)
                case["inactive_incremental_prune"] = removed
        return admit_writable_space(
            case, label, building=not cached_native, native_processes=native_processes
        )

    documentation_cohorts = doc_cohorts(
        full_inventory, args.packages, isolated_packages
    )

    def run_doc_cohort():
        if not (not active):
            raise AssertionError("Documentation remains idle-exclusive")
        pending = [c for c in cases if c["kind"] == "doc" and c.get("state") is None]
        if not (pending):
            raise AssertionError("Verification condition failed: pending")
        for cohort in documentation_cohorts:
            group, selection, package_names = (
                cohort.key,
                cohort.selection,
                cohort.packages,
            )
            selected = [c for c in pending if c["package"] in package_names]
            if not selected:
                continue
            expected = {name.replace("-", "_") for name in package_names}
            cohort_label = "workspace-doc-cohort" + (
                "" if group == "doc:*" else "-" + selected[0]["package"]
            )
            admission = selected[0]
            if not admit_build_space(admission, cohort_label):
                for case in selected[1:]:
                    case.update(
                        state="storage-deferred",
                        build_start_free_disk_bytes=admission[
                            "build_start_free_disk_bytes"
                        ],
                        build_space_policy=admission["build_space_policy"],
                        storage_preflights=admission["storage_preflights"],
                        reason=admission["reason"],
                    )
                save()
                continue
            flags = ["--all-features"] if args.all_features else []
            base = [
                "cargo",
                "test",
                "--locked",
                "--offline",
                *profile_flags,
                *selection,
                *flags,
                "--doc",
                "--no-fail-fast",
            ]
            prefix = [
                "python3",
                "-B",
                "scripts/limited_verification.py",
                "--cloud",
                "--cloud-cpus",
                "auto",
                "--",
            ]
            command_inventory = prefix + base + ["--", "--list"]
            inventory_path = out / (cohort_label + ".inventory.log")
            started = time.monotonic()
            with inventory_path.open("w") as stream:
                inventory = subprocess.run(
                    command_inventory,
                    stdout=stream,
                    stderr=subprocess.STDOUT,
                    env=build_env,
                )
            inventory_rows = split_doc_packages(inventory_path.read_text())
            if not (inventory.returncode == 0 and set(inventory_rows) == expected):
                raise AssertionError(
                    (
                        inventory.returncode,
                        set(inventory_rows),
                        expected,
                    )
                )
            command = (
                prefix
                + base
                + ["--", "--test-threads=1"]
                + (["--include-ignored"] if args.include_ignored else [])
            )
            label = "workspace-" + out.name + "-" + cohort_label
            log_path = out / (cohort_label + ".execution.log")
            watched = [
                "bash",
                "scripts/run-watched.sh",
                "--label",
                label,
                "--stall",
                "2400",
                "--poll",
                "1",
                "--",
                *command,
            ]
            if not admit_writable_space(admission, cohort_label + "-execution"):
                for case in selected[1:]:
                    case.update(
                        state="storage-deferred",
                        storage_preflights=admission["storage_preflights"],
                        reason=admission["reason"],
                    )
                save()
                continue
            with log_path.open("w") as stream:
                executed = subprocess.run(
                    watched, stdout=stream, stderr=subprocess.STDOUT, env=build_env
                )
            watch = root / "target/watched" / (label + ".log")
            if not (watch.is_file()):
                raise AssertionError(watch)
            execution_rows = split_doc_packages(watch.read_text())
            if not (set(execution_rows) == expected):
                raise AssertionError((set(execution_rows), expected))
            cohort = {
                "inventory_command": command_inventory,
                "command": command,
                "exit_code": executed.returncode,
                "inventory_log": evidence_path(inventory_path),
                "inventory_sha256": hashlib.sha256(
                    inventory_path.read_bytes()
                ).hexdigest(),
                "transcript_log": evidence_path(watch),
                "transcript_sha256": hashlib.sha256(watch.read_bytes()).hexdigest(),
                "policy": "One complete documentation cohort on the exact selected Cargo graph; explicitly isolated test roots remain separate. Rustdoc controls stay serial and idle-exclusive. Completed siblings never replace prior verdicts.",
            }
            build_groups[group] = cohort
            for case in selected:
                package = case["package"].replace("-", "_")
                label = "cohort-" + case["package"] + "-doc"
                inventory_piece = out / (label + ".inventory.log")
                inventory_piece.write_text(inventory_rows[package])
                transcript = out / (label + ".native.log")
                transcript.write_text(execution_rows[package])
                outcomes = [
                    dict(zip(["passed", "failed", "ignored"], map(int, row)))
                    for row in re.findall(
                        r"^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored;",
                        execution_rows[package],
                        re.M,
                    )
                ]
                count = len(re.findall(r"^.+: test$", inventory_rows[package], re.M))
                matches = (
                    len(outcomes) == 1
                    and sum(sum(row.values()) for row in outcomes) == count
                    and all(
                        row["ignored"] == 0 and row["failed"] == 0 for row in outcomes
                    )
                )
                case.update(
                    state="passed" if matches else "failed",
                    build_exit=inventory.returncode,
                    exit_code=0 if matches else 1,
                    cohort_exit_code=executed.returncode,
                    inventory_matches=matches,
                    inventoried_tests=count,
                    inventoried_benchmarks=len(
                        re.findall(r"^.+: benchmark$", inventory_rows[package], re.M)
                    ),
                    outcomes=outcomes,
                    inventory_log=evidence_path(inventory_piece),
                    inventory_sha256=hashlib.sha256(
                        inventory_piece.read_bytes()
                    ).hexdigest(),
                    transcript_log=evidence_path(transcript),
                    transcript_sha256=hashlib.sha256(
                        transcript.read_bytes()
                    ).hexdigest(),
                    inventory_command=command_inventory,
                    command=command,
                    build_group=group,
                    seconds=round(time.monotonic() - started, 3),
                    test_policy="complete Cargo documentation cohort, one libtest thread per rustdoc; idle-exclusive; cohort wall time is shared",
                )
                print("END", label, case["state"], outcomes, flush=True)
            cohort["completed_siblings"] = [
                {
                    "package": name,
                    "inventory_count": len(
                        re.findall(r"^.+: test$", inventory_rows[name], re.M)
                    ),
                    "transcript_sha256": hashlib.sha256(
                        execution_rows[name].encode()
                    ).hexdigest(),
                }
                for name in sorted(
                    expected - {c["package"].replace("-", "_") for c in selected}
                )
            ]
            save()
            if not (result["source_unchanged"]):
                raise AssertionError("Source changed during documentation cohort")

    save()
    for i, case in execution_order(
        cases, args.partition_cli, partition_native_targets, native_cpu_targets
    ):
        if case["kind"] == "doc" and args.continuation_phase in ("barriers", "all"):
            if case.get("state") is None:
                # The complete cohort sealed all documentation siblings already.
                # Only new documentation work needs an idle-exclusive barrier.
                while active:
                    reap(wait=True)
                print("START workspace-doc-cohort", flush=True)
                run_doc_cohort()
            continue
        reap()
        partitioned = is_partitioned_native(
            case, args.partition_cli, partition_native_targets
        )
        barrier = (
            is_barrier(case)
            or partitioned
            or native_cpu_target(case, native_cpu_targets)
        )
        if barrier:
            while active:
                reap(wait=True)
        while len(active) >= len(slots):
            reap(wait=True)
        cpu = next(cpu for cpu in slots if cpu not in active)
        label = f'{i:04d}-{case["package"]}-{case["target"]}'
        watch_label = f"workspace-{out.name}-{label}"
        print("START", label, flush=True)
        base = cargo_test_base(case)
        cached_native = case["kind"] != "doc" and group_key(case) in artifact_cohorts
        if args.idle_builds and not cached_native and active:
            waited_at = time.monotonic()
            waiting = len(active)
            while active:
                reap(wait=True)
            case["idle_build_wait"] = {
                "seconds": round(time.monotonic() - waited_at, 3),
                "native_processes_drained": waiting,
            }
        if not admit_build_space(
            case,
            label,
            cached_native=cached_native,
            native_processes=len(slots) if partitioned else 1,
        ):
            continue
        cpu = next(cpu for cpu in slots if cpu not in active)
        start = time.monotonic()
        case.update(
            state="building",
            started_utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        )
        save()
        buildlog = out / (label + ".build.jsonl")
        errlog = out / (label + ".build.stderr")
        executable = None
        if case["kind"] == "doc":
            inventory_command = [
                "taskset",
                "-c",
                str(cpu),
                "python3",
                "-B",
                "scripts/limited_verification.py",
                "--cloud",
                "--",
                *base,
                "--",
                "--list",
            ]
            inventory = subprocess.run(
                inventory_command, capture_output=True, text=True, env=build_env
            )
            inventory_path = out / (label + ".inventory.log")
            inventory_path.write_text(inventory.stdout + inventory.stderr)
            case["inventory_log"] = evidence_path(inventory_path)
            case["inventory_sha256"] = hashlib.sha256(
                inventory_path.read_bytes()
            ).hexdigest()
            case["inventory_command"] = inventory_command
            if not (inventory.returncode == 0):
                raise AssertionError((case, inventory.stderr))
            case["inventoried_tests"] = len(
                re.findall(r"^.+: test$", inventory.stdout, re.M)
            )
            case["inventoried_benchmarks"] = len(
                re.findall(r"^.+: benchmark$", inventory.stdout, re.M)
            )
            command = (
                base
                + ["--", "--test-threads=1"]
                + (["--include-ignored"] if args.include_ignored else [])
            )
        else:
            group = group_key(case)
            group_name = ":".join(group)
            reused_group = group in artifact_cohorts
            cohort_base = base
            command_build = cohort_base + [
                "--no-run",
                "--message-format=json-render-diagnostics",
            ]
            if active:
                command_build = [
                    "taskset",
                    "-c",
                    str(cpu),
                    "python3",
                    "-B",
                    "scripts/limited_verification.py",
                    "--cloud",
                    "--",
                    *command_build,
                ]
            else:
                command_build = [
                    "python3",
                    "-B",
                    "scripts/limited_verification.py",
                    "--cloud",
                    "--cloud-cpus",
                    "auto",
                    "--",
                    *command_build,
                ]
            case["build_command"] = command_build
            if reused_group:
                executable = artifact_cohorts[group][
                    (package_ids[case["package"]], case["target"], case["kind"])
                ]
                if not (executable.is_file()):
                    raise AssertionError(
                        "Verification condition failed: executable.is_file()"
                    )
                if "carried_unexecuted_outputs" in build_groups[group_name]:
                    admission = next(
                        row
                        for row in build_groups[group_name][
                            "carried_unexecuted_outputs"
                        ]
                        if row["path"] == evidence_path(executable)
                    )
                    if not (
                        hashlib.sha256(executable.read_bytes()).hexdigest()
                        == admission["sha256"]
                    ):
                        raise AssertionError(
                            "Verification condition failed: hashlib.sha256(executable.read_bytes()).hexdigest() == admission['sha256']"
                        )
                    case["carried_library_admission"] = admission
                case["build_exit"] = 0
                case["build_command"] = build_groups[group_name]["command"]
                case["reused_build_group"] = group_name
            else:
                graph_proof = None
                if args.selected_feature_roots and group[0] == "feature-roots":
                    if not result.get("intl_source_identity_preflight"):
                        result["intl_source_identity_preflight"] = (
                            feature_roots.verify_source_identities(
                                out / label, build_env
                            )
                        )
                        save()
                    selected = [c for c in cases if group_key(c) == group]
                    graph_proof = feature_roots.prove_graph(
                        cohort_base,
                        selected,
                        package_ids,
                        isolated_packages,
                        profile_flags,
                        out / label,
                        build_env,
                    )
                    case["feature_root_graph_proof"] = graph_proof
                    save()
                with buildlog.open("w") as stdout, errlog.open("w") as stderr:
                    build_exit = build_with_completed_cache_retirement(
                        command_build, stdout, stderr, case
                    )
                case["build_exit"] = build_exit
                if build_exit:
                    case.update(
                        state="build-failed", seconds=round(time.monotonic() - start, 3)
                    )
                    release_incremental(case)
                    seal_build_logs(case, [buildlog, errlog])
                    save()
                    print("BUILD FAILED", label, flush=True)
                    if args.stop_on_build_failure:
                        result["stopped_after_build_failure"] = label
                        break
                    continue
                expected = [
                    (package_ids[c["package"]], c["target"], c["kind"])
                    for c in (cases if graph_proof else full_inventory)
                    if group_key(c) == group
                ]
                auxiliary_key = (
                    (package_ids[feature_roots.PACKAGE], feature_roots.TARGET, "test")
                    if graph_proof
                    else None
                )
                if auxiliary_key:
                    expected.append(auxiliary_key)
                messages = [
                    json.loads(line) for line in buildlog.read_text().splitlines()
                ]
                artifact_cohorts[group] = profile_test_artifacts(messages, expected)
                executable = artifact_cohorts[group][
                    (package_ids[case["package"]], case["target"], case["kind"])
                ]
                build_groups[group_name] = {
                    "command": command_build,
                    "executables": [
                        {
                            "package_id": key[0],
                            "target": key[1],
                            "kind": key[2],
                            "path": evidence_path(path),
                        }
                        for key, path in artifact_cohorts[group].items()
                    ],
                    "policy": "One build per exact selector cohort; explicitly isolated test roots do not widen the normal graph; every selected package executable is independently inventoried and executed.",
                    "completed_unit_cache_retirement": case[
                        "completed_unit_cache_retirement"
                    ],
                }
                producers = []
                workspace_scripts = {
                    (p["id"], t["name"])
                    for p in metadata["packages"]
                    if p["id"] in members
                    for t in p["targets"]
                    if t["kind"] == ["custom-build"]
                }
                seen = {
                    (r["package_id"], r["target"]["name"])
                    for r in messages
                    if r.get("reason") == "compiler-artifact"
                    and r["target"]["kind"] == ["custom-build"]
                    and r["package_id"] in members
                }
                if not (seen <= workspace_scripts):
                    raise AssertionError(
                        "Cargo build scripts must match the exact workspace metadata"
                    )
                for package_id, target in sorted(seen):
                    producer = build_script_artifact(messages, package_id, target)
                    producers.append(
                        {
                            "package_id": package_id,
                            "target": target,
                            "path": evidence_path(producer),
                            "bytes": producer.stat().st_size,
                            "sha256": hashlib.sha256(producer.read_bytes()).hexdigest(),
                        }
                    )
                build_groups[group_name]["build_script_artifacts"] = producers
                if graph_proof:
                    build_groups[group_name]["feature_root_graph_proof"] = graph_proof
                    build_groups[group_name]["verification_auxiliary"] = (
                        feature_roots.verify_auxiliary(
                            artifact_cohorts[group][auxiliary_key],
                            out / label,
                            build_env,
                        )
                    )
                wanted = {
                    (package_ids[c["package"]], c["target"], c["kind"])
                    for c in cases
                    if group_key(c) == group
                }
                if auxiliary_key:
                    wanted.add(auxiliary_key)
                retired_siblings, preserved_siblings = finish_unselected_artifacts(
                    artifact_cohorts[group], wanted
                )
                build_groups[group_name][
                    "unselected_artifacts_retired"
                ] = retired_siblings
                build_groups[group_name][
                    "unselected_artifacts_preserved"
                ] = preserved_siblings
            case["build_group"] = group_name
            if args.shared_runtime_linker:
                # PATH is deliberately outside Cargo's compilation fingerprints.
                # A fresh static ELF must be explicitly relinked before claiming
                # data sharing. Targets with no embedded image need no DSO.
                dynamic = subprocess.check_output(
                    ["readelf", "-d", str(executable)], text=True
                )
                if "libscriptw_runtime_images_" not in dynamic:
                    symbols = subprocess.check_output(
                        ["nm", "-g", "--defined-only", str(executable)], text=True
                    )
                    if re.search(
                        r"\bSCRIPTW_RUNTIME_(PACKAGE|NATIVE|MANIFEST)$", symbols, re.M
                    ):
                        if not admit_build_space(case, label + "-shared-relink"):
                            seal_build_logs(case, [buildlog, errlog])
                            continue
                        case["static_executable_relinked"] = True
                        executable.unlink()
                        with buildlog.open("a") as stdout, errlog.open("a") as stderr:
                            rebuilt = build_with_completed_cache_retirement(
                                command_build, stdout, stderr, case
                            )
                        if not (rebuilt == 0):
                            raise AssertionError((case, errlog.read_text()[-4000:]))
                        dynamic = subprocess.check_output(
                            ["readelf", "-d", str(executable)], text=True
                        )
                        if not ("libscriptw_runtime_images_" in dynamic):
                            raise AssertionError(case)
            case["executable_sha256"] = hashlib.sha256(
                executable.read_bytes()
            ).hexdigest()
            case["executable_bytes"] = executable.stat().st_size
            if args.shared_runtime_linker:
                dynamic = subprocess.check_output(
                    ["readelf", "-d", str(executable)], text=True
                )
                needed = re.findall(r"\(NEEDED\).*\[([^]]+)\]", dynamic)
                case["needed_libraries"] = needed
                shared = [
                    name
                    for name in needed
                    if name.startswith("libscriptw_runtime_images_")
                ]
                if not (len(shared) <= 1):
                    raise AssertionError(
                        "Verification condition failed: len(shared) <= 1"
                    )
                if shared:
                    image = cargo_target_dir / "shared-test-runtime-images" / shared[0]
                    receipt = json.loads(image.with_suffix(".json").read_text())
                    if not (
                        hashlib.sha256(image.read_bytes()).hexdigest()
                        == receipt["shared_sha256"]
                    ):
                        raise AssertionError(
                            "Verification condition failed: hashlib.sha256(image.read_bytes()).hexdigest() == receipt['shared_sha256']"
                        )
                    if not (str(image.parent) in dynamic):
                        raise AssertionError(
                            "Verification condition failed: str(image.parent) in dynamic"
                        )
                    events = [
                        json.loads(line)
                        for line in (image.parent / "test-links.jsonl")
                        .read_text()
                        .splitlines()
                    ]
                    matched = [
                        event
                        for event in events
                        if event.get("executable_sha256") == case["executable_sha256"]
                        and event["shared_image"] == str(image)
                    ]
                    if not (matched):
                        raise AssertionError((case, "no sealed linker event"))
                    case["shared_link_event"] = matched[-1]
                    if args.strip_test_debug:
                        debug = matched[-1].get("debug_sections")
                        if not (
                            debug
                            and debug["stripped_sha256"] == case["executable_sha256"]
                        ):
                            raise AssertionError((case, "stripping not sealed"))
                    case["shared_runtime_image"] = receipt
            seal_build_logs(case, [buildlog, errlog])
            inventory = subprocess.run(
                [str(executable), "--list"]
                + (
                    [native_filters[case["package"] + ":" + case["target"]]]
                    if case["package"] + ":" + case["target"] in native_filters
                    else []
                ),
                capture_output=True,
                text=True,
            )
            inventory_path = out / (label + ".inventory.log")
            inventory_path.write_text(inventory.stdout + inventory.stderr)
            case["inventory_log"] = evidence_path(inventory_path)
            case["inventory_sha256"] = hashlib.sha256(
                inventory_path.read_bytes()
            ).hexdigest()
            if not (inventory.returncode == 0):
                raise AssertionError((case, inventory.stderr))
            case["inventoried_tests"] = len(
                re.findall(r"^.+: test$", inventory.stdout, re.M)
            )
            case["inventoried_benchmarks"] = len(
                re.findall(r"^.+: benchmark$", inventory.stdout, re.M)
            )
            if case["package"] + ":" + case["target"] in native_filters:
                if not (case["inventoried_tests"] > 0):
                    raise AssertionError("Native filter selected no tests")
            command = [str(executable), "--test-threads=1"] + (
                ["--include-ignored"] if args.include_ignored else []
            )
            if native_filter := native_filters.get(
                case["package"] + ":" + case["target"]
            ):
                command.append(native_filter)
                case["native_filter"] = native_filter
        partition_receipt = None
        if partitioned:
            if not (
                not case.get("native_filter") and args.include_ignored and not active
            ):
                raise AssertionError(
                    "Verification condition failed: not case.get('native_filter') and args.include_ignored and (not active)"
                )
            if not (len(slots) == args.test_processes):
                raise AssertionError(
                    "Verification condition failed: len(slots) == args.test_processes"
                )
            partition_output = out / (label + "-partitions")
            partition_receipt = partition_output / "receipt.json"
            partition_driver = root / "scripts/workspace_test_partitions.py"
            command = [
                "python3",
                "-B",
                str(partition_driver),
                "--executable",
                str(executable),
                "--inventory",
                str(out / (label + ".inventory.log")),
                "--output",
                str(partition_output),
                "--cpus",
                *map(str, slots),
                "--include-ignored",
                "--qualified-target",
                case["package"] + ":" + case["target"],
                "--stall-seconds",
                str(native_stall_seconds(case["package"])),
            ]
            if is_partitioned_cli(case, args.partition_cli) and args.cli_partition_count is not None:
                command += ["--partition-count", str(args.cli_partition_count)]
            case["partition_count"] = (args.cli_partition_count
                if is_partitioned_cli(case, args.partition_cli) and args.cli_partition_count is not None
                else len(slots))
            case["partition_driver_sha256"] = hashlib.sha256(
                partition_driver.read_bytes()
            ).hexdigest()
            case["partition_receipt_path"] = evidence_path(partition_receipt)
        # Builds may consume scratch after admission. Recheck before executing even
        # cached native images; a deferred scope has no fabricated native verdict.
        if not admit_writable_space(
            case,
            label + "-execution",
            native_processes=len(slots) if partitioned else 1,
        ):
            continue
        cpu = next(cpu for cpu in slots if cpu not in active)
        log = out / (label + ".run.log")
        watch = root / "target/watched" / (watch_label + ".log")
        case.update(
            state="running",
            command=command,
            test_cpu=cpu,
            test_policy=(
                "explicitly filtered native control with its matching exact inventory"
                if case.get("native_filter")
                else "complete native executable"
            )
            + " on one inherited CPU and one libtest thread; independent target concurrency recorded separately",
        )
        save()
        stream = log.open("w")
        launch = [
            "taskset",
            "-c",
            str(cpu),
            "./scripts/run-watched.sh",
            "--label",
            watch_label,
            "--stall",
            str(native_stall_seconds(case["package"])),
            "--poll",
            "1",
            "--",
            "python3",
            "-B",
            "scripts/limited_verification.py",
            "--cloud",
            "--",
            *command,
        ]
        if partitioned:
            launch = [
                "./scripts/run-watched.sh",
                "--label",
                watch_label,
                "--stall",
                str(native_stall_seconds(case["package"])),
                "--poll",
                "1",
                "--",
                "python3",
                "-B",
                "scripts/limited_verification.py",
                "--cloud",
                "--cloud-worker-cpus",
                "auto",
                "--",
                *command,
            ]
            case["test_cpus"] = slots
            case["test_policy"] = (
                "complete logical native target; all exact qualified IDs in disjoint source-bound partitions, each on one inherited CPU and one libtest thread; no other native target active; source-bound partition driver and original native stall allowance retained"
            )
            save()
        elif differential_native_compilation(
            case, args.differential_native_cpus
        ) or native_cpu_target(case, native_cpu_targets):
            if not (barrier and not active):
                raise AssertionError(
                    "Verification condition failed: barrier and (not active)"
                )
            resource_path = out / (label + ".native-resources.json")
            launch = [
                "./scripts/run-watched.sh",
                "--label",
                watch_label,
                "--stall",
                str(native_stall_seconds(case["package"])),
                "--poll",
                "1",
                "--",
                "python3",
                "-B",
                "scripts/limited_verification.py",
                "--cloud",
                "--cloud-cpus",
                "auto",
                "--",
                "python3",
                "-B",
                "-c",
                NATIVE_RESOURCE_EXEC,
                str(resource_path),
                *command,
            ]
            case["scheduler_slot"] = case.pop("test_cpu")
            case["execution_resource_log"] = evidence_path(resource_path)
            case["test_policy"] = (
                "complete idle-exclusive native scope; one libtest thread; explicit native compilation CPUs bounded by inherited affinity, cgroup quota and job limits; actual affinity, compiler/Rayon jobs and serial Cargo/libtest policy sealed in execution resource receipt; original deadlines unchanged"
            )
            save()
        run = subprocess.Popen(
            launch, stdout=stream, stderr=subprocess.STDOUT, env=build_env
        )
        active[cpu] = {
            "run": run,
            "stream": stream,
            "case": case,
            "start": start,
            "watch": watch,
            "log": log,
            "executable": executable,
            "label": label,
            "partition_receipt": partition_receipt,
        }
        if barrier:
            while active:
                reap(wait=True)
    while active:
        reap(wait=True)
    save()
    return int(
        not result["source_unchanged"]
        or any(c.get("state", "unstarted") != "passed" for c in cases)
    )


def main(argv=None):
    return run(parse_args(argv), Path.cwd())


if __name__ == "__main__":
    raise SystemExit(main())
