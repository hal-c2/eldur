# CSS layout to QML layout strategy

Target: Qt 6.11, QtQuick, QtQuick.Controls (Basic style), QtQuick.Layouts, QtQuick.Shapes, QtQuick.Effects. Every type and property named here was checked against the Qt 6.11 `.qmltypes` files; behaviour claims marked "measured" were checked with `qml6` off-screen.

## 0. Container selection

| CSS construct | QML container | Support | Notes |
|---|---|---|---|
| Root document | `ApplicationWindow` > `ScrollView` > page `Item` | Full | Body scrolls inside the ScrollView. |
| Block container (normal flow) | `ColumnLayout` | Full | See §1 for why. |
| Inline run of text | `Text { textFormat: Text.StyledText }`, or `EldurLines` when runs differ in metrics | Full | See §2. |
| `display: flex` | `RowLayout` / `ColumnLayout`, or `Flow` if wrapping | Approx | See §3. |
| `display: grid`, `table` | `GridLayout`, or `TableView` for large data tables | Approx | See §4, §8. |
| `position: relative` | Same parent, `transform: Translate` | Full | See §5. |
| `position: absolute` / `fixed` / `sticky` | Overlay child of the positioned ancestor / window / Flickable binding | Approx | See §5. |
| Box with padding, border, background | `Rectangle` + inner layout | Full | See §7. |
| `img` | `Image` | Full | See §9. |

Ordered rule: `display:none` is dropped, then `position:absolute|fixed` children are removed from their flow and placed as overlays, then `display` picks the container, then each child gets its `Layout.*` attributes from its box model and its CSS size.

`ApplicationWindow` is a `Window`, not an `Item`: it has no `states`, `anchors` or `transform`. Anything that needs those goes on an `Item` inside it.

## 1. Normal flow block layout

**Decision: `ColumnLayout`, not `Column` + anchors.**
- Layouts propagate `implicitHeight` upward. Auto-height parents, `contentHeight` of the ScrollView, and nested blocks all need that chain.
- `Layout.fillWidth: true` gives `width: auto` semantics for block children without per-item width bindings.
- A wrapped `Text` needs its width before its height is known. The Layout assigns the width of a fill item first, then the `Text`'s `implicitHeight` follows the wrapped width and the Layout re-lays out (measured: a 100px-wide `ColumnLayout` with a wrapped `Text` reports the wrapped `implicitHeight`). No `contentHeight` workaround is needed.

**HTML/CSS**
```html
<main class="page"><h1>Hello</h1><p>Long paragraph…</p></main>
```
```css
.page { width: 600px; margin: 0 auto; padding: 0 16px; }
h1 { margin: 24px 0 16px; font-size: 32px; font-weight: bold; }
p  { margin: 16px 0; }
```

Margin computation: `.page` has no top padding or border, so the `h1` top margin (24px) collapses through `.page`. The `h1` bottom (16) and `p` top (16) collapse to 16. `.page` uses the default `box-sizing: content-box`, so its border box is 600 + 2×16 = 632px wide.

**QML output**
```qml
ScrollView {
    id: scroll
    anchors.fill: parent
    contentWidth: availableWidth
    contentHeight: page.y + page.height

    Item {
        id: page
        width: 632                                           // content-box: 600 + 2 * 16 padding
        x: Math.max(0, (scroll.availableWidth - width) / 2)   // margin: 0 auto
        y: 24                                                // h1 margin-top escapes .page
        height: content.implicitHeight

        ColumnLayout {
            id: content
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: 16                           // padding: 0 16px
            anchors.rightMargin: 16
            spacing: 0

            Text {
                text: "Hello"
                font.pixelSize: 32
                font.weight: Font.Bold
                Layout.fillWidth: true
            }
            Text {
                text: "Long paragraph…"
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
                Layout.topMargin: 16                         // collapsed gap on the following item
            }
        }
    }
}
```

**Margin collapsing rule:** the collapsed gap (`max(prev.bottom, next.top)`) goes on the following item as `Layout.topMargin`. `spacing` stays 0 and the previous item gets no bottom margin, so gaps are not doubled. Layout margins add to spacing, so this is the only way to get exact CSS gaps. Collapse only applies in block flow, not between flex or grid items.

**Empty blocks a script may fill.** An empty block collapses through (CSS 2.2 §8.3.1) and is not emitted. On a script page one a script can reach is kept as a `Text` with no line box (`Layout.preferredHeight: text === "" ? 0 : -1`); margins still collapse through it, so the next item's top margin is what the empty block would hand it until the block has text or a child: `(host.text !== "" || host.children.length > 0) ? 14 : 0`.

**Centering (`margin: 0 auto`)**: inside a `ColumnLayout`, use `Layout.alignment: Qt.AlignHCenter` with `Layout.preferredWidth` set to the fixed border-box width (no `fillWidth`). For `max-width` with auto width, use `Layout.fillWidth: true` plus `Layout.maximumWidth`. Use the `x` binding above when the parent is a plain `Item`.

| Feature | Support | Notes |
|---|---|---|
| Block stacking | Full | |
| `width: auto` fills containing block | Full | `Layout.fillWidth: true` |
| Sibling margin collapse | Full | Rule above. |
| Parent/child margin collapse | Full | Emitted as the parent's offset (`y`) or the layout's margin. |
| `margin: 0 auto` centering | Full | Fixed or max width only. |
| Negative margins | Approx | A `Translate` moves the item visually but the following siblings do not move up. |
| `display: block` with floats | Emulated | See §5.5. |
| `writing-mode: vertical-*` | Unsupported | Warn. |

**Pitfalls**
- If a `ColumnLayout` has no definite width (inside a ScrollView whose `availableWidth` is unbound), the implicit width is the widest unwrapped line. Always bind `width` or anchors to the ScrollView's `availableWidth`.
- `Layout.*` attached properties have no effect outside a Layout. `anchors` on a direct child of a Layout logs a warning and gives undefined positioning.
- Percent widths inside a shrink-to-fit Layout create a binding loop (see §10).

### 1.1 Sub-pixel positions: the carry

Chromium places boxes at fractional offsets and snaps them only when painting. Qt Quick Layouts snap during layout. All of the following was measured in Qt 6.11:
- A `ColumnLayout` rounds an item's implicit height up. It keeps an explicit `Layout.preferredHeight` as it is.
- It rounds each cell (top margin + height + bottom margin) up with no tolerance: a 27 px item with a 1e-12 px bottom margin takes 28.
- It skips an invisible item, margins included.
- Negative margins are honoured.

A column of 27.2 px paragraphs therefore drifts 0.8 px per paragraph. Before this change, the corpus drifted up to 12.7 px (01-typography).

**The carry** (`emit/carry.rs`, run on every block-flow `ColumnLayout` and every single-line flex column):
- Each item's top margin takes back what the cell before it was rounded up by. Item k's carry is `ceil(top + H) − top − h`, where `H` is the height Qt lays it out at and `h` is its CSS height.
- Item k+1's top margin becomes `gap − carry`. Every cell edge is then whole, positions are exact, and the column's height is rounded once.
- A bottom margin (the strut below an inline replaced box) is folded into the next top margin, so each cell is two terms in Qt as in the binding.

What is known at conversion time stays a number. Only heights known at run time get a binding:
```qml
Text {                       // <h1>, line-height 1.2 at 32 px
    id: h1
    property real eldurCarry: Math.ceil(Layout.topMargin + Math.ceil(implicitHeight)) - Layout.topMargin - implicitHeight + 0.000001
}
Text {
    Layout.topMargin: 21.44 - h1.eldurCarry
}
Rectangle {                  // height: 40.5px; the next top margin is a number
    Layout.preferredHeight: 40.5
}
Text { Layout.topMargin: 3.5 }   // 4 − (41 − 40.5)
```

The rules:
- **Whole heights need nothing.** A column whose heights are all whole is emitted as before.
- **Painted boxes keep their exact height.** A `Text` paints nothing outside its glyphs, so it keeps its rounded height hint. Any other item gets `Layout.preferredHeight: implicitHeight`, so its background is as tall as in CSS. A column's last `Text` gets it too, so that the column total is rounded once rather than twice.
- **Fixed heights join the chain.** An item with a numeric `Layout.preferredHeight` is static and takes the carry like any other item. A fill, minimum or maximum height (`Layout.maximumHeight` other than `Number.POSITIVE_INFINITY`) is unknown, and the chain starts again after it.
- **Nested columns.** A carried column's implicit height is its CSS height plus its last item's carry, its *excess*. The parent sees the column, or a box around it (`inner.implicitHeight + insets`), as whole plus that excess, and hands it on. The column carries its excess in a comment until its parent is built, so the wrapper still names it.
- **One-column grids and flex rows.** A `GridLayout` with one column of content-high rows is carried like a column; its items stop stretching, since each row is as high as its item. A flex row of plain items (no nested layout, vertical margin or fixed height) with a fractional line height has an excess: its tallest item rounded up, less the item. In a `ColumnLayout`, a plain row whose margin is only the carry (negative) is a whole height less it to the nearest pixel, measured, so its carry rounds; with a margin of its own, in a one-column `GridLayout`, and for a column, the carry rounds up (03, 17 and 20 lose pixels otherwise). Known gaps: a grid of several columns or a row holding a nested carried column rounds as before.
- **A grid given its height is not carried.** The bet only holds for a grid sized by its content. A grid stretched by a flex row or a grid, or with `height`, `min-height` or a resolved `%`, shares free height as CSS does: `auto` rows an equal part (`align-content: normal/stretch`), `fr` rows all of it, other `align-content` values pack outside the tracks. Qt shares free height by preferred heights (measured: 0 and 40 at 300 give 6 and 294), so a zero-wide carrier item per row, in an extra last column, binds the row's height. No carriers (Qt's sharing stays) for a multi-column grid with a column gap, spans, hidden items or templates.
- **Floating-point residue.** A cell meant to be whole can come out a hair over in floating point (−0.8 + 28.8). Qt then rounds it a whole pixel up, and places a last item after the extra pixel. Each run-time carry is therefore a micropixel (`0.000001`) more than exact. A static top margin whose cell would land within 1e-9 above a whole number is a micropixel less.
- **Hidden items** (with a `visible` binding, an `objectName`, or reachable by scripts) carry `visible ? own : handed`, so the next item takes back what reached the hidden one. An item before a hideable one keeps its own bottom margin (the hidden item would take it along) and the chain restarts after it.
- **Spacers.** A flex column's `Item { Layout.fillHeight: true }` spacer sits between two whole cell edges, so the carry passes it.
- **Runtime-created items.** A template's items cannot name siblings outside the component, so templates are not carried. Their `Text` and other items get `Layout.preferredHeight: implicitHeight`, which keeps the error under a pixel. The e2e test `tests/carry_e2e.rs` checks static and created items against Chromium.
- **Spacing.** Only whole `spacing` keeps the cell edges whole. A column with fractional or bound spacing is not carried.

