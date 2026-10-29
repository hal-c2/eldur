# CSS property mapping

Target: Qt 6.11. Modules: QtQuick, QtQuick.Controls (Basic style), QtQuick.Layouts (including `FlexboxLayout`, added in 6.10), QtQuick.Shapes, and QtQuick.Effects (`MultiEffect`, `RectangularShadow`). `QtQuick.Timeline` is a separate module that is not part of the baseline install, so generated code must not depend on it. Every QML type, property and enum named below was checked against the Qt 6.11 `.qmltypes` files.

**Conventions**
- `parent` is the QML parent item. `Window` is the attached `Window` group (`Window.width`, `Window.height`, `Window.contentItem`). Both are `int` and only valid once the item is in a window; a generated root `Window` can expose `root.width` instead.
- "Unit conversion" gives the factor applied to the CSS value to get the QML value.
- Children of `RowLayout`, `ColumnLayout`, `GridLayout` and `FlexboxLayout` are placed by `Layout.*` (and `FlexboxLayout.alignSelf`); `x`/`y`/`anchors` on them are ignored. Children of the positioners `Row`, `Column`, `Grid` and `Flow` have `x`/`y` overwritten, and `Layout.*` has no effect on them.
- QML does not inherit `font` or `color` from parent items into `Text`. The converter resolves CSS inheritance and writes every value on each `Text`.
- Set `textFormat: Text.PlainText` on every `Text` that is not emitted with markup. The default `AutoText` would parse a `<` in page text as rich text.

**Support levels**

| Level | Meaning |
|---|---|
| Full | Same visible result from a direct QML property or construct |
| Approx | Visible result close but not identical |
| Emulated | Needs generated helper code (bindings, extra items, shaders) |
| Ignored | No visual effect; safe to drop |
| Unsupported | Not representable; the converter emits a warning |

---

## Box model & sizing

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| width, height | px, %, auto, calc() | `Item.width` / `Item.height`; `Layout.preferredWidth` / `preferredHeight` in layouts | px ×1; % → `parent.width * p / 100` | Full | `auto` on a block-level box fills the containing block (minus margins). `auto` on inline-block, flex/grid items, floats and absolutely positioned boxes uses `implicitWidth` / `implicitHeight` (shrink-to-fit) |
| min-width, max-width, min-height, max-height | px, %, none | `Layout.minimumWidth` / `Layout.maximumWidth` (and `Height`) in layouts; otherwise `width: Math.min(max, Math.max(min, expr))` | px ×1 | Full (layouts) / Emulated (absolute) | Plain items have no min/max properties |
| width / height: fit-content, min-content, max-content | keywords | `implicitWidth` / `implicitHeight` | — | Approx | Measured from children; no exact intrinsic sizing |
| margin (all sides) | px, %, auto, negative | `Layout.margins`, `Layout.leftMargin`, `Layout.topMargin`, etc. (layouts); `x`/`y` offsets (absolute) | px ×1 | Full (layouts) / Emulated (absolute) | `auto` horizontal with an explicit width → `Layout.alignment: Qt.AlignHCenter` (layouts) or `x: (parent.width - width) / 2`. Adjacent vertical margins do not collapse in QML |
| padding (all sides) | px, % | Inner `Item` with `anchors.fill: parent` and `anchors.margins`; `Row`/`Column`/`Flow`/`Grid` have their own `padding`, `topPadding`, … | px ×1 | Full | `Rectangle` has no padding property; the generator wraps content. Layouts have no padding either. Percent padding on all four sides is relative to the containing block's width |
| border-width | px, thin/medium/thick | `Rectangle.border.width` | px ×1; thin=1, medium=3, thick=5 | Full | Qt draws the border inside the item bounds, which matches `border-box` |
| border-style: none, hidden | — | `Rectangle.border.width: 0` | — | Full | |
| border-style: solid | — | `Rectangle.border.width` and `border.color` | — | Full | |
| border-style: dashed, dotted, double, groove, ridge, inset, outset | keywords | `Shape` + `ShapePath` with `PathRectangle` (radii supported, no fill), `strokeStyle: ShapePath.DashLine`, `dashPattern` (dotted: `[0, 2]` with `capStyle: ShapePath.RoundCap`) | — | Emulated | `double`/`groove`/`ridge` need two strokes; `inset`/`outset` need shading |
| aspect-ratio | ratio, auto | `height: width * h / w` (or `Layout.preferredHeight`) | — | Emulated | `auto` uses the replaced element's intrinsic ratio |

## Positioning

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| position: static | — | Default `Item` placement | — | Full | |
| position: relative | top/right/bottom/left offsets | `x`/`y` offset from the natural position; inside layouts a `Translate` in `Item.transform` | px ×1 | Full (absolute) / Emulated (layouts) | A layout overwrites `x`/`y`, so the offset must live in `transform` |
| position: absolute | offsets, width, height | `x`/`y`, or `anchors.left/right/top/bottom` with `anchors.*Margin`; the containing block is the nearest positioned ancestor | px ×1; % → of parent size | Full | Generator uses either `x` or anchors per axis, never both |
| position: fixed | offsets | Sibling of the scrolling `Flickable`, declared after it (or with higher `z`) and anchored to the root item | px ×1; % → of root size | Approx | Document scrolling does not move the element, which is what fixed means here |
| position: sticky | offsets | Child of the `Flickable`'s content item with `id: flick` on the `Flickable`: `y: Math.min(maxY, Math.max(naturalY, flick.contentY + top))`, where `maxY` keeps it inside its containing block | px ×1 | Emulated | Same for `x` with `contentX`. Only the edges that bind to a scrollable ancestor |
| top, right, bottom, left | px, %, auto | `y`, `x`, `anchors.*Margin` | px ×1; % → `parent.height * p / 100` (vertical) and `parent.width * p / 100` (horizontal) | Full | |
| inset | 1–4 values | `anchors.fill: parent` plus `anchors.margins` (or per-side margins) | px ×1 | Full | |
| z-index | integer, auto | `Item.z` | — | Approx | `z` only orders siblings. CSS stacking contexts (opacity, transform, filter) are approximated by nesting |

## Display & flexbox

