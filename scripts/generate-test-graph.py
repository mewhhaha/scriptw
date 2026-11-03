#!/usr/bin/env python3
"""Generate the README's dated Test262 bar chart from published evidence."""
import argparse
from html import escape
from pathlib import Path
import re

from status_publication import load_publication
from test262_chart_evidence import (
    EVIDENCE, FULL_EVIDENCE, build as build_reconciliation,
    build_full, load as load_reconciliation,
)

ROOT = Path(__file__).resolve().parents[1]
START = "<!-- scriptw-test-graph:start -->"
END = "<!-- scriptw-test-graph:end -->"
SNAPSHOT = "test262/snapshots/published-status-wasm-aot.json"
CHART = "docs/rust-rewrite/assets/test262-status.svg"


def categories(publication, reconciliation=None):
    real = publication["real_suite"]
    outcomes = {}
    for row in real["counts_per_outcome"]:
        label, count = row["label"], row["count"]
        if label in outcomes:
            raise ValueError("duplicate published outcome")
        if type(count) is not int or count < 0:
            raise ValueError("outcome counts must be nonnegative integers")
        outcomes[label] = count
    if set(outcomes) != {"Success", "NotImplemented", "Crash", "Bug"}:
        raise ValueError("unknown published outcomes need an explicit chart category")
    if (
        type(real["total"]) is not int
        or real["total"] <= 0
        or type(real["passed"]) is not int
        or sum(outcomes.values()) != real["total"]
        or outcomes["Success"] != real["passed"]
    ):
        raise ValueError("published totals disagree")
    if reconciliation is not None:
        projected = reconciliation["scoped_chart_counts"]
        if sum(projected.values()) != real["total"]:
            raise ValueError("reconciled chart changed the full denominator")
        outcomes = projected
    return [
        ("passing", "Passing", outcomes["Success"], "#2da44e"),
        ("failing", "Failing", outcomes["Bug"] + outcomes["Crash"], "#cf222e"),
        ("not-implemented", "Not implemented", outcomes["NotImplemented"], "#8c959f"),
        # Historical publications lack scope classification; numeric counts
        # require the independently verified case-level projection.
        ("out-of-scope", "Out of scope", outcomes.get("OutOfScope"), "#0969da"),
    ]


def chart_display(publication, reconciliation=None):
    real = publication["real_suite"]
    rows = categories(publication, reconciliation)
    identity = real.get("compiler_identity")
    measured = None
    if identity:
        revision = identity["source_revision"]
        if revision["kind"] == "git-commit":
            measured = revision["commit"][:9]
        elif revision["kind"] == "unversioned-archive":
            measured = "archive " + identity["source_fingerprint"][:9]
        else:
            raise ValueError("unknown compiler source revision kind")
    return {
        "rows": rows,
        "real": real,
        "measured": measured,
        "current": bool(identity),
        "date": real["refresh_date"],
        "pin": real["pinned_revisions"]["test262"][:9],
        "total": real["total"],
    }


