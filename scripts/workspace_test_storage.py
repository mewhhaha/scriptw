"""Preflight writable filesystems without pruning cache or native evidence."""

from pathlib import Path
import shutil
import tempfile

from filesystem_probe import probe_filesystem


BUILD_RESERVE_BYTES = 2 * 1024**3
NATIVE_SCRATCH_BYTES = 256 * 1024**2
CACHE_TIERS = (
    "cranelift-functions-v1",
    "native-modules-v1",
    "wasmtime-modules-v1",
    "program-wasm-v1",
)


def writable_paths(target_directory, output_directory, environment):
    """Match configured cache paths and both Python and Rust temporary roots."""
    rust_temp = Path(environment.get("TMPDIR", "/tmp"))
    if "SCRIPTW_CACHE_DIR" in environment:
        cache = Path(environment["SCRIPTW_CACHE_DIR"])
    elif "XDG_CACHE_HOME" in environment:
        cache = Path(environment["XDG_CACHE_HOME"]) / "scriptw"
    elif "HOME" in environment:
        cache = Path(environment["HOME"]) / ".cache" / "scriptw"
    else:
        cache = rust_temp / "scriptw"
    return [
        ("cargo-target", Path(target_directory)),
        ("verification-output", Path(output_directory)),
        ("python-temporary", Path(tempfile.gettempdir())),
        ("rust-temporary", rust_temp),
        *[("cache-" + tier, cache / tier) for tier in CACHE_TIERS],
    ]


def storage_snapshot(
    target_directory, output_directory, environment, *, native_processes, building=False
):
    """Deduplicate aliases by their actual device, including missing leaves.

    This is a conservative admission margin, not a reservation or a prediction
    of every test's writes. Concurrent external writers can consume it later.
    The existing two-GiB link reserve remains independent of native concurrency.
    No configured cache limit, source, artifact or cache entry is changed here.
    """
    if native_processes < 1:
        raise ValueError("storage admission requires at least one native process")
    scratch = native_processes * NATIVE_SCRATCH_BYTES
    filesystems = {}
    for role, logical in writable_paths(
        target_directory, output_directory, environment
    ):
        physical, existing, device, free = probe_filesystem(logical)
        required = max(
            scratch, BUILD_RESERVE_BYTES if building and role == "cargo-target" else 0
        )
        row = filesystems.setdefault(
            device,
            {
                "device": device,
                "free_bytes": free,
                "required_bytes": required,
                "paths": [],
            },
        )
        row["free_bytes"] = min(row["free_bytes"], free)
        row["required_bytes"] = max(row["required_bytes"], required)
        row["paths"].append(
            {
                "role": role,
                "logical_path": str(logical.absolute()),
                "physical_path": str(physical),
                "probed_ancestor": str(existing),
            }
        )
    rows = list(filesystems.values())
    return {
        "native_processes": native_processes,
        "native_scratch_bytes_per_process": NATIVE_SCRATCH_BYTES,
        "build_reserve_bytes": BUILD_RESERVE_BYTES if building else 0,
        "filesystems": rows,
        "sufficient": all(row["free_bytes"] >= row["required_bytes"] for row in rows),
        "policy": "Preflight 256 MiB per active and scheduled native process "
        "on each writable physical filesystem; preserve the "
        "two-GiB Cargo/rustdoc link reserve. This does not reserve "
        "space against later or external writes.",
    }