`display: flex` maps to `FlexboxLayout`. For a plain row or column without wrap, justify or align-content, the generator may use `RowLayout` / `ColumnLayout` instead. `Flow` and spacer items are the fallback when `FlexboxLayout` is not available (before Qt 6.10); the Notes column gives the fallback.

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| display: block | — | `Item` for the box; block children stacked in a `ColumnLayout { spacing: 0 }` with `Layout.fillWidth: true` on each | — | Approx | Margin collapsing is not modelled |
| display: inline, inline-block | — | Runs of inline content become one `Text` with `textFormat: Text.StyledText` (or `RichText` for `sub`/`sup`/`text-indent`); inline-block boxes use `Flow`, or a `Row` sized by `implicitWidth` | — | Approx | Inline text flow across elements is not exact |
| display: flex | row, column | `FlexboxLayout` | — | Full | Fallback: `RowLayout` / `ColumnLayout` |
| display: inline-flex | row, column | `FlexboxLayout` sized by `implicitWidth` instead of filling the parent | — | Approx | |
| display: grid, inline-grid | — | `GridLayout` (see Grid) | — | Approx | |
| display: none | — | `visible: false` (layouts and positioners skip invisible items); or a `Loader` with `active: false` to skip instantiation | — | Full | |
| display: contents | — | Children moved to the grandparent; wrapper removed | — | Emulated | Generator rewrites the tree |
| display: table, table-row, table-cell | — | `GridLayout` | — | Approx | |
| display: list-item | — | Bullet handled by list-style (see Lists) | — | Emulated | |
| flex-direction | row, column, row-reverse, column-reverse | `direction: FlexboxLayout.Row` / `Column` / `RowReverse` / `ColumnReverse` | — | Full | Fallback: `RowLayout` / `ColumnLayout`; `row-reverse` → `layoutDirection: Qt.RightToLeft` (changes item order only, not text alignment); `column-reverse` → reversed child order |
| flex-wrap | nowrap, wrap, wrap-reverse | `wrap: FlexboxLayout.NoWrap` / `Wrap` / `WrapReverse` | — | Full | Fallback: `Flow` with `flow: Flow.LeftToRight` (row) or `Flow.TopToBottom` (column); Flow does not grow children across lines; `wrap-reverse` → reversed order |
| justify-content | flex-start, normal, center, flex-end, space-between, space-around, space-evenly | `justifyContent: FlexboxLayout.JustifyStart` / `JustifyCenter` / `JustifyEnd` / `JustifySpaceBetween` / `JustifySpaceAround` / `JustifySpaceEvenly` | — | Full | Fallback: spacer `Item`s with `Layout.fillWidth: true` at the ends or between children (`RowLayout` has no justify property) → Emulated |
| align-items | flex-start, flex-end, center, stretch, normal, baseline | `alignItems: FlexboxLayout.AlignStart` / `AlignEnd` / `AlignCenter` / `AlignStretch` / `AlignBaseline` | — | Full | `normal` = `AlignStretch`. Fallback: `Layout.alignment` on each child (`Qt.AlignTop` / `AlignBottom` / `AlignVCenter` in a row; `AlignLeft` / `AlignRight` / `AlignHCenter` in a column) or `Layout.fillHeight` / `fillWidth` for stretch; stretch does not apply to children with an explicit cross size; `baseline` → Approx |
| align-self | same as align-items, auto | `FlexboxLayout.alignSelf: FlexboxLayout.AlignEnd` etc.; `auto` → `AlignAuto` | — | Full | Fallback: `Layout.alignment` on the child |
| align-content | multi-line values | `alignContent: FlexboxLayout.AlignStart` / … / `AlignSpaceBetween` | — | Full | Only meaningful with `wrap`. `Flow` has no equivalent → Emulated |
| gap, row-gap, column-gap | px | `FlexboxLayout.gap` / `rowGap` / `columnGap`; `GridLayout.rowSpacing` / `columnSpacing`; `RowLayout.spacing` / `ColumnLayout.spacing` | px ×1 | Full | `Flow.spacing` is a single value for both axes, so differing row and column gaps are Approx there |
| flex-grow | number | `Layout.fillWidth: true` (row) / `Layout.fillHeight: true` (column) for any value above 0 | — | Approx | In `RowLayout` / `ColumnLayout`, unequal weights use `Layout.horizontalStretchFactor` / `verticalStretchFactor`. `FlexboxLayout.alignSelf` is its only own attached property, so weights other than 0 and 1 are not expressible there |
| flex-shrink: 0 | — | `Layout.minimumWidth` set to `Layout.preferredWidth` | — | Emulated | Other values have no direct target |
| flex-basis | px, %, auto, content | `Layout.preferredWidth` (row) / `Layout.preferredHeight` (column) | px ×1; % → `parent.width * p / 100` | Full (px) / Approx (%) | `auto`/`content` → `implicitWidth` |
| flex, flex-flow, place-content | shorthands | Expanded into the longhands above | — | Full | |
| order | integer | Child order reordered at compile time | — | Full (static) / Emulated (dynamic) | Dynamic order uses a `Repeater` over a sorted model |

## Grid

CSS grid line numbers start at 1; `Layout.row` and `Layout.column` start at 0.

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| grid-template-columns, grid-template-rows | px, auto, min-content, max-content | `GridLayout.columns` / `rows`; child `Layout.preferredWidth` / `preferredHeight` | px ×1 | Full | |
| grid-template-columns, grid-template-rows: fr | fr | `Layout.preferredWidth: (parent.width - gaps) * fr / totalFr` (binding generated with the fraction computed at compile time) | — | Emulated | `Layout.fillWidth` alone does not give exact ratios. All-equal `1fr` tracks: `fillWidth` on children plus `uniformCellWidths: true` (`uniformCellHeights` for rows) |
| grid-template-columns: repeat() | repeat(n, …), auto-fill, auto-fit | Expanded at compile time for fixed `n`; `auto-fill`/`auto-fit` become `columns: Math.max(1, Math.floor((parent.width + gap) / (minWidth + gap)))` | — | Emulated | |
| grid-template-columns: minmax() | minmax(min, max) | `Layout.minimumWidth` / `Layout.maximumWidth` | px ×1 | Approx | |
| grid-column, grid-row | line numbers, span n | `Layout.column`, `Layout.row`, `Layout.columnSpan`, `Layout.rowSpan` | line − 1 | Full | |
| grid-area | row / col / span values | `Layout.row`, `Layout.column`, `Layout.rowSpan`, `Layout.columnSpan` | line − 1 | Full | Named areas resolved by the generator |
| grid-template-areas | ASCII area strings | Row, column, and span values per item, generated | — | Emulated | |
| grid-auto-flow: row, column | — | `GridLayout.flow`: `GridLayout.LeftToRight` (row) / `GridLayout.TopToBottom` (column) | — | Approx | `dense` is Ignored |
| grid-auto-rows, grid-auto-columns | px, auto | `Layout.preferredHeight` / `preferredWidth` on implicit cells | px ×1 | Approx | |
| place-items | align values | `Layout.alignment` on each child | — | Approx | |
| place-self | align values | `Layout.alignment` on the child | — | Full | |
| gap (grid) | px | `GridLayout.rowSpacing`, `GridLayout.columnSpacing` | px ×1 | Full | |

