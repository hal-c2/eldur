#!/usr/bin/env python3
"""Unit and integration tests for the tools/visual parity harness."""

from __future__ import annotations

import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

import numpy as np
from PIL import Image

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import compare  # noqa: E402
import drift  # noqa: E402
import regions  # noqa: E402
import render_qml  # noqa: E402
import run as visual_run  # noqa: E402


class CompareMetricsTests(unittest.TestCase):
    def test_identical_images_score_100(self) -> None:
        arr = np.zeros((64, 96, 3), dtype=np.uint8)
        arr[:, :] = [240, 244, 248]
        arr[12:44, 16:72] = [30, 64, 175]

        metrics, _, _ = compare.compare_arrays(arr, arr.copy())
        self.assertEqual(metrics["pixel_mismatch"], 0.0)
        self.assertEqual(metrics["ssim"], 1.0)
        self.assertEqual(metrics["edge_iou"], 1.0)
        self.assertEqual(metrics["score"], 100.0)

    def test_pixel_mismatch_threshold_is_strictly_greater_than_32(self) -> None:
        base = np.full((32, 32, 3), 100, dtype=np.uint8)
        at_thresh = np.full((32, 32, 3), 132, dtype=np.uint8)
        over_thresh = np.full((32, 32, 3), 133, dtype=np.uint8)

        m_at, _, _ = compare.compare_arrays(base, at_thresh)
        m_over, _, _ = compare.compare_arrays(base, over_thresh)
        self.assertEqual(m_at["pixel_mismatch"], 0.0)
        self.assertEqual(m_over["pixel_mismatch"], 1.0)

    def test_flat_black_vs_white_matches_analytical_ssim(self) -> None:
        black = np.zeros((32, 32, 3), dtype=np.uint8)
        white = np.full((32, 32, 3), 255, dtype=np.uint8)

        metrics, _, _ = compare.compare_arrays(black, white)
        expected_ssim = compare.SSIM_C1 / (255.0**2 + compare.SSIM_C1)
        self.assertAlmostEqual(metrics["ssim"], expected_ssim, places=6)
        self.assertEqual(metrics["edge_iou"], 1.0)
        self.assertEqual(metrics["pixel_mismatch"], 1.0)

    def test_edge_iou_is_colour_independent_and_layout_sensitive(self) -> None:
        a = np.full((80, 120, 3), 250, dtype=np.uint8)
        b_same_box = np.full((80, 120, 3), 250, dtype=np.uint8)
        c_shifted_box = np.full((80, 120, 3), 250, dtype=np.uint8)

        # Same geometry, very different fill colours (navy vs medium slate).
        a[20:60, 15:55] = [15, 23, 42]
        b_same_box[20:60, 15:55] = [120, 130, 145]
        # Disjoint box placement on the right side.
        c_shifted_box[20:60, 70:110] = [15, 23, 42]

        m_same, _, _ = compare.compare_arrays(a, b_same_box)
        m_shifted, _, _ = compare.compare_arrays(a, c_shifted_box)

        self.assertEqual(m_same["edge_iou"], 1.0)
        self.assertGreater(m_same["pixel_mismatch"], 0.1)
        self.assertEqual(m_shifted["edge_iou"], 0.0)

    def test_score_formula_and_diff_png_generation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            img_a = tmp_dir / "a.png"
            img_b = tmp_dir / "b.png"
            diff_png = tmp_dir / "diff.png"

            arr_a = np.full((48, 64, 3), 245, dtype=np.uint8)
            arr_b = arr_a.copy()
            arr_a[10:36, 10:40] = [37, 99, 235]
            arr_b[12:38, 12:42] = [37, 99, 235]

            Image.fromarray(arr_a).save(img_a)
            Image.fromarray(arr_b).save(img_b)

            out_buf = io.StringIO()
            with redirect_stdout(out_buf):
                rc = compare.main([str(img_a), str(img_b), "--diff", str(diff_png)])
            self.assertEqual(rc, 0)

            parsed = json.loads(out_buf.getvalue())
            expected_score = 100.0 * (
                0.5 * parsed["ssim"]
                + 0.3 * parsed["edge_iou"]
                + 0.2 * (1.0 - parsed["pixel_mismatch"])
            )
            self.assertAlmostEqual(parsed["score"], expected_score, places=2)
            self.assertTrue(diff_png.is_file())
            with Image.open(diff_png) as diff_im:
                self.assertEqual(diff_im.size, (3 * 64 + 4, 48 + 26))

    def test_dimension_mismatch_and_tiny_image_fail_loudly(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            p1 = tmp_dir / "1.png"
            p2 = tmp_dir / "2.png"
            p_tiny = tmp_dir / "tiny.png"

            Image.fromarray(np.zeros((32, 32, 3), dtype=np.uint8)).save(p1)
            Image.fromarray(np.zeros((32, 48, 3), dtype=np.uint8)).save(p2)
            Image.fromarray(np.zeros((4, 4, 3), dtype=np.uint8)).save(p_tiny)

            err_buf = io.StringIO()
            with redirect_stderr(err_buf):
                rc = compare.main([str(p1), str(p2)])
            self.assertEqual(rc, 1)
            self.assertIn("dimension mismatch", err_buf.getvalue())

            with self.assertRaises(compare.CompareError):
                compare.compare_images(p_tiny, p_tiny)


class RenderQmlTests(unittest.TestCase):
    def test_renders_both_application_window_and_item_roots(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)

            appwin_dir = tmp_dir / "appwin"
            appwin_dir.mkdir()
            (appwin_dir / "Main.qml").write_text(
                """import QtQuick
import QtQuick.Controls.Basic

ApplicationWindow {
    width: 400
    height: 300
    visible: true
    color: "#1e3a8a"

    Rectangle {
        x: 20
        y: 20
        width: 80
        height: 60
        color: "#ef4444"
    }
}
""",
                encoding="utf-8",
            )

            item_dir = tmp_dir / "item"
            item_dir.mkdir()
            (item_dir / "Main.qml").write_text(
                """import QtQuick

Item {
    Rectangle {
        x: 20
        y: 20
        width: 80
        height: 60
        color: "#ef4444"
    }
}
""",
                encoding="utf-8",
            )

            appwin_png = tmp_dir / "appwin.png"
            item_png = tmp_dir / "item.png"

            w1 = render_qml.render_qml(
                appwin_dir, appwin_png, width=200, height=120, strict=True, backend="software"
            )
            w2 = render_qml.render_qml(
                item_dir, item_png, width=200, height=120, strict=True, backend="software"
            )
            self.assertEqual(w1, [])
            self.assertEqual(w2, [])

            arr_win = compare.load_rgb(appwin_png)
            arr_item = compare.load_rgb(item_png)

            self.assertEqual(arr_win.shape, (120, 200, 3))
            self.assertEqual(arr_item.shape, (120, 200, 3))

            # ApplicationWindow.color (#1e3a8a = 30, 58, 138) must be preserved.
            self.assertEqual(arr_win[5, 5].tolist(), [30, 58, 138])
            self.assertEqual(arr_win[40, 40].tolist(), [239, 68, 68])

            # Transparent Item root must composite over a white (#ffffff) host canvas.
            self.assertEqual(arr_item[5, 5].tolist(), [255, 255, 255])
            self.assertEqual(arr_item[40, 40].tolist(), [239, 68, 68])

    def test_invalid_qml_fails_with_qml6_diagnostics(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text(
                "import QtQuick\nRectangle { broken syntax !!! }\n",
                encoding="utf-8",
            )
            err_buf = io.StringIO()
            with redirect_stderr(err_buf):
                rc = render_qml.main([str(tmp_dir), str(tmp_dir / "out.png"), "--backend", "software"])
            self.assertEqual(rc, 1)
            self.assertIn("Main.qml", err_buf.getvalue())

    def test_render_qml_a_pages_console_log_is_not_a_warning(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text(
                """import QtQuick

Rectangle {
    color: "#2563eb"
    Component.onCompleted: {
        console.log("the page said this")
        console.warn("the page warned")
    }
}
""",
                encoding="utf-8",
            )
            warnings = render_qml.render_qml(
                tmp_dir, tmp_dir / "out.png", width=120, height=80, backend="software"
            )
            self.assertFalse(any("page said" in line for line in warnings), warnings)
            self.assertTrue(any("page warned" in line for line in warnings), warnings)

    def test_render_qml_strict_fails_on_runtime_warnings(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text(
                """import QtQuick

Rectangle {
    color: "#2563eb"
    property int brokenBinding: undefinedVariable + 1
}
""",
                encoding="utf-8",
            )
            out_png = tmp_dir / "out.png"

            warnings = render_qml.render_qml(
                tmp_dir, out_png, width=120, height=80, strict=False, backend="software"
            )
            self.assertTrue(any("ReferenceError" in line for line in warnings), warnings)

            err_buf = io.StringIO()
            with redirect_stderr(err_buf):
                rc = render_qml.main(
                    [str(tmp_dir), str(out_png), "--width", "120", "--height", "80", "--strict",
                     "--backend", "software"]
                )
            self.assertEqual(rc, 1)
            self.assertIn("ReferenceError", err_buf.getvalue())


SHADOW_QML = """import QtQuick
import QtQuick.Effects

Item {
    RectangularShadow {
        x: 40
        y: 30
        width: 80
        height: 40
        blur: 12
        color: "#000000"
    }
    Rectangle {
        x: 40
        y: 30
        width: 80
        height: 40
        color: "#ffffff"
    }
}
"""


# The final appearance arrives only after five changes 50 ms apart, as text
# wrapping around a float settles through callLater/forceLayout chains. Each
# change is visible, so a grab before the chain ends shows a wrong colour or a
# wrong wrap width.
DELAYED_QML = """import QtQuick

Item {
    id: page
    property int step: 0
    readonly property var colours: ["#ff0000", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#00ff00"]
    Rectangle {
        id: box
        x: 0; y: 0; width: 160; height: 60
        color: page.colours[page.step]
    }
    Rectangle {
        x: 0; y: 60
        width: 40 + page.step * 24; height: 40
        color: "#000000"
    }
    Timer {
        interval: 50
        repeat: true
        running: page.step < 5
        onTriggered: page.step += 1
    }
}
"""

# Never quiet: Shot.qml must still grab, but warn that the page did not settle.
RESTLESS_QML = """import QtQuick

Rectangle {
    width: 40; height: 40
    color: "#ff0000"
    RotationAnimation on rotation { from: 0; to: 360; duration: 1000; loops: Animation.Infinite }
}
"""


def _assert_delayed_final_state(test: unittest.TestCase, png: Path) -> None:
    arr = compare.load_rgb(png)
    test.assertEqual(arr[30, 80].tolist(), [0, 255, 0], "box colour is not the final step's")
    # The bar ends at x = 40 + 5 * 24 = 160.
    test.assertEqual(arr[80, 155].tolist(), [0, 0, 0], "bar is not at its final width")
    test.assertEqual(arr[80, 165].tolist(), [255, 255, 255], "bar is wider than its final width")


class PaintRegionTests(unittest.TestCase):
    """Each paint feature's own pixels against Chromium's (`regions.py`): a layout
    test cannot see paint, nor a page's score one wrong box."""

    CASES = ["45-inline-boxes", "46-backgrounds", "47-clip", "48-shadows-filters", "49-borders", "51-runtime-backgrounds", "52-runtime-effects", "53-runtime-borders"]

    def test_regions_hold(self) -> None:
        corpus = Path(visual_run.__file__).resolve().parents[2] / "tests" / "corpus"
        for name in self.CASES:
            with self.subTest(case=name), tempfile.TemporaryDirectory() as tmp:
                html, qml, warnings = regions.render_case(corpus / name, Path(tmp))
                self.assertEqual(warnings, [])
                bad = [r for r in regions.check_case(corpus / name, html, qml) if not r["ok"]]
                self.assertEqual(bad, [])


class PaintSnapTests(unittest.TestCase):
    """A conversion of paint/page.html paints as Chromium did (layout.md §1.2).

    The expected colours and inked rows are Chromium's, recorded by
    paint/gen.mjs. Layout stays fractional; only painting snaps.
    """

    FUZZ = 2
    maxDiff = None
    # The probe at exactly half a pixel. The one at 0.51 is 0.5 to Chromium,
    # in 1/64 px units, and 0.51 to Qt; both come out on the same column.
    HALF_X = 510.5
    tmp: tempfile.TemporaryDirectory[str]
    measured: dict
    pixels: dict[str, np.ndarray]

    @classmethod
    def setUpClass(cls) -> None:
        here = Path(visual_run.__file__).resolve().parent / "paint"
        cls.measured = json.loads((here / "chromium.json").read_text())
        cls.tmp = tempfile.TemporaryDirectory()
        qml = Path(cls.tmp.name) / "qml"
        ok, err = visual_run.run_converter(
            "cargo run -q -p eldur_cli -- convert", here / "page.html", qml, timeout=600.0
        )
        if not ok:
            raise AssertionError(err)
        png = Path(cls.tmp.name) / "gl.png"
        warnings = render_qml.render_qml(qml, png, backend="gl")
        if warnings:
            raise AssertionError(warnings)
        cls.pixels = {"gl": np.asarray(Image.open(png).convert("RGB")).astype(int)}

    @classmethod
    def tearDownClass(cls) -> None:
        cls.tmp.cleanup()

    def _differences(self, expected: list[str], got: np.ndarray, start: int) -> list[str]:
        out = []
        for i, colour in enumerate(expected):
            want = [int(c) for c in colour.split(",")]
            if abs(got[i] - want).max() > self.FUZZ:
                out.append(f"{start + i}: chromium {colour}, qt {got[i].tolist()}")
        return out

    def test_box_edges_land_on_chromiums_pixels(self) -> None:
        image = self.pixels["gl"]
        wrong = {}
        for box in self.measured["boxes"]:
            x, y0, rows = box["column"], box["from"], box["rows"]
            diff = self._differences(rows, image[y0 : y0 + len(rows), x], y0)
            if "cols" in box:
                y, x0, cols = box["row"], box["fromX"], box["cols"]
                if box["x"] != self.HALF_X:
                    diff += self._differences(cols, image[y, x0 : x0 + len(cols)], x0)
            if diff:
                wrong[f"{box['x']},{box['y']} {box['width']}x{box['height']}"] = diff[:4]
        self.assertEqual(wrong, {})

    def test_a_box_at_exactly_half_a_pixel_in_x_is_one_column_left_of_chromium(self) -> None:
        # Known difference (layout.md §1.2): the GPU's fill rule takes the
        # column whose centre is on the edge, Chromium rounds half up. This
        # fails when that changes; move the case into the test above then.
        image = self.pixels["gl"]
        box = next(b for b in self.measured["boxes"] if b["x"] == self.HALF_X)
        y, x0, cols = box["row"], box["fromX"], box["cols"]
        left = int(np.floor(box["x"]))
        self.assertEqual(cols[left - x0], "255,255,255")
        self.assertEqual(image[y, left].tolist(), [0, 0, 0])
        self.assertEqual(cols[left + 1 - x0], "0,0,0")

    # Lines that do not land (layout.md §1.2, known limits). This fails when
    # one starts to; take it out then.
    KNOWN_ROWS: dict[str, tuple[list[int], list[int]]] = {
        # A monospace run of the body's size: a row above what its metrics give.
        "r3 240": ([245, 256], [244, 255]),
        "r3 240.25": ([245, 256], [244, 255]),
        "r3 240.5": ([246, 257], [245, 256]),
        "r3 240.75": ([246, 257], [245, 256]),
        # The shift is the tallest run's, so a line without the run is low.
        "r9 540": ([546, 583], [547, 583]),
        "r9 540.25": ([546, 583], [547, 583]),
        "r9 540.5": ([547, 583], [548, 584]),
        "r9 540.75": ([547, 583], [548, 584]),
        # Half a pixel from a fractional height: Qt's sum is just under it.
        "c9 695.5": ([712, 721], [711, 720]),
    }

    def test_glyphs_land_on_chromiums_rows(self) -> None:
        image = self.pixels["gl"]
        wrong = {}
        for line in self.measured["lines"]:
            y0 = int(np.floor(line["y"])) - 4
            y1 = int(np.ceil(line["y"] + line["height"])) + 4
            x0 = int(np.floor(line["x"]))
            ink = np.nonzero((image[y0:y1, x0 : x0 + 50, 1] < 128).any(axis=1))[0]
            got = [y0 + int(ink[0]), y0 + int(ink[-1])] if len(ink) else None
            if got != line["ink"]:
                wrong[f"{line['font']} {line['y']}"] = (line["ink"], got)
        self.assertEqual(wrong, self.KNOWN_ROWS)


class RenderQmlSettleTests(unittest.TestCase):
    def test_software_capture_waits_for_delayed_changes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text(DELAYED_QML, encoding="utf-8")
            out_png = tmp_dir / "out.png"
            warnings = render_qml.render_qml(
                tmp_dir, out_png, width=200, height=120, strict=True, backend="software"
            )
            self.assertEqual(warnings, [])
            _assert_delayed_final_state(self, out_png)

    @unittest.skipUnless(shutil.which("sway"), "sway is not installed")
    def test_gl_capture_waits_for_delayed_changes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text(DELAYED_QML, encoding="utf-8")
            out_png = tmp_dir / "out.png"
            warnings = render_qml.render_qml(
                tmp_dir, out_png, width=200, height=120, strict=True, backend="gl"
            )
            self.assertEqual(warnings, [])
            _assert_delayed_final_state(self, out_png)

    def test_page_that_never_settles_is_grabbed_with_a_warning(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text(RESTLESS_QML, encoding="utf-8")
            out_png = tmp_dir / "out.png"
            with mock.patch.object(render_qml, "SETTLE_MAX_MS", 600):
                warnings = render_qml.render_qml(
                    tmp_dir, out_png, width=120, height=80, backend="software"
                )
            self.assertTrue(out_png.is_file())
            self.assertTrue(any("did not settle" in line for line in warnings), warnings)

    def test_scene_graph_other_than_the_requested_backend_fails(self) -> None:
        # The software scene graph, told it should be OpenGL, as when Qt cannot
        # initialise GL and quietly falls back.
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text("import QtQuick\nItem {}\n", encoding="utf-8")
            out_png = tmp_dir / "out.png"
            env = dict(
                os.environ,
                QT_QPA_PLATFORM="offscreen",
                QT_QUICK_BACKEND="software",
                QT_FORCE_STDERR_LOGGING="1",
            )
            proc = subprocess.run(
                ["qml6", "--software", str(render_qml.SHOT_QML), "--",
                 (tmp_dir / "Main.qml").as_uri(), "60", "40", str(out_png), "50", "2000", "gl"],
                env=env, capture_output=True, text=True, timeout=10, check=False,
            )
            self.assertEqual(proc.returncode, 3, proc.stderr)
            self.assertIn("scene graph API", proc.stderr)
            self.assertFalse(out_png.exists())


class RenderQmlBackendTests(unittest.TestCase):
    def test_gl_backend_without_sway_fails_clearly(self) -> None:
        real_which = render_qml.shutil.which

        def which(name: str) -> str | None:
            return None if name == "sway" else real_which(name)

        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text("import QtQuick\nItem {}\n", encoding="utf-8")
            with mock.patch.object(render_qml.shutil, "which", side_effect=which):
                with self.assertRaisesRegex(render_qml.QmlRenderError, "needs sway"):
                    render_qml.render_qml(tmp_dir, tmp_dir / "out.png", backend="gl")

    def test_unknown_backend_is_rejected(self) -> None:
        with self.assertRaisesRegex(render_qml.QmlRenderError, "Unknown backend"):
            render_qml.render_qml(Path("."), Path("out.png"), backend="vulkan")

    def test_default_backend_is_gl(self) -> None:
        for parser in (render_qml.build_parser(), visual_run.build_parser()):
            args = parser.parse_args(["x", "y"] if parser.prog == "render_qml.py" else [])
            self.assertEqual(args.backend, "gl")
            self.assertFalse(hasattr(args, "gl_text"))
        self.assertEqual(render_qml.DEFAULT_BACKEND, "gl")

    def test_gl_draws_the_glyphs_the_output_asks_for(self) -> None:
        # An inherited switch must not score something users never see.
        qml = """import QtQuick

Item {
    Text { x: 10; y: 10; text: "Glyph placement"; font.pixelSize: 16; color: "#000000"%s }
}
"""
        with tempfile.TemporaryDirectory() as tmp, mock.patch.dict(
            os.environ, {"QML_DISABLE_DISTANCEFIELD": "1"}
        ):
            images = {}
            for name, extra in (("default", ""), ("native", "; renderType: Text.NativeRendering")):
                tmp_dir = Path(tmp) / name
                tmp_dir.mkdir()
                (tmp_dir / "Main.qml").write_text(qml % extra, encoding="utf-8")
                png = tmp_dir / "out.png"
                render_qml.render_qml(tmp_dir, png, width=200, height=40, backend="gl")
                images[name] = compare.load_rgb(png)
            self.assertFalse(np.array_equal(images["default"], images["native"]))

    def test_default_without_sway_says_to_pass_software_instead_of_falling_back(self) -> None:
        real_which = render_qml.shutil.which

        def which(name: str) -> str | None:
            return None if name == "sway" else real_which(name)

        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text("import QtQuick\nItem {}\n", encoding="utf-8")
            with mock.patch.object(render_qml.shutil, "which", side_effect=which):
                err = io.StringIO()
                with redirect_stderr(err):
                    rc = render_qml.main([str(tmp_dir), str(tmp_dir / "out.png")])
                self.assertEqual(rc, 1)
                self.assertIn("pass --backend software", err.getvalue())
                self.assertFalse((tmp_dir / "out.png").exists())

                err = io.StringIO()
                with redirect_stdout(io.StringIO()), redirect_stderr(err):
                    rc = visual_run.main(["--selftest", "--out", str(tmp_dir / "vis")])
                self.assertEqual(rc, 1)
                self.assertIn("pass --backend software", err.getvalue())
                self.assertFalse((tmp_dir / "vis" / "report.json").exists())

    def test_gl_unavailable_stops_the_run(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            calls: list[str] = []

            def fake_render_qml(*_args: object, **_kwargs: object) -> list[str]:
                calls.append("qml")
                raise render_qml.GlUnavailableError("no GL; pass --backend software")

            with (
                mock.patch.object(render_qml, "HeadlessSway"),
                mock.patch.object(visual_run, "render_html"),
                mock.patch.object(render_qml, "render_qml", side_effect=fake_render_qml),
            ):
                with self.assertRaisesRegex(visual_run.HarnessError, "pass --backend software"):
                    visual_run.evaluate_cases(
                        [("a", tmp_dir), ("b", tmp_dir)],
                        out_dir=tmp_dir / "out",
                        width=100,
                        height=100,
                        selftest=True,
                        convert_cmd="",
                        baseline_scores=None,
                        backend="gl",
                    )
            self.assertEqual(calls, ["qml"])

    def test_runner_stops_sway_when_the_run_is_interrupted(self) -> None:
        started: list[mock.MagicMock] = []

        def fake_sway(*_args: object) -> mock.MagicMock:
            inst = mock.MagicMock()
            started.append(inst)
            return inst

        with (
            mock.patch.object(render_qml, "HeadlessSway", side_effect=fake_sway),
            mock.patch.object(visual_run, "_evaluate_cases", side_effect=KeyboardInterrupt),
        ):
            with self.assertRaises(KeyboardInterrupt):
                visual_run.evaluate_cases(
                    [],
                    out_dir=Path("unused"),
                    width=1280,
                    height=800,
                    selftest=True,
                    convert_cmd="",
                    baseline_scores=None,
                    backend="gl",
                )
        self.assertEqual(len(started), 1)
        started[0].start.assert_called_once()
        started[0].stop.assert_called_once()


@unittest.skipUnless(shutil.which("sway"), "sway is not installed")
class RenderQmlGlTests(unittest.TestCase):
    def test_gl_renders_exact_size_and_stops_sway(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text(
                """import QtQuick

Item {
    Rectangle { x: 0; y: 0; width: 200; height: 10; color: "#ef4444" }
    Rectangle { x: 190; y: 110; width: 10; height: 10; color: "#2563eb" }
}
""",
                encoding="utf-8",
            )
            out_png = tmp_dir / "out.png"
            with render_qml.HeadlessSway(200, 120) as sway:
                runtime_dir = sway.runtime_dir
                proc = sway._proc
                warnings = render_qml.render_qml(
                    tmp_dir, out_png, width=200, height=120, strict=True, backend="gl", sway=sway
                )
            self.assertEqual(warnings, [])
            assert runtime_dir is not None and proc is not None
            self.assertFalse(runtime_dir.exists())
            self.assertIsNotNone(proc.poll())

            # A window resized by the compositor would be stretched by the grab,
            # moving these edge rectangles off their pixels.
            arr = compare.load_rgb(out_png)
            self.assertEqual(arr.shape, (120, 200, 3))
            self.assertEqual(arr[0, 0].tolist(), [239, 68, 68])
            self.assertEqual(arr[9, 199].tolist(), [239, 68, 68])
            self.assertEqual(arr[10, 100].tolist(), [255, 255, 255])
            self.assertEqual(arr[110, 190].tolist(), [37, 99, 235])
            self.assertEqual(arr[109, 189].tolist(), [255, 255, 255])

    def test_gl_draws_rectangular_shadow_that_software_omits(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text(SHADOW_QML, encoding="utf-8")
            sw_png = tmp_dir / "sw.png"
            gl_png = tmp_dir / "gl.png"
            render_qml.render_qml(tmp_dir, sw_png, width=160, height=100, backend="software")
            render_qml.render_qml(tmp_dir, gl_png, width=160, height=100, backend="gl")
            sw = compare.load_rgb(sw_png)
            gl = compare.load_rgb(gl_png)
            # Just outside the white box's bottom edge, inside the blur radius.
            self.assertEqual(sw[72, 80].tolist(), [255, 255, 255])
            self.assertLess(int(gl[72, 80].max()), 250)

    def test_gl_native_text_matches_software_glyph_placement(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            (tmp_dir / "Main.qml").write_text(
                """import QtQuick

Item {
    Text { x: 10; y: 10; text: "Glyph placement"; font.pixelSize: 16; color: "#000000" }
}
""",
                encoding="utf-8",
            )
            pngs = {}
            for name, backend in (("sw", "software"), ("gl", "gl")):
                pngs[name] = tmp_dir / f"{name}.png"
                render_qml.render_qml(tmp_dir, pngs[name], width=200, height=40, backend=backend)

            def ink_box(path: Path) -> tuple[int, int, int, int]:
                ys, xs = np.nonzero(compare.load_rgb(path).min(axis=2) < 128)
                return int(xs.min()), int(ys.min()), int(xs.max()), int(ys.max())

            # Same layout on both.
            sw_box = ink_box(pngs["sw"])
            box = ink_box(pngs["gl"])
            for got, want in zip(box, sw_box):
                self.assertLessEqual(abs(got - want), 1, (box, sw_box))


class RenderHtmlAndRunnerTests(unittest.TestCase):
    def test_render_html_disables_animations_and_matches_viewport(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            html_file = tmp_dir / "page.html"
            out_png = tmp_dir / "out.png"
            html_file.write_text(
                """<!doctype html>
<html>
<head>
<style>
  @keyframes flash { from { background: #ef4444; } to { background: #ef4444; } }
  html, body {
    margin: 0;
    background: #10b981;
    animation: flash 0.01s infinite;
  }
</style>
</head>
<body></body>
</html>
""",
                encoding="utf-8",
            )

            visual_run.render_html(html_file, out_png, width=220, height=140)
            arr = compare.load_rgb(out_png)
            self.assertEqual(arr.shape, (140, 220, 3))
            # With animation: none!important, background stays #10b981 (16, 185, 129).
            self.assertEqual(arr[10, 10].tolist(), [16, 185, 129])

            bad_proc = subprocess.run(
                [
                    "node",
                    str(SCRIPT_DIR / "render_html.mjs"),
                    str(tmp_dir / "nonexistent.html"),
                    str(out_png),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(bad_proc.returncode, 0)
            self.assertIn("does not exist", bad_proc.stderr)

    def test_corpus_runner_convert_failed_and_baseline_regression(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            corpus = tmp_dir / "corpus"
            out_dir = tmp_dir / "out"
            baseline = tmp_dir / "baseline.json"
            converter = tmp_dir / "fake_convert.py"

            for case_name in ("good_case", "broken_convert"):
                cdir = corpus / case_name
                cdir.mkdir(parents=True)
                (cdir / "index.html").write_text(
                    "<!doctype html><html><body style='margin:0;background:#2563eb'></body></html>",
                    encoding="utf-8",
                )

            converter.write_text(
                """import sys
from pathlib import Path

html_in = Path(sys.argv[1])
out_dir = Path(sys.argv[3])
if "broken_convert" in str(html_in):
    sys.stderr.write("unsupported feature in broken_convert\\n")
    sys.exit(1)
out_dir.mkdir(parents=True, exist_ok=True)
color = Path(sys.argv[4]).read_text().strip() if len(sys.argv) > 4 else "#2563eb"
(out_dir / "Main.qml").write_text(
    f'import QtQuick\\nRectangle {{ color: "{color}" }}\\n',
    encoding="utf-8",
)
""",
                encoding="utf-8",
            )

            color_cfg = tmp_dir / "color.txt"
            color_cfg.write_text("#2563eb\n", encoding="utf-8")
            convert_cmd = (
                f"{sys.executable} {converter} {{input}} -o {{output}} {color_cfg}"
            )

            with redirect_stdout(io.StringIO()):
                rc1 = visual_run.main(
                    [
                        "--backend",
                        "software",
                        "--corpus",
                        str(corpus),
                        "--out",
                        str(out_dir),
                        "--convert-cmd",
                        convert_cmd,
                        "--baseline",
                        str(baseline),
                        "--update-baseline",
                        "--width",
                        "160",
                        "--height",
                        "120",
                    ]
                )
            self.assertEqual(rc1, 0)
            self.assertTrue(baseline.is_file())
            self.assertTrue((out_dir / "report.md").is_file())
            report1 = json.loads((out_dir / "report.json").read_text(encoding="utf-8"))
            statuses = {c["case"]: c["status"] for c in report1["cases"]}
            self.assertEqual(statuses, {"broken_convert": "convert_failed", "good_case": "ok"})
            self.assertIn(
                "[side-by-side](cases/good_case/diff.png)",
                (out_dir / "report.md").read_text(encoding="utf-8"),
            )

            # Degrade good_case colour to trigger > 2 point regression against baseline.
            color_cfg.write_text("#ef4444\n", encoding="utf-8")
            err_buf = io.StringIO()
            with redirect_stdout(io.StringIO()), redirect_stderr(err_buf):
                rc2 = visual_run.main(
                    [
                        "--backend",
                        "software",
                        "--corpus",
                        str(corpus),
                        "--out",
                        str(out_dir),
                        "--convert-cmd",
                        convert_cmd,
                        "--baseline",
                        str(baseline),
                        "--cases",
                        "good_case",
                        "--width",
                        "160",
                        "--height",
                        "120",
                    ]
                )
            self.assertEqual(rc2, 1)
            self.assertIn("good_case", err_buf.getvalue())

    def test_compare_error_records_compare_failed_and_writes_reports(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            corpus = tmp_dir / "corpus"
            out_dir = tmp_dir / "out"
            converter = tmp_dir / "conv.py"

            for case_name in ("case_bad_cmp", "case_ok"):
                cdir = corpus / case_name
                cdir.mkdir(parents=True)
                (cdir / "index.html").write_text(
                    "<!doctype html><html><body style='margin:0;background:#2563eb'></body></html>",
                    encoding="utf-8",
                )

            converter.write_text(
                """import sys
from pathlib import Path

out_dir = Path(sys.argv[3])
out_dir.mkdir(parents=True, exist_ok=True)
(out_dir / "Main.qml").write_text('import QtQuick\\nRectangle { color: "#2563eb" }\\n', encoding="utf-8")
""",
                encoding="utf-8",
            )
            convert_cmd = f"{sys.executable} {converter}"

            real_compare = compare.compare_images

            def flaky_compare(path_a: Path, path_b: Path, *, diff_path: Path | None = None):
                if "case_bad_cmp" in str(path_a):
                    raise compare.CompareError("corrupt PNG stream in case_bad_cmp")
                return real_compare(path_a, path_b, diff_path=diff_path)

            with mock.patch.object(compare, "compare_images", side_effect=flaky_compare):
                with redirect_stdout(io.StringIO()):
                    rc = visual_run.main(
                        [
                            "--backend",
                            "software",
                            "--corpus",
                            str(corpus),
                            "--out",
                            str(out_dir),
                            "--convert-cmd",
                            convert_cmd,
                            "--width",
                            "120",
                            "--height",
                            "80",
                        ]
                    )

            self.assertEqual(rc, 0)
            self.assertTrue((out_dir / "report.md").is_file())
            report = json.loads((out_dir / "report.json").read_text(encoding="utf-8"))
            self.assertEqual(report["summary"]["compare_failed"], 1)
            by_case = {c["case"]: c for c in report["cases"]}
            self.assertEqual(by_case["case_bad_cmp"]["status"], "compare_failed")
            self.assertIn("corrupt PNG", by_case["case_bad_cmp"]["error"])
            self.assertEqual(by_case["case_ok"]["status"], "ok")
            self.assertEqual(by_case["case_ok"]["score"], 100.0)

    def test_update_baseline_merges_subset_and_refuses_selftest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            corpus = tmp_dir / "corpus"
            out_dir = tmp_dir / "out"
            baseline = tmp_dir / "baseline.json"
            state_file = tmp_dir / "state.json"
            converter = tmp_dir / "conv.py"

            for case_name in ("alpha", "beta"):
                cdir = corpus / case_name
                cdir.mkdir(parents=True)
                (cdir / "index.html").write_text(
                    "<!doctype html><html><body style='margin:0;background:#2563eb'></body></html>",
                    encoding="utf-8",
                )

            state_file.write_text(
                json.dumps({"alpha": "#2563eb", "beta": "#2563eb"}),
                encoding="utf-8",
            )
            converter.write_text(
                """import json
import sys
from pathlib import Path

html_in = Path(sys.argv[1])
out_dir = Path(sys.argv[3])
state = json.loads(Path(sys.argv[4]).read_text(encoding="utf-8"))
case = html_in.parent.name
val = state[case]
if val == "FAIL":
    sys.exit(1)
out_dir.mkdir(parents=True, exist_ok=True)
(out_dir / "Main.qml").write_text(f'import QtQuick\\nRectangle {{ color: "{val}" }}\\n', encoding="utf-8")
""",
                encoding="utf-8",
            )
            convert_cmd = f"{sys.executable} {converter} {{input}} -o {{output}} {state_file}"

            # 1) Refuse --selftest --update-baseline without touching baseline.json.
            err_buf = io.StringIO()
            with redirect_stderr(err_buf):
                rc_refused = visual_run.main(
                    ["--selftest", "--update-baseline", "--baseline", str(baseline)]
                )
            self.assertEqual(rc_refused, 1)
            self.assertIn("--update-baseline", err_buf.getvalue())
            self.assertFalse(baseline.exists())

            # 2) Seed baseline with both alpha and beta.
            with redirect_stdout(io.StringIO()):
                rc_seed = visual_run.main(
                    [
                        "--backend",
                        "software",
                        "--corpus",
                        str(corpus),
                        "--out",
                        str(out_dir),
                        "--convert-cmd",
                        convert_cmd,
                        "--baseline",
                        str(baseline),
                        "--update-baseline",
                        "--width",
                        "120",
                        "--height",
                        "80",
                    ]
                )
            self.assertEqual(rc_seed, 0)
            b1 = json.loads(baseline.read_text(encoding="utf-8"))
            self.assertEqual(sorted(b1["cases"].keys()), ["alpha", "beta"])
            self.assertEqual(b1["cases"]["alpha"]["score"], 100.0)
            self.assertEqual(b1["cases"]["beta"]["score"], 100.0)

            # 3) Partial --cases alpha --update-baseline updates alpha and preserves beta.
            state_file.write_text(
                json.dumps({"alpha": "#1e40af", "beta": "#2563eb"}),
                encoding="utf-8",
            )
            with redirect_stdout(io.StringIO()):
                rc_partial = visual_run.main(
                    [
                        "--backend",
                        "software",
                        "--corpus",
                        str(corpus),
                        "--out",
                        str(out_dir),
                        "--convert-cmd",
                        convert_cmd,
                        "--baseline",
                        str(baseline),
                        "--cases",
                        "alpha",
                        "--update-baseline",
                        "--width",
                        "120",
                        "--height",
                        "80",
                    ]
                )
            self.assertEqual(rc_partial, 0)
            b2 = json.loads(baseline.read_text(encoding="utf-8"))
            self.assertEqual(sorted(b2["cases"].keys()), ["alpha", "beta"])
            self.assertLess(b2["cases"]["alpha"]["score"], 100.0)
            self.assertEqual(b2["cases"]["beta"]["score"], 100.0)

            # 4) Failed beta during --update-baseline must not drop beta from baseline.json.
            state_file.write_text(
                json.dumps({"alpha": "#2563eb", "beta": "FAIL"}),
                encoding="utf-8",
            )
            with redirect_stdout(io.StringIO()):
                rc_fail_keep = visual_run.main(
                    [
                        "--backend",
                        "software",
                        "--corpus",
                        str(corpus),
                        "--out",
                        str(out_dir),
                        "--convert-cmd",
                        convert_cmd,
                        "--baseline",
                        str(baseline),
                        "--update-baseline",
                        "--width",
                        "120",
                        "--height",
                        "80",
                    ]
                )
            self.assertEqual(rc_fail_keep, 0)
            b3 = json.loads(baseline.read_text(encoding="utf-8"))
            self.assertEqual(sorted(b3["cases"].keys()), ["alpha", "beta"])
            self.assertEqual(b3["cases"]["alpha"]["score"], 100.0)
            self.assertEqual(b3["cases"]["beta"]["score"], 100.0)

    def test_baseline_viewport_mismatch_fails_loudly(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            corpus = tmp_dir / "corpus"
            (corpus / "only").mkdir(parents=True)
            (corpus / "only" / "index.html").write_text(
                "<!doctype html><html><body></body></html>",
                encoding="utf-8",
            )
            baseline = tmp_dir / "baseline.json"
            baseline.write_text(
                json.dumps(
                    {
                        "viewport": {"width": 160, "height": 120},
                        "cases": {"only": {"score": 95.0}},
                    }
                ),
                encoding="utf-8",
            )

            err_buf = io.StringIO()
            with redirect_stderr(err_buf):
                rc = visual_run.main(
                    [
                        "--backend",
                        "software",
                        "--corpus",
                        str(corpus),
                        "--out",
                        str(tmp_dir / "out"),
                        "--baseline",
                        str(baseline),
                        "--width",
                        "320",
                        "--height",
                        "240",
                    ]
                )
            self.assertEqual(rc, 1)
            self.assertIn("viewport mismatch", err_buf.getvalue())
            self.assertIn("160x120", err_buf.getvalue())
            self.assertIn("320x240", err_buf.getvalue())

    def test_runner_strict_default_marks_qml_warnings(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            corpus = tmp_dir / "corpus"
            out_dir = tmp_dir / "out"
            baseline = tmp_dir / "baseline.json"
            converter = tmp_dir / "conv.py"

            (corpus / "warn_case").mkdir(parents=True)
            (corpus / "warn_case" / "index.html").write_text(
                "<!doctype html><html><body style='margin:0;background:#2563eb'></body></html>",
                encoding="utf-8",
            )

            converter.write_text(
                """import sys
from pathlib import Path

out_dir = Path(sys.argv[3])
out_dir.mkdir(parents=True, exist_ok=True)
(out_dir / "Main.qml").write_text(
    'import QtQuick\\nRectangle { color: "#2563eb"; property int bad: missingSym + 1 }\\n',
    encoding="utf-8",
)
""",
                encoding="utf-8",
            )
            convert_cmd = f"{sys.executable} {converter}"

            # Default (--strict): status must be 'qml_warnings' and warning surfaced in report.
            with redirect_stdout(io.StringIO()):
                rc_strict = visual_run.main(
                    [
                        "--backend",
                        "software",
                        "--corpus",
                        str(corpus),
                        "--out",
                        str(out_dir),
                        "--convert-cmd",
                        convert_cmd,
                        "--width",
                        "120",
                        "--height",
                        "80",
                    ]
                )
            self.assertEqual(rc_strict, 0)
            report_strict = json.loads((out_dir / "report.json").read_text(encoding="utf-8"))
            self.assertEqual(report_strict["summary"]["qml_warnings"], 1)
            self.assertEqual(report_strict["summary"]["ok"], 0)
            case_rec = report_strict["cases"][0]
            self.assertEqual(case_rec["status"], "qml_warnings")
            self.assertIn("ReferenceError", case_rec["error"])
            self.assertIn("ReferenceError", (out_dir / "report.md").read_text(encoding="utf-8"))

            # Against a clean baseline entry, introducing QML warnings flags a regression (rc == 1).
            baseline.write_text(
                json.dumps(
                    {
                        "viewport": {"width": 120, "height": 80},
                        "cases": {"warn_case": {"score": 100.0}},
                    }
                ),
                encoding="utf-8",
            )
            err_buf = io.StringIO()
            with redirect_stdout(io.StringIO()), redirect_stderr(err_buf):
                rc_reg = visual_run.main(
                    [
                        "--backend",
                        "software",
                        "--corpus",
                        str(corpus),
                        "--out",
                        str(out_dir),
                        "--convert-cmd",
                        convert_cmd,
                        "--baseline",
                        str(baseline),
                        "--width",
                        "120",
                        "--height",
                        "80",
                    ]
                )
            self.assertEqual(rc_reg, 1)
            self.assertIn("warn_case", err_buf.getvalue())

            # --no-strict keeps status 'ok' while still recording warnings.
            with redirect_stdout(io.StringIO()):
                rc_nostrict = visual_run.main(
                    [
                        "--backend",
                        "software",
                        "--corpus",
                        str(corpus),
                        "--out",
                        str(out_dir),
                        "--convert-cmd",
                        convert_cmd,
                        "--no-strict",
                        "--width",
                        "120",
                        "--height",
                        "80",
                    ]
                )
            self.assertEqual(rc_nostrict, 0)
            report_nostrict = json.loads((out_dir / "report.json").read_text(encoding="utf-8"))
            self.assertEqual(report_nostrict["cases"][0]["status"], "ok")
            self.assertTrue(
                any("ReferenceError" in w for w in report_nostrict["cases"][0]["warnings"])
            )


class DriftTests(unittest.TestCase):
    def test_tags_the_object_under_each_source_comment(self) -> None:
        qml = """Item {
    // <div class="a">
    Rectangle {
        // <p>
        Text {
        }
    }
    // not an element
    Item {
    }
}
"""
        tagged, keys = drift.tag_qml(qml)
        self.assertEqual(keys, ['<div class="a">', "<p>"])
        self.assertIn('    Rectangle {\n        property string eldurGeo: "0"', tagged)
        self.assertIn('        Text {\n            property string eldurGeo: "1"', tagged)
        self.assertEqual(tagged.count("eldurGeo"), 2)

    def test_matches_the_kth_element_of_each_description(self) -> None:
        box = lambda d, y: {"describe": d, "x": 0, "y": y, "width": 10, "height": 5}
        rows, unmatched_chromium, unmatched_qml = drift.match(
            [box("<p>", 0), box("<div>", 5), box("<p>", 10), box("<p>", 20)],
            [box("<div>", 6), box("<p>", 0), box("<p>", 12)],
        )
        self.assertEqual([(r["describe"], r["dy"]) for r in rows], [("<p>", 0), ("<div>", 1), ("<p>", 2)])
        self.assertEqual((unmatched_chromium, unmatched_qml), (1, 0))

    def test_measures_layout_rounding_against_chromium(self) -> None:
        if shutil.which("qml6") is None or shutil.which("node") is None:
            self.skipTest("needs qml6 and node")
        with tempfile.TemporaryDirectory() as tmp:
            tmp_dir = Path(tmp)
            page = tmp_dir / "index.html"
            page.write_text(
                "<!DOCTYPE html><body style='margin:0'>"
                "<div class=a style='height:10.5px'></div>"
                "<p style='margin:0;height:20px'></p></body>",
                encoding="utf-8",
            )
            qml_dir = tmp_dir / "qml"
            qml_dir.mkdir()
            (qml_dir / "Main.qml").write_text(
                """import QtQuick
import QtQuick.Layouts

Item {
    width: 200
    height: 100

    ColumnLayout {
        spacing: 0
        width: parent.width

        // <div class="a">
        Item {
            implicitHeight: 10.5
            Layout.fillWidth: true
        }

        // <p>
        Item {
            implicitHeight: 20
            Layout.fillWidth: true
        }
    }
}
""",
                encoding="utf-8",
            )
            result = drift.measure(page, qml_dir, backend="software", width=200, height=100)
        rows = {r["describe"]: r for r in result["rows"]}
        # Qt Layouts ceil an item's implicit height and its cell: 10.5 -> 11.
        self.assertEqual(rows['<div class="a">']["qml"]["height"], 11)
        self.assertEqual(rows['<div class="a">']["dh"], 0.5)
        self.assertEqual(rows["<p>"]["chromium"]["y"], 10.5)
        self.assertEqual(rows["<p>"]["dy"], 0.5)
        self.assertEqual(result["summary"]["unmatched_qml"], 0)


if __name__ == "__main__":
    unittest.main()