**Not carried:**
- `GridLayout`. It does not round rows: its positions are fractional, and top margins behave differently (measured). A column inside a grid cell is still rounded once (09-forms: 65.59 → 66 per field).
- `RowLayout` and flex rows along x.

**Painting.** Exact positions leave boxes and lines on fractions of a pixel. Layout stays fractional; §1.2 covers how they paint.

### 1.2 Painting at fractional positions

Measured at `deviceScaleFactor: 1` by `tools/visual/paint/gen.mjs`, which writes `page.html`, `chromium.json` and `shifts.txt` there. Regenerating them leaves `git status` clean. `PaintSnapTests` in `tools/visual/test_visual.py` converts that page and compares it with the recording pixel by pixel on the default gl backend. The results below were measured in Chromium through Playwright and Qt Quick 6.11.

**Chromium**
- Positions are in 1/64 px, rounded down: `top: 100.49px` is 100.484375, and `100.51px` is 100.5.
- A box's painted edges are its exact edges rounded half up: top `round(y)`, bottom `round(y + h)`. This also holds for x. No row is partial.
- A border keeps its whole width inside those edges, so a 1 px border is one solid row at any fraction.
- A line's baseline is `round(ascent) + floor((line-height − round(ascent) − round(descent)) / 2)` below the line's top, a whole number. The glyph row is therefore the line's top rounded half up, plus that number.

**Qt Quick, boxes** (`Rectangle` with fill, or with `border.width` 1 or 2; 10 fractions)