## Backgrounds & borders

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| background-color | color | `Rectangle.color` | — | Full | See Colors |
| background-image: linear-gradient, along an axis | `to bottom`/`to top` (180deg/0deg), `to right`/`to left` (90deg/270deg), stops | `Rectangle.gradient: Gradient { orientation: Gradient.Vertical }` or `Gradient.Horizontal`, with `GradientStop { position; color }`; reversed directions reverse the stop order | % → 0–1 | Full | Stops without a position are spread evenly by the converter; a hard stop is two stops at one position. Qt interpolates without premultiplied alpha, so `transparent` stops are emitted as the neighbouring colour with alpha 0 |
| background-image: linear-gradient, other angles | angle, to-corner | `Shape` with `ShapePath.fillGradient: LinearGradient { x1; y1; x2; y2 }` | Gradient line length `abs(W·sin θ) + abs(H·cos θ)`, centred on the box | Approx | Rounded corners: use `PathRectangle` as the path |
| background-image: radial-gradient | shape, size, stops | `Shape` with `RadialGradient { centerX; centerY; centerRadius; focalX; focalY }` | — | Approx | Circles only; ellipses need a scaled `Shape` |
| background-image: conic-gradient | from, stops | `Shape` with `ConicalGradient { centerX; centerY; angle }` | Qt sweeps counter-clockwise from 3 o'clock, CSS clockwise from 12 o'clock: convert the start angle and mirror stop positions (`1 - p`) | Approx | |
| background-image: repeating-linear-gradient, repeating-radial-gradient | — | The gradient above with `spread: ShapeGradient.RepeatSpread`, its endpoints spanning one period | — | Approx | `repeating-conic-gradient` → Emulated by repeating the stops |
| background-image: url() | url | `Image { source }` inside an `Item` with `clip: true` | — | Full | |
| background-size | cover, contain, px, % | `Image.fillMode`: `PreserveAspectCrop` (also needs `clip: true` on the `Image`) / `PreserveAspectFit`; px/% → `width`/`height` | px ×1; % of parent | Full (cover/contain) / Approx (px/%) | |
| background-position | keywords, px, % | `Image.horizontalAlignment` / `verticalAlignment` (left, right, centre only); px/% → `x`/`y` offset | px ×1 | Approx | |
| background-repeat: repeat | — | `Image.fillMode: Image.Tile` | — | Full | Tiling starts at the item's origin, so combining it with `background-position` offsets is Approx |
| background-repeat: repeat-x, repeat-y | — | `Image.TileHorizontally` / `Image.TileVertically` | — | Full | |
| background-repeat: no-repeat, space, round | — | no-repeat → `Image.Pad`; space/round Emulated | — | Approx | |
| background-attachment: fixed | — | Image positioned relative to the root item | — | Emulated | |
| background-clip, background-origin | keywords | Padding box offset | — | Ignored | Visually negligible in most pages |
| Multiple backgrounds | list | Stacked `Item`s with z ordering | — | Full | |
| border-top/right/bottom/left-width, color | px, color | Per-side: `Rectangle` strips or `Shape` strokes | px ×1 | Emulated | `Rectangle.border` is uniform only |
| border-radius (uniform) | px, % | `Rectangle.radius` | px ×1; % → `Math.min(width, height) * p / 100` | Full (px) / Approx (%) | Qt clamps to half the shorter side, as CSS does. A percentage is relative to width horizontally and height vertically |
| border-radius (per corner) | px | `Rectangle.topLeftRadius`, `topRightRadius`, `bottomLeftRadius`, `bottomRightRadius` | px ×1 | Full | Elliptical corners (`10px / 20px`) → `Shape` with `PathArc { radiusX; radiusY }` per corner, Emulated |
| border-image | url, slice, repeat | `BorderImage { source; border.left; border.right; border.top; border.bottom; horizontalTileMode; verticalTileMode }` (`BorderImage.Stretch` / `Repeat` / `Round`) | px ×1 | Approx | |
| outline-width, outline-color, outline-style | px, color, style | Extra `Rectangle { color: "transparent" }` with `border`, anchored to the box with negative margins so it takes no layout space | px ×1 | Emulated | Dashed outline via `Shape` |
| outline-offset | px | Negative anchor margin on the outline rectangle | px ×1 | Emulated | |
| box-shadow (outer) | offset-x, offset-y, blur, spread, color | `RectangularShadow { offset: Qt.vector2d(x, y); blur; spread; color; radius }` (per-corner `topLeftRadius` etc.) as a sibling behind the box, sized to it | px ×1 | Approx | CSS blur radius is twice the Gaussian σ; Qt's `blur` scale is not identical, so tune the factor visually. Does not blur the box's own content, unlike a `MultiEffect` on its layer |
| box-shadow (inset) | — | `ShaderEffect` with a precompiled `.qsb` fragment shader | — | Emulated | Qt 6 needs shaders compiled with `qsb`; GLSL source cannot be loaded at runtime |

