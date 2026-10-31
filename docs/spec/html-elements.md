# HTML element mapping

Target: Qt 6.11, QtQuick, QtQuick.Controls 2 (Basic style), QtQuick.Layouts, QtQuick.Shapes. QtMultimedia is needed for `video` and `audio`. WebView and QtWebEngine are excluded.

Support levels:
- **Full**: same visible result and behavior.
- **Approx**: visible result close but not identical.
- **Emulated**: needs generated helper code.
- **Ignored**: no visual effect, safe to drop.
- **Unsupported**: converter emits a warning.

Names marked **(verify)** have not been checked against the Qt 6.11 docs.

## Document and metadata

| Element | Box kind | QML output | Attributes mapped | Support | Notes |
|---|---|---|---|---|---|
| html | none | `ApplicationWindow` root | `dir` → `LayoutMirroring.enabled` | Approx | Root font and direction come from html styles |
| head | none | none | — | Ignored | Children are processed by the converter |
| title | metadata | `ApplicationWindow.title` | text → `title` | Full | First `<title>` only |
| base | metadata | none | `href` → base for URL resolution | Ignored | Applied at conversion time |
| link | metadata | `Window.icon` (`rel=icon`); `FontLoader.source` (font preload) | `href`, `rel` | Ignored | `rel=stylesheet` is consumed by the CSS pipeline |
| meta | metadata | `Accessible.description` on root (`name=description`) | `name`, `content` | Ignored | `viewport` and `charset` have no effect |
| style | metadata | none | — | Ignored | Consumed by the cascade, not emitted |
| body | block | root `Rectangle` inside `ApplicationWindow` | `background-color` → `color`; padding → `padding` | Full | Body background fills the window |
| script | metadata | JS functions and bindings | `src` → imported file; `type=module` → inlined | Emulated | See JS translation; no `eval`, no DOM |
| noscript | none | none | — | Ignored | Assumes scripting is enabled |
| template | none | `Component` | `id` → component id | Emulated | Cloned in JS via `createObject` |
| slot | inline | `Loader` or placeholder `Item` | `name` | Emulated | Shadow DOM is not supported in QML; slotted content is inlined |

## Sectioning and flow containers

| Element | Box kind | QML output | Attributes mapped | Support | Notes |
|---|---|---|---|---|---|
| article | block | `Column` in `Item` (+ `Rectangle` if styled) | `id` → `objectName` | Full | |
| section | block | `Column` | `id` | Full | |
| nav | block | `Column`; `Accessible.role: Pane` | `aria-label` → `Accessible.name` | Full | Landmark exposed as a pane |
| aside | block | `Column`; `Accessible.role: Pane` | `aria-label` → `Accessible.name` | Full | |
| header | block | `Column` | — | Full | Also used as a landmark when not inside a sectioning element |
| footer | block | `Column` | — | Full | Same as header |
| main | block | `Column`; `Accessible.role: Pane` | — | Full | |
| search | block | `Row` or `Column` | — | Approx | Newer element; treated as a `div` with landmark role |
| address | block | `Text` with `font.italic: true` | — | Full | |
| hgroup | block | `Column` | — | Ignored | Grouping only |
| div | block | `Item` / `Rectangle` | `style` via cascade | Full | `display:flex` → `RowLayout` / `ColumnLayout`; `display:grid` → `GridLayout`; plain flow → `Column` |
| figure | block | `Column` | — | Full | |
| figcaption | block | `Text` | — | Full | Sits under the image in the parent `Column` |

## Text blocks

