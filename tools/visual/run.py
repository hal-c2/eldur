#!/usr/bin/env python3
"""Visual parity harness runner for html-2-qml.

Usage:
    python3 tools/visual/run.py [--corpus tests/corpus] [--out target/visual]
                                [--convert-cmd "cargo run -q -p h2q_cli -- convert"]
                                [--cases a,b] [--baseline tools/visual/baseline.json]
                                [--update-baseline] [--width 1280] [--height 800]
    python3 tools/visual/run.py --selftest

For each case directory containing `index.html`:
  1. Renders `index.html` in headless Playwright Chromium (`render_html.mjs`).
  2. Converts `index.html` to QML via `--convert-cmd` (unless `--selftest`, where
     hand-written `Main.qml` sits alongside `index.html`). Conversion failures are
     recorded as `convert_failed` and execution continues.
  3. Renders `Main.qml` off-screen in `qml6` (`render_qml.py` + `Shot.qml`).
  4. Computes `score`, `ssim`, `edge_iou`, `pixel_mismatch`, and a side-by-side
     `diff.png` (`compare.py`).
  5. Writes `<out>/report.md` and `<out>/report.json`, compares against
     `tools/visual/baseline.json` when present (flagging regressions > 2 points),
     and updates `baseline.json` when `--update-baseline` is passed.
"""

from __future__ import annotations

import argparse
import json
import shlex
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import compare  # noqa: E402
import render_qml  # noqa: E402

RENDER_HTML_MJS = SCRIPT_DIR / "render_html.mjs"
DEFAULT_BASELINE_PATH = SCRIPT_DIR / "baseline.json"
SELFTEST_DIR = SCRIPT_DIR / "selftest"

DEFAULT_CORPUS_DIR = Path("tests/corpus")
DEFAULT_OUT_DIR = Path("target/visual")
DEFAULT_CONVERT_CMD = "cargo run -q -p h2q_cli -- convert"
REGRESSION_THRESHOLD_POINTS = 2.0
SELFTEST_MIN_GOOD_SCORE = 85.0
SELFTEST_MIN_MARGIN_POINTS = 20.0

SELFTEST_GOOD_CASES = (
    "01_colored_box",
    "02_two_columns",
    "03_list",
)
SELFTEST_WRONG_CASE = "04_wrong_layout"


class HarnessError(RuntimeError):
    """Raised on invalid CLI invocation or missing harness prerequisites."""


def _positive_int(raw: str) -> int:
    try:
        value = int(raw, 10)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(f"expected positive integer, got {raw!r}") from exc
    if value <= 0:
        raise argparse.ArgumentTypeError(f"must be > 0, got {value}")
    return value


def discover_cases(root_dir: Path, selected_csv: str | None = None) -> list[tuple[str, Path]]:
    """Discover sorted `(case_name, case_dir)` pairs containing `index.html` under `root_dir`."""
    if not root_dir.exists():
        raise HarnessError(
            f"Corpus directory does not exist: {root_dir} "
            "(pass --selftest to run the built-in self-test suite)"
        )
    if not root_dir.is_dir():
        raise HarnessError(f"Corpus path is not a directory: {root_dir}")

    found: dict[str, Path] = {}
    for html_file in sorted(root_dir.rglob("index.html")):
        rel_parent = html_file.parent.relative_to(root_dir)
        if any(part.startswith(".") for part in rel_parent.parts):
            continue
        case_name = rel_parent.as_posix()
        if case_name == ".":
            case_name = root_dir.name
        found[case_name] = html_file.parent

    if not found:
        raise HarnessError(f"No case directories containing index.html found under {root_dir}")

    if selected_csv is not None:
        wanted = [item.strip() for item in selected_csv.split(",") if item.strip()]
        if not wanted:
            raise HarnessError("--cases was provided but contained no case names")
        missing = [name for name in wanted if name not in found]
        if missing:
            available = ", ".join(sorted(found))
            raise HarnessError(
                f"Unknown case(s) in --cases: {', '.join(missing)} (available: {available})"
            )
        return [(name, found[name]) for name in wanted]

    return [(name, found[name]) for name in sorted(found)]