## Typography

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| color | color | `Text.color` | — | Full | Inherited values are resolved by the converter |
| font-family | list, generic families | `Text.font.family` | — | Approx | `font` has a single `family`, not a fallback list: use the first listed family (bundled `FontLoader` fonts take precedence); generic families map to platform defaults |
| font-size | px, pt, em, rem, %, keywords | `Text.font.pixelSize` | px ×1; pt ×4/3; em/% × parent size; rem × root size; keywords (xx-small=9, x-small=10, small=13, medium=16, large=18, x-large=24, xx-large=32, xxx-large=48) | Approx | `pixelSize` is an `int`, so fractional sizes are rounded. Use `pixelSize`, not `pointSize`, for DPI independence |
| font-weight: 100–900 | numbers, normal, bold | `Text.font.weight` | — | Full | The number is used directly (Qt 6 accepts 1–1000); `Font.Normal` = 400, `Font.Bold` = 700 |
| font-weight: bolder, lighter | keywords | Relative to inherited weight, resolved by the converter | — | Approx | |
| font-style: italic, oblique | — | `Text.font.italic: true` | — | Full (italic) / Approx (oblique) | |
| font-variant: small-caps | — | `Text.font.capitalization: Font.SmallCaps` | — | Full | |
| font-variant-numeric, font-feature-settings | tabular-nums, etc. | `Text.font.features: ({ "tnum": 1 })` (OpenType tag → value) | — | Approx | |
| font-stretch, font-variation-settings | keywords, %, axis values | `Text.font.variableAxes: ({ "wdth": 75 })` | % → `wdth` axis value | Approx | Only variable fonts that have the axis. `font` has no `stretch` property |
| line-height: unitless, px, em, % | number, length | `Text.lineHeight` with `lineHeightMode: Text.FixedHeight` | unitless/em × font size; % × font size; px ×1 | Approx | `Text.ProportionalHeight` multiplies the font's natural line height (about 1.15–1.2 × size), not the font size, so it does not match unitless CSS values. CSS centres the glyphs in the line box (half-leading); QML does not |
| line-height: normal | — | Default (`lineHeight: 1`, proportional) | — | Full | |
| letter-spacing | px, em | `Text.font.letterSpacing` | px ×1; em × font size | Full | |
| word-spacing | px, em | `Text.font.wordSpacing` | px ×1; em × font size | Full | |
| text-align: left, right, center | — | `Text.horizontalAlignment` | — | Full | Needs a `Text` wider than its text, so give it the container width |
| text-align: justify | — | `Text.AlignJustify` | — | Full | |
| text-align: start, end | — | `Text.AlignLeft` / `Text.AlignRight` by direction | — | Full | |
| text-decoration: underline | — | `Text.font.underline: true` | — | Full | |
| text-decoration: line-through | — | `Text.font.strikeout: true` | — | Full | |
| text-decoration: overline | — | `Text.font.overline: true` | — | Full | |
| text-decoration color | color | Separate `Rectangle` line over the text | — | Approx | `Text` uses the text color for decorations |
| text-decoration: wavy, dotted | keywords | Unsupported (decoration style) | — | Unsupported | |
| text-decoration-thickness, text-underline-offset | px | Not configurable | — | Ignored | |
| text-transform: uppercase | — | `Text.font.capitalization: Font.AllUppercase` | — | Full | |
| text-transform: lowercase | — | `Font.AllLowercase` | — | Full | |
| text-transform: capitalize | — | `Font.Capitalize` | — | Full | |
| text-indent | px, em | `Text` with `textFormat: Text.RichText` and a block `text-indent` style, or leading non-breaking spaces (first line only) | px ×1 | Emulated | No plain `Text` property |
| text-overflow: ellipsis | — | `Text.elide: Text.ElideRight` with `wrapMode: Text.NoWrap` | — | Full | `Text` needs an explicit `width` for eliding |
| text-overflow: clip | — | `clip: true` on the text container | — | Approx | |
| -webkit-line-clamp | integer | `Text.maximumLineCount` with `elide: Text.ElideRight` and a wrapping `wrapMode` | — | Approx | |
| white-space: normal | — | `Text.wrapMode: Text.WordWrap` | — | Full | `Text` defaults to `NoWrap` and needs a width to wrap |
| white-space: nowrap | — | `Text.wrapMode: Text.NoWrap` | — | Full | |
| white-space: pre | — | `Text.wrapMode: Text.NoWrap`, `PlainText`, with source newlines kept | — | Full | |
| white-space: pre-wrap, pre-line | — | `Text.wrapMode: Text.WordWrap` with newlines kept | — | Approx | Space collapsing differs |
| word-break: break-all | — | `Text.wrapMode: Text.WrapAnywhere` | — | Full | |
| word-break: keep-all | — | `Text.wrapMode: Text.WordWrap` | — | Approx | |
| overflow-wrap: break-word, anywhere | — | `Text.WrapAtWordBoundaryOrAnywhere` | — | Full | Breaks at word boundaries first and mid-word only when a word does not fit, as CSS does |
| vertical-align: baseline, top, middle, bottom | keywords | `Text.verticalAlignment` for block text; inline alignment is Approx | — | Approx | |
| vertical-align: sub, super | — | Rich text `<sub>`/`<sup>` with `Text.textFormat: Text.RichText` | — | Approx | `StyledText` does not support these tags |
| vertical-align: px, % | length | Emulated `y` offset of the inline run | px ×1 | Emulated | |
| text-shadow: offset, color (no blur) | px, color | Duplicate `Text` behind with offset; `Text.style: Text.Raised` with `styleColor` only matches a (1px, 1px) offset | px ×1 | Approx | |
| text-shadow: blur | px | `layer.enabled: true` with `layer.effect: MultiEffect { shadowEnabled: true; … }` | px ×1 | Approx | |
| writing-mode: vertical-* | keywords | Unsupported | — | Unsupported | |
| direction: rtl | — | `LayoutMirroring.enabled: true` and `LayoutMirroring.childrenInherit: true` on the element, plus an explicit `Text.horizontalAlignment` | — | Approx | `Text` has no `layoutDirection`; Qt's bidi handling orders the text itself |