| Element | Box kind | QML output | Attributes mapped | Support | Notes |
|---|---|---|---|---|---|
| h1–h6 | block | `Text` with `font.pointSize` / `font.bold` | `id` | Full | Default sizes follow UA stylesheet values |
| p | block | `Text` (`wrapMode: Text.Wrap`, `width` from parent) | — | Full | Inline content goes through "Inline formatting" |
| pre | block | `Text` with monospace `font.family`, `textFormat: Text.PlainText` | — | Approx | No `white-space` property on `Text`; whitespace is preserved only in plain text |
| blockquote | block | `Rectangle` (left border) with `Text` | `cite` → Accessible description | Approx | Padding and border from the cascade |
| hr | block | `Rectangle` (height 1) | — | Approx | `Accessible.role: Separator` (verify) |
| ol | block | `Column` of `li` rows | `start`, `reversed`, `type` | Approx | Markers are `Text` items; `type` picks the numbering style |
| ul | block | `Column` of `li` rows | `type` | Approx | Bullet is a `Text` item |
| menu | block | same as `ul` | — | Approx | |
| li | list-item | `Row` (marker `Text` + content `Item`) | `value` | Approx | `value` resets the counter |
| dl | block | `Column` | — | Full | |
| dt | block | `Text` with `font.bold: true` | — | Full | |
| dd | block | `Item` with left padding | — | Full | |

## Inline elements

Inline runs become one `Text`. See "Inline formatting" for when they split.

| Element | Box kind | QML output | Attributes mapped | Support | Notes |
|---|---|---|---|---|---|
| a | inline | `Text` with `<a href>` (StyledText) or `Text` + `MouseArea` | `href` → `Qt.openUrlExternally()`; `#id` → scroll to target; `title` → `ToolTip` | Approx | External links open in the system browser |
| em | inline | `<i>` in StyledText | — | Full | |
| strong | inline | `<b>` in StyledText | — | Full | |
| small | inline | `<small>` in StyledText (verify) | — | Approx | |
| s | inline | `font.strikeout: true` | — | Full | |
| cite | inline | `font.italic: true` | — | Full | |
| q | inline | text wrapped in curly quotes | `cite` → ignored | Approx | Quotes are inserted as text |
| dfn | inline | `font.italic: true` | — | Full | |
| abbr | inline | `Text` + `ToolTip` | `title` → `ToolTip.text` | Emulated | Tooltip needs hover handling |
| ruby | inline | `Column` pairing rt above base | — | Emulated | Qt `Text` has no ruby support |
| rt | inline | small `Text` above base | — | Emulated | Part of the ruby `Column` |
| rp | inline | none | — | Ignored | Fallback parentheses are not needed |
| data | inline | `Text` | `value` ignored | Full | |
| time | inline | `Text` | `datetime` ignored | Full | |
| code | inline | `Text` with monospace family | — | Approx | |
| var | inline | `Text` with `font.italic: true` and monospace | — | Approx | |
| samp | inline | `Text` with monospace family | — | Approx | |
| kbd | inline | `Text` with monospace family | — | Approx | No key-cap border; add `Rectangle` if styled |
| sub | inline | `<sub>` in StyledText (verify) | — | Approx | Shifts baseline |
| sup | inline | `<sup>` in StyledText (verify) | — | Approx | Shifts baseline |
| i | inline | `<i>` in StyledText | — | Full | |
| b | inline | `<b>` in StyledText | — | Full | |
| u | inline | `font.underline: true` | — | Full | |
| mark | inline | `Flow` item with background `Rectangle` | — | Emulated | Highlight needs a rectangle behind the text |
| bdi | inline | `Text` | `dir` → `LayoutMirroring` | Approx | Bidi isolation handled by the Qt text layout |
| bdo | inline | `Text` with `horizontalAlignment` | `dir` | Approx | Override direction only |
| span | inline | `Text` (plain) | cascade | Full | Split when styled (see Inline formatting) |
| br | inline | `\n` in text or `<br>` in StyledText | — | Full | |
| wbr | inline | none | — | Ignored | |
| ins | inline | `font.underline: true` + color | — | Approx | Color comes from the cascade |
| del | inline | `font.strikeout: true` | — | Full | |

## Media and embedded content

