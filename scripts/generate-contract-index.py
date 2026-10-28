#!/usr/bin/env python3
"""Keep the contract catalogue navigable without changing contract contents."""
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def render(root):
    directory = root / "docs/rust-rewrite/contracts"
    files = sorted(
        (path for path in directory.glob("*.md") if path.name != "README.md"),
        key=lambda path: path.stem.casefold(),
    )
    rows = [
        "# Compiler contract catalogue",
        "",
        "Contracts describe owned boundaries and their validation. Read each document's",
        "scope and supersession notes; this index does not turn a proposal or historical",
        "observation into a current acceptance result.",
        "",
        "[Project](../../../README.md) ·",
        "[Development](../../../CONTRIBUTING.md#development-workflow) ·",
        "[Verification](../../../CONTRIBUTING.md#verification) ·",
        "[Runtime requirements](../runtime-requirements.md)",
        "",
        "Keep current invariants with their owning domain. Git retains superseded",
        "designs; dated checkpoints retain their own source and verification scope.",
        "",
        f"{len(files)} contracts. Refresh with `python3 scripts/generate-contract-index.py`.",
        "",
    ]
    initial = None
    for path in files:
        group = path.stem[0].upper()
        if group != initial:
            rows.extend([f"## {group}", ""])
            initial = group
        rows.append(f"- [{path.stem}](<{path.name}>)")
    return "\n".join(rows) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    path = ROOT / "docs/rust-rewrite/contracts/README.md"
    text = render(ROOT)
    if args.check:
        if not path.is_file() or path.read_text() != text:
            parser.exit(1, "contract catalogue is stale\n")
    else:
        path.write_text(text)


if __name__ == "__main__":
    main()