## Visual

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| opacity | 0–1 | `Item.opacity` | — | Approx | QML multiplies opacity per item, so overlapping children show through each other. Use `layer.enabled: true` to composite the group first, as CSS does |
| visibility: visible | — | `visible: true` | — | Full | |
| visibility: hidden | — | `opacity: 0` with `enabled: false` (keeps layout space) | — | Approx | Layouts skip `visible: false` items; children cannot re-show themselves through a transparent parent |
| visibility: collapse | — | `visible: false` | — | Full | |
| overflow: hidden | — | `clip: true` | — | Approx | `clip` cuts to the rectangular bounds only, not to `border-radius`. With a radius use `layer.enabled: true` and `layer.effect: MultiEffect { maskEnabled: true; maskSource }` (Emulated) |
| overflow: scroll, auto | — | `Flickable` (or `ScrollView`) with `ScrollBar.vertical.policy: ScrollBar.AlwaysOn` (scroll) or `ScrollBar.AsNeeded` (auto); set `contentWidth`/`contentHeight` | — | Approx | Scrollbar styling differs |
| overflow-x, overflow-y | hidden, scroll, auto | `Flickable.flickableDirection` (`Flickable.HorizontalFlick` / `VerticalFlick` / `HorizontalAndVerticalFlick`) | — | Approx | |
| transform: translate, translate3d (2D) | px, % | `Translate { x; y }` in `Item.transform` | px ×1; % × own size | Full | `Item.x`/`y` would be overwritten inside layouts |
| transform: rotate | deg, turn, rad, grad | `Item.rotation` (both clockwise) | deg ×1; turn ×360; rad ×180/π; grad ×0.9 | Full | |
| transform: scale | number | `Item.scale` (uniform); non-uniform → `Scale { xScale; yScale; origin.x; origin.y }` | — | Full | |
| transform: skew, skewX, skewY | deg | `Matrix4x4` with generated skew matrix | tan(deg) | Emulated | |
| transform: matrix() | 6 values | `Matrix4x4 { matrix: Qt.matrix4x4(a, c, 0, e, b, d, 0, f, 0, 0, 1, 0, 0, 0, 0, 1) }` (row-major) for `matrix(a, b, c, d, e, f)` | — | Full | |
| transform: several functions | list | Multiplied at compile time into one `Matrix4x4`, left function outermost, so the list order of `Item.transform` never matters | — | Full | |
| transform: 3D functions, perspective | — | `Matrix4x4` (Approx for perspective) | — | Approx | |
| transform-origin: keywords | left, center, bottom right, etc. | `Item.transformOrigin` (`Item.TopLeft`, `Item.Center`, …) | — | Full | Applies to `rotation` and `scale` only, not to `Matrix4x4`. Default is 50% 50% = `Item.Center` |
| transform-origin: px, % | — | `origin.x` / `origin.y` on `Rotation` / `Scale`; for `Matrix4x4`, bake `T(o)·M·T(−o)` into the matrix | px ×1; % × size | Emulated | |
| transform-style: preserve-3d | — | Unsupported | — | Unsupported | |
| filter: brightness, contrast, saturate, grayscale | number, % | `layer.enabled: true` with `layer.effect: MultiEffect { brightness; contrast; saturation }` | brightness b → `b - 1`; contrast c → `c - 1`; saturate s → `s - 1`; grayscale g → `saturation: -g` (all clamped to −1…1) | Approx | `MultiEffect` ranges are −1…1 and additive, CSS is multiplicative |
| filter: blur | px | `layer.effect: MultiEffect { blurEnabled: true; blur; blurMax }` | `blur = Math.min(1, px / blurMax)` (default `blurMax` is 32) | Approx | |
| filter: drop-shadow | offsets, blur, color | `MultiEffect { shadowEnabled: true; shadowColor; shadowHorizontalOffset; shadowVerticalOffset; shadowBlur }` on the layer | px ×1; blur → 0–1 | Approx | Follows the element's alpha, like CSS |
| filter: sepia | number | `MultiEffect { colorization; colorizationColor }` | — | Approx | |
| filter: invert, hue-rotate | — | `ShaderEffect` with a precompiled `.qsb` fragment shader | deg for hue | Emulated | |
| backdrop-filter | — | Unsupported | — | Unsupported | |
| clip-path: inset, circle, ellipse, polygon | shapes | `layer.enabled: true` with `layer.effect: MultiEffect { maskEnabled: true; maskSource }`; the mask is a `Shape` (`PathRectangle`, `PathPolyline`, arcs) that is itself `layer.enabled: true` and `visible: false` | px ×1; % × size | Emulated | |
| clip-path: path() | SVG path | Mask `Shape` with `PathSvg { path }` | — | Emulated | |
| cursor | keywords, url() | `MouseArea.cursorShape` or `HoverHandler.cursorShape` | — | Full | pointer → `Qt.PointingHandCursor`, text → `Qt.IBeamCursor`, wait → `Qt.WaitCursor`, progress → `Qt.BusyCursor`, not-allowed → `Qt.ForbiddenCursor`, move → `Qt.SizeAllCursor`, crosshair → `Qt.CrossCursor`, grab/grabbing → `Qt.OpenHandCursor`/`Qt.ClosedHandCursor`, help → `Qt.WhatsThisCursor`, none → `Qt.BlankCursor`; `url()` falls back to the keyword |
| pointer-events: none | — | Input handlers `enabled: false` or omitted | — | Approx | |
| mix-blend-mode | keywords | Unsupported | — | Unsupported | |
| isolation: isolate | — | Nothing (grouping only) | — | Ignored | |
| will-change | — | Nothing | — | Ignored | |
| contain | keywords | Nothing | — | Ignored | |

## Animation