| Element | Box kind | QML output | Attributes mapped | Support | Notes |
|---|---|---|---|---|---|
| picture | replaced | `Image` (chosen source) | — | Emulated | Source chosen with media queries at conversion time or via `Screen` bindings |
| source | metadata | none (used by `picture`) | `srcset`, `media`, `type` | Emulated | |
| img | replaced | `Image` (`fillMode: Image.PreserveAspectFit`) | `src` → `source`; `alt` → `Accessible.name`; `width`/`height` → size; `loading=lazy` → `asynchronous: true` | Full | `srcset` picks one density at conversion time |
| iframe | replaced | none | — | Unsupported | WebView is excluded; warn and leave a placeholder |
| embed | replaced | `Image` for image types; otherwise none | `src`, `type` | Unsupported | Non-image types warn |
| object | replaced | `Image` for image data; otherwise none | `data`, `type` | Unsupported | Same as embed |
| video | replaced | `MediaPlayer` + `VideoOutput` (QtMultimedia) | `src` → `MediaPlayer.source`; `loop` → `loops: MediaPlayer.Infinite`; `muted` → `AudioOutput.muted`; `poster` → `Image` overlay | Emulated | Controls are generated `Row` of `Button`s |
| audio | replaced | `MediaPlayer` + `AudioOutput` (QtMultimedia) | `src`, `loop`, `muted` | Emulated | No visual; generated controls |
| track | metadata | none | `src`, `kind`, `srclang` | Unsupported | Subtitles are not rendered |
| map | block | none (wrapper for `area`) | `name` | Emulated | Hit areas are used by the `img` they belong to |
| area | none | `MouseArea` over the image region | `shape`, `coords`, `href`, `alt` | Emulated | `href` → `Qt.openUrlExternally()` |
| canvas | replaced | `Canvas` (QtQuick) | `width`, `height` | Emulated | `getContext("2d")` maps to the Canvas API subset |
| svg (inline) | replaced | `Shape` + `ShapePath` (`PathSvg`) | `viewBox` → scale; `fill`/`stroke` → `ShapePath` props | Approx | Gradients and filters are partial; text in SVG becomes `Text` |
| math | inline | none | — | Unsupported | Warn and show source text |

## Tables

| Element | Box kind | QML output | Attributes mapped | Support | Notes |
|---|---|---|---|---|---|
| table | table-part | `GridLayout`, or a `ColumnLayout` where a script may reach it (`layout.md` §8) | `border`, `cellpadding` → margins | Approx | `border-collapse` handled by drawn borders |
| caption | table-part | `Text` above the `GridLayout` | — | Approx | |
| colgroup | table-part | none (sets column widths) | `span`, `width` → `Layout.preferredWidth` | Approx | |
| col | table-part | none (sets column width) | `span`, `width` | Approx | |
| thead | table-part | none (rows are placed in grid); live: `ColumnLayout` | — | Ignored | Grouping only, unless live |
| tbody | table-part | none; live: `ColumnLayout` | — | Ignored | Grouping only, unless live |
| tfoot | table-part | none; live: `ColumnLayout` | — | Ignored | Grouping only, unless live |
| tr | table-part | row index for children; live: `Item` | — | Full | Used for `Layout.row` unless live |
| td | table-part | `Item` with `Layout.row`, `Layout.column`, `Layout.rowSpan`, `Layout.columnSpan` | `colspan`, `rowspan` | Full | |
| th | table-part | same as td, bold `Text` | `scope` → `Accessible.role` | Full | `scope` decides `ColumnHeader` or `RowHeader` |

## Forms

