# CSS layout to QML layout strategy

Target: Qt 6.11, QtQuick, QtQuick.Controls (Basic style), QtQuick.Layouts, QtQuick.Shapes, QtQuick.Effects. Every type and property named here was checked against the Qt 6.11 `.qmltypes` files; behaviour claims marked "measured" were checked with `qml6` off-screen.

## 0. Container selection

| CSS construct | QML container | Support | Notes |
|---|---|---|---|
| Root document | `ApplicationWindow` > `ScrollView` > page `Item` | Full | Body scrolls inside the ScrollView. |
| Block container (normal flow) | `ColumnLayout` | Full | See §1 for why. |
| Inline run of text | `Text { textFormat: Text.StyledText }` | Approx | See §2. |
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

## 2. Inline formatting context

**Decision:** one `Text` per inline formatting context, with `textFormat: Text.StyledText`. Use `Flow` only when inline-block elements sit in the line. Inline images use the StyledText `<img>` tag.

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
| `text-align` | Full | `horizontalAlignment`. |
| `text-decoration` | Full | `<u>`, `<s>`, or `font.underline`/`font.strikeout` (whole `Text` only). |
| `text-shadow` | Approx | Second `Text` behind, offset by `x`/`y` in the shadow colour (blur is lost), or `style: Text.Raised` with `styleColor` for a 1px shadow. |
| Inline `img` | Full | `<img src="…" width="W" height="H" align="middle">` inside the StyledText. |
| `inline-block` | Emulated | Build a `Flow` of per-word `Text` items and the block. Baselines are aligned by offsetting `y` using `baselineOffset`. |
| `vertical-align` | Approx | `y` offset from `baselineOffset`, only in the `Flow` emulation. |

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

The containing block is the padding box of the nearest positioned ancestor. In QML the child must be a direct child of an unlayouted `Item` that is that ancestor. If the ancestor is a `ColumnLayout` or `RowLayout`, the absolute child cannot be inside it, because the Layout would position it.

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
| Auto size with only `left` or `top` | Approx | Size from `implicitWidth`/`implicitHeight` (CSS shrink-to-fit). |
| Nearest positioned ancestor is a Layout | Emulated | Wrap that Layout in a plain `Item` and overlay the absolute child. |
| `z-index` | Approx | `z`, see §5.1. |
| `position: static` | Full | No change. |

### 5.3 `position: fixed`

Fixed elements are siblings of the ScrollView inside the `ApplicationWindow`; their `parent` is the window's `contentItem`, which is the containing block (the viewport). They do not scroll.

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
| Auto column widths | Approx | Specified or max-content, at least min-content; extra space in proportion to max-content, as measured in Chromium. Min-content is the longest word, measured with `FontMetrics`. |
| `table-layout: fixed` | Emulated | `<col>` widths and the first row; the rest shared equally. |
| `<col>` / `<colgroup>` widths | Full | `span` and the `width` attribute included. A `width` on a spanning cell is ignored, with a diagnostic. |
| `border-collapse: collapse` | Approx | CSS 2.1 §17.6.2.1 picks each edge; a cell draws its half. A spanning cell's side uses the widest segment along it, so differing segments are not reproduced. Border styles other than solid draw solid. |
| `border-collapse: separate`, `border-spacing` | Full | `columnSpacing`, `rowSpacing`, and insets for the outer edges. |
| `thead`/`tbody`/`tfoot` | Full | First `thead` first, `tfoot` last, in any source order. |
| `caption`, `caption-side` | Full | An outer `ColumnLayout` holding captions and the table. |
| Row, group and column backgrounds | Emulated | Items cannot overlap in a `GridLayout`, so each cell paints them, flattened source-over. Background images on rows and groups are reported. |
| `vertical-align` in cells | Approx | `top`, `middle`, `bottom`; `baseline` is the top. |
| Scripts targeting rows or groups | Unsupported | They have no QML object. Warn. |
| `visibility: collapse`, row `opacity` | Unsupported | Reported. |
| `empty-cells`, presentational attributes (`cellspacing`, `bgcolor`, …) | Unsupported | Not read. |
| Sticky `th` | Approx | See §5.4. |

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