Generated animation uses only QtQuick types (`Behavior`, `Transition`, `SequentialAnimation`, `ParallelAnimation`, `PropertyAnimation` family, `PauseAnimation`, `PropertyAction`).

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| transition-property: numeric | any numeric property | `Behavior on x { NumberAnimation { } }` | — | Full | |
| transition-property: color | color | `Behavior on color { ColorAnimation { } }` | — | Full | |
| transition-property: all | — | One `Behavior` per animatable property | — | Approx | |
| transition-duration | s, ms | `NumberAnimation.duration` | s ×1000 | Full | |
| transition-timing-function: linear | — | `easing.type: Easing.Linear` | — | Full | |
| transition-timing-function: ease, ease-in, ease-out, ease-in-out | keywords | `easing.type: Easing.BezierSpline` with `easing.bezierCurve` from the standard CSS curves, e.g. `ease` = `[0.25, 0.1, 0.25, 1.0, 1, 1]` | — | Full | `bezierCurve` is `[c1x, c1y, c2x, c2y, 1, 1]`; the last pair is the end point |
| transition-timing-function: cubic-bezier() | 4 values | `easing.type: Easing.BezierSpline`; `bezierCurve: [x1, y1, x2, y2, 1, 1]` | — | Full | |
| transition-timing-function: steps() | n, jump-* | Discrete `PropertyAction` steps | — | Emulated | |
| transition-delay | s, ms | `PauseAnimation` inside a `SequentialAnimation` | s ×1000 | Emulated | |
| transition (shorthand) | — | Expanded into the longhands above | — | Full | |
| transition on states | — | `Transition { from; to; NumberAnimation { } }` | — | Full | Used for `:hover` and `:checked` |
| transition-behavior: allow-discrete | — | Ignored | — | Ignored | |
| animation-name, animation-duration | name, s/ms | `SequentialAnimation` / `ParallelAnimation` of property animations | s ×1000 | Emulated | |
| @keyframes | percentages, properties | A `ParallelAnimation` with one `SequentialAnimation` per property; each keyframe interval is a `NumberAnimation` / `ColorAnimation { to; duration }`; transform-only keyframes can use `XAnimator`, `YAnimator`, `ScaleAnimator`, `RotationAnimator`, `OpacityAnimator` | % → interval duration | Emulated | `QtQuick.Timeline` (`Timeline`, `Keyframe`) is deliberately not used |
| animation-timing-function (per keyframe) | keywords, cubic-bezier | `easing` on that interval's animation | — | Emulated | |
| animation-delay | s, ms | `PauseAnimation` | s ×1000 | Emulated | |
| animation-iteration-count: n | number | `Animation.loops: n` | — | Full (integers) / Approx (fractions) | `loops` is an `int` |
| animation-iteration-count: infinite | — | `Animation.loops: Animation.Infinite` | — | Full | |
| animation-direction: alternate, alternate-reverse | — | Forward then backward `SequentialAnimation` with `loops` | — | Emulated | `loops` has no direction |
| animation-direction: reverse | — | Animation with `from`/`to` swapped | — | Full | |
| animation-fill-mode: forwards, both | — | A finished animation leaves the last value on the property | — | Full | The binding the property had is lost; the converter sets the value explicitly |
| animation-fill-mode: none, backwards | — | `PropertyAction` restoring the original value at the end; first keyframe applied before the delay for `backwards` | — | Emulated | QML does not revert on its own |
| animation-play-state: paused | — | `Animation.paused: true` | — | Full | |

## Lists & tables

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| list-style-type: disc, circle, square | keywords | `Repeater` with bullet `Text` or `Rectangle` per item | — | Emulated | |
| list-style-type: decimal, roman, alpha | keywords | `Repeater` with counter `Text` | — | Emulated | |
| list-style-type: none | — | Nothing | — | Ignored | |
| list-style-position: inside, outside | keywords | Bullet offset by the item's left margin | px ×1 | Emulated | |
| list-style-image | url() | `Image` as the bullet | — | Emulated | |
| list-style (shorthand) | — | Expanded into the longhands above | — | Emulated | |
| border-collapse: separate | — | Normal `GridLayout` cells | — | Full | |
| border-collapse: collapse | — | `GridLayout` with zero spacing and shared borders | — | Emulated | |
| border-spacing | px | `GridLayout.rowSpacing`, `GridLayout.columnSpacing` | px ×1 | Full | |
| table-layout: auto | — | `GridLayout` with implicit column widths | — | Approx | |
| table-layout: fixed | — | Fixed `Layout.preferredWidth` on the first row's cells | px ×1 | Emulated | |
| caption-side: top, bottom | keywords | Caption placed first or last in a `ColumnLayout` | — | Emulated | |
| colspan, rowspan (HTML attributes) | integer | `Layout.columnSpan`, `Layout.rowSpan` | — | Full | |
| vertical-align (table cells) | top, middle, bottom | `Layout.alignment` (`Qt.AlignTop` / `Qt.AlignVCenter` / `Qt.AlignBottom`) on the cell | — | Full | |

## Misc

| Property | Values handled | QML target | Unit conversion | Support | Notes |
|---|---|---|---|---|---|
| box-sizing: content-box | — | Default in CSS; the converter adds padding and border to `width`/`height` | px ×1 | Emulated | A QML item's size is always the outer (border-box) size |
| box-sizing: border-box | — | `width`/`height` used as the outer size | — | Full | |
| content (::before, ::after) | string | `Text` child inserted before or after the element | — | Full | |
| content: attr() | attribute name | Attribute value substituted at compile time | — | Full | |
| content: url() | url | `Image` child | — | Full | |
| content: counter(), counters() | — | Counter values generated by the converter | — | Emulated | |
| content: open-quote, close-quote | — | Quote characters from `quotes` | — | Emulated | |
| Custom properties, var() | `--name` | Substituted at compile time; dynamic `style.setProperty` maps to a QML property with a binding | — | Full (static) / Emulated (dynamic) | |
| calc() | +, -, *, / with units | Compile-time value when all terms are constant; otherwise a QML binding expression | px ×1; % × parent size | Full (constant) / Emulated (mixed) | |
| min(), max(), clamp() | length arguments | `Math.min`, `Math.max`, or both in a binding | px ×1 | Full | |
| !important | — | Resolved in the cascade by the converter | — | Full | Converter-only; no QML target |
| user-select: none | — | Plain `Text` (not selectable) | — | Full | `Text` has no selection support at all |
| user-select: text, all | — | `TextEdit { readOnly: true; selectByMouse: true }` | — | Approx | `TextEdit` has no `elide`, and its layout differs slightly from `Text` |
| object-fit: fill | — | `Image.fillMode: Image.Stretch` | — | Full | |
| object-fit: cover | — | `Image.fillMode: Image.PreserveAspectCrop` with `clip: true` | — | Full | |
| object-fit: contain | — | `Image.fillMode: Image.PreserveAspectFit` | — | Full | |
| object-fit: none | — | `Image.Pad` at natural size | — | Full | |
| object-fit: scale-down | — | `Image.PreserveAspectFit` clamped to natural size | — | Approx | |
| object-position | keywords, px, % | `Image.horizontalAlignment` / `verticalAlignment`; px/% as offsets | px ×1 | Approx | |
| aspect-ratio | see Box model | See Box model & sizing | — | Emulated | |
| @media (min-width, max-width) | media features | Evaluated at compile time for the default size; runtime queries become `State { when: Window.width >= n }` | px ×1 | Approx | |
| @media (prefers-color-scheme) | light, dark | `Qt.styleHints.colorScheme === Qt.Dark` in a `State.when` | — | Emulated | |
| @media (orientation, hover, pointer) | keywords | Compile-time evaluation or `State` on `Window.width > Window.height` | — | Approx | |
| @font-face | src, family | `FontLoader { source: "…" }`; set `font.family: loader.name`, since the family name registered by the font file can differ from the CSS name | — | Full | Font files must be bundled |
| font-display | keywords | Nothing | — | Ignored | |
| @import (local) | url | Inlined at compile time | — | Full | |
| @import (remote) | url | Unsupported unless fetched and bundled | — | Unsupported | |
| @layer | names | Cascade resolved by the converter | — | Full | |
| @supports | conditions | Evaluated at compile time | — | Full | |
| @container | conditions | Unsupported | — | Unsupported | |
| :hover | — | `HoverHandler { id: hh }` with `State { when: hh.hovered }` | — | Full | |
| :hover on ancestors (`.a:hover .b`) | selectors | Ancestor `HoverHandler` drives child state | — | Approx | |
| :active | — | `TapHandler` with `pressed` in `when` | — | Full | |
| :focus | — | `when: item.activeFocus` | — | Full | |
| :focus-visible | — | `when: control.visualFocus` on Controls; `activeFocus` on plain items | — | Full (Controls) / Approx (items) | |
| :checked | — | `when: checkbox.checked` (Controls `CheckBox`) or a bound state | — | Full | |
| :disabled | — | `enabled: false` | — | Full | |
| :first-child, :last-child, :nth-child() | structural | Resolved at compile time for static trees | — | Full (static) / Emulated (dynamic) | |
| :not(), :is(), :where() | selector lists | Resolved at compile time | — | Full | |
| :has() | selector | Unsupported | — | Unsupported | |
| ::placeholder | — | `TextField.placeholderText` and `placeholderTextColor` (Controls) | — | Full | |
| ::selection | — | `selectionColor` and `selectedTextColor` on `TextEdit` / `TextField` | — | Approx | |