def render_html(
    html_file: Path,
    out_png: Path,
    *,
    width: int,
    height: int,
    timeout: float = 30.0,
) -> None:
    """Invoke `render_html.mjs` to screenshot `html_file` at `width`x`height`."""
    if shutil.which("node") is None:
        raise HarnessError("node executable not found on PATH")
    if not RENDER_HTML_MJS.is_file():
        raise HarnessError(f"Missing HTML renderer script: {RENDER_HTML_MJS}")

    out_png.parent.mkdir(parents=True, exist_ok=True)
    if out_png.exists():
        out_png.unlink()

    cmd = [
        "node",
        str(RENDER_HTML_MJS),
        str(html_file),
        str(out_png),
        "--width",
        str(width),
        "--height",
        str(height),
    ]
    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(f"render_html.mjs timed out after {timeout:.1f}s for {html_file}") from exc

    if proc.returncode != 0 or not out_png.is_file():
        detail = "\n".join(p.strip() for p in (proc.stdout, proc.stderr) if p and p.strip())
        raise RuntimeError(
            f"render_html.mjs failed (exit {proc.returncode}) for {html_file}: {detail}"
        )


def run_converter(
    convert_cmd: str,
    index_html: Path,
    qml_out_dir: Path,
    *,
    timeout: float = 120.0,
) -> tuple[bool, str]:
    """Run converter command to populate `qml_out_dir/Main.qml`. Return `(ok, error_msg)`."""
    if qml_out_dir.exists():
        shutil.rmtree(qml_out_dir)
    qml_out_dir.mkdir(parents=True, exist_ok=True)

    tokens = shlex.split(convert_cmd)
    if not tokens:
        return False, "Empty --convert-cmd"

    if any("{input}" in tok or "{output}" in tok for tok in tokens):
        cmd = [
            tok.format(input=str(index_html), output=str(qml_out_dir))
            for tok in tokens
        ]
    else:
        cmd = [*tokens, str(index_html), "-o", str(qml_out_dir)]

    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except FileNotFoundError as exc:
        return False, f"Converter executable not found ({cmd[0]}): {exc}"
    except subprocess.TimeoutExpired:
        return False, f"Converter timed out after {timeout:.1f}s"

    output = "\n".join(p.strip() for p in (proc.stdout, proc.stderr) if p and p.strip())
    if proc.returncode != 0:
        msg = output if output else f"Converter exited with code {proc.returncode}"
        return False, msg

    main_qml = qml_out_dir / "Main.qml"
    if not main_qml.is_file():
        extra = f"\n{output}" if output else ""
        return False, f"Converter exited 0 but did not produce {main_qml}{extra}"

    return True, ""


