# DOM & Web API Mapping to QML (`eldur_js`)

This specification defines the subset of Browser DOM, Event, and Web APIs supported by Eldur when rewriting JavaScript (`crates/eldur_js/src/lower.rs`) for execution in Qt 6.11 QML/V4 (`crates/eldur_js/src/eldur_runtime.js`), how each construct is lowered or shimmed, and what is unsupported and why.

---

## 1. Architecture Overview

JavaScript in HTML documents is transformed into two QML-compatible artifacts:
1. **`page.js`**: A `.pragma library` (or QML-imported script) containing the rewritten user code, top-level declarations, generated event handler functions (`__eldur_on_<event>_<id>`), and an initialization entry point (`__eldur_init(rootItem)`).
2. **`eldur_runtime.js` (`Eldur`)**: A `.pragma library` runtime helper providing:
   - Timer polyfills (`setTimeout`, `setInterval`, `requestAnimationFrame`) backed by dynamically instantiated `QtQuick.Timer` objects.
   - `localStorage` (backed by `QtQuick.LocalStorage` SQLite with in-memory fallback) and `sessionStorage` (in-memory `Map`).
   - `fetch()` polyfill backed by QML's built-in `XMLHttpRequest`.
   - Dynamic DOM tree walk, CSS selector matching (`tag`, `#id`, `.class1.class2`, `[attr]`, `[attr=val]`, `[attr^=val]`, `[attr$=val]`, `[attr*=val]`, `[attr~=val]`, `[attr|=val]`, descendant `A B`, child `A > B`, and comma-separated selector lists), `classList`, `dataset`, `style`, `getComputedStyle`, dynamic `addEventListener` / `removeEventListener` / `dispatchEvent`, `EventTarget`, typed `Event` constructors, and `Element` wrapper (`Eldur.wrap(item)`).

Rewriting uses **Static Resolution First, Runtime Fallback Second**:
- If a DOM query (`document.getElementById('save-btn')`) can be statically resolved to a unique `QmlTarget` at compile time, property accesses and event registrations on that reference are rewritten directly to the QML item `id` (`save_btn.text`, `save_btn.visible`, etc.) or `Eldur.wrap(save_btn)` for structural/DOM-specific operations.
- Static resolution is kept only where it is provably equivalent to asking the live DOM. `Analysis::live_queries` lists the literal lookups a script on the page could change the answer of (id writes, removals, parsed markup, clones, moves of page elements, writes to a class the selector names, text writes that can empty an element); those go to `Eldur.document.*` at runtime. The check is flow-insensitive, because a handler can run at any time.
- A resolved element is the same object to scripts as a looked-up one: wherever it escapes as a value (argument, `this`, object property, comparison, returned value) it is `Eldur.wrap(<qml_id>)`. Direct property accesses stay on the QML item, where nothing can observe the difference.
- If a query is dynamic (`document.getElementById(dynamicId)`), returns multiple elements (`querySelectorAll`), or cannot be statically resolved, it falls back to `Eldur.document.*` and `Eldur.wrap(item)` at runtime.

---

## 2. Document Query & Creation APIs

| Browser API | Static Lowering (when selector is literal & unique) | Runtime Fallback (`eldur_runtime.js`) | Status |
| :--- | :--- | :--- | :--- |
| `document.getElementById(id)` | Elided or replaced with `<qml_id>` | `Eldur.document.getElementById(id)` (searches QML `children` tree by `objectName == id`) | Supported & tested |
| `document.querySelector(sel)` | Elided or replaced with `<qml_id>` | `Eldur.document.querySelector(sel)` (supports compound `tag#id.cls[attr]`, `A B`, `A > B`, comma lists) | Supported & tested |
| `document.querySelectorAll(sel)` | `[<qml_id_1>, <qml_id_2>, ...].map(Eldur.wrap)` | `Eldur.document.querySelectorAll(sel)` (`NodeList` with `.item()`, `.forEach()`, `.entries()`, `.keys()`, `.values()`) | Supported & tested |
| `document.getElementsByClassName(cls)` | Static list if known, else runtime | `Eldur.document.getElementsByClassName(cls)` (`HTMLCollection` with `.item()`, `.namedItem()`, `forEach === undefined`) | Supported & tested |
| `document.getElementsByTagName(tag)` | Static list if known, else runtime | `Eldur.document.getElementsByTagName(tag)` (`HTMLCollection` with `.item()`, `.namedItem()`, `forEach === undefined`) | Supported & tested |
| `el.querySelector(sel)` / `el.querySelectorAll(sel)` | `Eldur.wrap(<id>).querySelector(sel)` / `querySelectorAll(sel)` | `Eldur.wrap(el).querySelector(sel)` / `querySelectorAll(sel)` (`NodeList`) | Supported & tested |
| `el.getElementsByClassName(cls)` / `el.getElementsByTagName(tag)` | `Eldur.wrap(<id>).getElementsByClassName(cls)` / `getElementsByTagName(tag)` | `Eldur.wrap(el).getElementsByClassName(cls)` / `getElementsByTagName(tag)` (`HTMLCollection`) | Supported & tested |
| `el.matches(sel)` | `Eldur.wrap(<id>).matches(sel)` | `Eldur.wrap(el).matches(sel)` | Supported & tested |
| `el.closest(sel)` | `Eldur.wrap(<id>).closest(sel)` | `Eldur.wrap(el).closest(sel)` (walks self then ancestors via `_logicalParent`) | Supported & tested |
| `el.contains(other)` | `Eldur.wrap(<id>).contains(other)` | `Eldur.wrap(el).contains(other)` (true if `other === el` or descendant) | Supported & tested |
| `document.body` / `document.documentElement` | Root content `QmlTarget` (`root` or `body`) | `Eldur.document.body` / `Eldur.document.documentElement` (`Eldur.wrap(root)`) | Supported & tested |
| `document.head` | `Eldur.document.head` | Synthetic `<head>` element wrapper | Supported & tested |
| `document.title` (get / set) | `Eldur.getTitle()` / `Eldur.setTitle(val)` | Reads/writes `root.title` if `Window`, else internal title string | Supported & tested |
| `document.activeElement` | `Eldur.document.activeElement` | Focused QML item (`activeFocus` or `focus`) wrapped in `Eldur.wrap`, defaulting to `document.body` | Supported & tested |
| `document.hidden` / `document.visibilityState` | `Eldur.document.hidden` / `Eldur.document.visibilityState` | `false` / `"visible"` (unless root window is hidden) | Supported & tested |
| `document.createElement(tag)` | `Eldur.document.createElement(tag)` | Dynamically creates QML item via `Qt.createQmlObject` wrapped in `Eldur.wrap` | Supported & tested |
| `document.createTextNode(text)` | `Eldur.document.createTextNode(text)` | Dynamically creates QML `Text` item wrapped in `Eldur.wrap` | Implemented (untested) |
| `document.createDocumentFragment()` | `Eldur.document.createDocumentFragment()` | Lightweight fragment container (`nodeType === 11`); children splice into parent on insertion | Supported & tested |