| Rectangle | gl | software |
|---|---|---|
| No radius, `antialiasing` unset or false | Same pixels as Chromium, except below | Top edge and borders the same. A fractional height can leave the bottom edge a row low (`height: 20.25` at y = 50 ends at 71, Chromium's at 70) |
| No radius, `antialiasing: true` | Partial rows at both edges | Same as with it false |
| With a radius (`antialiasing` defaults to true) | Partial rows along every straight edge, as at y = 124.5 in 08-table | not measured separately |
| With a radius, `antialiasing: false` | Straight edges the same as Chromium, corners aliased | not measured separately |

The exception on gl: an edge at exactly x.5 in **x** takes the column whose centre is on the edge, one to the left of Chromium's (`left: 510.5px`). y.5 agrees. Fixing it would take a paint offset; it stays a known difference, pinned by its own test.

**Qt Quick, text.** A fixed-height line's baseline is the font's ascent, rounded, below the line's top. On gl the whole row follows the rounded position as in Chromium, so a whole `Translate` lands every probed line on Chromium's rows (6 fonts × 10 fractions). `shifts.txt` holds each font's numbers. On software 22 of those 60 lines are a row off in either direction, because it rounds differently. Only gl is matched.

**What the converter does**
- **Boxes without a radius** need no paint mechanism on gl, only their exact height. A box around a carried column (§1.1) is laid out at `Layout.preferredHeight: implicitHeight - <excess>`, and a fractional fixed height stays as it is. The carry keeps the cell whole. Before this, the excess made the bottom edge up to a row low (02-block-flow's hero, 187 vs 188).
- **Rounded boxes** (`emit/paint.rs`) bind `antialiasing` to "x, y, width and height are whole to 1/64 px". On whole boxes the corners stay smooth like Chromium's. On fractional ones the edges are crisp and the corners aliased. Boxes under a transform or a layer keep Qt's default.
- **Glyphs** (`emit/text.rs`): the `Translate` is Chromium's baseline less Qt's.
- **Render type** (`emit/paint.rs`): text asks for `Text.NativeRendering`. On a GPU Qt's default is distance fields, softer and lighter than Chromium's glyphs: the corpus scores 90.45 with this rule and 89.40 with none. Text under a `Scale`, `Rotation` or `Matrix4x4`, or anything with `scale` or `rotation`, keeps the default, as native glyphs are placed for unscaled pixels and come out with uneven spacing or jagged (20-pricing's featured card 86.21 vs 85.84). That includes a transform which is identity at rest, such as `:active { transform: scale(.97) }`, which costs a little (12-hover-states 92.91 vs 93.02). QML has no document-level setting, so it is per item. A Button's label is emitted as a `Text` to have the property; the score is the Basic style's `IconLabel` within 0.25 on every case. The runtime holds to the same rule for what a script creates: its element sources and text nodes ask for native glyphs, and an inserted subtree is checked once it is connected, when its ancestors are known. It reads their `rotation`, `scale` and `transform` as they are then, so a transform that is identity at the time keeps the item native, and it does not look again if an ancestor turns later.
- **RichText lines** (a run in another family, or with a background or padding) get the same whole `Translate`, and need Qt's baseline for such a line:
  - Chromium's line baseline is the largest `whole_baseline` among the strut and the runs, each at its own font and line height.
  - Qt's is `floor(lh − descent)`, `lh` in 1/64 px and `descent` the largest among the line's runs. When ascent + descent exceeds `lh`, the line grows and the baseline is the largest ascent, rounded. Measured for r1–r9 in `gen.mjs`; `rich.txt` holds both numbers.
  - Qt's row follows the rounded position as for plain lines (4 fractions, 9 lines).
  - A paragraph has one `Translate`, and it is the tallest run's: the corpus scored higher that way than with the block's font alone (26-docs-page 95.56 vs 93.72, 19-article 89.37 vs 88.54, 02-block-flow 95.56 vs 94.93; 01-typography goes the other way, 92.95 vs 93.38).
- **Table cells** (`emit/decoration.rs`): Chromium centres a `vertical-align: middle` cell's content exactly, and the glyph row is the rounded result like any line (`cN` in `gen.mjs`: 9 cells × 2 fractions). Qt's anchors round an odd height's centre up unless `anchors.alignWhenCentered: false` is set **on the anchored item and on its parent**, so both get it. The offset `(top inset − bottom inset) / 2` stays. A table's `ColumnLayout` with a caption is carried (§1.1), which keeps its rows on their exact y.

**Alternatives scored** (corpus mean on gl, from 90.19 before any rounded-box change):
- `antialiasing: false` on every rounded box: 90.35, but it aliases every corner and pill, including whole ones (20-pricing −0.37, 07-lists −0.18).
- The binding on x and y only: 90.25.
- The binding on all four, as chosen: 90.29.

Snapping a rounded box's position instead (`Translate` or margins) needs its scene position, which no binding can follow cheaply, and would have to spare its children.

**Known limits**
- A rounded box whole in a fractional parent looks whole to its own binding and stays soft on its edges.
- Faces whose real files are not on the measuring machine (§1.3) have no measured row: Qt's reported metrics stand in, scaled, so such glyphs can land a row off.
- Text a row off Chromium through layout costs more with native glyphs than with soft ones: 11-positioning, 15-script-counter, 16-script-todo and 29-class-toggles score 0.7–1.7 below distance fields, and in each the worst band matches Chromium best shifted down 1 px.
- A paragraph with a taller run (r9) goes through `EldurLines` now; its last line is a row low at y fractions .5 and .75, first row right. The cause is not worked out.
- A cell whose exact text position is half a pixel after a fractional height can land a row high (c9 at 695.5): Qt's sum comes out just under the half. Each case above is pinned in `PaintSnapTests.KNOWN_ROWS` or `richtext_lines_shift_to_chromiums_baseline`.
- A page whose table is already off through a non-carried flex or grid parent can lose a little: 18-dashboard −0.74, where the old half pixel partly cancelled its 2 px offset.
- Flex rows and grid cells round heights their own way (§1.1, not carried).

### 1.3 Fonts and `line-height: normal`

Converting never reads the machine's fonts, so the output and its cache key are the same everywhere; the running page decides, from the font Qt actually loaded (`eldur_fonts.js`, one table, also read by the converter).

**Family.** Qt gives a CSS name to fontconfig as written, and fontconfig's answer differs from what Chromium shows: here Helvetica is Nimbus Sans in Qt and Liberation Sans in Chromium. So `font.family` is `EldurFonts.family("<css stack>")`: each CSS name maps to a fixed ordered list, and the first one installed wins. A machine with none of them keeps Qt's own fallback. A name the table does not know is tried as written, as Chromium does; an unavailable one is skipped.

| CSS name | Reaches here (`CSS.getPlatformFontsForNode`) | Then |
|---|---|---|
| none given, Georgia alone, `cursive`, `fantasy`, `math`, `fangsong`, `emoji`, `ui-*` | Liberation Serif | Tinos, Noto Serif, DejaVu Serif |
| Helvetica, Arial, `-apple-system`, BlinkMacSystemFont, `system-ui`, `sans-serif` | Liberation Sans | Arimo, then the name itself or Noto Sans, DejaVu Sans |
| Times, Times New Roman, `serif` | Liberation Serif | Tinos, then the name itself or Noto Serif, DejaVu Serif |
| Courier, Courier New | Liberation Mono | Cousine, then the name itself |
| `monospace` | JetBrainsMono Nerd Font | Liberation Mono, Cousine, Noto Sans Mono, DejaVu Sans Mono |

`monospace` is this machine's fontconfig choice, not a browser's; it is kept so measurements here compare like with like. Measured with `tests/fonts/gen.mjs`, stacks included; `tests/fonts_e2e.rs` holds the running QML to the recorded `chromium.json`.

**The rule, measured (Chromium, 760 size/weight/family pairs).** With the face's hhea ascent, descent and line gap in em and the CSS size `s` (fractional, as Chromium keeps it):

```
A = round(ascent * s)    D = round(descent * s)    L = round(gap * s)
normal line      = A + D + L
inline box       = A + D
baseline, normal = A + floor(L / 2)                    from the line's top
baseline, own lh = A + floor((lh - A - D) / 2 + 1e-4)  lh in 1/64 px
```

**Why a table.** QML reports the OS/2 win metrics with no gap; hhea is not reachable from it. So a face's hhea numbers are measured and kept per face, and per style where they differ. A face not in the table uses Qt's `FontMetrics` through the same rounding, without a gap. There is no other per-font correction.

| Face | Ascent | Descent | Gap |
|---|---|---|---|
| Liberation Sans | 0.905273 | 0.211914 | 0.032715 |
| Liberation Serif | 0.891113 | 0.216309 | 0.042480 |
| Liberation Mono | 0.832520 | 0.300293 | 0 |
| Nimbus Sans (italic, bold italic 0.9 / 0.25) | 0.729 | 0.271 | 0.2 |
| Nimbus Roman (bold 0.676 / 0.324, bold italic 0.699 / 0.301) | 0.683 | 0.317 | 0.2 |
| Nimbus Mono PS | 0.603 | 0.397 | 0.2 |
| Nimbus Sans Narrow | 0.718 | 0.282 | 0.2 |
| Noto Sans, Noto Serif, Noto Sans Mono | 1.069 | 0.293 | 0 |
| JetBrainsMono Nerd Font | 1.02 | 0.3 | 0 |

**Fallback faces.** A character outside the ranges every listed face covers (the table's `covered`: Latin, Greek, Cyrillic, general punctuation, currency) may be drawn by another face, and in Chromium that face's ascent and descent, rounded and with its own half-leading, count in a `normal` line. So a `Text` whose text has such characters (`emit/fonts.rs has_strays`) passes its text to `EldurFonts.normalLine` and `shift`, which lay the strays out once in a `TextMetrics` and take its logical box: QML cannot ask a face whether it has a glyph, but the box reaches as far as the faces drawn in it. Generated text and runs on lines (`EldurLines`) do the same. A face that is not in the table has only Qt's metrics, so a row can still be a pixel off (a serif `✓`).

Arial, Times New Roman, Arimo, Tinos, Cousine and DejaVu are not installed where this was measured (Arimo is an alias reaching Liberation Sans), so they have no row.

**Why Nimbus Sans at 16 px is 19 in one and 22 in the other.** Chromium: `round(11.66) + round(4.34) + round(3.2)` = 12 + 4 + 3 = 19, box 16, baseline 13. Qt's natural height uses the win metrics, 17.19 + 4.78 = 21.97, so 22. The same split explains Liberation Sans at 24 px: Chromium's 22 + 5 + 1 = 28, Qt's 26.8 → 27, without the gap, which is the 1 px on an `h1`.

**In the output.** Every `Text` has a fixed line: `lineHeightMode: Text.FixedHeight`, `lineHeight` the CSS value or `EldurFonts.normalLine(font)`. Qt draws glyphs at a fixed line's top, so `Translate` moves them by `EldurFonts.shift(...)`, Chromium's baseline less Qt's rounded ascent. `Text.AlignVCenter` and `AlignBottom` centre by the font's natural height, not the line's, so a placed text adds its own remainder to the same `Translate`. A `RowLayout` lines up Qt baselines, so baseline-aligned items take one shift, the largest font's. `EldurFonts.slack` keeps an implicit height from falling short when the line is under the font's.

The converter's own numbers (`emit/fonts.rs`, `extents`, `normal_line`) decide layout choices only; what the page lays out comes from the bindings. Controls' `implicitHeight`s are such numbers and match Chromium in the table's faces.

**Not done:**
- `Text.RichText` keeps Qt's natural height for `normal`, with a literal shift.
- A literal `shift` argument does not follow a script's font change on a `Text` the runtime does not repaint. The runtime repaints those it sets (`script_font`).
- Grids do not give baseline-aligned items one shift; flex rows do.
- Width, same face: Qt is narrower by at most 0.16 px per 26 characters (12 px: 151.98 vs 152.09; 16 px: 202.63 vs 202.78; 96 px: equal). `renderType` and `PreferNoHinting` do not change it, `PreferFullHinting` is worse. No setting reaches it.
- Qt rounds a fractional size to a whole pixel, so a 13.33 px control's text is 13 px, widened by `text_scale`'s 13.33 / 13.

#### Inline runs: the line box, measured

The rule below reproduces Chromium's table in `crates/eldur_convert/tests/inline_runs/chromium.json` (196 cases, recorded by its `gen.mjs`). All sizes are CSS px at the run's own font.

- **A run's font.** `A = round(ascent·s)`, `D = round(descent·s)`, `G = round(gap·s)`. An inline element's rect is `A + D` high, with its top `A` above the run's baseline: the font's, not the line's.
- **Its line height.** `normal` is `A + D + G`. A number is `n ×` the run's own size, since the number is what inherits. A length, `%` or `em` is the parent's computed px. Floored to 1/64 px.
- **Its box.** `above = A + floor((lh − A − D) / 2 + 1e-4)`, `below = lh − above`. With `normal`: `above = A + floor(G/2)`, `below = D + ceil(G/2)`.
- **Its shift** (down positive, from the parent's baseline, cumulative, floored in magnitude to 1/64):

| `vertical-align` | Shift |
|---|---|
| `sub` | `+(parent size / 5 + 1)` |
| `super` | `−(parent size / 3 + 1)` |
| length | `−px` |
| `%` | `−` that share of the run's own line height |
| `middle` | `−xheight(parent) / 2 + (above − below) / 2` |
| `text-top` | `above − A(parent)` |
| `text-bottom` | `D(parent) − below` |

- **The line box** is `max(above − shift) + max(below + shift)` over the strut and every box on the line. That counts each ancestor inline box of the fragment, also one holding only a smaller run, and empty and whitespace-only spans in standards mode. A box counts only on lines where it has a fragment (four lines of one span: 16 + 23 + 23 + 16).
- **`top` and `bottom`** boxes form a group placed afterwards: the line grows if the group is taller, and the group sits at the line's top or bottom.
- **Bold and italic** do not change the line for Liberation or Noto, and do for Nimbus, so faces are compared by extents and not by name. A family alone can change the line.

Examples: `line-height: 1.5` at 14 px with a 24 px span is 36; `a<sub>2</sub>b<sup>3</sup>` at 16 px is 22.453125.

**Size of `monospace`** (`crates/eldur_css/tests/monospace_size/chromium.jsonl`, 49 cases). The generic family alone, not in a list, shrinks sizes that come from a keyword. Absolute keywords `xx-small…xxx-large` are 9, 10, 12, 13, 16, 20, 26, 39. Sizes derived from a keyword by `em`, `%`, `smaller`, `larger` are the unscaled size × 13/16. `px` and `rem` are exempt. Leaving for another family restores the unscaled size.

**Why not rich text** (measured in Qt 6.11, against Chromium's value):

| Content | `Text.RichText` | What is wrong with it |
|---|---|---|
| 14 + 20 px, wrapped to three lines | 55 | Line heights come from Qt's font metrics and not the rule above; plain text is 3 × 16 = 48 |
| `<div line-height:21px>` with a 24 px span, three lines | 69 (27 + 21 + 21) | Qt grows a tall line by its own natural height |
| `a<sub>2</sub>b<sup>3</sup>` at 16 px | 16 | Chromium's is 22.45: the shifts do not grow the line |

Qt's document layout has no per-line box of this kind and no fractional heights, so a paragraph whose runs differ in metrics goes through `EldurLines`.

**Cost.** Qt's compile of a document grows faster than the number of JavaScript bindings and enum assignments in it: 400 paragraphs of seven runs took 22 s cold when each run wrote its own, and 0.6 s now (0.5 s for the same page as one `Text` each). So what all runs share lives in `EldurRun.qml`, `eldurRun` is a JSON string, and a run's family is a literal that `EldurLines` resolves once.

**A run's own box.** Padding and border do not change the line box and are in the run's rect: left on the line it opens, right on the line it closes, top and bottom on every line. Generated content is in the rect of the element it belongs to, on whichever run it landed. `position: relative` moves a run where it is painted and in its rect, and does not grow the line. A span with horizontal padding is on `EldurLines` too: the joiner a `RichText` line would use made such a line 22 px where Chromium has 18.

## 2. Inline formatting context

**Decision:** one `Text` per inline formatting context, with `textFormat: Text.StyledText`, as long as its runs share one size, family and line height, and nothing moves one off the baseline; `<b>`, `<i>`, `<a>` stay in it. Otherwise the content is laid out by `EldurLines`, one `EldurRun` per run with its exact size (§1.3), never `<big>` or `<small>`. Content holding an atomic box (a replaced element, a control, `inline-block`, `inline-flex`) goes the same way: see below. On a page with scripts a change to a run (`style.fontSize`, a class, `textContent`) splits a `Text` into lines at that moment and gives the runs their boxes (`setRunLevels`); a font set on the block itself stays in place and keeps the models.

**Not done, runs:**
- Qt draws whole pixel sizes only, so a 13.33 px run is 13 px, and a long one drifts up to about 3% in width against its box (26 px over a 900 px line). Rects come from the CSS size where a run is short enough to agree; see the `KNOWN` list in `tests/inline_runs_e2e.rs`.
- A block adopted for another cause (a parent's font, a class causing `_gAdopt`) may still lose its inline models.
- `getClientRects` gives one rect for a `<q>` Chromium gives three (the quotes are generated) and misses a `<br>`'s empty rect; the bounding box and lines agree.
- A run in a fractional position is drawn there, blurred by Qt's filtering, while Chromium's glyphs look placed differently. Rounding or flooring the origin was measured and is worse over the corpus (`64-inline-runs` −0.65, `66-inline-runs-script` −1.26; `65-inline-text-elements` −1.39, `26-docs-page` −0.59).
- A `<abbr title>`'s dotted underline in a single `Text` is a solid `<u>`: tags cannot draw it.
- On a page with scripts and 400 such paragraphs, a first change to a run settles in 4 to 7 s against 1.9 s when these were one `Text` each. Most of it is `_gStaticChanges` walking every item at the settle, and each paragraph now has seven.

**Lines with boxes.** `EldurLines.qml` is one line layouter for static and script pages, written beside `Main.qml` when the page uses it or has a runtime. Its children are the content in order. Text is cut only at boxes: each run between two boxes stays one `Text`, shaped whole, so no ligature or kerning is lost at a word boundary, and `lineLaidOut` places that `Text`'s lines. The tradeoff is that a scriptless page with such a line now has JavaScript functions in QML, though not `eldur_runtime.js`. Heights follow CSS 2.2 §10.8 with Chromium's rounding: the strut is `round(ascent) + floor(leading / 2)` above and `round(descent) + ceil(leading / 2)` below, and `normal` is the rule in §1.3 (`EldurLines` reads it from `EldurFonts`). A box's baseline is its text's (rounded ascent), the bottom border edge for a check box, radio or slider, the bottom margin edge for a textarea or a box with no line. A line with nothing on it has no strut. The layouter runs a frame after a width change, because a table reads implicit sizes while working out that width; a script's read settles it first.

A run's edge space is the space CSS has there: Qt draws a trailing space in rich text and before a closing tag in styled text but drops a leading one, and `EldurLines` takes off only what Qt drew, then counts each collapsed space between runs once itself (`inline_code_e2e`). A one-line run in styled text whose markup only colours, links or decorates (`<font color>`, `<a href>`, `<u>`, `<s>`, no entities) is measured as its plain text in its own font, as a plain run is, plus the space Qt draws before a closing tag: Qt's laid-out width of such a run also carries the last glyph's ink overhang. A run's natural width is no more than its measured lines end to end, since once a narrower room has broken a word Qt's implicit width stays past the text's.

A justified line with boxes shares what it has left across its spaces, those between items and those in its text: a Text's one line by word spacing, the lines of a longer Text by Qt's own justification. **Not done:** the last line of a longer Text keeps its words' spacing when a box follows it on a justified line; the box itself is still placed as CSS says.

A created box's own inline children are on the same layouter: the runtime groups a box's children into runs and blocks at each layout and gives each run an anonymous `EldurLines`, which the DOM does not see. A flex or grid container's children leave their lines again. A block's anonymous static text becomes a line where a created box lands next to it, and is not a `Text` any more. A run's own `line-height` counts in its line's height. A press beside a run's short line is not on the run: its box is the bounding box of its lines, and hit testing asks the layouter which lines it covers.

**HTML/CSS**
```html
<p>Read the <strong>docs</strong> at <a href="https://x.example">the site</a>, it's <em>fast</em>.</p>
```
```css
p { line-height: 1.5; font-size: 16px; color: #222; }
a { color: #0066cc; }
```

**QML output**
```qml
Text {
    Layout.fillWidth: true
    wrapMode: Text.WordWrap
    textFormat: Text.StyledText
    font.pixelSize: 16
    color: "#222222"
    lineHeight: 1.5
    lineHeightMode: Text.ProportionalHeight
    linkColor: "#0066cc"
    text: "Read the <b>docs</b> at <a href=\"https://x.example\">the site</a>, it's <i>fast</i>."
    onLinkActivated: (link) => Qt.openUrlExternally(link)
}
```

| Feature | Support | Notes |
|---|---|---|
| `strong`/`b`, `em`/`i`, `u`, `s`/`del` | Full | StyledText subset. |
| `a href` | Full | `linkActivated` signal. Confirm before opening external URLs. `linkColor` colours every link in the `Text`, so per-link colours need split runs. |
| `span` with color | Full | `<font color="#rrggbb">`. |
| `span` with font-size | Approx | `<font size>` takes the 1–7 scale, not pixels. Use a separate `Text` run for exact sizes. |
| `font-family` on a span | Approx | `<font>` has no documented `face`; do not emit it. Use per-run `Text` for other families. |
| `line-height` | Full | `lineHeight` + `ProportionalHeight` (or `FixedHeight` for lengths). |
| `letter-spacing`, `word-spacing` | Full | `font.letterSpacing`, `font.wordSpacing`. |
| `white-space: pre` | Approx | `wrapMode: Text.NoWrap` with `textFormat: Text.PlainText`. Collapse whitespace for the normal modes at convert time, before emitting. |
| `text-align` | Full | `horizontalAlignment`. A line with boxes aligns as a whole; `justify` is not spread there. |
| `text-decoration` | Full | `<u>`, `<s>`, or `font.underline`/`font.strikeout` (whole `Text` only). |
| `text-shadow` | Approx | Second `Text` behind, offset by `x`/`y` in the shadow colour (blur is lost), or `style: Text.Raised` with `styleColor` for a 1px shadow. |
| Inline `img` | Full | `<img src="…" width="W" height="H" align="middle">` inside the StyledText. |
| `inline-block` | Full | A box on an `EldurLines` line, with the text around it in whole `Text` runs. |
| `vertical-align` | Full on a box, Approx on text | On `EldurLines`: `baseline`, `middle`, `top`, `bottom`, `text-top`, `text-bottom`, `sub`, `super`, lengths and percentages. |

**Pitfalls**
- StyledText is not HTML and its parser is strict: tags must close and nest. Escape `<`, `>`, `&` in text runs.
- Do not use `Text.RichText` for a whole page (excluded by the spec).

## 3. Flexbox

Base element: `RowLayout` (row) or `ColumnLayout` (column). Use `Flow` for `flex-wrap: wrap`. For cross-axis and main-axis alignment, use `Layout.alignment` on children and spacer `Item`s.

**Important:** items with `Layout.fillWidth` take extra space, so spacers and grow items compete. Non-growing items must not set `fillWidth`, so they keep their preferred size. In a `RowLayout`, an item without `Layout.alignment` and `fillHeight` is centred on the cross axis (measured), so always emit the alignment explicitly (`AlignTop` for the CSS default `stretch`-less cases).

**Example:** `display:flex; justify-content: space-between; align-items: center; gap: 8px;`
```qml
RowLayout {
    spacing: 0                                   // gap goes on the spacer, see §3.3
    Layout.fillWidth: true

    Text {
        text: "Brand"
        Layout.alignment: Qt.AlignVCenter        // align-items: center
    }
    Item {                                       // justify-content: space-between
        Layout.fillWidth: true
        Layout.minimumWidth: 8                   // gap: 8px
    }
    Button {
        text: "Menu"
        Layout.preferredWidth: 120               // flex: none; width: 120px
        Layout.alignment: Qt.AlignVCenter
    }
}
```

### 3.1 Direction

| CSS | QML | Support | Notes |
|---|---|---|---|
| `row` | `RowLayout` | Full | |
| `column` | `ColumnLayout` | Full | |
| `row-reverse` | `RowLayout { layoutDirection: Qt.RightToLeft }` | Full | `layoutDirection` is a `Layout` property: items keep DOM order (so focus order stays correct) and are placed from the right. It does not mirror text. |
| `column-reverse` | `ColumnLayout`, children emitted in reverse order | Emulated | `layoutDirection` is horizontal only. Tab and focus order also reverse. |

### 3.2 Wrap

| CSS | QML | Support | Notes |
|---|---|---|---|
| `nowrap` (default) | `RowLayout` | Full | |
| `wrap` | `Flow { spacing: N }` | Approx | `Flow` does not read `Layout.*`. Child sizes come from `width`/`implicitWidth`. Per-line grow and per-line alignment are not applied. Line spacing is uniform. Inside a Layout, give the `Flow` `Layout.fillWidth: true`; otherwise `width: parent.width`. |
| `column` + `wrap` | `Flow { flow: Flow.TopToBottom }` | Approx | Needs an explicit `height`. |
| `wrap-reverse` | `Flow`, reversed emission | Approx | Same limits as `wrap`. |
| `align-content` | Approx | Emulated with explicit `y` per line from a computed layout. |

### 3.3 `justify-content`

Spacers are empty `Item`s with `Layout.fillWidth: true` (or `fillHeight` in a column) and a `Layout.horizontalStretchFactor` (`verticalStretchFactor`). Spacers have preferred size 0, so equal stretch factors give equal shares of the free space.

| CSS | QML | Support | Notes |
|---|---|---|---|
| `flex-start` (default) | No spacer | Full | |
| `flex-end` | Leading spacer | Emulated | |
| `center` | Leading and trailing spacers, factor 1 each | Emulated | |
| `space-between` | One spacer between each pair of items, factor 1 | Emulated | |
| `space-around` | Edge spacers factor 1, interior spacers factor 2 | Emulated | Each edge gap is half of an interior gap. |
| `space-evenly` | All spacers factor 1 | Emulated | |

**Gap with spacers:** `spacing` is added around every item including spacers, which would double the gap. When spacers are emitted, set `spacing: 0`, give each interior spacer `Layout.minimumWidth: gap`, and give an item that directly follows another item (no spacer between) `Layout.leftMargin: gap`. Without spacers, `spacing: gap` is exact.

**Grow items:** when any item grows (`flex-grow > 0`), the free space is already consumed and `justify-content` has no effect in CSS. Emit no spacers.

### 3.4 `align-items` (cross axis)

Shown for a row; in a column swap to `fillWidth` and `AlignLeft` / `AlignHCenter` / `AlignRight`.

| CSS | QML | Support | Notes |
|---|---|---|---|
| `stretch` (default, auto height) | `Layout.fillHeight: true` | Full | Only when the child's height is auto. |
| `flex-start` | `Layout.alignment: Qt.AlignTop` | Full | |
| `center` | `Layout.alignment: Qt.AlignVCenter` | Full | |
| `flex-end` | `Layout.alignment: Qt.AlignBottom` | Full | |
| `baseline` | `Layout.alignment: Qt.AlignTop` + `Layout.topMargin: maxBaseline - item.baselineOffset` | Approx | `maxBaseline` is the max of sibling `baselineOffset`. Layouts do not align baselines natively. |

### 3.5 `flex` grow, shrink, basis

| CSS | QML | Support | Notes |
|---|---|---|---|
| `flex-basis: Npx` | `Layout.preferredWidth: N` | Full | |
| `flex-basis: 0` | `Layout.preferredWidth: 0` | Full | |
| `flex-grow: 1` / `flex: 1` | `Layout.fillWidth: true` | Full | |
| `flex-grow` ratios (1 : 2) | `Layout.fillWidth: true` + `Layout.horizontalStretchFactor: 1` / `2` | Approx | Exact only when all bases are equal (use `preferredWidth: 0`). Measured: bases 100 and 0 with factors 1 and 2 in 600px gave 171 and 429, not the CSS result. |
| `flex-shrink: 0` | `Layout.minimumWidth` = the basis | Full | |
| `flex-shrink: 1` (default) | Default `minimumWidth` (0) | Approx | Layout items shrink down to `minimumWidth`, which defaults to 0 for plain items (measured). Shrink ratios are not expressible. |
| `min-width: auto` on flex items | `Layout.minimumWidth: implicitWidth` | Approx | Not the default in QML. For wrapped `Text` the CSS min-content is the longest word, which is not available. |

`Layout.horizontalStretchFactor` and `verticalStretchFactor` exist in 6.11 (`int`, on `QQuickLayoutAttached`).

**Pitfalls**
- Using `Layout.fillWidth` on non-grow items causes them to stretch. Only set it on grow items.
- A `RowLayout` inside an auto-width parent has no definite width. Its implicit width is the sum of preferred widths, which may exceed the window. Bind its width.

## 4. CSS Grid

Base element: `GridLayout { columns: N; columnSpacing; rowSpacing }`. Child placement uses `Layout.row`, `Layout.column`, `Layout.rowSpan`, `Layout.columnSpan`. Emit explicit `Layout.row`/`Layout.column` whenever spans or `grid-template-areas` are used.

**HTML/CSS**
```css
.grid {
  display: grid;
  grid-template-columns: 200px 1fr 2fr;
  gap: 8px 16px;
}
.wide { grid-column: span 2; }
```

**QML output**
```qml
GridLayout {
    columns: 3
    columnSpacing: 16
    rowSpacing: 8
    Layout.fillWidth: true

    Rectangle {
        Layout.preferredWidth: 200
        Layout.minimumWidth: 200                // fixed track: pin all three
        Layout.maximumWidth: 200
        Layout.preferredHeight: 40
    }
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 0                // equal bases make the factors exact
        Layout.minimumWidth: 0                  // minmax(0, 1fr)
        Layout.horizontalStretchFactor: 1       // 1fr
        Layout.preferredHeight: 40
    }
    Rectangle {
        Layout.fillWidth: true
        Layout.preferredWidth: 0
        Layout.minimumWidth: 0
        Layout.horizontalStretchFactor: 2       // 2fr
        Layout.preferredHeight: 40
    }
    Rectangle {
        Layout.columnSpan: 2
        Layout.fillWidth: true
        Layout.preferredHeight: 40
    }
}
```

| Feature | Support | Notes |
|---|---|---|
| Fixed tracks (`px`) | Full | Pin `preferredWidth`, `minimumWidth` and `maximumWidth` on every item in the column, as the column takes the largest value. |
| `fr` tracks | Approx | `fillWidth` + `horizontalStretchFactor` with `preferredWidth: 0`. `minimumWidth` defaults to 0 (`minmax(0, 1fr)`); CSS `1fr` means `minmax(auto, 1fr)`, so emit `minimumWidth: implicitWidth` for non-text items to match. |
| `grid-column`, `grid-row` span | Full | `Layout.columnSpan`, `Layout.rowSpan`. |
| `gap` | Full | `columnSpacing`, `rowSpacing`. |
| `repeat(N, …)` | Full | Expand at convert time. |
| `repeat(auto-fill, minmax(a, 1fr))` | Approx | See §4.1. |
| `repeat(auto-fit, …)` | Approx | See §4.1. |
| `grid-template-areas` | Emulated | Convert to explicit row/column numbers. |
| `justify-items` / `align-items` | Approx | Per-cell `Layout.alignment`. |
| `grid-auto-flow: column` | Full | `flow: GridLayout.TopToBottom` with `rows: N`. |
| `grid-auto-flow: dense` | Unsupported | Warn. Items keep source order. |
| `subgrid` | Unsupported | Warn. |
| `minmax(min, max)` with max px | Approx | Clamp with `Layout.maximumWidth`. |

### 4.1 `repeat(auto-fill, minmax(200px, 1fr))`

Column count is computed from the grid's own width. Bind `width` to the parent (or `Layout.fillWidth: true` when the parent is a Layout), never to `implicitWidth`.

```qml
GridLayout {
    id: cards
    readonly property real minCell: 200
    readonly property int fitCols: Math.max(1, Math.floor((width + columnSpacing) / (minCell + columnSpacing)))
    width: parent.width
    columnSpacing: 12
    rowSpacing: 12
    uniformCellWidths: true
    columns: Math.min(Math.max(1, cardsRepeater.count), fitCols)   // auto-fit

    Repeater {
        id: cardsRepeater
        model: items                          // converter-emitted list
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 120
        }
    }
}
```

`auto-fit` collapses empty tracks, which is what the binding above does. `auto-fill` keeps empty tracks, so a partly filled single row keeps cell width `minCell`-ish instead of stretching. For `auto-fill`, use `columns: fitCols`, drop `fillWidth` and give items `Layout.preferredWidth: (cards.width - (cards.fitCols - 1) * cards.columnSpacing) / cards.fitCols`.

**Pitfalls**
- Column count changes during layout cause a relayout pass. Keep the binding simple.
- `width: parent.width` is required. `width: implicitWidth` creates a loop, because the layout's implicit width depends on column count.

## 5. Positioning

### 5.1 `position: relative`

```css
.nudge { position: relative; top: 4px; left: 8px; }
```
```qml
Item {
    transform: Translate { x: 8; y: 4 }
}
```

- `top` wins over `bottom`: `y = top` else `y = -bottom`. `left` wins over `right`: `x = left` else `x = -right` (left-to-right documents). Percent offsets resolve against the containing block.
- `transform` does not affect layout slots. The item keeps its layout position and is visually offset. This matches CSS relative positioning.

| Feature | Support | Notes |
|---|---|---|
| `top`/`left`/`right`/`bottom` offsets | Full | Via `Translate`. |
| Inside a Layout | Full | Offsets do not change the slot. |
| `z-index` | Approx | `z` orders siblings only. Stacking contexts must be flattened by the converter into sibling order. |

### 5.2 `position: absolute`

The containing block is the padding box of the nearest positioned or transformed or filtered ancestor; with none, it is the initial containing block (§5.2.1). In QML the child must be a direct child of an unlayouted `Item` that is that ancestor. If the ancestor is a `ColumnLayout` or `RowLayout`, the absolute child cannot be inside it, because the Layout would position it.

**HTML/CSS**
```html
<div class="card"><h2>Title</h2><span class="badge">3</span></div>
```
```css
.card  { position: relative; padding: 12px; }
.badge { position: absolute; top: 8px; right: 8px; width: 24px; height: 24px; }
```

**QML output**
```qml
Rectangle {                              // .card, plain Item, not a Layout
    id: card
    width: 300
    implicitHeight: col.implicitHeight + 24
    color: "#ffffff"

    ColumnLayout {                       // flow content of .card
        id: col
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 12
        Text { text: "Title"; font.pixelSize: 20; Layout.fillWidth: true }
    }

    Rectangle {                          // .badge, sibling of the layout, not inside it
        width: 24
        height: 24
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.topMargin: 8
        anchors.rightMargin: 8
        radius: 12
        color: "#d33"
        Text { anchors.centerIn: parent; text: "3"; color: "#fff" }
    }
}
```

**Re-parenting rule:** absolute children are removed from their source parent and emitted as siblings of the flow layout, inside the positioned ancestor. Only `top`/`bottom`/`left`/`right` set anchors. If only `left` and `top` are given, use `x`/`y`. If `width` is auto with both `left` and `right`, use `anchors.left` and `anchors.right` together. Offsets are measured from the padding edge: when the ancestor is a `Rectangle` with a border, add the border width to the anchor margins (a `Rectangle` draws its border inside its bounds).

| Feature | Support | Notes |
|---|---|---|
| Offsets with explicit size | Full | Anchors or `x`/`y`. |
| Auto size with `left` + `right` | Full | Anchors. |
| Auto size with only `left` or `top` | Approx | Size from `implicitWidth`/`implicitHeight` (CSS shrink-to-fit). At run time the width is min(max-content, max(min-content, available)): a `white-space: nowrap` tooltip in a narrow positioned span stays on one line past the span's width. |
| Nearest positioned ancestor is a Layout | Emulated | Wrap that Layout in a plain `Item` and overlay the absolute child. |
| `z-index` | Approx | `z`, see §5.1. |
| `position: static` | Full | No change. |

#### 5.2.1 Where a box goes

An out-of-flow box is emitted in its containing block however deep it is in the source. A wrapper that holds one takes it as a sibling of its flow layout. Without a positioned ancestor it goes to the initial containing block, an item only pages that use it get:

```qml
Flickable {
    id: flickable
    contentHeight: Math.max(page.implicitHeight, initialContainingBlock.childrenRect.y + initialContainingBlock.childrenRect.height)

    ColumnLayout { id: page; width: flickable.width }

    Item {                                 // scrolls with the page, unlike `fixed`
        id: initialContainingBlock
        width: flickable.width
        height: flickable.height
        Rectangle { x: 20; y: 10; width: 30; height: 40 }
    }
}
```

Boxes with a negative `z-index` go to a second such item, under the page.

**Static position.** An axis with neither offset uses where the box would have been in flow. Only then does it leave a slot, an empty `Item` at its place in the layout, and read it through the root's `staticX`/`staticY`, which sum `x` or `y` up the parents, so they follow layout. Margins before the slot are added; a page's boxes that leave no slot leave no trace. The slot takes no room, and margins collapse through it as through nothing. When an ancestor up to the containing block can be hidden (a script target, `display: none`), the box also binds `visible` to a slot.

An inline box that is absolute does not split its line. Its `y` is the top of the line it was on, its `x` the start of that line.

**Scripts.** A class or inline style changes offsets, `z-index` and `display` of a box in place, and static positions follow. A template of a created box declares `eldurBlock` (`"initial"`, `"parent"`, `"window"` or the id of the block) and the runtime moves the new item out of its DOM parent's layout. Below the initial block it goes straight in it; under another block it goes in an item bound to that block's place and size, so the block needs no room for it.

**The DOM of a moved box.** A box's QML parent says where it paints, not where it is in the DOM, so with a script on the page every out-of-flow box has a slot, its stand-in among its DOM siblings: `eldurSlot` on the box, `eldurBox` on the slot. A created box gets its slot from the runtime, and an item that shows it only while the slot's parent shows. Every DOM read and mutation goes through the slot: `children` and `parentNode`, `contains` and being in the document, and insertion, removal and `innerHTML`. Removing a box takes its slot out and hides it; putting it back shows it in the slot's new place. A slot beside a flex or grid container's items holds its DOM index in `eldurOrder`. A page without a script has no slots beyond those of §5.2's static position.

**Painting over what follows.** Every QML item is a stacking context, and of equal `z` the later sibling paints on top. So an in-flow box that is positioned, or holds an out-of-flow box, gets a `z` over its later siblings (`z-index` + 1 when it has one), and so does each ancestor up to the page. Measured in the `paint_*` pages of `tests/absolute/` by hit tests against Chromium.

| Case | Support | Notes |
|---|---|---|
| Any depth, initial or positioned containing block | Full | Measured against Chromium in `tests/absolute_e2e.rs`. |
| Transformed or filtered ancestor, `fixed` in one | Full | |
| Percentages against the padding box; `auto` margins centring | Full | |
| Absolute in a flex or grid container | Approx | Its slot is the container's start corner. Container alignment of the static position is not applied: warning. |
| Inline box with no `left` or `right`, after text | Approx | At the start of the line: warning. |
| Static position in a template (a created box without offsets) | Approx | At the containing block's start: warning. |
| Containing block that is a `<table>`, `<tr>` or `<fieldset>` | Approx | Placed against the next block out: warning. |
| Opacity of an ancestor up to the containing block | Not done | The box leaves that item: warning. |
| Stacking across containing blocks | Approx | `z` orders siblings only. A positioned box's overlay stays under the initial block's boxes, whatever its `z-index` (warning). |
| A class switching `position` between static and absolute | Not done | `dynamic-form` warning: worlds must share a shape, so it keeps its first form. |
| Overlays of different ancestors | Approx | Raising the ancestors orders them by those ancestors. A box inside a `z-index` context with a lower `z` than an outer overlay is right; two contexts crossing at several depths are not reordered. |
| A static `getElementById` or `querySelector` with a literal | Approx | Resolved at compile time to the item, so it still finds a box the script removed. A computed id goes through the runtime and is right. |
| A static box moved under another parent by a script | Approx | It keeps the containing block it was emitted in. |
| `id` selectors on a created box | Not done | The script sets the id, which the template's cascade cannot predict. A class works. |
| Removed created boxes | Leak | Kept, hidden, as a script may hold them and put them back. Creating and removing them in a loop grows the page. |
| A script setting `style.position` | Not done | The box stays in flow; the runtime warns on the console. A class on a created box works. |

### 5.3 `position: fixed`

Fixed elements are siblings of the ScrollView inside the `ApplicationWindow`; their `parent` is the window's `contentItem`, which is the containing block (the viewport). They do not scroll. A transformed or filtered ancestor is their containing block instead (§5.2.1).

```qml
ApplicationWindow {
    id: root
    width: 800
    height: 600

    ScrollView {
        anchors.fill: parent
        // page body
    }

    Rectangle {                              // position: fixed; bottom: 0; left: 0; right: 0
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 48
        color: "#222"
    }
}
```

| Feature | Support | Notes |
|---|---|---|
| Fixed to viewport | Full | Parent is the window's `contentItem`. Later siblings paint above the ScrollView. |
| Fixed inside a transformed ancestor | Emulated | In CSS an ancestor with `transform`, `filter` or `perspective` becomes the containing block of its fixed descendants. Treat it as `position: absolute` relative to that ancestor (§5.2). |

### 5.4 `position: sticky`

Sticky elements are emulated by a binding that clamps the natural position to the visible scroll position. `ScrollView.contentItem` is a `Flickable`; expose it typed:

```qml
ScrollView {
    id: scroll
    readonly property Flickable flick: contentItem as Flickable   // measured: works in 6.11
}
```

Parent is a plain `Item` (page-level sticky header):
```qml
Rectangle {
    id: stickyHead
    readonly property real naturalY: 200          // y of the element in the flow
    readonly property real stickTop: 0            // CSS top: 0
    readonly property real containerBottom: 1200  // bottom of the containing block
    y: Math.min(Math.max(naturalY, scroll.flick.contentY + stickTop), containerBottom - height)
}
```

Parent is a Layout (`y` is ignored there): keep the slot in the layout and move the content with `Translate`.
```qml
Item {                                            // slot: keeps the layout position and size
    id: slot
    Layout.fillWidth: true
    implicitHeight: head.height
    Rectangle {
        id: head
        width: parent.width
        height: 40
        transform: Translate {
            // the contentHeight reference re-runs the binding when the page reflows
            readonly property real naturalY: (scroll.flick.contentHeight, slot.mapToItem(scroll.flick.contentItem, 0, 0).y)
            y: Math.max(0, Math.min(scroll.flick.contentY + 0 - naturalY,           // CSS top: 0
                                    containerBottom - naturalY - head.height))
        }
    }
}
```

| Feature | Support | Notes |
|---|---|---|
| Stick to top within the scroll | Approx | Binding on `contentY`. |
| Constrained to containing block | Approx | `containerBottom` is computed at convert time; wrong if the content reflows. |
| `sticky` in a Layout | Approx | Slot + `Translate`, as above. |

**Pitfalls**
- Binding `y` on a Layout child is ignored. Sticky elements inside a Layout must use the slot + `Translate` form.
- Every sticky element adds a binding on `contentY`, which re-evaluates on every scroll step. Keep the number small.

### 5.5 `float`

`float` is a property, not a `position` value.

| CSS | QML | Support | Notes |
|---|---|---|---|
| `float: left/right` next to text | `RowLayout` of the float box and a text block (reverse with `layoutDirection` for `right`) | Emulated | Text wraps beside the float but not under it. Wrap-around past the float's bottom edge is not reproduced. |
| `clear: both` | End the float group | Full | The next block is a sibling after the `RowLayout`; no spacer is needed. |

The float group is an `Item` holding the floats' `Flow` over a column of the blocks beside them (`emit/flow.rs` `float_group`); the runtime's `_gLayoutFlow` does the same for floats a script makes. Both follow these rules, which were checked against Chromium (`rule_float_margins_static`, `rule_float_margins_created`, corpus 70-float-margins):

- **Margins collapse past floats.** A float is placed after the margins pending so far, and those margins still collapse with the next block's. The group takes the joined margin as its top margin, and its floats move back up by the part that came after them.
- **Floats that come first wait for escaping margins.** When the margins escape through the parent's top, the floats sit at the parent's top, beside its first child.
- **Clearance absorbs the margins.** A block that clears the floats has its border edge at their bottom, unless the margins above it already take it further down (CSS 2.2 §9.5.2). The group's height is the larger of the column plus those margins and the floats' bottom.
- **Every line in the context is cut.** That includes the lines of nested blocks, but not those of a box that starts its own formatting context. Each `Text` cuts its lines in `eldurCut(line)`, which maps the text into the floats' coordinates. The runtime's measuring twin calls the same function, so element rects match what is drawn. Line boxes (`EldurLines`) take the floats as their `exclusion`.

## 6. Overflow

| CSS | QML | Support | Notes |
|---|---|---|---|
| `overflow: visible` (default) | No clipping | Full | |
| `overflow: hidden` (rectangular) | `clip: true` on the box | Full | Clips the item and its children to the item's rectangle. |
| `overflow: hidden` with `border-radius` | `layer.enabled: true` + `layer.effect: MultiEffect { maskEnabled: true; maskSource: … }` with a rounded-rectangle mask | Emulated | `clip` alone is rectangular and ignores `radius`. Qt5Compat's `OpacityMask` is a separate module; do not use it. |
| `overflow: auto` | `ScrollView` (default `ScrollBar.AsNeeded`) | Full | Vertical or horizontal content. |
| `overflow: scroll` | `ScrollView` + `ScrollBar.vertical.policy: ScrollBar.AlwaysOn` (and `horizontal` if x scrolls) | Full | |
| `overflow-x: hidden` / `overflow-y: hidden` | Clip the box and set that axis' `ScrollBar.*.policy: ScrollBar.AlwaysOff` | Approx | |

**Rounded clip pattern**
```qml
Item {
    id: box
    width: 200
    height: 100
    layer.enabled: true
    layer.effect: MultiEffect {
        maskEnabled: true
        maskSource: boxMask
    }
    // overflowing content here
}
Item {
    id: boxMask
    width: box.width
    height: box.height
    layer.enabled: true
    visible: false
    Rectangle { width: parent.width; height: parent.height; radius: 8 }
}
```

**Scrolling container pattern**

```qml
ScrollView {
    id: codeScroll
    width: parent.width
    height: 100
    clip: true
    contentWidth: pre.implicitWidth
    contentHeight: pre.implicitHeight

    Text {
        id: pre
        text: codeSource
        font.family: "monospace"
        wrapMode: Text.NoWrap                 // white-space: pre
    }
}
```

**Page body:** the root `ScrollView` is the page's scrolling surface (§1). Nested `overflow: auto` boxes use their own `ScrollView`.

**Pitfalls**
- A `ScrollView` with exactly one child uses its implicit size as the content size. Set `contentWidth`/`contentHeight` explicitly when the child has no implicit size, when it is anchored, or when there are several children (the root page in §1).
- A rounded `Rectangle` background with `clip: true` still clips its children to the rectangle, not the rounded outline; use the mask pattern.
- Nested ScrollViews: the inner `Flickable` takes wheel events. Whether the outer one takes over at the inner boundary is not guaranteed; there is no emulation.

## 7. Box model

Each CSS box with padding, border, or background becomes a `Rectangle` (background, border, radius). Its content is an inner `ColumnLayout` or `RowLayout`, inset by padding plus border. A `Rectangle` draws its border inside its `width`/`height`, so `border-box` maps one to one.

**HTML/CSS**
```css
.card {
  box-sizing: border-box;
  width: 300px;
  padding: 12px 16px;
  border: 1px solid #ccc;
  border-radius: 8px;
  background: #fff;
}
```

**QML output**
```qml
Rectangle {
    width: 300                                    // border-box: includes padding and border
    implicitHeight: body.implicitHeight + 26      // 12 + 12 padding, 1 + 1 border
    color: "#ffffff"
    radius: 8
    border.width: 1
    border.color: "#cccccc"

    ColumnLayout {
        id: body
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.leftMargin: 17                    // 16 padding + 1 border
        anchors.rightMargin: 17
        anchors.topMargin: 13                     // 12 padding + 1 border
    }
}
```

### 7.1 Box sizing

| CSS | QML | Support | Notes |
|---|---|---|---|
| `box-sizing: border-box` | Rectangle `width` = CSS width | Full | |
| `box-sizing: content-box` (CSS default) | Rectangle `width` = content + left/right (padding + border) | Full | Computed at convert time. |
| `padding` | Inner layout inset via anchors or `Layout` margins | Full | |
| `margin` | Parent Layout margin or `x`/`y` | Full | See §1. |

### 7.2 Borders

| CSS | QML | Support | Notes |
|---|---|---|---|
| Uniform border | `border.width`, `border.color` | Full | |
| Uniform circular `border-radius` | `radius` | Full | |
| Per-corner circular `border-radius` | `topLeftRadius`, `topRightRadius`, `bottomLeftRadius`, `bottomRightRadius` on `Rectangle` | Full | Unset corners fall back to `radius`. |
| Elliptical radii (`10px / 20px`) | `Shape` + `ShapePath` with `PathArc` (`radiusX`/`radiusY`) | Approx | |
| Non-uniform border widths or colors | One `Rectangle` per side, anchored to edges, `height`/`width` = side width | Emulated | Use thin `Rectangle`s; corner joins are not mitred. |
| `border-style: dashed/dotted` | `Shape` with `ShapePath.strokeStyle: ShapePath.DashLine` and `dashPattern` (in multiples of `strokeWidth`) | Approx | Plain solid otherwise. |
| `outline` | Sibling `Rectangle` with `border` and `color: "transparent"`, anchored with negative margins by `outline-offset + width` | Approx | Takes no layout space, as in CSS. |

**Non-uniform border example:** `border-left: 4px solid #39f; border-bottom: 1px solid #ccc`
```qml
Rectangle {
    color: "#ffffff"
    Rectangle {                                   // border-left
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 4
        color: "#3399ff"
    }
    Rectangle {                                   // border-bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 1
        color: "#cccccc"
    }
}
```

### 7.3 Backgrounds

| CSS | QML | Support | Notes |
|---|---|---|---|
| `background-color` | `Rectangle.color` | Full | |
| `background-image` (url) | Child `Image`, `anchors.fill: parent` | Full | |
| `background-size: cover` | `Image.fillMode: Image.PreserveAspectCrop` + `clip: true` | Full | Without `clip` the crop overflows the box. |
| `background-size: contain` | `Image.fillMode: Image.PreserveAspectFit` | Full | |
| `background-size: Npx` | `Image` with explicit `width`/`height` | Approx | Position with `horizontalAlignment`/`verticalAlignment` or `x`/`y`. |
| `background-repeat` | `Image.fillMode`: `Tile`, `TileHorizontally` (`repeat-x`), `TileVertically` (`repeat-y`) | Approx | Tiles use the natural size; origin and spacing may differ. |
| `linear-gradient` (`to bottom` / `to right`) | `Rectangle.gradient: Gradient { orientation: Gradient.Vertical / Horizontal; GradientStop … }` | Full | Follows the rectangle's `radius`. |
| `linear-gradient(angle)` | `Shape` + `ShapePath.fillGradient: LinearGradient { x1; y1; x2; y2 }` | Approx | Compute the endpoints from the angle and box size. Radius needs a rounded `PathRectangle`. |
| `radial-gradient` | `Shape` + `RadialGradient { centerX; centerY; centerRadius; focalX; focalY }` | Approx | Only circular. |
| `conic-gradient` | `Shape` + `ConicalGradient` | Approx | |
| Multiple backgrounds | Stacked `Rectangle`/`Image` children | Emulated | |

### 7.4 Shadows and effects

| CSS | QML | Support | Notes |
|---|---|---|---|
| `box-shadow` (no spread) | `layer.enabled: true` + `layer.effect: MultiEffect { shadowEnabled; shadowColor; shadowHorizontalOffset; shadowVerticalOffset; shadowBlur }` | Approx | `shadowBlur` is normalised 0..1 of `blurMax` (default 32); map `min(1, cssBlur / blurMax)`. |
| `box-shadow` with spread | Same, with `shadowScale` ≈ `(size + 2·spread) / size`; for blur 0, a `Rectangle` behind the box enlarged by spread | Approx | |
| `inset` shadow | Unsupported | Warn. |
| `opacity` | `Item.opacity` | Approx | `opacity` multiplies per item; overlapping children differ from CSS group opacity. Add `layer.enabled: true` when the box has overlapping children. |
| `filter: blur()` | `layer.effect: MultiEffect { blurEnabled: true; blur; blurMax }` | Approx | |
| `transform: rotate/scale` | `rotation` / `scale` with `transformOrigin: Item.Center` (default) | Full | Matches CSS `transform-origin: 50% 50%`. Use `transform: Rotation { origin.x; origin.y }` for other origins or several transforms. |
| `mix-blend-mode` | Unsupported | Warn. |

**Pitfalls**
- Percent padding is relative to containing block width (CSS) on all four sides. Compute at convert time or bind to the parent width.
- A Layout that is a child of another Layout must not carry `anchors`; use `Layout.*` there (see §12). A Layout inside a plain `Rectangle` may be anchored.

## 8. Tables

**Decision:** `GridLayout` with explicit `Layout.row`/`column`/`rowSpan`/`columnSpan`. `TableView` is not emitted: it has no spans. `GridLayout` cannot size a table by itself (stretch factors are not proportional, fractional rows are rounded up one by one, and preferred sizes bound to its own `implicitWidth` re-polish without end), so column widths and row heights come from `table*` functions emitted once on the root, only on pages with tables. Each cell declares `tableSlot` and `minContentWidth` for them.

**HTML/CSS**
```html
<table>
  <thead><tr><th>Name</th><th>Qty</th></tr></thead>
  <tbody><tr><td colspan="2">Total: 3</td></tr></tbody>
</table>
```
```css
th { font-weight: bold; background: #eee; }
td, th { padding: 4px 8px; border: 1px solid #ccc; }
```

**QML output**
```qml
GridLayout {
    id: tableGrid
    columns: 2
    columnSpacing: 0
    rowSpacing: 0
    readonly property var columnSpecs: [null, null]
    readonly property var columnWidths: root.tableColumnWidths(this, columnSpecs, false)
    readonly property var rowHeights: root.tableRowHeights(this, [0, 0])

    Rectangle {
        readonly property var tableSlot: [0, 0, 1, 1]
        readonly property real minContentWidth: tableFont.advanceWidth("Name") + 16
        Layout.row: 0
        Layout.column: 0
        Layout.preferredWidth: tableGrid.columnWidths[0]
        Layout.preferredHeight: tableGrid.rowHeights[0]
        // Text, anchored by `vertical-align`
    }
}
```

| Feature | Support | Notes |
|---|---|---|
| Anonymous table boxes (CSS 2.1 §17.2.1) | Full | Stray cells and rows get their missing parents; whitespace between parts makes nothing. |
| `colspan` / `rowspan` | Full | HTML forming algorithm; a rowspan stops at its row group's end, `rowspan=0` runs to it. |
| Auto column widths | Approx | Chromium's auto layout, measured (see *Auto column widths* below); columns land within a pixel of Chromium's. Min-content is the widest word, measured with `FontMetrics` over the few words an em estimate puts near the widest. Spanning cells are checked for one span only, and a `Text`'s implicit width counts a last glyph's overhang, which a browser does not. |
| `table-layout: fixed` | Emulated | `<col>` widths and the first row; the rest shared equally. |
| `<col>` / `<colgroup>` widths | Full | `span` and the `width` attribute included. A `width` on a spanning cell is ignored, with a diagnostic. |
| `border-collapse: collapse` | Approx | CSS 2.1 §17.6.2.1 picks each edge; a cell draws its half. A spanning cell's side uses the widest segment along it, so differing segments are not reproduced. Border styles other than solid draw solid. |
| `border-collapse: separate`, `border-spacing` | Full | `columnSpacing`, `rowSpacing`, and insets for the outer edges. |
| `thead`/`tbody`/`tfoot` | Full | First `thead` first, `tfoot` last, in any source order. |
| `caption`, `caption-side` | Full | An outer `ColumnLayout` holding captions and the table. |
| Row, group and column backgrounds | Emulated | Items cannot overlap in a `GridLayout`, so each cell paints them, flattened source-over. Background images on rows and groups are reported. |
| `vertical-align` in cells | Approx | `top`, `middle`, `bottom`; `baseline` is the top. |
| Scripts targeting static rows or groups | Live form | A table whose page has a table word in a script, or whose part a script wants, is a `ColumnLayout` of section `ColumnLayout`s and row `Item`s, with cells placed by the runtime through the same solver; `columnWidths` are written, not bound. Other tables keep the `GridLayout`. |
| Rows and tables a script makes | Same solver | The runtime places their slots and passes their cells to the page's `tableColumnWidths` and `tableRowHeights`. In a static table the grid counts them through `eldurCreatedCells`, so its header moves with them. They cannot add a column a static table does not have. |
| `visibility: collapse`, row `opacity` | Unsupported | Reported. |
| `empty-cells`, presentational attributes (`cellspacing`, `bgcolor`, …) | Unsupported | Not read. |
| Sticky `th` | Approx | See §5.4. |

**Auto column widths**, as measured in Chromium (`crates/eldur_convert/tests/table_widths/`), and as `tableColumnWidths` solves them:

- A column's minimum is its cells' min-content border-box width; a width in px or % does not raise it, so a table narrower than its px columns shrinks them. Its maximum is max-content, at least the minimum, and at least a px width. A content-box width adds padding and borders; a border-box width is the column width. A spanning cell's shortfall is shared over its columns in proportion to their maxima.
- Percentages add up to at most 100% in column order; a column past the cap is auto.
- Four guesses: every column at its minimum; then percent columns at their share of the width (at least their minimum); then px columns at their maximum; then auto columns at their maximum. A width between two guesses moves the columns that differ linearly, in proportion to how much they differ.
- Width past the last guess goes to auto columns with content, in proportion to their maxima; failing those, to px columns by width; then to percent columns by percentage; then to all columns equally.
- A table with `width: auto` is the sum of the maxima, or wider where percent columns need it: the other columns' maxima over the percent left over, and each percent column's maximum over its percentage. Its min-content width is the sum of the minima.
- Collapsed, the table's box holds half of each outer border and a column reaches the middle of the borders beside it. Cells here draw the outer borders whole and split an inner one with the odd pixel on the right, so the grid declares `tableEdges`, how far the browser's column edge lies right of the drawn one at each boundary, and the solver works in the browser's frame.
- Columns are rounded to whole pixels at cumulative edges. A column given its maximum is rounded up when its text can wrap (its maximum is over a pixel past its minimum), since a `Text` wraps in a box a fraction narrower than it, and the table's max-content width is at least those whole widths. A single word cannot wrap, so its column rounds to the nearest pixel and may overhang by a fraction, as a browser's pixel-snapped one does.
- `table-layout: fixed` keeps the specified widths and shares what is left equally among auto columns.

**Pitfalls**
- Cells with wrapped `Text` have a minimum width of 0 in a `GridLayout`, so they can be squeezed to nothing. Set `Layout.minimumWidth` to the longest-word width or the implicit width of unwrapped single-line cells.
- `TableView` is a `Flickable` and needs an explicit size. `columnWidthProvider` and `rowHeightProvider` are JS functions `function(index)` returning a real width/height (`-1` means "use the implicit size", `0` hides). Its model is `TableModel` with `TableModelColumn` from `import Qt.labs.qmlmodels`. That is a Qt Labs module, so confirm it is deployed; or supply a C++/`ListModel`-style `QAbstractItemModel`.

## 9. Replaced elements

**HTML/CSS**
```html
<img src="hero.jpg" width="400" height="200" style="object-fit: cover">
```

**QML output**
```qml
Image {
    source: "hero.jpg"
    width: 400
    height: 200
    fillMode: Image.PreserveAspectCrop
    clip: true                               // cover overflows the item otherwise
    asynchronous: true
}
```

If only `width` is given (height auto), derive height from the natural aspect ratio after the image loads:
```qml
Image {
    source: "hero.jpg"
    width: 400
    height: status === Image.Ready ? width * implicitHeight / implicitWidth : 0
    fillMode: Image.PreserveAspectFit
    sourceSize.width: width * Screen.devicePixelRatio   // decode hint, aspect ratio preserved
}
```

| CSS | QML | Support | Notes |
|---|---|---|---|
| `width`/`height` on `img` | `Image.width` / `height` | Full | |
| Auto height from aspect | Binding on `Image.Ready` | Approx | Zero height until loaded. |
| `object-fit: fill` | `fillMode: Image.Stretch` | Full | |
| `object-fit: contain` | `fillMode: Image.PreserveAspectFit` | Full | |
| `object-fit: cover` | `fillMode: Image.PreserveAspectCrop` + `clip: true` | Full | |
| `object-fit: none` | `fillMode: Image.Pad` + `clip: true` | Approx | Natural size. |
| `object-fit: scale-down` | `PreserveAspectFit` with `min(natural, box)` | Approx | |
| `object-position` | `Image.horizontalAlignment` / `verticalAlignment` | Approx | Only left/center/right and top/center/bottom. |
| `aspect-ratio: w / h` | `height: width / ratio` | Full | |
| `srcset` / `picture` | Choose one candidate at convert time | Approx | |
| `alt` text | `Accessible.name`, or `Text` fallback when `status === Image.Error` | Approx | |
| SVG `img` | `Image` | Full | Needs Qt's SVG image-format plugin at runtime; set `sourceSize` so it rasterises crisply. |
| `canvas`, `video`, `iframe` | Unsupported | Warn. | |

**Pitfalls**
- `sourceSize` must be at least display size × device pixel ratio for crisp images. With `PreserveAspectCrop`, do not set both dimensions to the box size; the decode would be smaller than the covering size. Set only the dimension that guarantees cover, or omit it.
- `implicitWidth`/`implicitHeight` are 0 until `status` is `Image.Ready` (or when `sourceSize` is set, they follow it); guard the ratio binding.

## 10. Percent sizing, viewport units, min/max

| CSS | QML | Support | Notes |
|---|---|---|---|
| `width: 50%` (block container, used width known) | `width: parent.width * 0.5` (plain Item) or `Layout.preferredWidth: parent.width * 0.5` | Full | Percent of the containing block's content width: subtract its padding. |
| `width: 50%` (shrink-to-fit container) | `auto` (no binding) | Approx | CSS sizes the container ignoring the percentage, then resolves it. Emitting a binding loops. |
| `height: 50%` (containing block height auto) | `auto` (no binding) | Full | CSS computes it as auto. |
| `height: 50%` (definite height) | `height: parent.height * 0.5` | Full | |
| `width: 50vw` | `width: root.width * 0.5` | Full | `root` is the ApplicationWindow id. |
| `height: 50vh` | `height: root.height * 0.5` | Full | |
| `vmin` / `vmax` | `Math.min(root.width, root.height) * n` | Full | `Math.max` for `vmax`. |
| `min-width` / `max-width` (Layout child) | `Layout.minimumWidth` / `Layout.maximumWidth` | Full | |
| `min-width` / `max-width` (plain Item) | `width: Math.max(min, Math.min(max, value))` | Full | `min` wins over `max`, as in CSS. |
| `min-height` / `max-height` (Layout child) | `Layout.minimumHeight` / `Layout.maximumHeight` | Full | |
| `aspect-ratio: w / h` | `height: width / ratio` | Full | |
| `calc()` | Compile to QML expression | Full | |
| `clamp(min, v, max)` | `Math.max(min, Math.min(max, v))` | Full | |
| `ch` | `TextMetrics { text: "0"; font: … }.width` | Approx | Width of the "0" glyph. |
| `ex` | `FontMetrics { font: … }.xHeight` | Approx | |
| `rem` / `em` | Resolve at convert time | Full | |

**Example:** `.hero { width: 50%; min-width: 200px; max-width: 480px; height: 50vh; }` inside a definite-width container
```qml
Rectangle {
    Layout.preferredWidth: parent.width * 0.5
    Layout.minimumWidth: 200
    Layout.maximumWidth: 480
    Layout.preferredHeight: root.height * 0.5
    color: "#eee"
}
```

**Pitfalls**
- **Percent width in a shrink-to-fit container creates a binding loop.** The container's implicit width depends on children's preferred width, which depends on the container's width. Detect at convert time: if the nearest ancestor's width is not definite (inline-block, float, auto-width absolute or flex item), emit `auto`.
- `Layout.minimumWidth` is not read by plain Items. Use the explicit `Math.max` form there.

## 11. Media queries

Each `@media` condition becomes a `readonly property bool` on the window. Prefer direct bindings: they compose, so overlapping queries follow the CSS cascade. A `State` is only safe when the queries are mutually exclusive: **only one `State` is active at a time and the first one in the list whose `when` is true wins** (measured), so states do not layer and source order does not make a later one override an earlier one.

`State`/`states` exist on `Item`, not on `ApplicationWindow`; put them on an `Item` inside the window.

**HTML/CSS**
```css
.sidebar { width: 240px; }
.grid    { grid-template-columns: repeat(3, 1fr); }

@media (max-width: 600px) {
  .sidebar { display: none; }
  .grid    { grid-template-columns: 1fr; }
}
```

**QML output**
```qml
ApplicationWindow {
    id: root
    width: 800
    height: 600
    readonly property bool narrow: width <= 600

    RowLayout {
        anchors.fill: parent

        Rectangle {
            id: sidebar
            Layout.preferredWidth: 240
            Layout.fillHeight: true
            visible: !root.narrow                       // display: none, removed from the layout
        }
        GridLayout {
            id: cards
            Layout.fillWidth: true
            columns: root.narrow ? 1 : 3                // later rule wins: later condition first
        }
    }
}
```

When one query changes many properties of an item and no other query overlaps, a `State` with `PropertyChanges` on an `Item` is acceptable:
```qml
Item {
    id: page
    states: State {
        name: "narrow"
        when: root.narrow
        PropertyChanges { target: cards; columns: 1 }
    }
}
```

| Media feature | QML | Support | Notes |
|---|---|---|---|
| `min-width: N` | `root.width >= N` | Full | Inclusive, as in CSS. |
| `max-width: N` | `root.width <= N` | Full | Inclusive, as in CSS. |
| `min-width` + `max-width` | `root.width >= A && root.width <= B` | Full | |
| `orientation: landscape` | `root.width > root.height` | Full | |
| `orientation: portrait` | `root.width <= root.height` | Full | CSS portrait is height ≥ width. |
| `prefers-color-scheme: dark` | `Qt.styleHints.colorScheme === Qt.Dark` | Full | Re-evaluated on change (measured: resolves in 6.11). |
| `prefers-reduced-motion` | Unsupported | Warn. | No reliable Qt query. |
| `hover: hover` / `pointer: fine` | Approx | Assume mouse on desktop targets. | |
| `print` | Ignored | | Only screen output is emitted. |
| Nested or `and` / `or` / `not` | Full | Compile to a boolean expression. | |

**Ordering:** several rules setting the same property compile to one conditional expression, last matching rule first: `root.w1 ? a : root.w2 ? b : c`.

**Pitfalls**
- `State.when` is evaluated continuously. It must not depend on values the state itself changes, or it oscillates.
- `PropertyChanges` on a property that is also bound elsewhere replaces the binding while the state is active and restores it on exit; test for stale values.
- Changing `visible` inside a Layout removes the child from layout calculations. This is the intended `display: none` behavior.

## 12. Cross-cutting pitfalls

| Pitfall | Cause | Mitigation |
|---|---|---|
| Layout ignores `anchors` on direct children | Qt Layouts take over positioning (warning logged) | Anchor only children whose parent is not a Layout. Use a wrapper `Item` otherwise. |
| Layout ignores `x`/`y` on children | Same | Use `transform: Translate` for offsets. |
| Binding loop: percent in a shrink-to-fit container | Circular size dependency | §10 rule: emit `auto`. |
| Binding loop: width from `implicitWidth` in a Layout that sets it | Circular | Bind width to the parent, not to implicit size. |
| Unbounded ColumnLayout width in ScrollView | Implicit width = longest line | Bind width to `scroll.availableWidth`. |
| Default cross-axis alignment is centred in `RowLayout` | Qt Layout default | Always emit `Layout.alignment` or `fillHeight` (§3). |
| Default `Layout.minimumWidth` is 0 | Qt Layout default | Emit `minimumWidth` for `flex-shrink: 0`, `min-width: auto`, wrapped table cells. |
| Relayout storm on resize | Many grid column recalculations | Keep column bindings simple; avoid per-item `width` bindings to the window. |
| `states` on `ApplicationWindow` | `Window` is not an `Item` | Use direct bindings or an inner `Item`. |
| Focus order changed by reversal | Reversed emission (`column-reverse`, `wrap-reverse`) | Document as Approx; `row-reverse` keeps DOM order via `layoutDirection`. |

## Review notes

- Removed the `(verify)` marker list and the appendix. Verified against the 6.11 `.qmltypes`: `Text.linkColor`, `lineHeightMode`, `Layout.horizontalStretchFactor`/`verticalStretchFactor`, `GridLayout.uniformCellWidths`, `Rectangle` per-corner radii, `Image.horizontalAlignment`/`verticalAlignment`, `ShapePath.strokeStyle`/`dashPattern`, `LinearGradient`/`RadialGradient`/`ConicalGradient`, `MultiEffect` shadow/blur/mask properties, `TableView` providers.
- §1: the `.page` example used `width: 600` with 16px padding under the default `content-box`; the border box is 632px. Dropped the redundant `Layout.bottomMargin: 0` and the `contentHeight` workaround for wrapped `Text` (measured unnecessary).
- §1: negative margins are not "Layout margins ≥ 0"; reworded as a visual-only `Translate` approximation. Centering with `margin: 0 auto` no longer needs `maximumWidth`.
- §2: `<font size>` is the 1–7 scale and `<font face>` is not documented; both are no longer promised. Inline `img` now maps to the StyledText `<img>` tag instead of a per-word `Flow`. `text-shadow` is Approx, not "Unsupported" with a contradictory workaround.
- §2: `white-space` collapsing is done at convert time; `linkColor` is per-`Text`.
- §3.1: `row-reverse` is now `layoutDirection: Qt.RightToLeft` on `RowLayout` (a `Layout` property, DOM order preserved). The old claim that it mirrors text was wrong.
- §3.2: added `Flow.TopToBottom` for column wrap, and the `Layout.fillWidth` rule for a `Flow` inside a Layout.
- §3.3: `spacing` is applied around spacers and doubled the gap; now `spacing: 0` with gap on spacer `minimumWidth`/item margins. The "set spacer factor to 0 when a grow item exists" rule is replaced by "emit no spacers", which is the CSS behaviour.
- §3 intro/§3.4/§12: measured that `RowLayout` centres non-fill items on the cross axis by default; explicit alignment is required.
- §3.5: measured that stretch factors do not split "above preferred" as claimed (171/429 for 1:2 with bases 100/0); exact only with equal bases. `flex-shrink: 0` maps to `minimumWidth = basis`. Default `minimumWidth` is 0, not the implicit size.
- §4: fixed tracks need `min`/`max` pinned; `fr` items get `preferredWidth: 0`; `model.count` in the auto-fit binding was invalid and is now `Repeater.count`; `auto-fill` vs `auto-fit` explained correctly; added `grid-auto-flow: column`.
- §5.1/5.2: `z` only orders siblings, so `z-index` is Approx. Absolute offsets are from the padding edge (border width added). Removed the bogus `position: float` row.
- §5.3: a transformed ancestor becomes the containing block for fixed descendants in CSS; the old "lift out of the subtree" advice was wrong.
- §5.4: `scroll.contentItem` is typed `Item`; added the `as Flickable` form (measured). Added a Layout-safe slot + `Translate` form. Dropped the unfounded "lags one frame" claim.
- §5.5: the float/clear table had broken columns; `clear` needs no spacer since the float group is a separate `RowLayout`.
- §6: replaced Qt5Compat `OpacityMask` with `MultiEffect` `maskEnabled`/`maskSource` (checked with qmllint and qml6). Corrected the wrong claim that `clip` leaves the item's own background unclipped. A single-child `ScrollView` uses its implicit size, so "unbound contentHeight cannot scroll" was wrong. Added `overflow: scroll` vs `auto` scrollbar policies.
- §7: `Rectangle` draws its border inside its bounds, resolving the inset question. Per-corner radii are native, not `Shape`-only. `cover` needs `clip: true`; `Gradient.orientation` covers horizontal gradients. `box-shadow` mapping uses real `MultiEffect` names and the 0..1 `shadowBlur` scale; `opacity` is per item; `transform` uses `rotation`/`scale` with the centre origin.
- §7 pitfalls: "a Layout inside a Rectangle must not carry anchors (§10)" was wrong; the rule concerns a Layout inside a Layout.
- §8: `tfoot` renders last; wrapped cells collapse to minimum width 0 (not single-line cells); `TableModel` is in `Qt.labs.qmlmodels`; provider signatures are `function(index)`.
- §9: `sourceSize` with both dimensions breaks `PreserveAspectCrop`; replaced it with `sourceSize.width` and added `clip: true`. SVG needs the runtime image-format plugin.
- §10: percent `width` against a block container is definite; only shrink-to-fit containers are cyclic. `aspect-ratio` was inverted in one place (`width * ratio`); now `width / ratio` everywhere. `ch`/`ex` use `TextMetrics`/`FontMetrics.xHeight`. Removed a vague "double updates" pitfall.
- §11: `ApplicationWindow` has no `states` (measured load error). Only one `State` is active, first match wins (measured), so "compile in source order so later wins" was wrong; direct conditional bindings are now the default. Fixed the portrait/landscape boundary and the layout-less `Layout.preferredWidth` sample.