---

## Units

Qt QML lengths are logical pixels. CSS px is also a logical pixel, so px needs no scaling.

| Unit | CSS meaning | QML conversion | Support | Notes |
|---|---|---|---|---|
| px | 1 CSS pixel | `value` (1 logical px) | Full | |
| em | Element's font size | `value * fontPixelSize` (compile time when the font size is static) | Full | `font-size: em` uses the parent's size |
| rem | Root element's font size | `value * rootFontPixelSize` (compile time) | Full | Runtime changes need a shared `property real rem` |
| % (width, left, right, margins, padding) | Width of containing block | `parent.width * value / 100` | Approx | Becomes a binding; resolves at layout time |
| % (height, top, bottom) | Height of containing block | `parent.height * value / 100` | Approx | Undefined if the parent's height is `auto`; falls back to `implicitHeight` |
| % (font-size) | Parent font size | `value / 100 * parentPixelSize` | Full | |
| vw | 1% of viewport width | `Window.width * value / 100` | Full | Use the root `Window` (or `root.width` in a window-less component) |
| vh | 1% of viewport height | `Window.height * value / 100` | Full | |
| vmin | 1% of smaller viewport dimension | `Math.min(Window.width, Window.height) * value / 100` | Full | |
| vmax | 1% of larger viewport dimension | `Math.max(Window.width, Window.height) * value / 100` | Full | |
| pt | 1/72 inch | `value * 4 / 3` px | Full | 1pt = 4/3 px at 96 DPI |
| pc | 12 pt | `value * 16` px | Full | |
| cm | Centimeter | `value * 96 / 2.54` (≈ `value * 37.795`) px | Full | Absolute, not screen-dependent |
| mm | Millimeter | `value * 96 / 25.4` (≈ `value * 3.7795`) px | Full | |
| in | Inch | `value * 96` px | Full | |
| ch | Width of "0" in the element's font | `value * fm.advanceWidth("0")` with `FontMetrics { id: fm; font: textItem.font }` | Emulated | Runtime binding: the converter has no font metrics |
| ex | x-height of the element's font | `value * fm.xHeight` with the same `FontMetrics`; fallback `value * 0.5 * fontPixelSize` | Emulated | Approximation when no `FontMetrics` is generated |
| Q | Quarter millimeter | `value * 96 / 101.6` px | Full | Rarely used |

---

## Colors

QML color literals: 6-digit hex is `#RRGGBB`. 8-digit hex is **`#AARRGGBB`** (alpha first). CSS `#rrggbbaa` must be rewritten to `#aarrggbb`.

| Syntax | QML target | Conversion | Support | Notes |
|---|---|---|---|---|
| Named colors (`red`, `rebeccapurple`, …) | Hex literal `"#RRGGBB"` | Looked up in the CSS Color 4 name table at compile time | Full | Qt's own name list is the older SVG set, so names are not passed through |
| `#rgb` | `"#RRGGBB"` | Each nibble doubled at compile time | Full | |
| `#rgba` | `"#AARRGGBB"` | Each nibble doubled; alpha moved to the front | Full | e.g. `#f808` → `"#88ff0088"` |
| `#rrggbb` | `"#RRGGBB"` | Passed through | Full | |
| `#rrggbbaa` | `"#AARRGGBB"` | Alpha byte moved to the front | Full | e.g. `#11223380` → `"#80112233"` |
| `rgb(r, g, b)` | `Qt.rgba(r/255, g/255, b/255, 1)` or hex literal | Compile-time hex when constant | Full | Percent components divided by 100 |
| `rgba(r, g, b, a)` | `Qt.rgba(r/255, g/255, b/255, a)` | Compile-time hex when constant | Full | |
| `rgb(r g b / a)` (space syntax) | Same as above | Same | Full | Alpha after `/` |
| `hsl(h, s, l)` / `hsla(h, s, l, a)` | `Qt.hsla(h/360, s, l, a)` | Hue normalised into 0–1; s, l in 0–1; compile-time hex when constant | Full | `Qt.hsla` argument order is hue, saturation, lightness, alpha |
| `hwb(h w b)` | Hex literal | Converted to sRGB at compile time | Full | No QML function exists; always compile-time |
| `lab()`, `lch()` | Hex literal | Converted to sRGB at compile time; out-of-gamut values clipped | Approx | Clipping differs from browser gamut mapping |
| `oklab()`, `oklch()` | Hex literal | Converted to sRGB at compile time; clipped | Approx | Same as above |
| `color-mix(in srgb, …)` | Hex literal (constant) or binding (dynamic) | Interpolated in sRGB at compile time | Full | |
| `color-mix(in oklab, …)`, other spaces | Hex literal | Interpolated in the given space, then converted to sRGB | Approx | |
| `currentColor` | `Text.color` / `Rectangle.border.color` of the element | Resolved from the element's `color` at compile time; binding to the element's color property for runtime changes | Full | |
| `transparent` | `"transparent"` or `Qt.rgba(0, 0, 0, 0)` | Compile time | Full | In gradients see Backgrounds |
| System colors | `palette.window` (`Canvas`), `palette.windowText` (`CanvasText`), `palette.base` (`Field`), `palette.text` (`FieldText`), `palette.highlight` (`Highlight`), `palette.highlightedText` (`HighlightText`), `palette.button` (`ButtonFace`), `palette.buttonText` (`ButtonText`), `palette.link` (`LinkText`), `palette.linkVisited` (`VisitedText`), `palette.placeholderText` (`GrayText`), `palette.accent` (`AccentColor`) | `Item.palette` bound to the platform/style palette | Approx | Exact system values differ per platform; `GrayText` is only close to `placeholderText` |
| Gradient stop colors | Same rules as above | Same | Full | |

