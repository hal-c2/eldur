#!/usr/bin/env python3
"""Render a QML scene (directory containing Main.qml) to a PNG screenshot.

Usage:
    python3 tools/visual/render_qml.py <dir-with-Main.qml> <out.png> [--width 1280] [--height 800]

Runs Qt 6 `qml6` off-screen (`QT_QPA_PLATFORM=offscreen`) with software
rasterisation (`QT_QUICK_BACKEND=software`, `--software`), deterministic DPI/scale
(`QT_FONT_DPI=96`, `QT_SCALE_FACTOR=1`), and a 10-second timeout via `Shot.qml`.
Supports both `ApplicationWindow`/`Window` and `Item` root components without
opening a second window.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image

SCRIPT_DIR = Path(__file__).resolve().parent
SHOT_QML = SCRIPT_DIR / "Shot.qml"
DEFAULT_TIMEOUT_SEC = 10.0


class QmlRenderError(RuntimeError):
    """Raised when qml6 fails to load or screenshot the target QML file."""


def _positive_int(raw: str) -> int:
    try:
        value = int(raw, 10)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(f"expected positive integer, got {raw!r}") from exc
    if value <= 0:
        raise argparse.ArgumentTypeError(f"must be > 0, got {value}")
    return value


def resolve_main_qml(target: Path) -> tuple[Path, Path]:
    """Return (qml_dir, main_qml_file) for a directory or explicit .qml path."""
    resolved = target.resolve()
    if not resolved.exists():
        raise QmlRenderError(f"QML path does not exist: {resolved}")
    if resolved.is_dir():
        main_qml = resolved / "Main.qml"
        if not main_qml.is_file():
            raise QmlRenderError(f"Main.qml not found in directory: {resolved}")
        return resolved, main_qml
    if resolved.is_file() and resolved.suffix == ".qml":
        return resolved.parent, resolved
    raise QmlRenderError(
        f"Expected a directory containing Main.qml (or a .qml file), got: {resolved}"
    )


def render_qml(
    qml_target: Path,
    out_png: Path,
    *,
    width: int = 1280,
    height: int = 800,
    timeout: float = DEFAULT_TIMEOUT_SEC,
) -> None:
    """Render `qml_target` at `width`x`height` into `out_png`, raising QmlRenderError on failure."""
    if width <= 0 or height <= 0:
        raise QmlRenderError(f"Invalid viewport dimensions: {width}x{height}")
    if not SHOT_QML.is_file():
        raise QmlRenderError(f"Wrapper Shot.qml is missing: {SHOT_QML}")
    if shutil.which("qml6") is None:
        raise QmlRenderError("qml6 executable not found on PATH")

    qml_dir, main_qml = resolve_main_qml(qml_target)
    out_abs = out_png.resolve()
    out_abs.parent.mkdir(parents=True, exist_ok=True)
    if out_abs.exists():
        out_abs.unlink()

    env = os.environ.copy()
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["QT_QUICK_BACKEND"] = "software"
    env["QT_FONT_DPI"] = "96"
    env["QT_SCALE_FACTOR"] = "1"
    # Prevent stale .qmlc bytecode caches when Main.qml is regenerated between runs.
    env["QML_DISABLE_DISK_CACHE"] = "1"

    cmd = [
        "qml6",
        "--software",
        "-I",
        str(qml_dir),
        str(SHOT_QML),
        "--",
        main_qml.as_uri(),
        str(width),
        str(height),
        str(out_abs),
    ]

    try:
        proc = subprocess.run(
            cmd,
            cwd=str(qml_dir),
            env=env,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        out_so_far = ((exc.stdout or "") + "\n" + (exc.stderr or "")).strip()
        detail = f"\nqml6 output before timeout:\n{out_so_far}" if out_so_far else ""
        raise QmlRenderError(
            f"qml6 timed out after {timeout:.1f}s while rendering {main_qml}{detail}"
        ) from exc

    combined_output = "\n".join(
        part.strip() for part in (proc.stdout, proc.stderr) if part and part.strip()
    )

    if proc.returncode != 0:
        detail = f"\n{combined_output}" if combined_output else "\n(no output from qml6)"
        raise QmlRenderError(
            f"qml6 exited with status {proc.returncode} for {main_qml}:{detail}"
        )

    if not out_abs.is_file() or out_abs.stat().st_size == 0:
        detail = f"\n{combined_output}" if combined_output else ""
        raise QmlRenderError(
            f"qml6 exited 0 but did not write screenshot {out_abs}{detail}"
        )

    with Image.open(out_abs) as img:
        actual_w, actual_h = img.size
    if (actual_w, actual_h) != (width, height):
        raise QmlRenderError(
            f"Rendered QML screenshot {out_abs} has size {actual_w}x{actual_h}, expected {width}x{height}"
        )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="render_qml.py",
        description="Render a directory containing Main.qml to an off-screen PNG screenshot.",
    )
    parser.add_argument(
        "qml_dir",
        type=Path,
        help="Directory containing Main.qml (or direct path to a .qml file)",
    )
    parser.add_argument(
        "out_png",
        type=Path,
        help="Destination PNG file path",
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
    parser.add_argument(
        "--timeout",
        type=float,
        default=DEFAULT_TIMEOUT_SEC,
        help=f"Timeout in seconds for qml6 execution (default: {DEFAULT_TIMEOUT_SEC:g})",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        render_qml(
            args.qml_dir,
            args.out_png,
            width=args.width,
            height=args.height,
            timeout=args.timeout,
        )
    except QmlRenderError as exc:
        print(f"render_qml.py: error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