def load_baseline(baseline_path: Path) -> dict[str, float]:
    """Load baseline scores keyed by case name from `baseline.json`."""
    try:
        raw = json.loads(baseline_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise HarnessError(f"Failed to read baseline file {baseline_path}: {exc}") from exc

    cases_obj = raw.get("cases", raw) if isinstance(raw, dict) else None
    if not isinstance(cases_obj, dict):
        raise HarnessError(f"Invalid baseline format in {baseline_path}: expected object")

    scores: dict[str, float] = {}
    for case_name, entry in cases_obj.items():
        if isinstance(entry, (int, float)):
            scores[case_name] = float(entry)
        elif isinstance(entry, dict) and isinstance(entry.get("score"), (int, float)):
            scores[case_name] = float(entry["score"])
    return scores


def write_baseline(
    baseline_path: Path,
    results: list[dict[str, Any]],
    *,
    width: int,
    height: int,
) -> None:
    """Write `baseline.json` from completed case results."""
    cases_payload: dict[str, dict[str, float]] = {}
    for entry in sorted(results, key=lambda r: str(r["case"])):
        if entry.get("status") != "ok":
            continue
        cases_payload[str(entry["case"])] = {
            "score": float(entry["score"]),
            "ssim": float(entry["ssim"]),
            "edge_iou": float(entry["edge_iou"]),
            "pixel_mismatch": float(entry["pixel_mismatch"]),
        }

    payload = {
        "viewport": {"width": width, "height": height},
        "cases": cases_payload,
    }
    baseline_path.parent.mkdir(parents=True, exist_ok=True)
    baseline_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def _rel_posix(path: Path, base: Path) -> str:
    return path.resolve().relative_to(base.resolve()).as_posix()


def format_report_markdown(
    report: dict[str, Any],
    *,
    has_baseline: bool,
) -> str:
    """Format `report.md` with summary metrics, links to side-by-side PNGs, and failure logs."""
    vp = report["viewport"]
    summary = report["summary"]
    mean_str = f"{summary['mean_score']:.2f}" if summary["mean_score"] is not None else "n/a"

    lines: list[str] = [
        "# Visual Parity Report",
        "",
        f"- **Viewport:** {vp['width']}×{vp['height']}",
        f"- **Cases:** {summary['total']} total, {summary['ok']} scored, "
        f"{summary['convert_failed']} `convert_failed`, {summary['render_failed']} render failed",
        f"- **Mean score:** {mean_str}",
    ]
    if has_baseline:
        reg_list = summary["regressions"]
        reg_summary = ", ".join(reg_list) if reg_list else "none"
        lines.append(
            f"- **Regressions (> {REGRESSION_THRESHOLD_POINTS:g} pts vs baseline):** {reg_summary}"
        )
    lines.append("")

    if has_baseline:
        lines.append(
            "| Case | Status | Score | Baseline (Δ) | SSIM | Edge IoU | Mismatch | Images |"
        )
        lines.append("|---|---|---:|---:|---:|---:|---:|---|")
    else:
        lines.append("| Case | Status | Score | SSIM | Edge IoU | Mismatch | Images |")
        lines.append("|---|---|---:|---:|---:|---:|---|")

    failures: list[tuple[str, str, str]] = []

    for item in report["cases"]:
        case = item["case"]
        status = item["status"]
        if item.get("regression"):
            status_cell = f"{status} (REGRESSION)"
        else:
            status_cell = status

        if status == "ok":
            score_cell = f"{item['score']:.2f}"
            ssim_cell = f"{item['ssim']:.4f}"
            edge_cell = f"{item['edge_iou']:.4f}"
            mismatch_cell = f"{item['pixel_mismatch']:.4f}"
        else:
            score_cell = "—"
            ssim_cell = "—"
            edge_cell = "—"
            mismatch_cell = "—"
            if item.get("error"):
                failures.append((case, status, str(item["error"])))

        links: list[str] = []
        if item.get("diff_png"):
            links.append(f"[side-by-side]({item['diff_png']})")
        if item.get("html_png"):
            links.append(f"[html]({item['html_png']})")
        if item.get("qml_png"):
            links.append(f"[qml]({item['qml_png']})")
        links_cell = " · ".join(links) if links else "—"

        if has_baseline:
            b_score = item.get("baseline_score")
            delta = item.get("delta")
            if b_score is None:
                base_cell = "new"
            elif delta is None:
                base_cell = f"{b_score:.2f} (failed)"
            else:
                base_cell = f"{b_score:.2f} ({delta:+.2f})"
            lines.append(
                f"| `{case}` | {status_cell} | {score_cell} | {base_cell} | "
                f"{ssim_cell} | {edge_cell} | {mismatch_cell} | {links_cell} |"
            )
        else:
            lines.append(
                f"| `{case}` | {status_cell} | {score_cell} | "
                f"{ssim_cell} | {edge_cell} | {mismatch_cell} | {links_cell} |"
            )

    if failures:
        lines.extend(["", "## Errors", ""])
        for case, status, err in failures:
            trimmed = "\n".join(err.splitlines()[:25])
            lines.extend(
                [
                    f"### `{case}` (`{status}`)",
                    "",
                    "```text",
                    trimmed,
                    "```",
                    "",
                ]
            )

    return "\n".join(lines).rstrip() + "\n"


def evaluate_cases(
    cases: list[tuple[str, Path]],
    *,
    out_dir: Path,
    width: int,
    height: int,
    selftest: bool,
    convert_cmd: str,
    baseline_scores: dict[str, float] | None,
) -> dict[str, Any]:
    """Run rendering, optional conversion, and comparison across `cases`."""
    out_abs = out_dir.resolve()
    cases_out_root = out_abs / "cases"
    cases_out_root.mkdir(parents=True, exist_ok=True)

    results: list[dict[str, Any]] = []
    regressions: list[str] = []

    for case_name, case_dir in cases:
        case_out = cases_out_root / case_name
        case_out.mkdir(parents=True, exist_ok=True)

        index_html = case_dir / "index.html"
        html_png = case_out / "html.png"
        qml_png = case_out / "qml.png"
        diff_png = case_out / "diff.png"

        record: dict[str, Any] = {
            "case": case_name,
            "status": "ok",
            "score": None,
            "ssim": None,
            "edge_iou": None,
            "pixel_mismatch": None,
            "baseline_score": baseline_scores.get(case_name) if baseline_scores else None,
            "delta": None,
            "regression": False,
            "html_png": None,
            "qml_png": None,
            "diff_png": None,
            "error": None,
        }

        try:
            render_html(index_html, html_png, width=width, height=height)
            record["html_png"] = _rel_posix(html_png, out_abs)
        except Exception as exc:
            record["status"] = "html_render_failed"
            record["error"] = str(exc)
            if record["baseline_score"] is not None:
                record["regression"] = True
                regressions.append(case_name)
            results.append(record)
            continue

        if selftest:
            qml_source_dir = case_dir
        else:
            qml_source_dir = case_out / "qml"
            ok, err_msg = run_converter(convert_cmd, index_html, qml_source_dir)
            if not ok:
                record["status"] = "convert_failed"
                record["error"] = err_msg
                if record["baseline_score"] is not None:
                    record["regression"] = True
                    regressions.append(case_name)
                results.append(record)
                continue

        try:
            render_qml.render_qml(qml_source_dir, qml_png, width=width, height=height)
            record["qml_png"] = _rel_posix(qml_png, out_abs)
        except render_qml.QmlRenderError as exc:
            record["status"] = "qml_render_failed"
            record["error"] = str(exc)
            if record["baseline_score"] is not None:
                record["regression"] = True
                regressions.append(case_name)
            results.append(record)
            continue

        metrics = compare.compare_images(html_png, qml_png, diff_path=diff_png)
        record["diff_png"] = _rel_posix(diff_png, out_abs)
        record["score"] = metrics["score"]
        record["ssim"] = metrics["ssim"]
        record["edge_iou"] = metrics["edge_iou"]
        record["pixel_mismatch"] = metrics["pixel_mismatch"]

        b_score = record["baseline_score"]
        if b_score is not None:
            delta = round(float(metrics["score"]) - float(b_score), 4)
            record["delta"] = delta
            if (float(b_score) - float(metrics["score"])) > REGRESSION_THRESHOLD_POINTS:
                record["regression"] = True
                regressions.append(case_name)

        results.append(record)

    ok_scores = [float(r["score"]) for r in results if r["status"] == "ok" and r["score"] is not None]
    convert_failed_count = sum(1 for r in results if r["status"] == "convert_failed")
    render_failed_count = sum(
        1 for r in results if r["status"] in ("html_render_failed", "qml_render_failed")
    )

    report = {
        "viewport": {"width": width, "height": height},
        "summary": {
            "total": len(results),
            "ok": len(ok_scores),
            "convert_failed": convert_failed_count,
            "render_failed": render_failed_count,
            "mean_score": round(sum(ok_scores) / len(ok_scores), 4) if ok_scores else None,
            "regressions": regressions,
        },
        "cases": results,
    }

    report_json_path = out_abs / "report.json"
    report_md_path = out_abs / "report.md"
    report_json_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    report_md_path.write_text(
        format_report_markdown(report, has_baseline=baseline_scores is not None),
        encoding="utf-8",
    )
    return report


def print_console_summary(report: dict[str, Any], out_dir: Path) -> None:
    """Print a concise per-case score table to stdout."""
    header = f"{'CASE':<24} {'STATUS':<18} {'SCORE':>7} {'SSIM':>8} {'EDGE_IOU':>9} {'MISMATCH':>9}"
    print(header)
    print("-" * len(header))
    for item in report["cases"]:
        case = str(item["case"])
        status = str(item["status"])
        if item.get("regression"):
            status = f"{status}!"
        if item["status"] == "ok":
            print(
                f"{case:<24} {status:<18} {item['score']:>7.2f} "
                f"{item['ssim']:>8.4f} {item['edge_iou']:>9.4f} {item['pixel_mismatch']:>9.4f}"
            )
        else:
            print(f"{case:<24} {status:<18} {'-':>7} {'-':>8} {'-':>9} {'-':>9}")
    print("-" * len(header))
    summary = report["summary"]
    mean_txt = f"{summary['mean_score']:.2f}" if summary["mean_score"] is not None else "n/a"
    print(
        f"Total: {summary['total']} | OK: {summary['ok']} | "
        f"convert_failed: {summary['convert_failed']} | mean score: {mean_txt}"
    )
    print(f"Report written to {out_dir / 'report.md'} and {out_dir / 'report.json'}")


def verify_selftest(report: dict[str, Any]) -> None:
    """Assert selftest score thresholds and ordering, raising HarnessError on failure."""
    by_name = {str(item["case"]): item for item in report["cases"]}

    required = [*SELFTEST_GOOD_CASES, SELFTEST_WRONG_CASE]
    missing = [name for name in required if name not in by_name]
    if missing:
        raise HarnessError(f"Selftest missing expected case(s): {', '.join(missing)}")

    for name, item in by_name.items():
        if item["status"] != "ok":
            raise HarnessError(
                f"Selftest case {name!r} failed with status {item['status']}: {item.get('error')}"
            )

    good_scores: dict[str, float] = {}
    for name in SELFTEST_GOOD_CASES:
        score = float(by_name[name]["score"])
        good_scores[name] = score
        if score <= SELFTEST_MIN_GOOD_SCORE:
            raise HarnessError(
                f"Selftest matching pair {name!r} scored {score:.2f}, "
                f"expected > {SELFTEST_MIN_GOOD_SCORE:.1f}"
            )

    wrong_score = float(by_name[SELFTEST_WRONG_CASE]["score"])
    min_good_name = min(good_scores, key=lambda k: good_scores[k])
    min_good_score = good_scores[min_good_name]

    if wrong_score >= min_good_score - SELFTEST_MIN_MARGIN_POINTS:
        raise HarnessError(
            f"Selftest deliberately wrong pair {SELFTEST_WRONG_CASE!r} scored {wrong_score:.2f}, "
            f"not clearly lower than {min_good_name!r} ({min_good_score:.2f}; "
            f"required margin >= {SELFTEST_MIN_MARGIN_POINTS:.1f} pts)"
        )

    print(
        f"SELFTEST PASSED: all {len(SELFTEST_GOOD_CASES)} matching pairs scored "
        f"> {SELFTEST_MIN_GOOD_SCORE:.0f} (min={min_good_score:.2f} on {min_good_name}), "
        f"and {SELFTEST_WRONG_CASE} scored clearly lower ({wrong_score:.2f})."
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="run.py",
        description="Run the HTML-to-QML visual parity benchmark or built-in self-test.",
    )
    parser.add_argument(
        "--corpus",
        type=Path,
        default=DEFAULT_CORPUS_DIR,
        help=f"Directory of corpus cases containing index.html (default: {DEFAULT_CORPUS_DIR})",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=DEFAULT_OUT_DIR,
        help=f"Output directory for screenshots and reports (default: {DEFAULT_OUT_DIR})",
    )
    parser.add_argument(
        "--convert-cmd",
        type=str,
        default=DEFAULT_CONVERT_CMD,
        help=f'Converter command prefix (default: "{DEFAULT_CONVERT_CMD}")',
    )
    parser.add_argument(
        "--cases",
        type=str,
        default=None,
        help="Comma-separated subset of case names to run (e.g. a,b)",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=None,
        help=f"Baseline JSON path (default: {DEFAULT_BASELINE_PATH})",
    )
    parser.add_argument(
        "--update-baseline",
        action="store_true",
        help="Write current case scores to baseline.json",
    )
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="Run built-in hand-written HTML/QML pairs in tools/visual/selftest/ without converter",
    )
    parser.add_argument(
        "--width",
        type=_positive_int,
        default=1280,
        help="Viewport width in pixels (default: 1280)",
    )
    parser.add_argument(
        "--height",
        type=_positive_int,
        default=800,
        help="Viewport height in pixels (default: 800)",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        corpus_dir = SELFTEST_DIR if args.selftest else args.corpus
        out_dir = args.out

        baseline_path = args.baseline if args.baseline is not None else DEFAULT_BASELINE_PATH
        use_baseline = (not args.selftest or args.baseline is not None) and baseline_path.is_file()
        baseline_scores = load_baseline(baseline_path) if use_baseline else None

        cases = discover_cases(corpus_dir, args.cases)
        report = evaluate_cases(
            cases,
            out_dir=out_dir,
            width=args.width,
            height=args.height,
            selftest=args.selftest,
            convert_cmd=args.convert_cmd,
            baseline_scores=baseline_scores,
        )
        print_console_summary(report, out_dir)

        if args.update_baseline:
            write_baseline(
                baseline_path,
                report["cases"],
                width=args.width,
                height=args.height,
            )
            print(f"Updated baseline: {baseline_path}")

        if args.selftest:
            verify_selftest(report)
            return 0

        regressions = report["summary"]["regressions"]
        if regressions and not args.update_baseline:
            print(
                f"run.py: error: visual parity regressions (> {REGRESSION_THRESHOLD_POINTS:g} pts): "
                f"{', '.join(regressions)}",
                file=sys.stderr,
            )
            return 1

        if any(r["status"] == "html_render_failed" for r in report["cases"]):
            print("run.py: error: one or more HTML renders failed", file=sys.stderr)
            return 1

    except HarnessError as exc:
        print(f"run.py: error: {exc}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
