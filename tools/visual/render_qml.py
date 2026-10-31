#!/usr/bin/env python3
"""Render a QML scene (directory containing Main.qml) to a PNG screenshot.

Usage:
    python3 tools/visual/render_qml.py <dir-with-Main.qml> <out.png> [--width 1280] [--height 800]
                                       [--strict] [--backend software|gl]

Two backends, both with deterministic DPI/scale (`QT_FONT_DPI=96`,
`QT_SCALE_FACTOR=1`) and a 10-second timeout via `Shot.qml`:

- `software` (default): `qml6` off-screen (`QT_QPA_PLATFORM=offscreen`) with the
  software scene graph (`QT_QUICK_BACKEND=software`, `--software`). Needs no
  display or GPU, but the software scene graph does not draw shader effects:
  `RectangularShadow` and `MultiEffect` (box-shadow, focus rings) are missing.
- `gl`: `qml6` on the Wayland platform with the OpenGL RHI
  (`QSG_RHI_BACKEND=opengl`), as a real Qt app on a GPU renders. The
  compositor is a private headless sway (`HeadlessSway`) with its own
  `XDG_RUNTIME_DIR` under a temp dir; the user's own `WAYLAND_DISPLAY` and
  compositor are never used. The offscreen and minimalegl platforms cannot do
  OpenGL here (offscreen falls back to software; minimalegl with
  `EGL_PLATFORM=surfaceless` finds no EGLConfig), hence the compositor.
  `--gl-text distancefield` (default) draws `Text.QtRendering` text with
  distance fields, as a real app on a GPU does; `--gl-text native` sets
  `QML_DISABLE_DISTANCEFIELD=1` so glyphs are rasterised like the software
  backend (and closer to Chromium) while shader effects still draw. Neither
  changes layout: glyph advances come from the same design metrics.

Supports both `ApplicationWindow`/`Window` and `Item` root components without
opening a second window.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from PIL import Image

SCRIPT_DIR = Path(__file__).resolve().parent
SHOT_QML = SCRIPT_DIR / "Shot.qml"
DEFAULT_TIMEOUT_SEC = 10.0
BACKENDS = ("software", "gl")
DEFAULT_BACKEND = "software"
GL_TEXT_MODES = ("distancefield", "native")
DEFAULT_GL_TEXT = "distancefield"
SWAY_START_TIMEOUT_SEC = 10.0

# Under sway's default tiling a window is resized to its tile minus borders and
# title bar (a 1280x800 request became 1276x772), and Shot.qml's grab then
# stretched that back to 1280x800, so everything came out ~4% taller. A
# floating, borderless window keeps the size it asks for.
SWAY_CONFIG_TEMPLATE = """\
output HEADLESS-1 resolution {width}x{height} position 0 0 scale 1
xwayland disable
swaybg_command -
default_border none
default_floating_border none
floating_maximum_size -1 x -1
for_window [app_id=".*"] floating enable, border none
focus_follows_mouse no
# Ends sway if the harness dies without running its cleanup (SIGKILL, OOM
# killer). PR_SET_PDEATHSIG cannot do this: exec clears it because the sway
# binary carries a file capability (cap_sys_nice).
# Quoted: sway itself splits commands on ";".
exec "tail --pid={harness_pid} -f /dev/null; swaymsg exit"
"""


class QmlRenderError(RuntimeError):
    """Raised when qml6 fails to load or screenshot the target QML file."""


class HeadlessSway:
    """A private headless sway compositor for `--backend gl`.

    Use as a context manager; the compositor is stopped and its runtime dir
    removed on exit, including on errors and Ctrl-C. One instance can serve
    any number of `render_qml(..., backend="gl", sway=...)` calls.
    """

    def __init__(self, width: int = 1280, height: int = 800) -> None:
        self.width = width
        self.height = height
        self.runtime_dir: Path | None = None
        self.wayland_display: str | None = None
        self._proc: subprocess.Popen[bytes] | None = None
        self._log_path: Path | None = None

    def start(self) -> HeadlessSway:
        sway_bin = shutil.which("sway")
        if sway_bin is None:
            raise QmlRenderError(
                "--backend gl needs sway (a headless Wayland compositor) on PATH; "
                "install sway or use --backend software"
            )
        # mkdtemp creates the directory mode 0700, which Wayland requires.
        self.runtime_dir = Path(tempfile.mkdtemp(prefix="eldur-visual-sway-"))
        config = self.runtime_dir / "sway.cfg"
        config.write_text(
            SWAY_CONFIG_TEMPLATE.format(
                width=self.width, height=self.height, harness_pid=os.getpid()
            ),
            encoding="utf-8",
        )
        self._log_path = self.runtime_dir / "sway.log"

        env = os.environ.copy()
        for key in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DISPLAY", "SWAYSOCK", "I3SOCK"):
            env.pop(key, None)
        env["XDG_RUNTIME_DIR"] = str(self.runtime_dir)
        env["WLR_BACKENDS"] = "headless"
        env["WLR_LIBINPUT_NO_DEVICES"] = "1"

        with self._log_path.open("wb") as log:
            self._proc = subprocess.Popen(
                [sway_bin, "-c", str(config)],
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=subprocess.STDOUT,
            )

        deadline = time.monotonic() + SWAY_START_TIMEOUT_SEC
        while time.monotonic() < deadline:
            if self._proc.poll() is not None:
                break
            sockets = sorted(
                p.name
                for p in self.runtime_dir.glob("wayland-*")
                if not p.name.endswith(".lock") and p.is_socket()
            )
            if sockets:
                self.wayland_display = sockets[0]
                return self
            time.sleep(0.05)

        log_text = self._read_log()
        self.stop()
        raise QmlRenderError(f"headless sway did not start a Wayland socket:\n{log_text}")

    def _read_log(self) -> str:
        if self._log_path is None or not self._log_path.is_file():
            return "(no sway log)"
        return self._log_path.read_text(encoding="utf-8", errors="replace").strip()[-4000:]

    def stop(self) -> None:
        proc, self._proc = self._proc, None
        if proc is not None and proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        if self.runtime_dir is not None:
            shutil.rmtree(self.runtime_dir, ignore_errors=True)
            self.runtime_dir = None
        self.wayland_display = None

    def env(self) -> dict[str, str]:
        if self._proc is None or self._proc.poll() is not None or self.wayland_display is None:
            raise QmlRenderError(f"headless sway is not running:\n{self._read_log()}")
        return {
            "XDG_RUNTIME_DIR": str(self.runtime_dir),
            "WAYLAND_DISPLAY": self.wayland_display,
        }

    def __enter__(self) -> HeadlessSway:
        return self.start()

    def __exit__(self, *exc: object) -> None:
        self.stop()


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
    strict: bool = False,
    backend: str = DEFAULT_BACKEND,
    sway: HeadlessSway | None = None,
    gl_text: str = DEFAULT_GL_TEXT,
) -> list[str]:
    """Render `qml_target` at `width`x`height` into `out_png`.

    Returns any diagnostic/warning lines printed by `qml6`. When `strict=True`,
    any diagnostic output from `qml6` raises `QmlRenderError`.

    `backend="gl"` renders on `sway` when given (it must be large enough for
    the viewport), otherwise on a headless sway started for this call only.
    """
    if backend not in BACKENDS:
        raise QmlRenderError(f"Unknown backend {backend!r}; expected one of {', '.join(BACKENDS)}")
    if gl_text not in GL_TEXT_MODES:
        raise QmlRenderError(
            f"Unknown gl_text {gl_text!r}; expected one of {', '.join(GL_TEXT_MODES)}"
        )
    if backend == "gl" and sway is None:
        with HeadlessSway(width, height) as own_sway:
            return render_qml(
                qml_target,
                out_png,
                width=width,
                height=height,
                timeout=timeout,
                strict=strict,
                backend=backend,
                sway=own_sway,
                gl_text=gl_text,
            )
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
    if backend == "gl":
        assert sway is not None
        for key in ("WAYLAND_SOCKET", "DISPLAY", "QT_QUICK_BACKEND"):
            env.pop(key, None)
        env.update(sway.env())
        env["QT_QPA_PLATFORM"] = "wayland"
        env["QSG_RHI_BACKEND"] = "opengl"
        # Client-side decorations would take part of the window for a title bar.
        env["QT_WAYLAND_DISABLE_WINDOWDECORATION"] = "1"
        env.pop("QML_DISABLE_DISTANCEFIELD", None)
        if gl_text == "native":
            env["QML_DISABLE_DISTANCEFIELD"] = "1"
    else:
        env["QT_QPA_PLATFORM"] = "offscreen"
        env["QT_QUICK_BACKEND"] = "software"
    env["QT_FONT_DPI"] = "96"
    env["QT_SCALE_FACTOR"] = "1"
    # Prevent stale .qmlc bytecode caches when Main.qml is regenerated between runs.
    env["QML_DISABLE_DISK_CACHE"] = "1"
    # Qt 6 routes QML/qWarning diagnostics to systemd-journald when stderr is a
    # pipe (non-TTY) unless stderr logging is explicitly forced.
    env["QT_FORCE_STDERR_LOGGING"] = "1"
    env["QT_ASSUME_STDERR_HAS_CONSOLE"] = "1"

    cmd = [
        "qml6",
        *(["--software"] if backend == "software" else []),
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
    warnings = [line for line in combined_output.splitlines() if line.strip()]

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

    if strict and warnings:
        raise QmlRenderError(
            f"qml6 emitted warnings/errors for {main_qml}:\n{combined_output}"
        )

    return warnings


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
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Fail with non-zero exit status if qml6 prints any QML warnings or errors",
    )
    parser.add_argument(
        "--backend",
        choices=BACKENDS,
        default=DEFAULT_BACKEND,
        help="software: offscreen software scene graph (default); "
        "gl: OpenGL on a private headless sway (draws shadows and MultiEffect)",
    )
    parser.add_argument(
        "--gl-text",
        choices=GL_TEXT_MODES,
        default=DEFAULT_GL_TEXT,
        help="Text rasterisation under --backend gl: distancefield (Qt's GPU default) "
        "or native (QML_DISABLE_DISTANCEFIELD=1, like the software backend)",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        warnings = render_qml(
            args.qml_dir,
            args.out_png,
            width=args.width,
            height=args.height,
            timeout=args.timeout,
            strict=args.strict,
            backend=args.backend,
            gl_text=args.gl_text,
        )
    except QmlRenderError as exc:
        print(f"render_qml.py: error: {exc}", file=sys.stderr)
        return 1
    for line in warnings:
        print(f"render_qml.py: warning: {line}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