Runtime selector matching inspects `objectName` for `#id`, `eldurTag` (with QML type-name inference fallback for common controls) for tag selectors, and `eldurClasses` / `cls_<name>` booleans for class selectors. On a converted page whose scripts reach elements at run time, `eldur_convert` emits `eldurTag` and `eldurClasses` on every element that has its own QML object (see [Section 11](#converter-metadata-eldur_convert)).

---

## 3. Element Properties & Attributes

### 3.1 Text & Value Properties

Mapping depends on the `TargetKind` of the resolved `QmlTarget`:

| DOM Property | `TargetKind::Text` / `Button` | `TargetKind::Box` (`Rectangle` / `Item`) | `TargetKind::TextField` / `TextArea` | `TargetKind::CheckBox` / `RadioButton` | `TargetKind::Select` (`ComboBox`) | `TargetKind::Range` (`Slider` / `SpinBox`) / `Progress` |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `el.textContent` / `el.innerText` | `<id>.text` | `<id>Text.text` (or `Eldur.getText(<id>)` / `Eldur.setText(<id>, v)`) | `<id>.text` | `<id>.text` | `<id>.currentText` | N/A |
| `el.innerHTML` (read) | `<id>.text` | `Eldur.getInnerHTML(<id>)` | `<id>.text` | `<id>.text` | `<id>.currentText` | N/A |
| `el.innerHTML = html` | `Eldur.setInnerHTML(<id>, html)` | `Eldur.setInnerHTML(<id>, html)` (parses simple tags or sets RichText) | N/A | N/A | N/A | N/A |
| `el.value` (read) | `Eldur.wrap(<id>).value` | `Eldur.wrap(<id>).value` | `<id>.text` | `<id>.value` (via `Eldur.wrap`, defaults to `"on"`) | `<id>.currentText` | `String(<id>.value)` for `Range` (`Slider`/`SpinBox`); `<id>.value` for `Progress` |
| `el.value = v` | `Eldur.wrap(<id>).value = v` | `Eldur.wrap(<id>).value = v` | `<id>.text = String(v)` | `Eldur.wrap(<id>).value = v` | `Eldur.selectByValue(<id>, v)` | `<id>.value = Number(v)` |
| `el.checked` | N/A | N/A | N/A | `<id>.checked` | N/A | N/A |
| `el.disabled` | `!<id>.enabled` | `!<id>.enabled` | `!<id>.enabled` | `!<id>.enabled` | `!<id>.enabled` | `!<id>.enabled` |
| `el.hidden` | `!<id>.visible` | `!<id>.visible` | `!<id>.visible` | `!<id>.visible` | `!<id>.visible` | `!<id>.visible` |
| `el.placeholder` | N/A | N/A | `<id>.placeholderText` | N/A | N/A | N/A |
| `el.readOnly` | N/A | N/A | `<id>.readOnly` | N/A | N/A | N/A |
| `el.selectedIndex` | N/A | N/A | N/A | N/A | `<id>.currentIndex` | N/A |
| `el.min` / `el.max` / `el.step` | `Eldur.wrap(<id>).min` / `max` / `step` | `Eldur.wrap(<id>).min` / `max` / `step` | `Eldur.wrap(<id>).min` / `max` / `step` | N/A | N/A | Updates both `m.attrs` (`min`/`max`/`step`) and QML `<id>.from` / `<id>.to` / `<id>.stepSize` on `Slider` and `SpinBox` |

### 3.2 Reflected Properties, `a.href` Resolution, and Control Range Syncing

- **`a.href`, `img.src`, and Document Base URL Resolution (`Eldur.init(rootItem, baseUrl?)`)**:
  - `Eldur.init(rootItem, baseUrl?)` accepts an optional `baseUrl` parameter (or reads `rootItem.baseUrl` if present) and initializes `Eldur.location`.
  - `a.getAttribute("href")` and `img.getAttribute("src")` return the raw attribute string (and extract `href="..."` from `<a href="...">` inside QML `Text.text` when `eldur_convert` emits rich text).
  - `a.href` and `img.src` resolve the attribute against the document base URL configured via `Eldur.init(root, baseUrl)` or `root.baseUrl`:
    - Absolute URLs (`https://example.com`, `https://example.com/a/b/../c?q=1#h`) are normalized per URL rules (bare origins receive a trailing `/`, and `.` / `..` path segments are collapsed, working around Qt 6.11 V4's non-normalizing `new URL()`).
    - Non-hierarchical schemes (`mailto:`, `tel:`, `javascript:`, `data:`, `blob:`) are returned verbatim.
    - When `Eldur.init(root, baseUrl)` or `root.baseUrl` provides an `http:`/`https:`/`file:` base URL, relative URLs (`/path`, `sub/page.html`, `../up`, `?query`, `#hash`) on both `a.href` and `img.src` resolve against that base URL; if no base URL is configured, the raw attribute value is returned unchanged.
    - Assigning `location.href` (or calling `location.assign` / `location.replace`) with a non-hash URL logs a runtime warning (`console.warn`) and does **not** navigate or set the document base URL.
- **Range (`Slider`) and Number (`SpinBox`) `min` / `max` / `step` Syncing**:
  - Setting `el.min`, `el.max`, `el.step` or calling `el.setAttribute("min" | "max" | "step", v)` on a `Slider` (`<input type="range">`) or `SpinBox` (`<input type="number">`) updates the QML control's `from`, `to`, and `stepSize` properties in addition to storing the string attribute.
  - If an `<input>` created via `document.createElement("input")` has `min`/`max`/`step` set before `input.type = "range"` or `input.type = "number"`, those attributes are re-applied to the newly created `Slider` or `SpinBox` (`from`/`to`/`stepSize`).

### 3.3 Geometry & Layout Read Properties

| DOM Property / Method | QML Lowering | Notes |
| :--- | :--- | :--- |
| `el.offsetWidth` / `el.clientWidth` | `<id>.width` | Read-only number |
| `el.offsetHeight` / `el.clientHeight` | `<id>.height` | Read-only number |
| `el.offsetLeft` | `<id>.x` | Read-only number |
| `el.offsetTop` | `<id>.y` | Read-only number |
| `el.scrollWidth` / `el.scrollHeight` | `Eldur.wrap(<id>).scrollWidth` / `scrollHeight` | Computes scrollable content extent across `Flickable`, `eldur_convert` clipped overflow containers (`clip: true` + inner layout `implicitWidth`/`implicitHeight`), and child bounds |
| `el.scrollTop` / `el.scrollLeft` | `Eldur.wrap(<id>).scrollTop` / `scrollLeft` | Clamps to `[0, scrollDim - clientDim]`, updates `Flickable` `contentY`/`contentX` or `eldur_convert` clipped inner layout `anchors.topMargin`/`leftMargin` (or direct child positions), and dispatches `scroll` |
| `el.getBoundingClientRect()` | `Eldur.getRect(<id>)` | Returns `{ x, y, left, top, width, height, right, bottom }` using `mapToItem(root, 0, 0)` |

### 3.4 `classList`, `className`, `dataset`, and Attributes

| Browser API | Lowering | Notes |
| :--- | :--- | :--- |
| `el.className` (get / set) | `Eldur.getClassName(<id>)` / `Eldur.setClassName(<id>, v)` | Reads/updates `eldurClasses` string and triggers style state re-evaluation |
| `el.classList.add(...c)` | `Eldur.classListAdd(<id>, ...c)` | Mutates `eldurClasses` and applies any class-bound properties |
| `el.classList.remove(...c)` | `Eldur.classListRemove(<id>, ...c)` | Mutates `eldurClasses` |
| `el.classList.toggle(c, force?)` | `Eldur.classListToggle(<id>, c, force)` | Returns boolean |
| `el.classList.contains(c)` | `Eldur.classListContains(<id>, c)` | Returns boolean |
| `el.classList.replace(oldC, newC)` | `Eldur.wrap(<id>).classList.replace(oldC, newC)` | Replaces `oldC` with `newC` and returns boolean |
| `el.dataset.<camelKey>` (get / set) | `Eldur.getDataset(<id>, "<camelKey>")` / `Eldur.setDataset(<id>, "<camelKey>", v)` | Backed by per-item metadata table initialized from HTML `data-*` attributes |
| `el.getAttribute(name)` | `Eldur.getAttribute(<id>, name)` | Special-cases `id`, `class`, `disabled`, `hidden`, `value`, `href`, `src`, `min`, `max`, `step`, `data-*`, plus arbitrary attributes |
| `el.setAttribute(name, val)` | `Eldur.setAttribute(<id>, name, val)` | Updates backing property (`from`/`to`/`stepSize`, `enabled`, `visible`, etc.) + attribute table |
| `el.removeAttribute(name)` | `Eldur.removeAttribute(<id>, name)` | Clears attribute and resets reflected boolean properties (`disabled`, `hidden`, `checked`, `readOnly`) |
| `el.hasAttribute(name)` | `Eldur.hasAttribute(<id>, name)` | Returns boolean |
| `el.toggleAttribute(name, force?)` | `Eldur.toggleAttribute(<id>, name, force)` | Toggles attribute presence and returns boolean |

---

## 4. Inline Style (`el.style.<prop>`), `style.cssText`, and `getComputedStyle`

### 4.1 Static Property Lowering (when `el` is a resolved `QmlTarget`)

| `el.style.<prop>` | QML Property on `<id>` | Conversion Helper |
| :--- | :--- | :--- |
| `display` | `<id>.visible` | `val !== "none"` (reads as `<id>.visible ? "block" : "none"`) |
| `visibility` | `<id>.visible` | `val !== "hidden"` |
| `opacity` | `<id>.opacity` | `Eldur.cssNumber(val, 1)` |
| `backgroundColor` / `background` | `<id>.color` (on `Rectangle`) | `Eldur.cssColor(val)` |
| `color` | `<id>.color` (on `Text`) or `<id>Text.color` (on `Box`) | `Eldur.cssColor(val)` |
| `width` / `height` | `<id>.width` / `<id>.height` | `Eldur.cssPx(val)` (strips `"px"` suffix, parses float) |
| `minWidth` / `minHeight` / `maxWidth` / `maxHeight` | `<id>.Layout.minimumWidth` / etc. | `Eldur.cssPx(val)` |
| `left` / `top` | `<id>.x` / `<id>.y` | `Eldur.cssPx(val)` |
| `fontSize` | `<id>.font.pixelSize` | `Eldur.cssPx(val)` |
| `fontWeight` | `<id>.font.weight` / `<id>.font.bold` | `val === "bold" \|\| Number(val) >= 600` |
| `fontStyle` | `<id>.font.italic` | `val === "italic"` |
| `textAlign` | `<id>.horizontalAlignment` | `Eldur.cssTextAlign(val)` (`Text.AlignLeft`, `Text.AlignHCenter`, `Text.AlignRight`) |
| `borderRadius` | `<id>.radius` (on `Rectangle`) | `Eldur.cssPx(val)` |
| `borderColor` | `<id>.border.color` (on `Rectangle`) | `Eldur.cssColor(val)` |
| `borderWidth` | `<id>.border.width` (on `Rectangle`) | `Eldur.cssPx(val)` |
| `zIndex` | `<id>.z` | `Eldur.cssNumber(val, 0)` |

### 4.2 Style Helper Methods, `cssText`, and `getComputedStyle`

| Browser API | Lowering | Notes |
| :--- | :--- | :--- |
| `el.style.setProperty(name, val)` | `Eldur.wrap(<id>).style.setProperty(name, val)` | Supports both standard CSS properties (`background-color`, `width`, etc.) and CSS custom properties (`--*`) |
| `el.style.getPropertyValue(name)` | `Eldur.wrap(<id>).style.getPropertyValue(name)` | Reads custom properties (`--*`) or mapped inline style values |
| `el.style.removeProperty(name)` | `Eldur.wrap(<id>).style.removeProperty(name)` | Removes custom or inline property and returns previous value |
| `el.style.cssText` (get / set) | `Eldur.wrap(<id>).style.cssText` | Parses `"prop: val; ..."` declarations and applies each via `setStyle` / `setProperty` (emits `Unsupported` static diagnostic because arbitrary CSS declarations cannot be statically bound in QML) |
| `window.getComputedStyle(el)` / `getComputedStyle(el)` | `Eldur.getComputedStyle(el)` | Returns live-like style view with `getPropertyValue(name)` and camelCase properties (`display`, `visibility`, `opacity`, `width`, `height`, `color`, `backgroundColor`, `borderRadius`, `borderWidth`, `fontSize`) |

---

## 5. DOM Tree Traversal & Mutation

| Browser API | Lowering / Runtime Support |
| :--- | :--- |
| `el.parentElement` / `el.parentNode` | `Eldur.wrap(el).parentElement` (unwraps internal layout containers like `ColumnLayout`/`RowLayout`/`GridLayout` to the logical parent element) |
| `el.children` / `el.childNodes` | `Eldur.wrap(el).children` (`HTMLCollection` with `.item(i)`, `.namedItem(name)`, named/id properties, `forEach === undefined`) / `childNodes` (`NodeList` with `.item(i)`, `.forEach()`, `.entries()`, `.keys()`, `.values()`), skipping internal `<id>Text` label children on `Box` items |
| `el.childElementCount` | `Eldur.wrap(el).childElementCount` |
| `el.firstElementChild` / `el.lastElementChild` | `Eldur.wrap(el).firstElementChild` / `lastElementChild` |
| `el.firstChild` / `el.lastChild` | `Eldur.wrap(el).firstChild` / `lastChild` |
| `el.nextElementSibling` / `el.previousElementSibling` | `Eldur.wrap(el).nextElementSibling` / `previousElementSibling` |
| `el.nextSibling` / `el.previousSibling` | `Eldur.wrap(el).nextSibling` / `previousSibling` |
| `parent.appendChild(child)` | `Eldur.wrap(parent).appendChild(child)` (supports `DocumentFragment`) |
| `parent.removeChild(child)` | `Eldur.wrap(parent).removeChild(child)` |
| `parent.insertBefore(newNode, refNode)` | `Eldur.wrap(parent).insertBefore(newNode, refNode)` |
| `parent.replaceChild(newChild, oldChild)` | `Eldur.wrap(parent).replaceChild(newChild, oldChild)` |
| `parent.append(...nodesOrStrings)` | `Eldur.wrap(parent).append(...)` (converts string arguments to text nodes) |
| `parent.prepend(...nodesOrStrings)` | `Eldur.wrap(parent).prepend(...)` |
| `el.before(...nodesOrStrings)` / `el.after(...nodesOrStrings)` | `Eldur.wrap(el).before(...)` / `after(...)` |
| `el.replaceWith(...nodesOrStrings)` | `Eldur.wrap(el).replaceWith(...)` |
| `parent.replaceChildren(...nodesOrStrings)` | `Eldur.wrap(parent).replaceChildren(...)` |
| `el.insertAdjacentHTML(pos, html)` | `Eldur.wrap(el).insertAdjacentHTML(pos, html)` (`"beforebegin"`, `"afterbegin"`, `"beforeend"`, `"afterend"`) |
| `el.insertAdjacentElement(pos, targetEl)` | `Eldur.wrap(el).insertAdjacentElement(pos, targetEl)` |
| `el.insertAdjacentText(pos, text)` | `Eldur.wrap(el).insertAdjacentText(pos, text)` |
| `el.remove()` | `Eldur.wrap(el).remove()` (`item.parent = null; item.visible = false; item.destroy()`) |
| `el.cloneNode(deep?)` | `Eldur.wrap(el).cloneNode(deep)` (clones tag, classes, attributes, text/value, and optionally children) |

---

### 5.1 Containers a script fills

An element's form follows what it holds in the document, so the converter
looks for the boxes a script may put block children in (`Gains` in
`scripts.rs`): the parents of created non-inline elements, and receivers of a
node or markup analysis cannot name. A receiver analysis cannot name makes
every queried element without element children a candidate.

- A box with text or nothing gets a `ColumnLayout` below its `Text`; both take
  no height while empty, as an empty CSS box takes none.
- An empty flex container places its spacers at run time (`eldurPack`), and an
  empty grid gives each item its cell and column size (`eldurTracks`).
- A table with no rows stacks its sections, and created rows lay out their
  cells in a `RowLayout`. Column widths come from each cell's own `width`, not
  from the table's column solver.
- Elsewhere the runtime makes the column when a block arrives (`_appendTarget`).

Reading geometry (`getBoundingClientRect`, `offset*`, `client*`, `scroll*`,
`window.scrollX/Y`) first lays out what changed, as a browser does.

## 6. Focus, Blur, and Scrolling APIs

| Browser API | Static Lowering (on resolved `QmlTarget`) | Runtime Support (`eldur_runtime.js`) |
| :--- | :--- | :--- |
| `el.focus()` | `<id>.forceActiveFocus()` | `Eldur.focus(el)` (sets `focus = true`, calls `forceActiveFocus()`, updates `document.activeElement`, dispatches `focus` if not already focused) |
| `el.blur()` | `<id>.focus = false` | `Eldur.blur(el)` (sets `focus = false`, clears `document.activeElement`, dispatches `blur`) |
| `el.click()` | `Eldur.wrap(<id>).click()` | Dispatches cancelable bubbling `click` event on `el` |
| `window.scrollTo(x, y \| opts)` / `window.scroll(x, y \| opts)` | `Eldur.scrollTo(x, y)` | Updates `window.scrollX`/`scrollY` (`pageXOffset`/`pageYOffset`), syncs root `Flickable` (`contentX`/`contentY`) if present, and dispatches `scroll` |
| `window.scrollBy(dx, dy \| opts)` | `Eldur.scrollBy(dx, dy)` | Adds `(dx, dy)` to current window scroll position |
| `el.scrollTo(x, y \| opts)` / `el.scrollBy(dx, dy \| opts)` | `Eldur.wrap(<id>).scrollTo(...)` / `scrollBy(...)` | Updates element `scrollLeft`/`scrollTop` across `Flickable` (`contentX`/`contentY`) and `eldur_convert` clipped overflow containers (`anchors.leftMargin`/`topMargin`), clamping to `[0, scrollDim - clientDim]` and dispatching `scroll` |
| `el.scrollIntoView(arg?)` | `Eldur.wrap(<id>).scrollIntoView(arg)` | Scrolls window to element's mapped `(x, y)` coordinates |

---

## 7. Event Mapping

### 7.1 Supported Events & QML Signal Bindings

| DOM Event | `TargetKind` | QML Signal / Handler | Synthetic `event` Object Properties |
| :--- | :--- | :--- | :--- |
| `click` / `dblclick` | `Button` / `CheckBox` / `RadioButton` | `onClicked` / `clicked` | `{ type, target, currentTarget, button: 0, clientX, clientY, preventDefault(), stopPropagation(), stopImmediatePropagation(), defaultPrevented }` |
| `click` / `dblclick` | `Box` / `Text` / `Image` | Injected `MouseArea { anchors.fill: parent; onClicked / onDoubleClicked }` | `{ type, target, currentTarget, clientX: mouse.x, clientY: mouse.y, button: mouse.button, ... }` |
| `mousedown` / `mouseup` / `pointerdown` / `pointerup` / `touchstart` / `touchend` | `Button` / `MouseArea` | `pressed` / `released` | `{ type, target, currentTarget, ... }` |
| `contextmenu` | `MouseArea` | `canceled` / programmatic dispatch | `{ type: "contextmenu", target, currentTarget, ... }` |
| `input` | `TextField` / `TextArea` | `onTextEdited` / `textChanged` | `{ type: "input", target, currentTarget, data: <id>.text, ... }` |
| `input` / `change` | `Range` (`Slider` / `SpinBox`) | `onMoved` / `onValueModified` / `valueChanged` | `{ type, target, currentTarget, ... }` |
| `change` | `TextField` | `onEditingFinished` | `{ type: "change", target, currentTarget, ... }` |
| `change` | `CheckBox` / `RadioButton` | `onToggled` | `{ type: "change", target, currentTarget, ... }` |
| `change` | `Select` (`ComboBox`) | `onActivated` | `{ type: "change", target, currentTarget, ... }` |
| `submit` | `Box` (`<form>`) | Triggered when a submit `Button` inside the form is clicked or `Enter` is pressed in a form `TextField` | `{ type: "submit", target, currentTarget, preventDefault(), defaultPrevented }` |
| `keydown` / `keyup` / `keypress` | Any `Item` | `Keys.onPressed` / `Keys.onReleased` (`focus: true` enabled on target) | `{ type, key: Eldur.keyName(event.key, event.text), code: Eldur.keyCode(event.key), keyCode: Eldur.domKeyCode(event.key), ctrlKey, shiftKey, altKey, metaKey, repeat: event.isAutoRepeat, preventDefault() (sets event.accepted = true) }` |
| `focus` / `blur` / `focusin` / `focusout` | Any `Item` | `activeFocusChanged` and `focusChanged` (deduplicated per focus state transition) | `{ type, target, currentTarget }` |
| `mouseenter` / `mouseleave` / `mouseover` / `mouseout` | Any `Item` | `hoveredChanged` or `MouseArea { hoverEnabled: true; onEntered; onExited }` | `{ type, target, currentTarget }` |
| `DOMContentLoaded` / `load` | `document` / `window` | Executed at the end of `__eldur_init(root)` inside `Component.onCompleted` | `{ type: "DOMContentLoaded" \| "load", target: Eldur.document }` |
| `resize` | `window` | `root.widthChanged` / `root.heightChanged` | `{ type: "resize", target: Eldur.window }` |
| `scroll` | `window` / `el` | Dispatched on `scrollTo` / `scrollBy` / `scrollIntoView` | `{ type: "scroll", target }` |
| `hashchange` | `window` | Dispatched when `location.hash` changes | `{ type: "hashchange", target: Eldur.window }` |
| `popstate` | `window` | Dispatched on `history.back()` / `forward()` / `go()` | `{ type: "popstate", state, target: Eldur.window }` |
| `visibilitychange` | `document` | Dispatched if visibility state changes | `{ type: "visibilitychange", target: Eldur.document }` |

### 7.2 Event Constructors & `EventTarget`

- `new Event(type, opts?)` and `new CustomEvent(type, opts?)` (with `detail`, `bubbles`, `cancelable`, `preventDefault()`, `stopPropagation()`, `stopImmediatePropagation()`, `defaultPrevented`).
- Typed event constructors: `new KeyboardEvent(type, opts?)`, `new MouseEvent(type, opts?)`, `new PointerEvent(type, opts?)`, `new FocusEvent(type, opts?)`, `new InputEvent(type, opts?)`, `new WheelEvent(type, opts?)`.
- `new EventTarget()`: standalone event target supporting `addEventListener`, `removeEventListener`, and `dispatchEvent`.

### 7.3 Static vs. Dynamic Event Registration

1. **Inline HTML attributes** (`<button onclick="doWork(event)">`):
   - Extracted during HTML analysis (`extract_inline_handlers`).
   - Lowered into a generated function `function __eldur_attr_onclick_<id>(event) { doWork(event); }` in `page.js` and emitted as an `EventBinding`.
2. **`el.addEventListener(type, fn, opts?)` on a statically resolved `QmlTarget`**:
   - If the target has a single static listener and does not use `removeEventListener`, `dispatchEvent`, `{ once: true }`, or mixed `on<event>` + `addEventListener`, it takes the static fast path (`__eldur_on_<type>_<id>_<n>`).
   - Otherwise, all listeners on `(target, type)` are unified onto `Eldur.listen` / `Eldur.unlisten` / `Eldur.setOn` / `Eldur.dispatchEvent` with a single QML forwarding handler.
3. **`el.on<event> = fn`**:
   - Lowered to a reassignable handler slot or `Eldur.setOn(<id>, "<event>", fn)` when unified with `addEventListener`.

---

4. **Dispatch by path**: if a listener could hear another element (a bubbling listener on an element with element children, a listener on `document`, `window`, `body` or an unresolved element) or a script wants an inline node, the page is `dispatch_by_path`. Every listener, property and attribute handler of that page then goes through `Eldur.listen`/`Eldur.setOn`, in Chromium's order (a property or attribute keeps the place where it was first set). A handler attribute on an inline node has a root property `eldurNode<key>`, set from `Eldur.inlineNode(key)` at start. Other pages keep the static bindings above: nothing above the target listens, so nothing needs a path.

## 8. Timers, Storage, Network, Observers & Browser Globals

| Browser Global / API | Lowering in `page.js` | Implementation in `eldur_runtime.js` | Status |
| :--- | :--- | :--- | :--- |
| `setTimeout(fn, ms, ...args)` | `Eldur.setTimeout(fn, ms, ...args)` | Creates one-shot `QtQuick.Timer`, returns integer timer ID | Supported & tested |
| `clearTimeout(id)` | `Eldur.clearTimeout(id)` | Stops and destroys timer `id` | Supported & tested |
| `setInterval(fn, ms, ...args)` | `Eldur.setInterval(fn, ms, ...args)` | Creates repeating `QtQuick.Timer`, returns integer timer ID | Supported & tested |
| `clearInterval(id)` | `Eldur.clearInterval(id)` | Stops and destroys timer `id` | Supported & tested |
| `requestAnimationFrame(fn)` | `Eldur.requestAnimationFrame(fn)` | 16 ms one-shot `QtQuick.Timer` passing `Eldur.performance.now()` timestamp | Supported & tested |
| `cancelAnimationFrame(id)` | `Eldur.cancelAnimationFrame(id)` | Calls `clearTimeout(id)` | Supported & tested |
| `queueMicrotask(fn)` | `Promise.resolve().then(fn)` | Native V4 `Promise` microtask queue | Supported & tested |
| `performance.now()` | `Eldur.performance.now()` | `Date.now() - _startTime` | Supported & tested |
| `localStorage` | `Eldur.localStorage` | `getItem`, `setItem`, `removeItem`, `clear`, `key`, `length` backed by `QtQuick.LocalStorage` SQLite (with in-memory fallback) | Supported & tested |
| `sessionStorage` | `Eldur.sessionStorage` | Same API backed by in-memory object | Supported & tested |
| `fetch(url, opts?)` | `Eldur.fetch(url, opts)` | `Promise` wrapping QML `XMLHttpRequest`; supports `.ok`, `.status`, `.statusText`, `.headers.get()`, `.text()`, `.json()` | Supported & tested |
| `alert(msg)` | `Eldur.alert(msg)` | Logs `[alert] <msg>` via `console.warn` and invokes optional alert hook | Supported & tested |
| `confirm(msg)` / `prompt(msg, def?)` | `Eldur.confirm(msg)` / `Eldur.prompt(msg, def)` | Logs unsupported warning and returns `false` / `null` (emits `Unsupported` diagnostic) | Stubbed & tested |
| `atob(s)` / `btoa(s)` | `Eldur.atob(s)` / `Eldur.btoa(s)` | Standard Web API Latin1 Base64 encode/decode | Supported & tested |
| `structuredClone(val)` | `Eldur.structuredClone(val)` | Recursive structural clone preserving arrays/dates/plain objects | Supported & tested |
| `window.innerWidth` / `window.innerHeight` | `Eldur.window.innerWidth` / `Eldur.window.innerHeight` | Reads `root.width` / `root.height` (fallback `1280` / `800`) | Supported & tested |
| `window.scrollX` / `scrollY` / `pageXOffset` / `pageYOffset` | `Eldur.window.scrollX` / `scrollY` / `pageXOffset` / `pageYOffset` | Tracks window scroll offset | Supported & tested |
| `window.devicePixelRatio` | `Eldur.window.devicePixelRatio` | `Qt.styleHints.mousePressAndHoldInterval ? 1 : 1` (defaults to `1`) | Supported & tested |
| `window.getComputedStyle(el)` | `Eldur.getComputedStyle(el)` | Computed style view for `el` | Supported & tested |
| `window.matchMedia(query)` | `Eldur.matchMedia(query)` | Evaluates `(min-width)`, `(max-width)`, `(min-height)`, `(max-height)`, and `(prefers-color-scheme: dark)` against `root` | Supported & tested |
| `location` (`href`, `origin`, `protocol`, `host`, `hostname`, `port`, `pathname`, `search`, `hash`, `assign`, `replace`, `reload`) | `Eldur.location` | Parses assigned URLs, serves as document base URL for `a.href`, and dispatches `hashchange` on `window` when `hash` changes (emits `Unsupported` static diagnostic because QML has no multi-page navigation) | Shimmed & tested |
| `history` (`state`, `length`, `pushState`, `replaceState`, `back`, `forward`, `go`) | `Eldur.history` | In-memory session history stack; updates `location` and dispatches `popstate` on `back()`/`forward()`/`go()` (emits `Unsupported` static diagnostic) | Shimmed & tested |
| `navigator` (`userAgent`, `language`, `languages`, `onLine`, `platform`, `clipboard.writeText`, `clipboard.readText`) | `Eldur.navigator` | In-memory async `Promise` clipboard + environment metadata (emits `Unsupported` static diagnostic) | Shimmed & tested |
| `MutationObserver` / `ResizeObserver` / `IntersectionObserver` | `Eldur.MutationObserver` / `Eldur.ResizeObserver` / `Eldur.IntersectionObserver` | Safe no-op observer constructors with `observe()`, `unobserve()`, `disconnect()`, `takeRecords()` so scripts constructing observers do not throw `ReferenceError` (emits `Unsupported` static diagnostic) | Stubbed & tested |
| `console.log` / `warn` / `error` / `info` / `debug` | Unchanged (native in QML) | `console.table` lowered to `Eldur.consoleTable` | Supported & tested |

---

## 9. JavaScript Language Lowering for Qt 6.11 V4

Qt 6.11's V4 engine supports ES2015–ES2020 core syntax (`let`, `const`, arrow functions, template literals, destructuring declarations, `for..of`, `Map`, `Set`, `Symbol`, `Promise`, optional chaining `?.`, nullish coalescing `??`) but lacks or has quirks with several newer constructs. `eldur_js` automatically lowers the following syntax into V4-safe ES6:

1. **Object Spread (`{ ...a, x: 1 }`)** → `Object.assign({}, a, { x: 1 })`.
2. **Object Rest Destructuring (`const { a, ...rest } = obj`)** → `const __eldur_tmp = obj, a = __eldur_tmp.a, rest = Eldur.objectRest(__eldur_tmp, ["a"])`.
3. **Logical Assignment Operators (`a &&= b`, `a ||= b`, `a ??= b`)** → `a && (a = b)`, `a || (a = b)`, `(a !== null && a !== undefined) ? a : (a = b)`.
4. **Optional Catch Binding (`try { ... } catch { ... }`)** → `try { ... } catch (__eldur_err) { ... }`.
5. **ES6 `class` Declarations with Public Instance/Static Fields** → Lowered by moving field initializers into the `constructor` (or after the class for `static` fields). Private `#fields` emit an `Unsupported` diagnostic.
6. **`async function` / `await` (simple linear & try/catch forms)** → Lowered to `.then()` chains where possible; complex control flow (`await` inside loops) emits an `Unsupported` diagnostic.
7. **ES Modules (`<script type="module">`, `import`, `export`)** → Local module scripts are topologically ordered and bundled into `page.js` with per-module hygienic symbol renaming (`__eldur_m<N>_<name>`); external URL `import`s emit an `Unsupported` diagnostic.

---

## 10. Unsupported APIs & Why

The following browser APIs are not supported in QML and emit `Diagnostic` entries (`Severity::Unsupported` or `Severity::Warning`) during `analyze()` / `rewrite()`:

- **Multi-window navigation & blocking modal dialogs (`window.open`, `window.close`, `window.print`, synchronous `confirm()`, `prompt()`)**: Qt Quick runs in a single off-screen or embedded QML scene without browser tab management or synchronous event-loop-blocking modal text prompts (`confirm` returns `false`, `prompt` returns `null`).
- **Dynamic code evaluation (`eval`, `new Function`)**: Disallowed for static analysis predictability and QML `.pragma library` scope isolation.
- **Background threads & network sockets (`Worker`, `SharedWorker`, `ServiceWorker`, `WebSocket`, `EventSource`, `BroadcastChannel`)**: Qt Quick's V4 JS environment does not expose Web Workers, Service Workers, or browser `WebSocket` constructors without additional C++ plugin modules.
- **Browser storage & binary databases (`IndexedDB`, `caches`, `cookie`)**: QML provides `QtQuick.LocalStorage` (used by `Eldur.localStorage`), not IndexedDB or HTTP cookie jars.
- **Web Components & Shadow DOM (`customElements`, `attachShadow`, `ShadowRoot`, `HTMLTemplateElement.content`)**: QML uses its own component system (`Component` / `Qt.createQmlObject`) rather than HTML Shadow DOM encapsulation.
- **Rich text selection ranges (`document.createRange`, `window.getSelection`, `Range`, `Selection`)**: QML `Text` and `TextField` do not expose DOM `Node`/character offset `Range` trees.
- **2D/WebGL Canvas & Media (`canvas.getContext`, `Audio`, `Video`, `WebGLRenderingContext`, `OffscreenCanvas`)**: `eldur_convert` maps `<canvas>` to a placeholder QML `Rectangle` rather than a full HTML5 `CanvasRenderingContext2D` / WebGL surface.
- **Live layout/mutation observer callbacks (`MutationObserver`, `ResizeObserver`, `IntersectionObserver`)**: Constructor stubs are provided in `Eldur` so page scripts do not throw `ReferenceError` on `.observe()` / `.disconnect()`, and `analyze()` emits an `Unsupported` diagnostic because arbitrary DOM mutation records and intersection thresholds are not synthesized automatically.
- **Unsupported JS runtime intrinsics (`BigInt`, `WeakRef`, `FinalizationRegistry`, `Proxy`, private `#fields`)**: Not supported by Qt 6.11's V4 JavaScript engine.

---

## 11. Coverage

An item counts as tested only when a test checks its result. Across the **142** DOM & Web API table items in Sections 2–8 (and **149** total items including the **7** Section 9 language-lowering items):

- **Asserted by a `qml6` runtime test** (`crates/eldur_js/tests/qml_runtime.rs`): **106** DOM/Web API items (**113** including all 7 Section 9 lowering items)
- **Asserted only by a rewrite test** (`crates/eldur_js/tests/rewrite_tests.rs`): **10** items
- **Not asserted at all**: **26** items

| Section | Total Items | `qml6` Runtime Test | Rewrite Test Only | Not Asserted |
| :--- | ---: | ---: | ---: | ---: |
| 2. Document Query & Creation APIs | 18 | 15 | 1 | 2 |
| 3.1 Text & Value Properties | 12 | 12 | 0 | 0 |
| 3.3 Geometry & Layout Read Properties | 7 | 5 | 0 | 2 |
| 3.4 `classList`, `className`, `dataset`, and Attributes | 12 | 10 | 0 | 2 |
| 4.1 Static Inline Style Properties | 16 | 4 | 1 | 11 |
| 4.2 Style Helpers, `cssText`, and `getComputedStyle` | 5 | 5 | 0 | 0 |
| 5. DOM Tree Traversal & Mutation | 21 | 17 | 2 | 2 |
| 6. Focus, Blur, and Scrolling APIs | 7 | 6 | 1 | 0 |
| 7.1 Supported Events & QML Signal Bindings | 19 | 12 | 1 | 6 |
| 8. Timers, Storage, Network, Observers & Browser Globals | 25 | 20 | 4 | 1 |
| **Subtotal (Sections 2–8 DOM & Web APIs)** | **142** | **106** | **10** | **26** |
| 9. JS Language Lowering for Qt 6.11 V4 | 7 | 7 | 0 | 0 |
| **Total (Sections 2–9)** | **149** | **113** | **10** | **26** |

- **10 items asserted only by a rewrite test**: `document.body` / `document.documentElement`, `el.style.backgroundColor` / `background`, `el.insertAdjacentElement`, `el.insertAdjacentText`, `el.click()`, `mouseenter`/`mouseleave`/`mouseover`/`mouseout`, `cancelAnimationFrame`, `fetch`, `alert`, `confirm` / `prompt`.
- **26 items not asserted at all**: `document.head`, `document.createTextNode`, `el.offsetLeft`, `el.offsetTop`, `el.removeAttribute`, `el.hasAttribute`, `el.style.visibility`, `el.style.minWidth`/`minHeight`/`maxWidth`/`maxHeight`, `el.style.left`/`top`, `el.style.fontSize`, `el.style.fontWeight`, `el.style.fontStyle`, `el.style.textAlign`, `el.style.borderRadius`, `el.style.borderColor`, `el.style.borderWidth`, `el.style.zIndex`, `parent.removeChild`, `parent.insertBefore`, `mousedown`/`mouseup`/`pointerdown`/`pointerup`/`touchstart`/`touchend`, `contextmenu`, `change` on `TextField`, `submit`, `resize`, `visibilitychange`, `queueMicrotask`.

### Converter Metadata (`eldur_convert`)

What `eldur_convert` emits for the runtime (`docs/design/dynamic-styles.md` §7, §8.1):

| Name | On | When |
|---|---|---|
| `property string eldurTag` (lower-case tag) | every element with its own QML object | the page's analysis has `runtime_dom` (runtime selectors, traversal, `matches`/`closest`, event targets, or DOM mutation) |
| `property string eldurClasses` (the `class` attribute as written) | same, when the element has a non-empty `class` | same |
| `objectName` (the HTML id) | same, when the element has an `id` | same (and, as before, on every element a script reaches by a literal query) |
| `property bool cls_<s>` (initially whether the element has the class) | static elements a script writes classes on through a literal query, for each class it names | the page has class writes |
| `property var eldurStyledClasses` (sorted classes that selectors name) | root | `runtime_dom` |
| `property string baseUrl` | root | the page has any script |

`baseUrl` is the page's URL for `http(s)` inputs, `Qt.resolvedUrl("Main.qml")` for local inputs (the converted app, next to which assets are copied), and `<base href>` resolved against either. Pages with no element access beyond literal queries are byte-identical to before apart from `baseUrl`; pages without scripts are unchanged. Classes that sanitise to the same `cls_` name (`a-b`, `a_b`) get an `info[js]` diagnostic. Elements merged into a `StyledText` run have no object and no metadata.

Asserted end to end in qml6 against Chromium's answers for the same page (`crates/eldur_convert/tests/metadata_e2e.rs`, values recorded by `tests/metadata/chromium.mjs`): `el.matches` (class, tag and class, non-matching class and tag), `el.closest` (by class, by tag, no match → `null`) and reading `.id`/`.className` of the result, `classList.contains`, `classList.toggle` followed by `matches`, `document.querySelector` by class and by descendant class selector, `tagName`, `className`, `location.href`, and `a.href` for relative, `../` and `/` URLs, both from the page URL and from `<base href>`. For a local page `location.href` is the converted `Main.qml`, not the source file (dynamic-styles §14, decision 2).

Known gaps:
- Inline elements and options are nodes without objects: see §12.
- `cls_<s>` is not yet declared on elements reached without a literal query (`event.target`, `this`) or on created elements (dynamic-styles S6/S7).

## 12. Inline nodes

A run of inline content is one `Text`, so `<span id>`, `<b>`, inline `<a>`, and `<option>` have no QML object of their own. To a script they are still nodes. Measured against Chromium in `crates/eldur_convert/tests/inline_dom_e2e.rs`.

**Design.** The converter declares what is inside such a `Text` (or a select's `ComboBox`) as `readonly property string eldurInline`: the nodes as written, with attributes in order, and each element's computed style. Only pages whose scripts could reach these nodes get it: an inline element or option in the script's `wanted` set forces `runtime_dom`, and `eldurInline` follows `runtime_dom`. A page with no script emits what it did. The root's `eldurInlineRules` is the cascade's input for the properties a run can show (`eldur_css::inline_rules`), present when the page has a model, or a script that mutates the DOM and holds a block with its own text, since a created element may land there.

At run time each node is a JS object that passes `_isQObject`, so `wrap`, `_meta`, selector matching and the tree walks need no second path; the funnels (`getText`, `setText`, `setStyle`, `getAttribute`, the wrapper's mutators) start with a hook for it. Once a node changes, or a rule matching it does, the runtime writes the `Text`'s markup from the model. A `Text` nothing touched keeps the text the converter wrote, byte for byte.

Why this and not one QML object per element, or splitting the block whenever a script may touch it: `Text` is what wraps a line and shapes it across elements. A `Flow` of pieces breaks at element edges rather than mid-element and loses kerning and baseline agreement (`docs/design/dynamic-styles.md` §6.8). Virtual nodes keep paint identical until a script writes, and a write is one `Text` update, so the block's layout follows by itself.

**Style.** A node's computed value is the converter's until the signature of what matches it (rule indices, inline style, `hidden`) moves from its first value, or its parent's does. Then the runtime runs the cascade over `eldurInlineRules`: origin, `!important`, specificity, order, inheritance from the node's parent or the block's live font. Spares every page the cost until a write, and keeps the two cascades from disagreeing on values no script changed. Selector matching is the runtime's own (`_matchesList`), with a quick reject on the rule's last simple selector. Any class change, on any element, recomputes the rendered and the now-dirty runs.

**Hit testing.** A tap asks which node is under the point. `Text` has `linkAt` and nothing like `positionAt`. Making each run of the painted text a link would change its paint (link colour, what a tap does), so each model has a hidden copy, same width, font, wrap and line height, with every run a link named after its node. `linkAt` on the copy names the run; it is laid out by the same engine from the same text, so it is right on every line of a wrapped element, where measuring would not be. `getBoundingClientRect` scans the copy in 2 px steps, so an inline box's rectangle is approximate; the copy exists only once something asks.

**Events.** Not every handler under the point hears a tap: a link and a control take the press for themselves, and those that do hear hear separately. On a page that dispatches by path (below), the first to hear (`_beginTap`) finds the target and runs the whole path with one shared event; the rest find their turn taken. A control's `clicked`, once any click listener exists on the page, starts the path from the control, so an ancestor hears a control with no listener. A label's tap runs the label's click, then the control's unless prevented. Capture listeners run from the root down, then at the target in registration order, then bubbling from the target up. So `target`, `stopPropagation`, `stopImmediatePropagation` and `preventDefault` behave as in a browser. `onLinkActivated` and a bare link's `onTapped` check `preventDefault`.

**Writes.** `textContent`, `innerHTML` (a small inline-markup parser), `className`/`classList`, `style`, attributes, `hidden`, and `append`/`remove`/`insertBefore`/`replaceWith` and the rest. A created element is a QML object until it goes into a line of text, then becomes a node, keeping its wrapper; only tags that are inline by default do. `createElement("option")` is a node from the start.

**Options.** Nodes of their select, with HTML's selectedness on the node: `selected`, `value`, `index`, `text`, `label`, `defaultSelected`, and on the select `options` (live, `length` truncates), `selectedOptions`, `add`, `remove(i)`, `item`, `new Option(...)`. A change rebuilds the `ComboBox`'s `model`; the user picking a line writes the flag back.

**Known gaps.**
- A run with an atomic inline (`<img>`, `<button>`, inline-block) stays a `Flow`, and its inline elements have no nodes; the converter's "no QML object of its own" warning stays for them.
- Whitespace-only text between blocks is not a node, and a real element's `childNodes` adds only its text runs to its element children, so counts differ when whitespace is all there is.
- On a page whose elements analysis can all name (§13) the table holds no `var()`; nested rules and `@layer` order come from the richer table only there too. `:is()` specificity takes the maximum for every match. `text-transform` is not applied to a rewritten run. `display: block` on an inline node paints it inline.
- A change to a real element's attribute, other than a class, does not recompute inline nodes.
- A `Text` shown as its one element's style goes back to the block's on first write, using what the converter computed; a later class change on the block is then read live.
- A node moved into a box that lays out only real items becomes a real item, as text and inline style: best effort.
- `mouseover`, `mouseout`, `mouseenter`, `mouseleave` and `focus`/`blur` are heard on the element that listens; the path only serves click, input, change, keydown, keyup, submit, focusin and focusout, and only `click` and the key and form events are checked against Chromium.
- A radio's `preventDefault` unchecks it but does not recheck the radio it replaced. Enter in a text field fires `submit` but not a synthetic `click` on the submit button.
- A tap on a box that overflows its parent reaches only handlers on boxes containing the point; a created box gets its own handler for this, a static one does not.
- On a page not dispatching by path, a text field's static `change` listener still runs on `editingFinished`, edited or not.
- `<select>` change, `dblclick`, and controls added through `innerHTML` have no Chromium comparison; a control so added is not connected until a later listener sweeps the page.
- Created block children of a `<div>` that was empty at conversion are not stacked: the div is a `Text`, and they overlap at its top.
- `dataset` writes do not recompute styles.

## 13. Created elements

Whether a page's created elements are styled by templates or at run time is
decided per page, by `Analysis::open_elements`. It is set when analysis cannot
name what an element is or which classes it gets: `createElement(expr)`,
`el.className = expr` and other open class writes, a variable reassigned to
another `createElement`, or markup with a variable inside a tag.

**Closed pages** (no such site) keep their templates and variants. A template
can paint what one generic box cannot (gradients, shadows, transforms,
transitions, `:hover`), its output is what the corpus has always checked, and
pages that need nothing new keep their QML.

**Open pages** get `eldurOpenElements: true`, the rule table
(`eldur_css::runtime_rules`, `eldurInlineRules`: every declaration of a box or
text, with `var()` kept, nested selectors composed, `:is()` measured per part,
layer ranks, and `@media` settled at the conversion viewport), `eldurStyle` on
each element and `eldurLiteralClasses`. CSS parsing stays in the converter; the
`_g*` section of the runtime only matches and applies.

On an open page a created element is one generic box, with a synchronous pass
in JavaScript (`_gSettle`): Qt's layouts have no margin collapse, no baseline
line boxes and no answer to a read in the same tick. Each pass re-resolves
`_gMatched` by DOM place, inherits along the DOM parent, and places flow, flex,
grid, inline-block and out-of-flow children. An exact-key template stays only
when a matched rule needs what the box cannot paint (`_gTemplateFits`). A static
element whose matched rules change through a class with no literal variant is
adopted to the box form (`_gStaticChanges`, `_gAdopt`), as is a static empty
container filled with inline-level children (`_gFillsLine`).

**Known gaps.**
- A closed page's template keeps the converter's picture, except its margins:
  after a script changes the tree, the runtime works out each changed column's
  gaps, collapse through parents included, and holds each by a binding that keeps
  the carry of a fractional height. `var()` in an inline rule is not seen.
- A generic box paints linear and radial gradients (one image over the padding
  box: `background-size`, `-position` and `-repeat` warn), box-shadow as bands of
  a hard or stepped edge, 2-D transforms, opacity and square overflow clips.
  Rounded clips of children, `filter`, `animation-name`, 3-D transform functions,
  and values only a script writes (the table holds what the sheets say) warn at
  run time, naming the element and property. Transitions run on background-color,
  opacity and transform, with Qt's nearest easing; others warn and apply at once.
- `:hover`, `:active`, `:focus`, `:checked` on a box or control of an open page;
  on a closed page only through a template. Table-family elements keep their old
  forms. A computed read of `em`-based shadows or transforms has no parsed value.
- Grid is column tracks with auto placement; explicit placement is not honoured.
- `font` shorthand holding `var()` is not expanded.
- `@media` is not re-evaluated on a live resize.
- Inline nodes in a box always sit in its `Text`, before its block children.
- Inline-level created elements directly under a static parent with static
  siblings, or under `body`, do not form lines with them.
- Normal line height comes from Qt's metrics: 17 at 16px where Chromium has 18.
- Fonts are whole pixels, so control text is 13px against Chromium's 13.3333px
  and `getComputedStyle` reports `sans-serif` where Chromium says `Arial`.
- `<select>` is as wide as its current option, not its widest, within Qt's
  hinted glyph widths. `<input>` width is measured for Arial. `<textarea>` and
  `type=number` default sizes still differ from Chromium.
