"""Resolve one committed JSON/text status generation and verify both members."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

SNAPSHOT = "test262/snapshots/published-status-wasm-aot.json"
MANIFEST = "test262/snapshots/published-status-wasm-aot.manifest.json"
DOMAIN = b"scriptw-status-generation-v1\0"


def generation_id(json_bytes, text_bytes):
    digest = hashlib.sha256(DOMAIN)
    for data in (json_bytes, text_bytes):
        digest.update(len(data).to_bytes(8, "little"))
        digest.update(data)
    return digest.hexdigest()


def read_local_or_index(root, name):
    path = root / name
    if path.is_symlink():
        raise ValueError(f"status evidence must be a regular file: {name}")
    try:
        return path.read_bytes()
    except FileNotFoundError:
        result = subprocess.run(
            ["git", "show", ":" + name], cwd=root, capture_output=True
        )
        if result.returncode:
            raise FileNotFoundError(name) from None
        return result.stdout


def read_committed(root, name, ref="HEAD"):
    path = root / name
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"status evidence must be a regular file: {name}")
    entry = subprocess.check_output(
        ["git", "ls-tree", ref, "--", name], cwd=root, text=True
    ).strip()
    metadata, separator, recorded = entry.partition("\t")
    if not separator or recorded != name or not metadata.startswith("100644 blob "):
        raise ValueError(f"status evidence must be a committed regular blob: {name}")
    return subprocess.check_output(["git", "show", ref + ":" + name], cwd=root)


def read_generation(
    root, manifest_name=MANIFEST, read=read_local_or_index, manifest_bytes=None
):
    manifest = json.loads(
        read(root, manifest_name) if manifest_bytes is None else manifest_bytes
    )
    if (
        not isinstance(manifest, dict)
        or set(manifest) != {"schema_version", "backend", "generation", "json", "text"}
        or type(manifest["schema_version"]) is not int
        or manifest["schema_version"] != 1
        or manifest["backend"] != "wasm-aot"
        or not isinstance(manifest["generation"], str)
        or not re.fullmatch(r"[0-9a-f]{64}", manifest["generation"])
    ):
        raise ValueError("invalid status generation manifest")
    data, paths = [], []
    for field, suffix in (("json", "json"), ("text", "txt")):
        member = manifest[field]
        expected = f"published-status-wasm-aot-{manifest['generation']}.{suffix}"
        if not isinstance(member, dict) or set(member) != {"file", "sha256"}:
            raise ValueError("invalid status generation member")
        if member["file"] != expected:
            raise ValueError("status member must name its own immutable generation")
        path = (Path(manifest_name).parent / expected).as_posix()
        payload = read(root, path)
        if hashlib.sha256(payload).hexdigest() != member["sha256"]:
            raise ValueError(f"status member digest mismatch: {path}")
        data.append(payload)
        paths.append(path)
    if generation_id(*data) != manifest["generation"]:
        raise ValueError("status generation does not bind both members")
    publication = json.loads(data[0])
    if (
        not isinstance(publication, dict)
        or publication.get("producer") != "scriptw"
        or not isinstance(publication.get("real_suite"), dict)
        or publication["real_suite"].get("backend") != "wasm-aot"
    ):
        raise ValueError(
            "status generation requires Scriptw wasm-aot publication evidence"
        )
    return publication, paths


def load_publication(root):
    try:
        manifest_bytes = read_local_or_index(root, MANIFEST)
    except FileNotFoundError:
        # Existing historical publication pairs remain readable. Once a
        # manifest exists, damage must fail rather than select that old pair.
        return json.loads(read_local_or_index(root, SNAPSHOT)), SNAPSHOT
    publication, paths = read_generation(root, manifest_bytes=manifest_bytes)
    return publication, paths[0]


def render_current_status(publication):
    real = publication["real_suite"]
    pins = real["pinned_revisions"]
    compiler = real["compiler_identity"]
    paths = real["snapshot_paths"]
    return (
        "## Current Status\n<!-- scriptw-status:start -->\n"
        f"Published Test262 (`{real['backend']}`, `{real['refresh_date']}`): "
        f"`{real['passed']}/{real['total']}` passed; `{real['failed']}` failed.\n"
        "Native-host conformance includes cases unsupported by standalone Wasm; "
        "it does not establish current HEAD coverage.\n"
        f"Pinned revisions: `ecma262={pins['ecma262']}` `test262={pins['test262']}`\n"
        f"Compiler build fingerprint: `{compiler['source_fingerprint']}`; "
        f"executing image SHA-256: `{compiler['executable_sha256']}`\n"
        f"Published status artifacts: `{paths['json']}` and `{paths['txt']}`\n"
        "[Full test details](docs/rust-rewrite/test262-current.md) · "
        "[Refresh workflow](CONTRIBUTING.md#test262).\n"
        "<!-- scriptw-status:end -->"
    )


def check_readme_projection(root, base_ref):
    manifest = read_committed(root, MANIFEST)
    if manifest != read_committed(root, MANIFEST, base_ref):
        raise ValueError("presentation-only changes require the existing status generation")
    published, paths = read_generation(root, read=read_committed)
    for path in paths:
        if read_committed(root, path) != read_committed(root, path, base_ref):
            raise ValueError("presentation-only changes require unchanged status members")
    raw = read_committed(root, "README.md").decode()
    heading = "## Current Status"
    start = "<!-- scriptw-status:start -->"
    end = "<!-- scriptw-status:end -->"
    if any(raw.count(marker) != 1 for marker in (heading, start, end)):
        raise ValueError("status projection requires one canonical heading and marker pair")
    heading_at, start_at, end_at = (raw.index(marker) for marker in (heading, start, end))
    if (
        (heading_at > 0 and raw[heading_at - 1] != "\n")
        or not heading_at + len(heading) <= start_at < end_at
        or raw[heading_at + len(heading) : start_at].strip()
        or raw[heading_at : end_at + len(end)] != render_current_status(published)
    ):
        raise ValueError("README must exactly project the unchanged published evidence")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--check-committed", action="store_true")
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--render-readme", action="store_true")
    action.add_argument("--check-readme-projection", metavar="BASE_REF")
    args = parser.parse_args()
    if args.check_readme_projection:
        check_readme_projection(args.root, args.check_readme_projection)
        return
    published, paths = read_generation(
        args.root, read=read_committed if args.check_committed else read_local_or_index
    )
    if args.render_readme:
        print(render_current_status(published))
        return
    for path in paths:
        print(path)


if __name__ == "__main__":
    main()