def render(publication, evidence_path=SNAPSHOT, reconciliation=None):
    display = chart_display(publication, reconciliation)
    real = display["real"]
    if reconciliation is not None:
        counts = reconciliation["scoped_chart_counts"]
        failed = counts["Bug"] + counts["Crash"]
        if reconciliation.get("full_current_source_rerun"):
            return f"""{START}
![Test262: {counts['Success']:,} passing in scope, {failed:,} failing, {counts['NotImplemented']:,} not implemented, {counts['OutOfScope']:,} out of standalone Wasm scope]({CHART})

**Published full run, {reconciliation['date']}:** {display['total']:,} mode-specific
executions at pin `{display['pin']}`, compiler `{display['measured']}`.
The **{counts['OutOfScope']:,} out-of-scope executions** passed through the native host;
standalone Wasm excludes their dynamic source compilation. All four categories
retain the full denominator, including every failure.
[Scope inventory]({FULL_EVIDENCE}) · [Published result]({evidence_path}) ·
[Scope policy](docs/rust-rewrite/runtime-requirements.md#out-of-scope-tests).
{END}"""
        repaired = categories(publication)[1][2] - failed
        return f"""{START}
![Test262 checkpoints: {counts['Success']:,} passing in scope, {failed:,} failing, {counts['NotImplemented']:,} not implemented, {counts['OutOfScope']:,} out of standalone Wasm scope]({CHART})

**Reconciled checkpoints, {reconciliation['date']}:** {display['total']:,} mode-specific
executions at pin `{display['pin']}`. This combines the complete **2026-10-20**
baseline with the exact original-failure replay: **{repaired:,} repaired, {failed:,} still failing**.
The other baseline successes have not had a fresh full rerun on the repaired compiler.

Blue is an audited **{counts['OutOfScope']:,}-execution subset** focused on dynamic
`eval` / Function source compilation outside the standalone Wasm product promise.
These cases passed through the native host; their historical results remain intact.
Ordinary API properties and `call` / `apply` / `bind` stay in the measured categories.
The four bars partition the original {display['total']:,} executions; scope is not a failure fix.
[Chart inputs and exact scope inventory]({EVIDENCE}) ·
[Original complete baseline]({evidence_path}) ·
[Failure replay](docs/rust-rewrite/failed-case-replay-20261021.md) ·
[Scope policy](docs/rust-rewrite/runtime-requirements.md#out-of-scope-tests).
{END}"""
    passed, failed, missing, _ = [row[2] for row in display["rows"]]
    tick = chr(96)
    if display["current"]:
        measured = display["measured"]
        units = "mode-specific executions"
        context = (
            f"Measured compiler: {tick}{measured}{tick} on the Wasm-AOT backend. "
            "This is a complete pinned-suite baseline, not full ECMAScript conformance."
        )
    else:
        units = "path-based entries"
        context = (
            "This historical snapshot does not establish\ncurrent compiler conformance."
        )
    return f"""{START}
![Test262: {passed:,} passing, {failed:,} failing, {missing:,} not implemented; out of scope not classified]({CHART})

Published snapshot: **{display['date']}**, pin {tick}{display['pin']}{tick},
{display['total']:,} {units}. Red combines {failed:,} bugs/crashes;
gray means unimplemented, not unrun. {context}
[Published evidence]({evidence_path}) · [Run details](docs/rust-rewrite/test262-current.md).

Blue reserves **out of scope** for dynamic `eval` / Function-constructor cases
requiring a parser or VM inside emitted Wasm. Scope classification is unaudited,
so that count is **unknown**, not zero; the published totals
above remain unchanged. [Scope details](docs/rust-rewrite/runtime-requirements.md#out-of-scope-tests).
{END}"""