| Element | Box kind | QML output | Attributes mapped | Support | Notes |
|---|---|---|---|---|---|
| form | block | `Item` (no `action`) | `action`, `method` | Emulated | Submit runs JS handler; no network request |
| label | inline | `Label` (Controls) or `Text` | `for` → `Accessible.name` of target | Approx | |
| input | inline-block | one row per type (below) | — | see below | |
| button | inline-block | `Button` (Controls) | `disabled` → `enabled: false`; `type` | Full | |
| select | inline-block | `ComboBox` | `multiple` → Emulated list; `size` | Approx | `multiple` is Emulated with a `ListView` of `CheckBox` |
| datalist | none | `ComboBox` with `editable: true` | `id` (matched by `list`) | Approx | |
| optgroup | none | section header `Text` in model | `label` | Approx | |
| option | none | model entry | `value`, `selected` → `currentIndex` | Full | |
| textarea | inline-block | `TextArea` (Controls) | `rows`, `cols` → size; `readonly`; `placeholder` → `placeholderText`; `maxlength` → `maximumLength` | Approx | `rows`/`cols` are approximate in font metrics |
| output | inline | `Text` | `for` ignored | Approx | |
| progress | inline-block | `ProgressBar` | `value` → `value`; `max` → `to` | Full | `indeterminate` when `value` is absent (verify) |
| meter | inline-block | `ProgressBar` with color from low/high/optimum | `value`, `min`, `max`, `low`, `high`, `optimum` | Emulated | Color thresholds are generated |
| fieldset | block | `GroupBox` (Controls) or `Frame` | `disabled` → `enabled` | Approx | |
| legend | block | `GroupBox.title` | — | Approx | |
| details | block | `Column` with generated toggle | `open` → visibility | Emulated | Collapse animation is generated |
| summary | block | clickable header `Button` | — | Emulated | Acts as the toggle for `details` |
| dialog | block | `Dialog` (Controls) or `Popup` | `open` → `open()`; `modal` → `Popup.modal` | Approx | Opening is driven by JS calls |
| script | metadata | see `script` in Document table | — | Emulated | |
| noscript | none | none | — | Ignored | |

### Input types

| Type | QML output | Attributes mapped | Support | Notes |
|---|---|---|---|---|
| text | `TextField` | `value` → `text`; `placeholder` → `placeholderText`; `maxlength` → `maximumLength`; `readonly` | Full | |
| password | `TextField` with `echoMode: TextInput.Password` (verify) | `placeholder` | Full | |
| email | `TextField` with `inputMethodHints: Qt.ImhEmailCharactersOnly` | `pattern` → Emulated validator | Approx | Validation is generated JS |
| number | `SpinBox` | `min` → `from`; `max` → `to`; `step` → `stepSize`; `value` | Approx | Decimal steps are approximate |
| range | `Slider` | `min` → `from`; `max` → `to`; `step` → `stepSize`; `value` → `value` | Full | |
| checkbox | `CheckBox` | `checked` → `checked`; `value` | Full | |
| radio | `RadioButton` in `ButtonGroup` | `name` → shared `ButtonGroup`; `checked` | Full | Grouping comes from `name` |
| date | `TextField` with `Popup` containing a generated calendar | `value`, `min`, `max` | Emulated | No built-in date input in Basic |
| color | `Rectangle` swatch with `ColorDialog` (QtQuick.Dialogs) | `value` → `color` | Emulated | Opens system color dialog |
| file | `Button` with `FileDialog` (QtQuick.Dialogs) | `accept`, `multiple` | Emulated | Selected files are kept in JS |
| search | `TextField` | `placeholder`, `maxlength` | Approx | Clear button is generated |
| tel | `TextField` with `inputMethodHints: Qt.ImhDialableCharactersOnly` | `pattern` | Approx | |
| url | `TextField` with `inputMethodHints: Qt.ImhUrlCharactersOnly` | `pattern` | Approx | |
| submit | `Button` | `value` → text | Emulated | Triggers form submit handler |
| button | `Button` | `value` → text | Full | |
| reset | `Button` | `value` → text | Emulated | Resets bound values |
| hidden | none | `value` stored in JS | Ignored | |

## Inline formatting

**A run of inline content becomes one `Text`.** Plain text, `b`, `i`, `a`, `span`, `code`, `br`, `sub`, `sup` and the other inline elements are combined into a single `Text` item. The converter picks one of these formats:

- **`Text.StyledText`** (default when the run uses only supported tags). Tags supported in Qt 6 (verify exact list):
  `a` (href), `b`, `big`, `br`, `font` (color, size, face), `h1`–`h6`, `i`, `img`, `li`, `ol`, `p`, `s`, `small`, `sub`, `sup`, `u`, `ul`.
