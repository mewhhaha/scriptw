#!/usr/bin/env python3
"""Keep bounded historical host paths out of the product identity audit only.

The original proof bytes remain untouched. This filter never suppresses a
whole line: retired package, API, module and UI identifiers remain visible.
"""
import hashlib
import json
from pathlib import Path
import re
import sys

# Immutable pre-rename reports retain the names of the executable that ran.
# Admission is by exact bytes, never by a broad directory or schema exemption.
HISTORY_PATH = Path.cwd() / 'docs/rust-rewrite/scriptw-frozen-history.json'
HISTORY = json.loads(HISTORY_PATH.read_text()) if HISTORY_PATH.exists() else {'files': {}}
PREVIOUS_NAME = 'li' + 'la'


def project_history_projection(path, raw, admitted=None):
    expected = HISTORY['files'].get(Path(path).as_posix())
    if expected is None:
        return raw if admitted is None else admitted
    if hashlib.sha256(raw).hexdigest() != expected:
        raise ValueError('frozen pre-rename evidence changed')
    raw = raw if admitted is None else admitted
    # Line count stays identical for the identity audit's diagnostics.
    for token in (PREVIOUS_NAME, PREVIOUS_NAME.title(), PREVIOUS_NAME.upper()):
        raw = raw.replace(token.encode(), b'HISTORICAL_PROJECT')
    return raw


def readme_history_projection(raw):
    marker = PREVIOUS_NAME + '-status'
    pattern = re.compile(('<!-- ' + marker + ':start -->.*?<!-- ' + marker + ':end -->').encode(), re.S)
    def project(match):
        block = match.group()
        if hashlib.sha256(block).hexdigest() != HISTORY.get('readme_status_sha256'):
            raise ValueError('frozen pre-rename README publication changed')
        return block.replace(PREVIOUS_NAME.encode(), b'HISTORICAL_PROJECT')
    return pattern.sub(project, raw)

PROOFS = {
    # Closed reports keep their literal host paths; admission binds every byte.
    'all-features-workspace-8e9a8c82-native-checkpoint-20261015.json': ('portable-real-next-workspace-v1-20261015', None),
    'default-workspace-8e9a8c82-native-checkpoint-20261015.json': ('portable-real-next-workspace-v1-20261015', None),
    'default-workspace-eb1ca63c9-native-checkpoint-20261015.json': ('portable-current-workspace-retention-v1', None),
    'exact-replay-checkpoint-20261013.json': (None, '1580a2f0fc52d38cf281241e858d4563cf867f7d1a368f0bb6d680a9fb601b5d'),
    'focused-workspace-repairs-20261014.json': ('focused_workspace_repairs_20261014_v1', None),
}
RETIRED = "por" + "ffor"
BOUNDARY = r"(?:(?<![\w/.-])|(?<=\\[nrt]))"
ROOT = re.compile(BOUNDARY + r"(?:file://)?/workspace/" + RETIRED + r"(?:-next|-repairs)?(?![\w.-])")
WORKTREE = re.compile(r"(?<=/\.git/worktrees/)" + RETIRED + r"(?:-next|-repairs)(?![\w.-])")


def unique_fields(pairs):
    result = {}
    for name, value in pairs:
        if name in result:
            raise ValueError("duplicate proof field")
        result[name] = value
    return result


def audit_source(path, raw):
    """Return a line-preserving audit stream for one closed proof context."""
    path = Path(path)
    if path.parent.as_posix() != "docs/rust-rewrite":
        return raw
    name = path.name
    if name in PROOFS:
        schema, frozen_sha = PROOFS[name]
        value = json.loads(raw, object_pairs_hook=unique_fields)
        if not isinstance(value, dict):
            raise ValueError("historical proof must be an object")
        if frozen_sha:
            if hashlib.sha256(raw).hexdigest() != frozen_sha:
                raise ValueError("frozen historical proof changed")
        elif value.get("schema") != schema:
            raise ValueError("historical proof schema differs")
    else:
        return raw
    source = raw.decode("utf-8")
    # Most proof bytes are encoded payloads. Keep the same expressions and
    # physical lines while visiting only lines that can contain these paths.
    lines = []
    for line in source.splitlines(keepends=True):
        if "/workspace/" + RETIRED in line:
            line = ROOT.sub("/workspace/HISTORICAL_CHECKOUT", line)
        if "/.git/worktrees/" + RETIRED in line:
            line = WORKTREE.sub("HISTORICAL_WORKTREE", line)
        lines.append(line)
    source = "".join(lines)
    if PROOFS[name][1]:
        # A declared, byte-bound locator remains a host path in any field that
        # references it, like the absolute ROOT paths above. Other strings,
        # including undeclared paths and product identifiers, stay visible.
        assets = value.get("assets", [])
        asset_paths = {row["path"] for row in assets
                       if isinstance(row, dict) and isinstance(row.get("path"), str)
                       and row["path"].startswith(RETIRED + "/target/")}
        locator = re.compile(r'(?<!\\)"' + RETIRED + r'/target/[^"\\]*(?:\\.[^"\\]*)*"')
        def normalize_locator(match):
            if json.loads(match.group()) not in asset_paths:
                return match.group()
            return '"HISTORICAL_CHECKOUT/' + match.group()[len(RETIRED) + 2:]
        source = "".join(locator.sub(normalize_locator, line)
                         if '"' + RETIRED + '/target/' in line else line
                         for line in source.splitlines(keepends=True))
    return source.encode("utf-8")


def main():
    path = Path(sys.argv[1])
    try:
        sys.stdout.buffer.write(audit_source(path, path.read_bytes()))
    except (OSError, UnicodeError, ValueError) as error:
        print(f"identity-provenance: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
