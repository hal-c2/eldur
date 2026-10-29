# Visual Parity Harness (`tools/visual/`)

Objective, repeatable visual comparison between Chromium's rendering of an
HTML5/CSS page and Qt Quick 6.11's off-screen rendering of the corresponding
QML application at the same viewport (default `1280×800`, `deviceScaleFactor = 1`).

## Quick Start

```bash
# Run the built-in self-test suite (no converter required):
python3 tools/visual/run.py --selftest
mise run visual -- --selftest

# Run the unit and integration test suite:
mise run visual test
mise run visual -- --test

# Score the converter on tests/corpus/ -> target/visual/report.{md,json}:
mise run visual
python3 tools/visual/run.py --corpus tests/corpus --out target/visual

# Run specific corpus cases or merge updates into tools/visual/baseline.json:
mise run visual -- --cases card,two-columns
mise run visual -- --update-baseline
```

## Components

### 1. `render_html.mjs`

```bash
node tools/visual/render_html.mjs <page.html> <out.png> [--width 1280 --height 800]
```

- Launches headless Playwright Chromium with `viewport: { width, height }` and
  `deviceScaleFactor: 1`.
- Loads the local `file://` URL and waits for `load`, injects
  `*{animation:none!important;transition:none!important;caret-color:transparent!important}`
  (including `::before`/`::after`), waits for `document.fonts.ready` plus two
  `requestAnimationFrame` ticks, and captures a viewport-only PNG (`fullPage: false`).
- **Playwright module resolution:** When Playwright is installed globally via
  `mise` rather than a workspace `node_modules/`, Node ESM cannot resolve bare
  `import("playwright")`. `render_html.mjs` first attempts `import("playwright")`,
  then falls back via `module.createRequire` through:
  1. `PLAYWRIGHT_MODULE_PATH` (if set),
  2. `<npm root -g>/playwright`,
  3. `~/.local/share/mise/installs/npm-playwright/latest/node_modules/playwright`,
  4. `../playwright` relative to the resolved `playwright` binary on `PATH`.

### 2. `Shot.qml` + `render_qml.py`

```bash
python3 tools/visual/render_qml.py <dir-with-Main.qml> <out.png> [--width 1280 --height 800] [--strict]
```

- Runs `qml6 --software` with `QT_QPA_PLATFORM=offscreen`,
  `QT_QUICK_BACKEND=software`, `QT_FONT_DPI=96`, `QT_SCALE_FACTOR=1`,
  `QML_DISABLE_DISK_CACHE=1`, and a 10 s timeout.
- Returns any QML runtime/binding warnings printed by `qml6`; passing `--strict`
  causes `render_qml.py` to exit non-zero whenever `qml6` emits warnings.
- `Shot.qml` uses a `QtObject` root and loads `Main.qml` via
  `Qt.createComponent`:
  - **`ApplicationWindow` / `Window` root:** resizes and shows the loaded window
    directly without opening a second window, binds a low-`z` backdrop
    `Rectangle` to `win.color` inside the top visual `QQuickRootItem` (because
    `QQuickItem::grabToImage` renders onto a transparent buffer without
    `QQuickWindow::color`, and `ApplicationWindow.contentItem` is an internal
    `QQuickContentItem` without a `QQmlEngine` context), waits for any
    `Image.Loading` items and two swapped frames, then saves `grabToImage`.
  - **`Item` root:** creates a single host `Window` sized to `W×H` with a
    `#ffffff` canvas backdrop, parents and sizes the root `Item` to `W×H`, waits
    two frames, and saves `window.contentItem.grabToImage`.

### 3. `compare.py`

```bash
python3 tools/visual/compare.py <a.png> <b.png> [--diff <out.png>]
```

Implemented with Pillow + NumPy only:

| Metric | Definition |
|---|---|
| `pixel_mismatch` | Fraction of pixels where $\max(\lvert R_a - R_b\rvert, \lvert G_a - G_b\rvert, \lvert B_a - B_b\rvert) > 32$ |
| `ssim` | Mean grayscale SSIM over sliding $8\times 8$ windows ($C_1 = (0.01\cdot 255)^2$, $C_2 = (0.03\cdot 255)^2$) |
| `edge_iou` | Jaccard IoU of thresholded $3\times 3$ Sobel gradient magnitude maps ($\lVert\nabla I\rVert / 4 > 20$) |
| `score` | $100 \times \bigl(0.5\cdot\text{ssim} + 0.3\cdot\text{edge\_iou} + 0.2\cdot(1 - \text{pixel\_mismatch})\bigr)$ |

Passing `--diff <out.png>` writes a labelled three-panel PNG:
`Browser (HTML) | Qt Quick (QML) | Diff Heatmap`.

### 4. `run.py` and `mise-tasks/visual`

- Discovers every case directory containing `index.html` under `--corpus`
  (default `tests/corpus`), optionally filtered by `--cases a,b`.
- Renders HTML, runs `--convert-cmd` (default
  `cargo run -q -p h2q_cli -- convert`), renders QML, compares screenshots, and
  writes `<out>/report.md` and `<out>/report.json`.
- Per-case statuses: `ok`, `qml_warnings` (when `--strict` is active, which is
  the default; pass `--no-strict` to keep status `ok` while still recording
  warnings), `convert_failed`, `html_render_failed`, `qml_render_failed`, and
  `compare_failed`.
- If `tools/visual/baseline.json` exists, validates that its recorded `viewport`
  matches `--width`/`--height`, then flags any case whose score drops by more
  than `2.0` points (or degrades from `ok` to a warning/failure status) and
  exits non-zero. `--update-baseline` merges new `ok` scores into
  `tools/visual/baseline.json` (preserving unselected or failed entries) and is
  refused when combined with `--selftest`.

## Self-Test & Font Choice (`tools/visual/selftest/`)

`python3 tools/visual/run.py --selftest` renders five hand-written HTML/QML
pairs without invoking the Rust converter:

1. `01_colored_box` — card box with badge, heading, wrapped paragraph, and bar (`ApplicationWindow` root).
2. `02_two_columns` — top bar + fixed sidebar + flexible main column (`Rectangle`/`Item` root).
3. `03_list` — bordered panel with a four-row bulleted checklist (`ApplicationWindow` root).
4. `04_near_miss` — subtle converter defects (`8px` uncollapsed offset + smaller `13px`/`17px` font sizes) on the two-column page (`ApplicationWindow` root).
5. `05_wrong_layout` — deliberately wrong layout, position, and colours (`ApplicationWindow` root).

It asserts strict score ranking: all three matching pairs score `> 85`,
`04_near_miss` scores clearly below the matching pairs (at least 10 points
lower), and `05_wrong_layout` scores clearly below `04_near_miss` (at least 15
points lower).

### Font Choice: `Liberation Sans`

Probing system fonts via `fc-list` and Qt's `Qt.fontFamilies()` showed that
`DejaVu Sans` is **not** installed on this host, whereas **`Liberation Sans`**
(as well as `Noto Sans` and `Liberation Mono`) is installed and discovered
identically by both Chromium (via fontconfig) and Qt 6.11 (`QT_FONT_DPI=96`,
`QT_SCALE_FACTOR=1`). All self-test HTML stylesheets and QML `Text` items
explicitly request `Liberation Sans` so parity scores reflect box layout and
typography geometry rather than fallback font substitution.