- **`Text.RichText`** (used for `pre`/`table`-like content or when more HTML is needed). It accepts the `QTextDocument` HTML subset, including `blockquote`, `center`, `code`, `dl`/`dt`/`dd`, `em`, `hr`, `pre`, `strong`, `span` with inline style (`color`, `font-size`, `font-weight`, `font-family`, `text-decoration`), `table`/`tr`/`td` (verify). Scripts and external stylesheets are not supported.
- **`Text.PlainText`** when the run has no markup.

**The converter splits the run into a `Flow` of separate `Text` items** when any of these is true:
- The inline element has `background`, `border`, `padding`, or `margin`.
- The element is `mark`, or a `span` with a background.
- The element has a click handler or hover state (`onclick`, `:hover`).
- The element is `inline-block` or `img` with CSS sizing beyond intrinsic size.
- The element has `vertical-align` other than baseline, `letter-spacing`, `text-shadow`, or `transform`.
- The element is `ruby`.
- Two adjacent runs need different `line-height`.

## Accessibility

Set `Accessible.role` (an attached property) on the item that represents the element. Only QAccessible role names are used. Roles marked **(verify)** should be checked against Qt 6.11.

| ARIA role | Implicit source | Accessible.role | Support |
|---|---|---|---|
| button | `button`, `input[type=button/submit/reset]` | `Accessible.Button` | Full |
| checkbox | `input[type=checkbox]` | `Accessible.CheckBox` | Full |
| radio | `input[type=radio]` | `Accessible.RadioButton` | Full |
| combobox | `select` (single), `datalist` | `Accessible.ComboBox` | Full |
| textbox | `input[type=text/email/tel/url]`, `textarea` | `Accessible.EditableText` | Full |
| searchbox | `input[type=search]` | `Accessible.EditableText` | Full |
| spinbutton | `input[type=number]` | `Accessible.SpinBox` | Full |
| slider | `input[type=range]` | `Accessible.Slider` | Full |
| progressbar | `progress`, `meter` | `Accessible.ProgressBar` | Full |
| link | `a[href]`, `area[href]` | `Accessible.Link` | Full |
| img | `img[alt]`, `svg` with title | `Accessible.Graphic` | Full |
| list | `ul`, `ol`, `menu` | `Accessible.List` | Full |
| listitem | `li` | `Accessible.ListItem` | Full |
| table | `table` | `Accessible.Table` | Full |
| row | `tr` | `Accessible.Row` | Full |
| cell / gridcell | `td` | `Accessible.Cell` | Full |
| columnheader | `th[scope=col]` | `Accessible.ColumnHeader` | Full |
| rowheader | `th[scope=row]` | `Accessible.RowHeader` | Full |
| heading | `h1`–`h6` | none; use `Accessible.StaticText` with name (verify whether a heading role exists) | Approx |
| dialog / alertdialog | `dialog` | `Accessible.Dialog` | Full |
| tooltip | `abbr[title]` (generated) | `Accessible.ToolTip` | Full |
| menu / menubar / menuitem | `menu` (context) | `Accessible.Menu`, `Accessible.MenuBar`, `Accessible.MenuItem` | Approx |
| toolbar | `nav` with toolbar role | `Accessible.ToolBar` | Approx |
| tab / tablist / tabpanel | tabs built from `details`/`summary` | `Accessible.PageTab`, `Accessible.PageTabList`, `Accessible.Pane` (verify) | Emulated |
| separator | `hr` | `Accessible.Separator` (verify) | Full |
| group | `fieldset`, `details` | `Accessible.Grouping` (verify) | Approx |
| document | `body` | `Accessible.Document` (verify) | Approx |
| region / landmark | `nav`, `aside`, `main`, `header`, `footer` | `Accessible.Pane` | Approx |
| text / paragraph / generic | `p`, `span`, `div` | `Accessible.StaticText` or `Accessible.Pane` | Full |

Also set these attached properties where they apply: `Accessible.name` (from `alt`, `aria-label`, or `label for`), `Accessible.description`, `Accessible.checkable` and `Accessible.checked`, `Accessible.readOnly`, `Accessible.passwordEdit`, `Accessible.multiLine`, and `Accessible.ignored` for decorative elements.
