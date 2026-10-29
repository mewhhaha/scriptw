#!/usr/bin/env python3
"""Run source-only backend guards before building the compiler dependency graph.

This is an additive diagnostic. The full Cargo structure_builtins,
structure_language, intl_temporal and engine structure targets remain required
for native acceptance.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import tomllib

from native_test_protocol import parse_inventory, result_summaries

# Keep these complete modules in the authoritative Cargo target. Do not extract
# only the text assertions from modules that also exercise typed/compiler APIs.
TARGETS = {
    "structure_builtins": frozenset({
        "array_species_create_operation_evidence_structure",
        "to_property_descriptor_operation_evidence_structure",
        "regexp_modifier_override_structure",
        "regexp_nullable_quantifier_progress_structure",
    }),
    "structure_language": frozenset({
        "global_object_environment_compound_assignment_structure",
        "global_object_environment_numeric_update_structure",
        "with_environment_compound_assignment_structure",
        "with_environment_numeric_update_structure",
    }),
    "intl_temporal": frozenset({
        "linked_bodies",
        "intl_dtf_time_zone_name_style_privacy_structure",
        "intl_host_imports",
        "locale_information_list_emission",
        "system_time_zone_imports",
        "temporal_calendar_shared_size",
    }),
    # Admit the std-only execution-mode guard before an expensive Engine build.
    # The remaining engine structure modules retain their full Cargo owner.
    "structure": frozenset({
        "compiler_fingerprint",
        "compiler_inspection",
        "engine_error_wasmtime_policy_authority_structure",
        "execution_backend_routing_structure",
        "module_entry_source_authority_structure",
        "oracle_entry_syntax",
        "rooted_completion_graph",
        "wasm_top_level_completion_kind_structure",
    }),
}


def run(root, output, target="structure_builtins"):
    cargo_modules = TARGETS[target]
    config = root / ".cargo/config.toml"
    if not config.is_file():
        raise ValueError(
            "missing tracked .cargo/config.toml; restore it before Cargo builds "
            "so a sparse checkout does not fork dependency fingerprints"
        )
    package = "scriptw-engine" if target == "structure" else "scriptw-aot-wasm"
    crate = root / "crates" / package
    entry = crate / "tests" / target / "main.rs"
    modules = []
    path_attribute = None
    for line in entry.read_text().splitlines():
        if not line.strip() or line.lstrip().startswith("//"):
            continue
        if re.fullmatch(r'#\[path = "[^"\\]+"\]', line):
            if path_attribute is not None:
                raise ValueError("multiple path attributes on a guard module")
            path_attribute = line
            continue
        match = re.fullmatch(r"mod ([a-z_][a-z_0-9]*);", line)
        if not match:
            raise ValueError(f"unclassified structure target declaration: {line}")
        if path_attribute is not None and match[1] not in cargo_modules:
            raise ValueError("path attribute requires an explicitly classified Cargo-only module")
        path_attribute = None
        modules.append(match[1])
    if path_attribute is not None:
        raise ValueError("dangling path attribute in structure target")
    if len(modules) != len(set(modules)) or not cargo_modules <= set(modules):
        raise ValueError("duplicate or missing explicitly classified guard module")
    selected = [name for name in modules if name not in cargo_modules]
    if not selected:
        raise ValueError("source-only guard selection is empty")
    manifest = dict(
        kind="source-only diagnostic; not full native or Wasm/IR acceptance",
        package=package,
        target=target,
        source_modules=selected,
        full_cargo_only_modules=sorted(cargo_modules),
        cargo_config=str(config),
    )
    (output / "coverage.json").write_text(json.dumps(manifest, indent=2) + "\n")
    harness = output / "source_guards.rs"
    harness.write_text("\n".join(
        f"#[path = {json.dumps(str(entry.parent / (name + '.rs')), ensure_ascii=False)}]\nmod {name};"
        for name in selected
    ) + "\n")
    edition = str(tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["edition"])
    environment = dict(os.environ, CARGO_MANIFEST_DIR=str(crate))
    binary = output / "source-guards"
    command = [os.environ.get("RUSTC", "rustc"), "--edition", edition,
               "--test", "-C", "opt-level=1", "-C", "debuginfo=0",
               str(harness), "-o", str(binary)]
    start = time.monotonic_ns()
    with (output / "build.stdout").open("w") as stdout, (output / "build.stderr").open("w") as stderr:
        build = subprocess.run(command, env=environment, stdout=stdout, stderr=stderr)
    build_ns = time.monotonic_ns() - start
    code = build.returncode
    inventory = []
    summaries = []
    if code == 0:
        listed = subprocess.run([str(binary), "--list"],
                                env=environment, cwd=root, capture_output=True, text=True)
        (output / "inventory.stdout").write_text(listed.stdout)
        (output / "inventory.stderr").write_text(listed.stderr)
        if listed.returncode != 0:
            raise ValueError("source guard inventory command failed")
        inventory = parse_inventory(listed.stdout)
        if {name.split("::", 1)[0] for name in inventory} != set(selected):
            raise ValueError("source guard inventory does not cover every selected module")
        start = time.monotonic_ns()
        with (output / "tests.stdout").open("w") as stdout, (output / "tests.stderr").open("w") as stderr:
            tests = subprocess.run([str(binary), "--test-threads=1", "--include-ignored"],
                                   env=environment, cwd=root, stdout=stdout, stderr=stderr)
        code = tests.returncode
        run_ns = time.monotonic_ns() - start
        summaries = result_summaries((output / "tests.stdout").read_text())
        if code == 0 and summaries != [("ok", str(len(inventory)), "0", "0", "0", "0")]:
            raise ValueError("successful source guard process did not execute its exact nonempty inventory")
    else:
        run_ns = None
    receipt = dict(**manifest, finished=True, passed=code == 0,
                   compile_ns=build_ns, execute_ns=run_ns, exit_code=code,
                   rustc_command=command, test_inventory=inventory,
                   result_summaries=summaries)
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    for name in ["build.stderr", "tests.stdout", "tests.stderr"]:
        path = output / name
        if not path.exists():
            continue
        raw = path.read_text()
        if code != 0:
            print(raw, end="")
        elif name == "tests.stdout":
            print("\n".join(line for line in raw.splitlines() if line.startswith("test result:")))
    print(f"Source-only {target}: {len(selected)}/{len(modules)} modules; "
          f"compile {build_ns / 1e9:.3f}s; full Cargo target still required.")
    return code


def run_targets(root, output, targets):
    if len(targets) != len(set(targets)) or not targets:
        raise ValueError("select distinct nonempty source guard targets")
    if len(targets) == 1:
        return run(root, output, targets[0])
    results = []
    for target in targets:
        directory = output / target
        directory.mkdir()
        code = run(root, directory, target)
        results.append(dict(target=target, exit_code=code, receipt=f"{target}/receipt.json"))
    code = next((r["exit_code"] for r in results if r["exit_code"]), 0)
    (output / "receipt.json").write_text(json.dumps(dict(
        kind="source-only target set; full Cargo targets remain required",
        finished=True, passed=code == 0, exit_code=code, targets=results,
    ), indent=2) + "\n")
    return code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--output", type=Path, help="fresh evidence directory; default: temporary")
    parser.add_argument("--target", action="append", choices=TARGETS,
                        help="repeatable target selection; default: all registered structural targets")
    args = parser.parse_args()
    targets = args.target or list(TARGETS)
    if args.output:
        args.output.mkdir(parents=True, exist_ok=False)
        return run_targets(args.root.resolve(), args.output.resolve(), targets)
    with tempfile.TemporaryDirectory(prefix="scriptw-source-guards-") as directory:
        return run_targets(args.root.resolve(), Path(directory), targets)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        raise SystemExit(f"source-guards: {error}")
