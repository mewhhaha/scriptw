#!/usr/bin/env python3
"""Batch the identity audit without changing its GNU grep expressions."""

import fnmatch
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

from identity_provenance import audit_source, project_history_projection, readme_history_projection, HISTORY

PREVIOUS = 'li' + 'la'


EXCLUDED = (
    "vendor/*", "test262/vendor/*", "test262/snapshots/*",
    "crates/scriptw-test262/tests/fixtures/*/snapshots/*",
    "docs/rust-rewrite/scriptw-identity-map.tsv",
    "docs/rust-rewrite/scriptw-identity-migration.md",
    "docs/rust-rewrite/scriptw-frozen-history.json",
    f"docs/rust-rewrite/{PREVIOUS}-identity-map.tsv",
    f"docs/rust-rewrite/{PREVIOUS}-identity-migration.md",
    "scripts/check-scriptw-identity.sh", "scripts/check-no-legacy-js.sh",
    "scripts/check-readme-status-artifacts.sh",
    "scripts/tests/check-readme-status-artifacts.sh",
)
RETIRED = "por" + "ffor"
DOCTEST = (
    '^[0-9]+:[[:space:]]*"/workspace/' + RETIRED
    + '/crates/scriptw-[^"]+[.]rs - [^"]+ [(]line [0-9]+[)]"[,]?[[:space:]]*$'
)


def grep(arguments, *, data=None, ignore_read_errors=False):
    result = subprocess.run(
        ["grep", *arguments], input=data, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode not in (0, 1) and not (ignore_read_errors and result.returncode == 2):
        raise OSError(os.fsdecode(result.stderr).rstrip())
    return result.stdout


def scan(pattern, files):
    """Return original GNU grep line-number/content records by audit filename."""
    matches = {}
    batches = []
    batch, size = [], 0
    # An unmerged index can list the same pathname at several stages. Scan its
    # current bytes once, then replay findings for each original Git record.
    for path in dict.fromkeys(files):
        length = len(os.fsencode(path)) + 1
        if batch and (len(batch) >= 128 or size + length > 32768):
            batches.append(batch)
            batch, size = [], 0
        batch.append(path)
        size += length
    if batch:
        batches.append(batch)
    for batch in batches:
        # The original per-file content scans suppressed grep read errors.
        output = grep(["-nHZI", "-E", pattern, "--", *batch], ignore_read_errors=True)
        position = 0
        while position < len(output):
            boundary = output.index(b"\0", position)
            end = output.index(b"\n", boundary)
            path = os.fsdecode(output[position:boundary])
            matches.setdefault(path, []).append(output[boundary + 1:end])
            position = end + 1
    return matches


def main(path_pattern, token_pattern, word_pattern):
    paths = subprocess.check_output([
        "git", "ls-files", "-z", "--cached", "--others", "--exclude-standard",
    ]).split(b"\0")
    path_expression = re.compile(os.fsencode(path_pattern))
    records = []
    failures = 0

    def fail(message):
        nonlocal failures
        sys.stderr.buffer.write(os.fsencode("check-scriptw-identity: " + message) + b"\n")
        failures += 1

    # Excluded historical maps/notes are immutable too, not blanket exemptions.
    for historical, expected in HISTORY['files'].items():
        file = Path(historical)
        if file.is_file():
            with file.open('rb') as stream:
                if hashlib.file_digest(stream, 'sha256').hexdigest() != expected:
                    fail('frozen pre-rename evidence changed: ' + historical)

    with tempfile.TemporaryDirectory(prefix="scriptw-identity-") as temporary:
        files = []
        for raw_path in paths:
            if not raw_path:
                continue
            path = os.fsdecode(raw_path)
            if any(fnmatch.fnmatchcase(path, pattern) for pattern in EXCLUDED):
                continue
            if not os.path.exists(path):
                continue
            record = {"path": path, "audit": path, "errors": []}
            records.append(record)
            if any(path_expression.search(line) for line in raw_path.split(b"\n")):
                record["errors"].append("transitional path remains: " + path)
            # Gitlinks are directories: grep previously found no file content.
            if os.path.isdir(path):
                continue
            if (path in HISTORY['files'] or path.endswith('.md') or fnmatch.fnmatchcase(path, "docs/rust-rewrite/*.json")
                    or fnmatch.fnmatchcase(path, "docs/rust-rewrite/verify-*.py")):
                try:
                    raw = Path(path).read_bytes()
                    normalized = audit_source(Path(path), raw)
                    normalized = project_history_projection(path, raw, normalized) if path != 'README.md' else readme_history_projection(normalized)
                    for historical in HISTORY['files']:
                        if PREVIOUS in historical:
                            normalized = normalized.replace(historical.encode(), b'HISTORICAL_DOCUMENT')
                            if path.endswith('.md'):
                                normalized = normalized.replace(Path(historical).name.encode(), b'HISTORICAL_DOCUMENT')
                except (OSError, UnicodeError, ValueError) as error:
                    record["errors"].append("identity-provenance: " + str(error))
                    record["errors"].append(path + " has invalid historical provenance context")
                    continue
                audit = str(Path(temporary) / str(len(records)))
                Path(audit).write_bytes(normalized)
                record["audit"] = audit
            files.append(record["audit"])

        tokens = scan(token_pattern, files)
        words = scan(word_pattern, files)
        for record in records:
            path, audit = record["path"], record["audit"]
            for error in record["errors"]:
                if error.startswith("identity-provenance: "):
                    sys.stderr.buffer.write(os.fsencode(error) + b"\n")
                else:
                    fail(error)
            for line in tokens.get(audit, []):
                fail(path + ":" + os.fsdecode(line))
            if path.endswith(".md") or path in ("index.html", "CNAME"):
                continue
            lines = [line for line in words.get(audit, [])
                if os.fsencode("https://github.com/mewhhaha/" + RETIRED) not in line
                and os.fsencode(RETIRED + ".dev") not in line]
            if lines and fnmatch.fnmatchcase(path, "docs/rust-rewrite/native-*.json"):
                # Keep the literal GNU ERE, including locale/whitespace behavior.
                lines = grep(["-Ev", DOCTEST], data=b"\n".join(lines) + b"\n").split(b"\n")
            for line in lines:
                if line:
                    fail(path + ":" + os.fsdecode(line))
    return int(failures != 0)


if __name__ == "__main__":
    try:
        raise SystemExit(main(*sys.argv[1:]))
    except (OSError, subprocess.SubprocessError, ValueError) as error:
        print("check-scriptw-identity: " + str(error), file=sys.stderr)
        raise SystemExit(1)