---

## Review notes

- Removed `QtQuick.Timeline` from the target modules (not installed, so snapshots would not load); `@keyframes` now maps to `ParallelAnimation` / `SequentialAnimation` of plain QtQuick animations, with per-interval `easing`.
- `display: flex` and its container properties now map to `FlexboxLayout` (QtQuick.Layouts, added in 6.10) with its real enums (`FlexboxLayout.Row`, `JustifySpaceBetween`, `AlignStretch`, `alignSelf`, `gap`/`rowGap`/`columnGap`); `RowLayout` / `ColumnLayout` / `Flow` remain as fallbacks. FlexboxLayout was introduced as a technology preview in 6.10; check its status in the 6.11 docs before making it the only target.
- Corrected the layout convention: `Flow` is a positioner, not a layout, so `Layout.*` does not apply to its children; `Flow` has no separate row/column spacing and no `align-content`.
- `flex-grow` weights: `Layout.horizontalStretchFactor` / `verticalStretchFactor` exist, so the old claim that weights cannot be proportional was wrong for `RowLayout` / `ColumnLayout`.
- `Rectangle` has per-corner radius properties (`topLeftRadius`, …) since 6.7; per-corner circular radii are now Full, and only elliptical corners need `Shape`/`PathArc`.
- `Gradient` has an `orientation` property (`Gradient.Horizontal`), so axis-aligned gradients no longer need `Shape`. Added stop handling for hard stops and premultiplied-alpha differences.
- Repeating gradients use `ShapeGradient.spread: RepeatSpread`, not "path repetition". Documented conical gradients' counter-clockwise sweep and the CSS angle-to-endpoint formula.
- Outer `box-shadow` now uses `RectangularShadow` (6.9+: `offset`, `blur`, `spread`, `color`, per-corner radii) instead of a `MultiEffect` layer, which also blurs the box's content. `MultiEffect` has no spread property (`shadowScale` is a scale factor).
- Shader-based emulations (inset shadow, invert, hue-rotate) now state that `ShaderEffect` needs precompiled `.qsb` shaders in Qt 6.
- `Text.font` has no `stretch` property; `font-stretch` maps to `font.variableAxes` (`wdth`) for variable fonts only. `font.features` and `font.overline` confirmed.
- `font.pixelSize` is an `int` (fractions are rounded); `font.family` is a single name, so the `Qt.fontFamilies()` fallback was dropped; added the missing `xx-small`, `x-small` and `xxx-large` keyword sizes.
- `line-height`: `Text.ProportionalHeight` scales the font's natural line height, not the font size, so unitless values now use `FixedHeight` (`n × font size`).
- `direction: rtl`: `Text` has no `layoutDirection`; now uses `LayoutMirroring`. Row-reverse via `layoutDirection` on layouts is confirmed to affect item order only.
- `sub`/`sup` need `Text.RichText`; `StyledText` does not support them. `overflow-wrap: anywhere`/`break-word` both map to `WrapAtWordBoundaryOrAnywhere`. `text-shadow` with `Text.Raised` only matches a (1px, 1px) offset. Added the `textFormat: Text.PlainText` rule.
- `ch`/`ex` now use `FontMetrics` (`advanceWidth("0")`, `xHeight`) evaluated at runtime; the converter cannot measure fonts at compile time. `TextMetrics` was the wrong type for `xHeight`.
- `position: sticky` used a nonexistent `Flickable.contentY` attached property; now binds to the `Flickable`'s `id`. `position: fixed` now uses a sibling of the scroller rather than reparenting into `Window.contentItem`.
- Transforms: `Item.x`/`y` are overwritten inside layouts (use `Translate`); `transformOrigin` only affects `rotation`/`scale`, not `Matrix4x4`; `matrix()` argument order given explicitly (`Qt.matrix4x4` is row-major with 16 arguments); multiple functions are multiplied into one matrix instead of relying on list order; non-uniform scale needs `Scale`.
- `width: auto` on block boxes fills the containing block; it is not `implicitWidth`. `display: block` now stacks in a `ColumnLayout` (a `Column` does not stretch children). `overflow: hidden` does not clip to rounded corners; added the mask approach. `visibility: hidden` also needs `enabled: false`. QML opacity is per item, not per group.
- `@font-face`: family name comes from `FontLoader.name`. `@media (prefers-color-scheme)` uses the confirmed `Qt.styleHints.colorScheme`. `:focus-visible` uses `Control.visualFocus`. `user-select: text` confirmed: `Text` has no selection, `TextEdit` does.
- Colors: named colors are resolved from the CSS Color 4 table rather than passed to Qt; system colors now map to confirmed `palette` roles including `accent` and `linkVisited`. Cursor mapping extended (`progress` → `BusyCursor`, grab, help, none, …).
- Image details: `PreserveAspectCrop` needs `clip: true`; `no-repeat` is `Image.Pad` (not `Stretch`); `BorderImage` tile modes added. `Animation.paused` confirmed; `animation-fill-mode` semantics corrected (QML keeps the last value, so `none` needs restoring).