def render_svg(publication, reconciliation=None):
    display = chart_display(publication, reconciliation)
    rows, real = display["rows"], display["real"]
    highest = max(count for _, _, count, _ in rows if count is not None)
    magnitude = 10 ** (len(str(highest)) - 1)
    maximum = max(
        4,
        next(
            magnitude * factor
            for factor in [1, 1.25, 1.5, 2, 2.5, 4, 5, 7.5, 10]
            if magnitude * factor >= highest
        ),
    )
    top, bottom, left, right = 78, 326, 96, 724
    height = bottom - top
    description = "; ".join(
        (
            f"{label}: {count:,}"
            if count is not None
            else f"{label}: not classified, count unknown"
        )
        for _, label, count, _ in rows
    )
    current = display["current"]
    source_unit = "executions" if current else "entries"
    source = (
        f"{display['date']} · pin {display['pin']} · {display['total']:,} {source_unit}"
    )
    if reconciliation is not None:
        source = f"Baseline {display['date']} + replay {reconciliation['date']} · pin {display['pin']} · {display['total']:,} executions"
    fresh = reconciliation is not None and reconciliation.get("full_current_source_rerun")
    if fresh:
        source = f"{display['date']} · compiler {display['measured']} · pin {display['pin']} · {display['total']:,} executions"
    context = ("Baseline + exact failure replay; scoped view" if reconciliation is not None else
               "Complete pinned-suite baseline" if current else "Historical snapshot")
    if fresh:
        context = "Fresh complete pinned-suite run; scoped view"
    title = "Test262 verification by product scope" if reconciliation is not None else "Published Test262 results"
    footer = (f"{display['rows'][-1][2]:,} audited dynamic-source executions outside standalone Wasm scope; not a fresh full rerun."
              if reconciliation is not None else "Out-of-scope count unknown; published totals unchanged.")
    if fresh:
        footer = f"{display['rows'][-1][2]:,} audited native-host passes outside standalone Wasm scope; failures remain counted."
    unit = "test executions" if current else "test entries"
    elements = [
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 760 422" role="img" aria-labelledby="title description">',
        f'<title id="title">{title}</title>',
        f'<desc id="description">{escape(description)}. Linear counts from zero. {context}: {escape(source)}. Not full ECMAScript conformance.</desc>',
        '<rect width="760" height="422" rx="12" fill="#ffffff"/>',
        '<g font-family="system-ui, sans-serif" fill="#24292f">',
        f'<text x="30" y="35" font-size="20" font-weight="600">{title}</text>',
        f'<text x="30" y="57" font-size="12" fill="#57606a">{context} · number of {unit}</text>',
    ]
    for index in range(5):
        value = maximum * index / 4
        y = bottom - height * index / 4
        label = f"{value:,.0f}" if value.is_integer() else f"{value:g}"
        elements.extend(
            [
                f'<path d="M {left} {y:g} H {right}" stroke="#d8dee4" stroke-width="1"/>',
                f'<text x="{left - 12}" y="{y + 4:g}" text-anchor="end" font-size="12" fill="#57606a">{label}</text>',
            ]
        )
    for (key, label, count, color), x in zip(rows, [174, 332, 490, 648], strict=True):
        if count is None:
            elements.extend(
                [
                    f'<g id="{key}" data-status="unclassified" fill="{color}">',
                    f'<text x="{x}" y="300" text-anchor="middle" font-size="12" font-weight="600">Not classified</text>',
                    f'<path d="M {x - 40} {bottom} H {x + 40}" stroke="{color}" stroke-width="3" stroke-dasharray="6 4"/>',
                    f'<text x="{x}" y="357" text-anchor="middle" font-size="15">{label}</text>',
                    "</g>",
                ]
            )
            continue
        bar_height = height * count / maximum
        y = bottom - bar_height
        elements.extend(
            [
                f'<rect id="{key}" data-count="{count}" x="{x - 40}" y="{y:.6f}" width="80" height="{bar_height:.6f}" fill="{color}"/>',
                f'<text x="{x}" y="{y - 12:.6f}" text-anchor="middle" font-size="17" font-weight="600">{count:,}</text>',
                f'<text x="{x}" y="357" text-anchor="middle" font-size="15">{label}</text>',
            ]
        )
    elements.extend(
        [
            f'<text x="380" y="378" text-anchor="middle" font-size="11" fill="#0969da">{footer}</text>',
            f'<text x="380" y="397" text-anchor="middle" font-size="12" fill="#57606a">{escape(source)}</text>',
            "</g>",
            "</svg>",
            "",
        ]
    )
    return "\n".join(elements)


def replace_block(readme, graph):
    if (
        readme.count(START) != 1
        or readme.count(END) != 1
        or readme.index(START) >= readme.index(END)
    ):
        raise ValueError("expected one ordered generated chart block in README")
    return re.sub(
        re.escape(START) + ".*?" + re.escape(END), lambda _: graph, readme, flags=re.S
    )


def generate(root=ROOT, check=False):
    publication, evidence_path = load_publication(root)
    reconciliation = load_reconciliation(root, publication)
    path = root / "README.md"
    outputs = {
        path: replace_block(path.read_text(), render(publication, evidence_path, reconciliation)),
        root / CHART: render_svg(publication, reconciliation),
    }
    stale = [
        path
        for path, text in outputs.items()
        if not path.exists() or path.read_text() != text
    ]
    if check and stale:
        raise ValueError(
            "stale generated chart output: "
            + ", ".join(str(path.relative_to(root)) for path in stale)
        )
    for path in stale:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(outputs[path])
    return stale


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--refresh-evidence", action="store_true", help="rebuild the pinned baseline/replay/scope reconciliation")
    parser.add_argument("--full-evidence", action="store_true", help="with --refresh-evidence, retain and verify a newly published full native run")
    args = parser.parse_args()
    if args.check and args.refresh_evidence:
        parser.error("--check is read-only; use --refresh-evidence separately")
    if args.full_evidence and not args.refresh_evidence:
        parser.error("--full-evidence requires --refresh-evidence")
    if args.refresh_evidence:
        import json
        full = args.full_evidence or (ROOT / FULL_EVIDENCE).exists()
        evidence = build_full(ROOT) if full else build_reconciliation(ROOT)
        (ROOT / (FULL_EVIDENCE if full else EVIDENCE)).write_text(json.dumps(evidence, indent=2) + "\n")
    generate(check=args.check)
    print("README Test262 chart: current with published evidence")


if __name__ == "__main__":
    main()
