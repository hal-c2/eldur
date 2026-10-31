# Visual Parity Harness (`tools/visual/`)

Objective, repeatable visual comparison between Chromium's rendering of an
HTML5/CSS page and Qt Quick 6.11's rendering of the corresponding QML
application (OpenGL on a private headless sway by default, or the off-screen
software scene graph with `--backend software`) at the same viewport (default `1280×800`, `deviceScaleFactor = 1`).

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

# The QML side renders with OpenGL and native glyphs by default (needs sway).
# Without sway or GL the run fails; it never falls back on its own. Choose:
mise run visual -- --backend software
mise run visual -- --gl-text distancefield
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
  `import("playwright")`. `playwright.mjs` (shared by `render_html.mjs` and
  `computed.mjs`) first attempts `import("playwright")`,
  then falls back via `module.createRequire` through:
  1. `PLAYWRIGHT_MODULE_PATH` (if set),
  2. `<npm root -g>/playwright`,
  3. `~/.local/share/mise/installs/npm-playwright/latest/node_modules/playwright`,
  4. `../playwright` relative to the resolved `playwright` binary on `PATH`.

### Computed-style oracle: `computed.mjs`

```bash
node tools/visual/computed.mjs cases.json   # or JSON on stdin
```

Reports what Chromium's `getComputedStyle()` gives for each case
(`{name, html, select, props}`), one JSON line per case. Use it to choose the
expected values in eldur_css tests, so that they assert browser behaviour:
error recovery in particular is easy to misread from the spec.

### Geometry oracle: `geometry.mjs`

```bash
node tools/visual/geometry.mjs page.html '.card' 'h1'
node tools/visual/geometry.mjs --html '<p>x</p>' 'p'
```

Reports Chromium's `getBoundingClientRect()` for every element each selector
matches, one JSON line per element, at the harness viewport (1280×800,
`deviceScaleFactor: 1`). Use it for the expected positions and sizes in
converter tests.

### 2. `Shot.qml` + `render_qml.py`

```bash
python3 tools/visual/render_qml.py <dir-with-Main.qml> <out.png> [--width 1280 --height 800] [--strict]
                                   [--backend gl|software] [--gl-text native|distancefield]
```

- Both backends set `QT_FONT_DPI=96`, `QT_SCALE_FACTOR=1`,
  `QML_DISABLE_DISK_CACHE=1`, and a 10 s timeout.
- The default is `--backend gl --gl-text native`. When gl cannot run (no
  sway on `PATH`, sway fails to start, or Qt did not get an OpenGL scene
  graph, which `Shot.qml` checks through `GraphicsInfo.api` before grabbing)
  the render fails with an error that says to pass `--backend software`;
  `run.py` stops the whole run. It never falls back silently: a score is only
  comparable to scores from the same backend, and `report.json` records
  `backend` and `gl_text`.
- `--backend software` runs `qml6 --software` with
  `QT_QPA_PLATFORM=offscreen` and `QT_QUICK_BACKEND=software`. The software
  scene graph does not draw shader effects, so `RectangularShadow` and
  `MultiEffect` (box-shadow, focus rings) are missing from its screenshots.
- `--backend gl` runs `qml6` with `QT_QPA_PLATFORM=wayland` and
  `QSG_RHI_BACKEND=opengl` on a **private headless sway**
  (`WLR_BACKENDS=headless`, its own `XDG_RUNTIME_DIR` under a temp dir, its own
  socket). The user's `WAYLAND_DISPLAY` and compositor are never used. `run.py`
  starts one sway per run and stops it at the end, also on errors and Ctrl-C;
  If the harness is killed outright (SIGKILL, OOM killer), a `tail --pid`
  watcher in the sway config ends sway too. The offscreen platform cannot
  use OpenGL here (it falls back to software), and minimalegl with
  `EGL_PLATFORM=surfaceless` finds no EGLConfig, hence the compositor.
  - The sway config makes every window floating and borderless at the
    viewport size. Under sway's default tiling a 1280×800 window became
    1276×772 (borders, title bar), and `grabToImage` stretched it back to
    1280×800, so text and boxes came out ~4% taller. `Shot.qml` fails when
    the window is not the requested size instead of stretching it. The
    config also lowers sway's 75×50 minimum floating size, which enlarged
    small viewports after their first frame.
  - `--gl-text native` (default) sets `QML_DISABLE_DISTANCEFIELD=1`, which
    rasterises glyphs like the software backend (and closer to Chromium)
    while effects still draw. `--gl-text distancefield` is Qt's GPU default
    for `Text.QtRendering`: distance-field glyphs, lighter and softer than
    Chromium's.
    Neither changes layout (advances come from the same design metrics). Text
    on a fractional y can land 1 px off from software, since the two backends
    round the baseline differently.
  - Known: text under a scale `transform` is not pixel-reproducible on gl
    (Mesa radeonsi here). 20-pricing's `Scale { 1.05 }` card gave 4 variants
    in 10 renders with native glyphs (≤ 7 pixels inside glyphs, same score to
    four decimals) and a different image every time with distance fields;
    removing the transform, or `--backend software`, makes it reproducible.
    The render loop (`QSG_RENDER_LOOP=basic`) makes no difference, so it is
    not capture timing.
- Returns any QML runtime/binding warnings printed by `qml6`; passing `--strict`
  causes `render_qml.py` to exit non-zero whenever `qml6` emits warnings.
- `Shot.qml` uses a `QtObject` root and loads `Main.qml` via
  `Qt.createComponent`:
  - **`ApplicationWindow` / `Window` root:** resizes and shows the loaded window
    directly without opening a second window, binds a low-`z` backdrop
    `Rectangle` to `win.color` inside the top visual `QQuickRootItem` (because
    `QQuickItem::grabToImage` renders onto a transparent buffer without
    `QQuickWindow::color`, and `ApplicationWindow.contentItem` is an internal
    `QQuickContentItem` without a `QQmlEngine` context), waits until the page
    has settled (below), then saves `grabToImage`.
  - **`Item` root:** creates a single host `Window` sized to `W×H` with a
    `#ffffff` canvas backdrop, parents and sizes the root `Item` to `W×H`, waits
    until the page has settled, and saves `window.contentItem.grabToImage`.
  - **Settling:** generated pages finish layout over several event-loop turns
    (text wrapping beside a float re-lays out through `onLineLaidOut` and
    `Qt.callLater(forceLayout)`), so a fixed frame count can catch them
    mid-relayout, and does under load. After images have loaded, `Shot.qml`
    requests one frame and then never forces another; on both the offscreen
    software and the Wayland/OpenGL render loops a frame is only swapped when
    an item changed. It grabs once no `frameSwapped` has arrived for 250 ms
    (`SETTLE_QUIET_MS`) after at least one frame at the requested size; a
    resize or a loading image restarts the wait. A page that never goes quiet
    (an infinite animation) is grabbed after 4 s (`SETTLE_MAX_MS`) with a
    `page did not settle` warning, which counts as a QML warning (`--strict`).

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
  `cargo run -q -p eldur_cli -- convert`), renders QML, compares screenshots, and
  writes `<out>/report.md` and `<out>/report.json`.
- `--backend software|gl` and `--gl-text` choose the QML renderer (see
  `render_qml.py`); the report records both.
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
