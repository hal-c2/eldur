#!/usr/bin/env python3
"""Per-element vertical drift between Chromium and the converted QML.

Usage:
    python3 tools/visual/drift.py tests/corpus/02-block-flow [--qml DIR] [--json OUT]
                                  [--backend software|gl]

Chromium's boxes come from `geometry.mjs --elements`: every element with a
box that is not inline, in document order, with the `<tag id class>` text
the converter writes in its source comments. The QML side converts the page
(or takes `--qml DIR`), tags the object under each source comment with
`property string eldurGeo: "<n>"` in a copy of `Main.qml`, renders it through
`render_qml.py`, and reads the scene geometry `Shot.qml` logs for tagged
objects once the page has settled (the moment it would grab).

Elements are matched by their description and its occurrence count (the
third `<p>` in Chromium is the third `// <p>` in the QML); unmatched ones
are counted. The table lists dy = qml.y - chromium.y and dh likewise, in
Chromium's document order, so where an error first appears and how it
grows can be read down the dy column.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from collections import defaultdict
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent.parent
sys.path.insert(0, str(SCRIPT_DIR))

from render_qml import render_qml  # noqa: E402

COMMENT = re.compile(r"^(\s*)// (<[a-z][^>]*>)\s*$")
OPENING = re.compile(r"^(\s*)(?:[A-Za-z_][\w.]*\s*:\s*)?[A-Z][\w.]*\s*\{\s*$")


def tag_qml(text: str) -> tuple[str, list[str]]:
    """Adds `property string eldurGeo: "<n>"` to the object after each source
    comment. Returns the new text and the description of each n."""
    lines = text.splitlines()
    out: list[str] = []
    keys: list[str] = []
    pending: str | None = None
    for line in lines:
        out.append(line)
        m = COMMENT.match(line)
        if m:
            pending = m.group(2)
            continue
        if pending is not None:
            o = OPENING.match(line)
            if o:
                out.append(f'{o.group(1)}    property string eldurGeo: "{len(keys)}"')
                keys.append(pending)
            pending = None
    return "\n".join(out) + "\n", keys


def chromium_elements(page: Path) -> list[dict[str, Any]]:
    proc = subprocess.run(
        ["node", str(SCRIPT_DIR / "geometry.mjs"), "--elements", str(page)],
        capture_output=True,
        text=True,
        check=True,
    )
    return [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]


def qml_elements(
    qml_dir: Path, *, backend: str, width: int, height: int
) -> list[dict[str, Any]]:
    with tempfile.TemporaryDirectory(prefix="eldur-drift-") as tmp:
        work = Path(tmp) / "qml"
        shutil.copytree(qml_dir, work)
        main = work / "Main.qml"
        tagged, keys = tag_qml(main.read_text(encoding="utf-8"))
        main.write_text(tagged, encoding="utf-8")
        lines = render_qml(
            work, Path(tmp) / "shot.png", width=width, height=height, backend=backend
        )
    rows = []
    for line in lines:
        i = line.find("eldurGeo ")
        if i < 0:
            continue
        r = json.loads(line[i + len("eldurGeo ") :])
        r["describe"] = keys[int(r.pop("key"))]
        rows.append(r)
    return rows


def match(
    chromium: list[dict[str, Any]], qml: list[dict[str, Any]]
) -> tuple[list[dict[str, Any]], int, int]:
    """Pairs the k-th Chromium element of a description with the k-th QML
    object of it. Returns rows in Chromium order and the unmatched counts."""
    by_desc: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for q in qml:
        by_desc[q["describe"]].append(q)
    seen: dict[str, int] = defaultdict(int)
    rows = []
    unmatched_chromium = 0
    for c in chromium:
        k = seen[c["describe"]]
        seen[c["describe"]] += 1
        candidates = by_desc.get(c["describe"], [])
        if k >= len(candidates):
            unmatched_chromium += 1
            continue
        q = candidates[k]
        rows.append(
            {
                "describe": c["describe"],
                "chromium": {key: c[key] for key in ("x", "y", "width", "height")},
                "qml": {key: q[key] for key in ("x", "y", "width", "height")},
                "dy": q["y"] - c["y"],
                "dh": q["height"] - c["height"],
            }
        )
    unmatched_qml = sum(max(0, len(v) - seen[d]) for d, v in by_desc.items())
    return rows, unmatched_chromium, unmatched_qml


def summarize(rows: list[dict[str, Any]], viewport_height: int) -> dict[str, Any]:
    visible = [r for r in rows if r["chromium"]["y"] < viewport_height]
    first = next((r for r in rows if abs(r["dy"]) > 0.5), None)
    return {
        "elements": len(rows),
        "max_abs_dy": max((abs(r["dy"]) for r in rows), default=0.0),
        "mean_abs_dy": sum(abs(r["dy"]) for r in rows) / len(rows) if rows else 0.0,
        "last_dy_in_viewport": visible[-1]["dy"] if visible else 0.0,
        "last_dy": rows[-1]["dy"] if rows else 0.0,
        "first_over_half_px": first["describe"] if first else None,
    }


def format_table(rows: list[dict[str, Any]]) -> str:
    out = [f"{'element':<44} {'chr.y':>9} {'qml.y':>9} {'dy':>7} {'chr.h':>8} {'qml.h':>8} {'dh':>7}"]
    for r in rows:
        c, q = r["chromium"], r["qml"]
        out.append(
            f"{r['describe'][:44]:<44} {c['y']:9.2f} {q['y']:9.2f} {r['dy']:+7.2f}"
            f" {c['height']:8.2f} {q['height']:8.2f} {r['dh']:+7.2f}"
        )
    return "\n".join(out)


def convert(page: Path, out: Path) -> None:
    subprocess.run(
        ["cargo", "run", "-q", "-p", "eldur_cli", "--", "convert", "--no-animations", "-q", str(page), "-o", str(out)],
        cwd=REPO_ROOT,
        check=True,
        stderr=subprocess.DEVNULL,
    )


def measure(
    page: Path, qml_dir: Path, *, backend: str = "software", width: int = 1280, height: int = 800
) -> dict[str, Any]:
    rows, uc, uq = match(
        chromium_elements(page), qml_elements(qml_dir, backend=backend, width=width, height=height)
    )
    return {
        "page": str(page),
        "summary": {**summarize(rows, height), "unmatched_chromium": uc, "unmatched_qml": uq},
        "rows": rows,
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("case", type=Path, help="corpus case directory or page.html")
    ap.add_argument("--qml", type=Path, help="converted output (default: convert now)")
    ap.add_argument("--json", type=Path, help="write the full result here")
    # Layout does not depend on the scene graph, so the default needs no sway.
    ap.add_argument("--backend", default="software", choices=("software", "gl"))
    args = ap.parse_args(argv)

    page = args.case / "index.html" if args.case.is_dir() else args.case
    with tempfile.TemporaryDirectory(prefix="eldur-drift-out-") as tmp:
        qml_dir = args.qml
        if qml_dir is None:
            qml_dir = Path(tmp) / "out"
            convert(page.resolve(), qml_dir)
        result = measure(page, qml_dir, backend=args.backend)
    print(format_table(result["rows"]))
    print(json.dumps(result["summary"]))
    if args.json:
        args.json.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
