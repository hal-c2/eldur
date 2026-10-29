#!/usr/bin/env python3
"""Compare two rendered PNG screenshots (browser HTML vs Qt Quick QML).

Usage:
    python3 tools/visual/compare.py <a.png> <b.png> [--diff <out.png>]

Prints JSON with:
  - pixel_mismatch: fraction of pixels where max(|R_a-R_b|, |G_a-G_b|, |B_a-B_b|) > 32
  - ssim:           mean SSIM on grayscale using an 8x8 window (Wang et al. constants)
  - edge_iou:       IoU of thresholded 3x3 Sobel gradient magnitude maps
  - score:          100 * (0.5 * ssim + 0.3 * edge_iou + 0.2 * (1 - pixel_mismatch))

When `--diff <out.png>` is supplied, writes a side-by-side comparison PNG:
    Browser | QML | Heat map
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

import numpy as np
from PIL import Image, ImageDraw, UnidentifiedImageError

PIXEL_DIFF_THRESHOLD = 32
SSIM_WINDOW = 8
SSIM_C1 = (0.01 * 255.0) ** 2
SSIM_C2 = (0.03 * 255.0) ** 2
# Normalized 3x3 Sobel threshold (20/255 luminance gradient step) captures
# structural UI edges and subtle borders while rejecting 1-2 level hinting noise.
SOBEL_EDGE_THRESHOLD = 20.0


class CompareError(RuntimeError):
    """Raised when input images cannot be loaded or compared."""


def load_rgb(path: Path) -> np.ndarray:
    """Load an image file as an (H, W, 3) uint8 RGB array composited over white."""
    if not path.is_file():
        raise CompareError(f"Image file does not exist: {path}")
    try:
        with Image.open(path) as im:
            if im.mode in ("RGBA", "LA") or (im.mode == "P" and "transparency" in im.info):
                rgba = im.convert("RGBA")
                white = Image.new("RGBA", rgba.size, (255, 255, 255, 255))
                rgb = Image.alpha_composite(white, rgba).convert("RGB")
            else:
                rgb = im.convert("RGB")
            return np.asarray(rgb, dtype=np.uint8)
    except (OSError, UnidentifiedImageError, ValueError) as exc:
        raise CompareError(f"Failed to read image {path}: {exc}") from exc


def to_grayscale(rgb: np.ndarray) -> np.ndarray:
    """Convert (H, W, 3) uint8 RGB to float64 ITU-R BT.601 luma in [0, 255]."""
    r = rgb[:, :, 0].astype(np.float64)
    g = rgb[:, :, 1].astype(np.float64)
    b = rgb[:, :, 2].astype(np.float64)
    return 0.299 * r + 0.587 * g + 0.114 * b


def _window_mean(gray: np.ndarray, win: int = SSIM_WINDOW) -> np.ndarray:
    """Compute exact sliding `win`x`win` box means via a 2D summed-area table."""
    padded = np.pad(gray, ((1, 0), (1, 0)), mode="constant", constant_values=0.0)
    sat = padded.cumsum(axis=0, dtype=np.float64).cumsum(axis=1, dtype=np.float64)
    sums = sat[win:, win:] - sat[:-win, win:] - sat[win:, :-win] + sat[:-win, :-win]
    return sums / float(win * win)


def compute_ssim(gray_a: np.ndarray, gray_b: np.ndarray, win: int = SSIM_WINDOW) -> float:
    """Compute mean grayscale SSIM over all valid `win`x`win` windows."""
    h, w = gray_a.shape
    if h < win or w < win:
        raise CompareError(
            f"Image dimensions {w}x{h} are smaller than SSIM window {win}x{win}"
        )

    mu_a = _window_mean(gray_a, win)
    mu_b = _window_mean(gray_b, win)

    mu_a_sq = mu_a * mu_a
    mu_b_sq = mu_b * mu_b
    mu_ab = mu_a * mu_b

    # Clamp against tiny negative values (~-1e-13) from floating-point cancellation on flat patches.
    sigma_a_sq = np.maximum(0.0, _window_mean(gray_a * gray_a, win) - mu_a_sq)
    sigma_b_sq = np.maximum(0.0, _window_mean(gray_b * gray_b, win) - mu_b_sq)
    sigma_ab = _window_mean(gray_a * gray_b, win) - mu_ab

    numerator = (2.0 * mu_ab + SSIM_C1) * (2.0 * sigma_ab + SSIM_C2)
    denominator = (mu_a_sq + mu_b_sq + SSIM_C1) * (sigma_a_sq + sigma_b_sq + SSIM_C2)
    ssim_map = numerator / denominator
    return float(np.mean(ssim_map))


def sobel_edges(gray: np.ndarray, threshold: float = SOBEL_EDGE_THRESHOLD) -> np.ndarray:
    """Return binary edge mask using 3x3 Sobel filters normalized to [0, ~360.6]."""
    p = np.pad(gray, 1, mode="edge")
    gx = (
        -p[:-2, :-2]
        + p[:-2, 2:]
        - 2.0 * p[1:-1, :-2]
        + 2.0 * p[1:-1, 2:]
        - p[2:, :-2]
        + p[2:, 2:]
    )
    gy = (
        -p[:-2, :-2]
        - 2.0 * p[:-2, 1:-1]
        - p[:-2, 2:]
        + p[2:, :-2]
        + 2.0 * p[2:, 1:-1]
        + p[2:, 2:]
    )
    magnitude = np.hypot(gx, gy) / 4.0
    return magnitude > threshold


def compute_edge_iou(gray_a: np.ndarray, gray_b: np.ndarray) -> float:
    """Compute Jaccard IoU between thresholded Sobel edge maps of two grayscale images."""
    edges_a = sobel_edges(gray_a)
    edges_b = sobel_edges(gray_b)
    union = int(np.count_nonzero(edges_a | edges_b))
    if union == 0:
        return 1.0
    intersection = int(np.count_nonzero(edges_a & edges_b))
    return float(intersection / union)


def _build_heatmap(max_diff: np.ndarray, gray_context: np.ndarray) -> np.ndarray:
    """Map per-pixel max RGB channel delta [0..255] to an informative heat map."""
    lut = np.zeros((256, 3), dtype=np.uint8)

    # 1..32 (at or below mismatch threshold): dark blue -> cyan (minor AA / hinting delta)
    idx_low = np.arange(1, PIXEL_DIFF_THRESHOLD + 1, dtype=np.float64)
    t_low = (idx_low - 1.0) / max(1.0, float(PIXEL_DIFF_THRESHOLD - 1))
    lut[1 : PIXEL_DIFF_THRESHOLD + 1, 0] = np.round(15.0 + t_low * (14.0 - 15.0)).astype(np.uint8)
    lut[1 : PIXEL_DIFF_THRESHOLD + 1, 1] = np.round(40.0 + t_low * (165.0 - 40.0)).astype(np.uint8)
    lut[1 : PIXEL_DIFF_THRESHOLD + 1, 2] = np.round(90.0 + t_low * (233.0 - 90.0)).astype(np.uint8)

    # 33..128 (above threshold): amber -> orange
    idx_mid = np.arange(PIXEL_DIFF_THRESHOLD + 1, 129, dtype=np.float64)
    t_mid = (idx_mid - (PIXEL_DIFF_THRESHOLD + 1)) / float(128 - (PIXEL_DIFF_THRESHOLD + 1))
    lut[PIXEL_DIFF_THRESHOLD + 1 : 129, 0] = np.round(250.0 + t_mid * (249.0 - 250.0)).astype(np.uint8)
    lut[PIXEL_DIFF_THRESHOLD + 1 : 129, 1] = np.round(204.0 + t_mid * (115.0 - 204.0)).astype(np.uint8)
    lut[PIXEL_DIFF_THRESHOLD + 1 : 129, 2] = np.round(21.0 + t_mid * (22.0 - 21.0)).astype(np.uint8)

    # 129..255 (severe mismatch): bright orange-red -> vivid crimson
    idx_hi = np.arange(129, 256, dtype=np.float64)
    t_hi = (idx_hi - 129.0) / float(255 - 129)
    lut[129:, 0] = np.round(239.0 + t_hi * (255.0 - 239.0)).astype(np.uint8)
    lut[129:, 1] = np.round(68.0 + t_hi * (24.0 - 68.0)).astype(np.uint8)
    lut[129:, 2] = np.round(68.0 + t_hi * (88.0 - 68.0)).astype(np.uint8)

    heat = lut[max_diff]

    # Dimmed grayscale silhouette on exact-match pixels so reviewers see layout context.
    zero_mask = max_diff == 0
    if np.any(zero_mask):
        dim = np.clip(np.round(12.0 + 0.14 * gray_context), 0.0, 255.0).astype(np.uint8)
        heat[zero_mask, 0] = dim[zero_mask]
        heat[zero_mask, 1] = np.clip(dim[zero_mask].astype(np.uint16) + 4, 0, 255).astype(np.uint8)
        heat[zero_mask, 2] = np.clip(dim[zero_mask].astype(np.uint16) + 12, 0, 255).astype(np.uint8)

    return heat


def write_diff_image(
    rgb_a: np.ndarray,
    rgb_b: np.ndarray,
    max_diff: np.ndarray,
    gray_a: np.ndarray,
    metrics: dict[str, float],
    diff_path: Path,
) -> None:
    """Write a side-by-side PNG (Browser | QML | Heat map) to `diff_path`."""
    h, w, _ = rgb_a.shape
    divider_w = 2
    header_h = 26
    total_w = 3 * w + 2 * divider_w
    total_h = header_h + h

    heatmap = _build_heatmap(max_diff, gray_a)

    canvas = np.zeros((total_h, total_w, 3), dtype=np.uint8)
    # Dark header background and slate vertical dividers.
    canvas[:header_h, :, :] = np.array([15, 23, 42], dtype=np.uint8)
    canvas[header_h:, :, :] = np.array([100, 116, 139], dtype=np.uint8)

    x0 = 0
    x1 = w + divider_w
    x2 = 2 * (w + divider_w)

    canvas[header_h:, x0 : x0 + w, :] = rgb_a
    canvas[header_h:, x1 : x1 + w, :] = rgb_b
    canvas[header_h:, x2 : x2 + w, :] = heatmap

    img = Image.fromarray(canvas, mode="RGB")
    draw = ImageDraw.Draw(img)
    draw.text((x0 + 8, 6), "Browser (HTML)", fill=(248, 250, 252))
    draw.text((x1 + 8, 6), "Qt Quick (QML)", fill=(248, 250, 252))
    summary = (
        f"Diff Heatmap  score={metrics['score']:.1f}  "
        f"ssim={metrics['ssim']:.3f}  "
        f"edge_iou={metrics['edge_iou']:.3f}  "
        f"mismatch={metrics['pixel_mismatch'] * 100.0:.1f}%"
    )
    draw.text((x2 + 8, 6), summary, fill=(248, 250, 252))

    diff_abs = diff_path.resolve()
    diff_abs.parent.mkdir(parents=True, exist_ok=True)
    img.save(diff_abs, format="PNG")


def compare_arrays(
    rgb_a: np.ndarray,
    rgb_b: np.ndarray,
) -> tuple[dict[str, float], np.ndarray, np.ndarray]:
    """Compare two equal-sized (H, W, 3) uint8 RGB arrays and return (metrics, max_diff, gray_a)."""
    if rgb_a.ndim != 3 or rgb_b.ndim != 3 or rgb_a.shape[2] != 3 or rgb_b.shape[2] != 3:
        raise CompareError(f"Expected (H, W, 3) RGB arrays, got {rgb_a.shape} and {rgb_b.shape}")
    if rgb_a.shape != rgb_b.shape:
        h1, w1, _ = rgb_a.shape
        h2, w2, _ = rgb_b.shape
        raise CompareError(f"Image dimension mismatch: {w1}x{h1} vs {w2}x{h2}")

    max_diff = np.abs(rgb_a.astype(np.int16) - rgb_b.astype(np.int16)).max(axis=2).astype(np.uint8)
    pixel_mismatch = float(np.mean(max_diff > PIXEL_DIFF_THRESHOLD))

    gray_a = to_grayscale(rgb_a)
    gray_b = to_grayscale(rgb_b)

    ssim = compute_ssim(gray_a, gray_b)
    edge_iou = compute_edge_iou(gray_a, gray_b)
    raw_score = 100.0 * (0.5 * ssim + 0.3 * edge_iou + 0.2 * (1.0 - pixel_mismatch))
    score = float(np.clip(raw_score, 0.0, 100.0))

    metrics = {
        "pixel_mismatch": round(pixel_mismatch, 6),
        "ssim": round(ssim, 6),
        "edge_iou": round(edge_iou, 6),
        "score": round(score, 4),
    }
    return metrics, max_diff, gray_a


def compare_images(
    path_a: Path,
    path_b: Path,
    *,
    diff_path: Path | None = None,
) -> dict[str, Any]:
    """Compare two PNG files and optionally write a side-by-side diff image."""
    rgb_a = load_rgb(path_a)
    rgb_b = load_rgb(path_b)
    if rgb_a.shape != rgb_b.shape:
        h1, w1, _ = rgb_a.shape
        h2, w2, _ = rgb_b.shape
        raise CompareError(
            f"Image dimension mismatch: {path_a} is {w1}x{h1}, {path_b} is {w2}x{h2}"
        )

    metrics, max_diff, gray_a = compare_arrays(rgb_a, rgb_b)
    if diff_path is not None:
        write_diff_image(rgb_a, rgb_b, max_diff, gray_a, metrics, diff_path)
    return metrics


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="compare.py",
        description="Compute visual parity metrics (SSIM, Sobel edge IoU, pixel mismatch, score) between two PNG images.",
    )
    parser.add_argument("image_a", type=Path, help="Reference (browser HTML) PNG path")
    parser.add_argument("image_b", type=Path, help="Candidate (Qt Quick QML) PNG path")
    parser.add_argument(
        "--diff",
        type=Path,
        default=None,
        help="Optional path to write side-by-side comparison PNG (Browser | QML | Heat map)",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        result = compare_images(args.image_a, args.image_b, diff_path=args.diff)
    except CompareError as exc:
        print(f"compare.py: error: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
