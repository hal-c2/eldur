#!/usr/bin/env python3
"""Unit and integration tests for the tools/visual parity harness."""

from __future__ import annotations

import io
import json
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

import numpy as np
from PIL import Image

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import compare  # noqa: E402
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

            render_qml.render_qml(appwin_dir, appwin_png, width=200, height=120)
            render_qml.render_qml(item_dir, item_png, width=200, height=120)

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
                rc = render_qml.main([str(tmp_dir), str(tmp_dir / "out.png")])
            self.assertEqual(rc, 1)
            self.assertIn("Main.qml", err_buf.getvalue())


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

            # First run with --update-baseline: broken_convert recorded as convert_failed,
            # good_case scores ~100 and writes baseline.json.
            with redirect_stdout(io.StringIO()):
                rc1 = visual_run.main(
                    [
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
            self.assertIn("[side-by-side](cases/good_case/diff.png)", (out_dir / "report.md").read_text(encoding="utf-8"))

            # Degrade good_case colour to trigger > 2 point regression against baseline.
            color_cfg.write_text("#ef4444\n", encoding="utf-8")
            err_buf = io.StringIO()
            with redirect_stdout(io.StringIO()), redirect_stderr(err_buf):
                rc2 = visual_run.main(
                    [
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


if __name__ == "__main__":
    unittest.main()
