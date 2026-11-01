#!/usr/bin/env python3
"""What a case paints in the regions its `regions.json` names, against Chromium.

The score of a whole page cannot see one wrong box, and a layout test cannot see
paint, so a paint feature is asserted here on its own pixels:

    python3 tools/visual/regions.py tests/corpus/46-backgrounds [--out DIR]

`regions.json` is a list of `{"selector", "pad"?, "max_mismatch"?, "max_delta"?,
"known"?}`. The rectangle is the element's Chromium box grown by `pad` px, so a
shadow or outline outside it counts. `mismatch` is the fraction of its pixels
with a channel more than 32 off (as compare.py); `delta` the mean channel
difference. A region with `known` says why it does not match yet: it must still
fail, so that a fixed gap cannot stay listed as a gap.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

import compare  # noqa: E402
import render_qml  # noqa: E402
import run as visual_run  # noqa: E402

MAX_MISMATCH = 0.03
MAX_DELTA = 6.0


def boxes(page: Path, selectors: list[str]) -> dict[str, dict]:
    done = subprocess.run(
        ["node", str(SCRIPT_DIR / "geometry.mjs"), str(page), *selectors],
        capture_output=True,
        text=True,
        check=True,
    )
    found: dict[str, dict] = {}
    for line in done.stdout.splitlines():
        box = json.loads(line)
        found.setdefault(box["select"], box)
    return found


def measure(html: np.ndarray, qml: np.ndarray, box: dict, pad: int) -> tuple[float, float]:
    height, width = html.shape[:2]
    x0 = max(0, int(np.floor(box["x"])) - pad)
    y0 = max(0, int(np.floor(box["y"])) - pad)
    x1 = min(width, int(np.ceil(box["x"] + box["width"])) + pad)
    y1 = min(height, int(np.ceil(box["y"] + box["height"])) + pad)
    a = html[y0:y1, x0:x1].astype(int)
    b = qml[y0:y1, x0:x1].astype(int)
    if a.size == 0:
        raise ValueError(f"empty region: {box}")
    diff = np.abs(a - b)
    return float((diff.max(axis=2) > 32).mean()), float(diff.mean())


def check_case(case: Path, html_png: Path, qml_png: Path) -> list[dict]:
    """One result per region; `ok` is whether it holds what `regions.json` says."""
    regions = json.loads((case / "regions.json").read_text())
    found = boxes(case / "index.html", [r["selector"] for r in regions])
    html = compare.load_rgb(html_png)
    qml = compare.load_rgb(qml_png)
    out = []
    for region in regions:
        selector = region["selector"]
        box = found.get(selector)
        if box is None:
            out.append({"selector": selector, "ok": False, "error": "no such element"})
            continue
        mismatch, delta = measure(html, qml, box, region.get("pad", 0))
        passes = mismatch <= region.get("max_mismatch", MAX_MISMATCH) and delta <= region.get(
            "max_delta", MAX_DELTA
        )
        known = region.get("known")
        out.append(
            {
                "selector": selector,
                "mismatch": round(mismatch, 4),
                "delta": round(delta, 2),
                "passes": passes,
                "known": known,
                "ok": passes != bool(known),
            }
        )
    return out


def render_case(case: Path, out: Path, env_backend: str = "gl") -> tuple[Path, Path, list[str]]:
    out.mkdir(parents=True, exist_ok=True)
    html_png = out / "html.png"
    qml_png = out / "qml.png"
    visual_run.render_html(case / "index.html", html_png, width=1280, height=800)
    ok, err = visual_run.run_converter(
        "cargo run -q -p eldur_cli -- convert --no-animations", case / "index.html", out / "qml", timeout=600.0
    )
    if not ok:
        raise RuntimeError(err)
    warnings = render_qml.render_qml(out / "qml", qml_png, backend=env_backend)
    compare.compare_images(html_png, qml_png, diff_path=out / "diff.png")
    return html_png, qml_png, warnings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("case", type=Path, nargs="+")
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    failed = False
    with tempfile.TemporaryDirectory() as tmp:
        for case in args.case:
            out = (args.out or Path(tmp)) / case.name
            html_png, qml_png, warnings = render_case(case, out)
            for warning in warnings:
                print(f"{case.name}: {warning}")
            failed |= bool(warnings)
            for r in check_case(case, html_png, qml_png):
                mark = "ok  " if r["ok"] else "FAIL"
                state = "pass" if r.get("passes") else "off "
                known = " KNOWN" if r.get("known") else ""
                print(
                    f"{mark} {state} {r.get('mismatch', 0):7.4f} {r.get('delta', 0):6.2f} "
                    f"{case.name} {r['selector']}{known}"
                )
                failed |= not r["ok"]
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
