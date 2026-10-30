# DOM & Web API Mapping to QML (`h2q_js`)

This specification defines the subset of Browser DOM, Event, and Web APIs supported by `html-2-qml` when rewriting JavaScript (`crates/h2q_js/src/lower.rs`) for execution in Qt 6.11 QML/V4 (`crates/h2q_js/src/h2q_runtime.js`), how each construct is lowered or shimmed, and what is unsupported and why.

---

## 1. Architecture Overview

JavaScript in HTML documents is transformed into two QML-compatible artifacts:
1. **`page.js`**: A `.pragma library` (or QML-imported script) containing the rewritten user code, top-level declarations, generated event handler functions (`__h2q_on_<event>_<id>`), and an initialization entry point (`__h2q_init(rootItem)`).
2. **`h2q_runtime.js` (`H2Q`)**: A `.pragma library` runtime helper providing:
   - Timer polyfills (`setTimeout`, `setInterval`, `requestAnimationFrame`) backed by dynamically instantiated `QtQuick.Timer` objects.
   - `localStorage` (backed by `QtQuick.LocalStorage` SQLite with in-memory fallback) and `sessionStorage` (in-memory `Map`).
   - `fetch()` polyfill backed by QML's built-in `XMLHttpRequest`.
   - Dynamic DOM tree walk, CSS selector matching (`tag`, `#id`, `.class1.class2`, `[attr]`, `[attr=val]`, `[attr^=val]`, `[attr$=val]`, `[attr*=val]`, `[attr~=val]`, `[attr|=val]`, descendant `A B`, child `A > B`, and comma-separated selector lists), `classList`, `dataset`, `style`, `getComputedStyle`, dynamic `addEventListener` / `removeEventListener` / `dispatchEvent`, `EventTarget`, typed `Event` constructors, and `Element` wrapper (`H2Q.wrap(item)`).

Rewriting uses **Static Resolution First, Runtime Fallback Second**:
- If a DOM query (`document.getElementById('save-btn')`) can be statically resolved to a unique `QmlTarget` at compile time, property accesses and event registrations on that reference are rewritten directly to the QML item `id` (`save_btn.text`, `save_btn.visible`, etc.) or `H2Q.wrap(save_btn)` for structural/DOM-specific operations.
- If a query is dynamic (`document.getElementById(dynamicId)`), returns multiple elements (`querySelectorAll`), or cannot be statically resolved, it falls back to `H2Q.document.*` and `H2Q.wrap(item)` at runtime.

---

## 2. Document Query & Creation APIs

| Browser API | Static Lowering (when selector is literal & unique) | Runtime Fallback (`h2q_runtime.js`) | Status |
| :--- | :--- | :--- | :--- |
| `document.getElementById(id)` | Elided or replaced with `<qml_id>` | `H2Q.document.getElementById(id)` (searches QML `children` tree by `objectName == id`) | Supported & tested |
| `document.querySelector(sel)` | Elided or replaced with `<qml_id>` | `H2Q.document.querySelector(sel)` (supports compound `tag#id.cls[attr]`, `A B`, `A > B`, comma lists) | Supported & tested |
| `document.querySelectorAll(sel)` | `[<qml_id_1>, <qml_id_2>, ...].map(H2Q.wrap)` | `H2Q.document.querySelectorAll(sel)` | Supported & tested |
| `document.getElementsByClassName(cls)` | Static list if known, else runtime | `H2Q.document.getElementsByClassName(cls)` | Supported & tested |
| `document.getElementsByTagName(tag)` | Static list if known, else runtime | `H2Q.document.getElementsByTagName(tag)` | Supported & tested |
| `el.querySelector(sel)` / `el.querySelectorAll(sel)` | `H2Q.wrap(<id>).querySelector(sel)` / `querySelectorAll(sel)` | `H2Q.wrap(el).querySelector(sel)` / `querySelectorAll(sel)` | Supported & tested |
| `el.getElementsByClassName(cls)` / `el.getElementsByTagName(tag)` | `H2Q.wrap(<id>).getElementsByClassName(cls)` / `getElementsByTagName(tag)` | `H2Q.wrap(el).getElementsByClassName(cls)` / `getElementsByTagName(tag)` | Supported & tested |
| `el.matches(sel)` | `H2Q.wrap(<id>).matches(sel)` | `H2Q.wrap(el).matches(sel)` | Supported & tested |
| `el.closest(sel)` | `H2Q.wrap(<id>).closest(sel)` | `H2Q.wrap(el).closest(sel)` (walks self then ancestors via `_logicalParent`) | Supported & tested |
| `el.contains(other)` | `H2Q.wrap(<id>).contains(other)` | `H2Q.wrap(el).contains(other)` (true if `other === el` or descendant) | Supported & tested |
| `document.body` / `document.documentElement` | Root content `QmlTarget` (`root` or `body`) | `H2Q.document.body` / `H2Q.document.documentElement` (`H2Q.wrap(root)`) | Supported & tested |
| `document.head` | `H2Q.document.head` | Synthetic `<head>` element wrapper | Supported & tested |
| `document.title` (get / set) | `H2Q.getTitle()` / `H2Q.setTitle(val)` | Reads/writes `root.title` if `Window`, else internal title string | Supported & tested |
| `document.activeElement` | `H2Q.document.activeElement` | Focused QML item (`activeFocus` or `focus`) wrapped in `H2Q.wrap`, defaulting to `document.body` | Supported & tested |
| `document.hidden` / `document.visibilityState` | `H2Q.document.hidden` / `H2Q.document.visibilityState` | `false` / `"visible"` (unless root window is hidden) | Supported & tested |
| `document.createElement(tag)` | `H2Q.document.createElement(tag)` | Dynamically creates QML item via `Qt.createQmlObject` wrapped in `H2Q.wrap` | Supported & tested |
| `document.createTextNode(text)` | `H2Q.document.createTextNode(text)` | Dynamically creates QML `Text` item wrapped in `H2Q.wrap` | Supported & tested |
| `document.createDocumentFragment()` | `H2Q.document.createDocumentFragment()` | Lightweight fragment container (`nodeType === 11`); children splice into parent on insertion | Supported & tested |

Every convert-emitted QML element carries `objectName: "<html-id>"` (when an HTML `id` is present), `property string h2qTag: "<tag>"`, and `property string h2qClasses: "<space-separated-classes>"` when scripts use runtime queries or class mutations.

---

## 3. Element Properties & Attributes

### 3.1 Text & Value Properties

Mapping depends on the `TargetKind` of the resolved `QmlTarget`:

| DOM Property | `TargetKind::Text` / `Button` | `TargetKind::Box` (`Rectangle` / `Item`) | `TargetKind::TextField` / `TextArea` | `TargetKind::CheckBox` / `RadioButton` | `TargetKind::Select` (`ComboBox`) | `TargetKind::Range` (`Slider` / `SpinBox`) / `Progress` |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `el.textContent` / `el.innerText` | `<id>.text` | `<id>Text.text` (or `H2Q.getText(<id>)` / `H2Q.setText(<id>, v)`) | `<id>.text` | `<id>.text` | `<id>.currentText` | N/A |
| `el.innerHTML` (read) | `<id>.text` | `H2Q.getInnerHTML(<id>)` | `<id>.text` | `<id>.text` | `<id>.currentText` | N/A |
| `el.innerHTML = html` | `H2Q.setInnerHTML(<id>, html)` | `H2Q.setInnerHTML(<id>, html)` (parses simple tags or sets RichText) | N/A | N/A | N/A | N/A |
| `el.value` (read) | `H2Q.wrap(<id>).value` | `H2Q.wrap(<id>).value` | `<id>.text` | `<id>.value` (via `H2Q.wrap`, defaults to `"on"`) | `<id>.currentText` | `String(<id>.value)` for `Range` (`Slider`/`SpinBox`); `<id>.value` for `Progress` |
| `el.value = v` | `H2Q.wrap(<id>).value = v` | `H2Q.wrap(<id>).value = v` | `<id>.text = String(v)` | `H2Q.wrap(<id>).value = v` | `H2Q.selectByValue(<id>, v)` | `<id>.value = Number(v)` |
| `el.checked` | N/A | N/A | N/A | `<id>.checked` | N/A | N/A |
| `el.disabled` | `!<id>.enabled` | `!<id>.enabled` | `!<id>.enabled` | `!<id>.enabled` | `!<id>.enabled` | `!<id>.enabled` |
| `el.hidden` | `!<id>.visible` | `!<id>.visible` | `!<id>.visible` | `!<id>.visible` | `!<id>.visible` | `!<id>.visible` |
| `el.placeholder` | N/A | N/A | `<id>.placeholderText` | N/A | N/A | N/A |
| `el.readOnly` | N/A | N/A | `<id>.readOnly` | N/A | N/A | N/A |
| `el.selectedIndex` | N/A | N/A | N/A | N/A | `<id>.currentIndex` | N/A |
| `el.min` / `el.max` / `el.step` | `H2Q.wrap(<id>).min` / `max` / `step` | `H2Q.wrap(<id>).min` / `max` / `step` | `H2Q.wrap(<id>).min` / `max` / `step` | N/A | N/A | Updates both `m.attrs` (`min`/`max`/`step`) and QML `<id>.from` / `<id>.to` / `<id>.stepSize` on `Slider` and `SpinBox` |

### 3.2 Reflected Properties, `a.href` Resolution, and Control Range Syncing

- **`a.href` and Document Base URL Resolution**:
  - `a.getAttribute("href")` returns the raw attribute string (and extracts `href="..."` from `<a href="...">` inside QML `Text.text` when `h2q_convert` emits rich text).
  - `a.href` resolves the attribute against the document base URL (`H2Q.location.href` or `root.baseUrl`):
    - Absolute URLs (`https://example.com`, `https://example.com/a/b/../c?q=1#h`) are normalized per URL rules (bare origins receive a trailing `/`, and `.` / `..` path segments are collapsed, working around Qt 6.11 V4's non-normalizing `new URL()`).
    - Non-hierarchical schemes (`mailto:`, `tel:`, `javascript:`, `data:`, `blob:`) are returned verbatim.
    - When `location.href` (or `root.baseUrl`) is set to an `http:`/`https:`/`file:` URL, relative URLs (`/path`, `sub/page.html`, `../up`, `?query`, `#hash`) resolve against that base URL.
    - **Why relative URLs return the raw attribute by default**: `h2q_convert` generates standalone QML (`Main.qml` + `page.js`) and calls `H2Q.init(root)` without passing a document origin URL (`location.href` defaults to `""`). When no base URL is known at runtime, `a.href` returns the raw relative attribute unchanged rather than inventing a synthetic origin.
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
| `el.scrollWidth` / `el.scrollHeight` | `H2Q.wrap(<id>).scrollWidth` / `scrollHeight` | Uses `contentWidth`/`contentHeight` or `implicitWidth`/`implicitHeight` or `width`/`height` |
| `el.scrollTop` / `el.scrollLeft` | `H2Q.wrap(<id>).scrollTop` / `scrollLeft` | Reads/writes `contentY`/`contentX` on `Flickable`, or per-element scroll state |
| `el.getBoundingClientRect()` | `H2Q.getRect(<id>)` | Returns `{ x, y, left, top, width, height, right, bottom }` using `mapToItem(root, 0, 0)` |

### 3.4 `classList`, `className`, `dataset`, and Attributes

| Browser API | Lowering | Notes |
| :--- | :--- | :--- |
| `el.className` (get / set) | `H2Q.getClassName(<id>)` / `H2Q.setClassName(<id>, v)` | Reads/updates `h2qClasses` string and triggers style state re-evaluation |
| `el.classList.add(...c)` | `H2Q.classListAdd(<id>, ...c)` | Mutates `h2qClasses` and applies any class-bound properties |
| `el.classList.remove(...c)` | `H2Q.classListRemove(<id>, ...c)` | Mutates `h2qClasses` |
| `el.classList.toggle(c, force?)` | `H2Q.classListToggle(<id>, c, force)` | Returns boolean |
| `el.classList.contains(c)` | `H2Q.classListContains(<id>, c)` | Returns boolean |
| `el.classList.replace(oldC, newC)` | `H2Q.wrap(<id>).classList.replace(oldC, newC)` | Replaces `oldC` with `newC` and returns boolean |
| `el.dataset.<camelKey>` (get / set) | `H2Q.getDataset(<id>, "<camelKey>")` / `H2Q.setDataset(<id>, "<camelKey>", v)` | Backed by per-item metadata table initialized from HTML `data-*` attributes |
| `el.getAttribute(name)` | `H2Q.getAttribute(<id>, name)` | Special-cases `id`, `class`, `disabled`, `hidden`, `value`, `href`, `src`, `min`, `max`, `step`, `data-*`, plus arbitrary attributes |
| `el.setAttribute(name, val)` | `H2Q.setAttribute(<id>, name, val)` | Updates backing property (`from`/`to`/`stepSize`, `enabled`, `visible`, etc.) + attribute table |
| `el.removeAttribute(name)` | `H2Q.removeAttribute(<id>, name)` | Clears attribute and resets reflected boolean properties (`disabled`, `hidden`, `checked`, `readOnly`) |
| `el.hasAttribute(name)` | `H2Q.hasAttribute(<id>, name)` | Returns boolean |
| `el.toggleAttribute(name, force?)` | `H2Q.toggleAttribute(<id>, name, force)` | Toggles attribute presence and returns boolean |

---

## 4. Inline Style (`el.style.<prop>`), `style.cssText`, and `getComputedStyle`

### 4.1 Static Property Lowering (when `el` is a resolved `QmlTarget`)

| `el.style.<prop>` | QML Property on `<id>` | Conversion Helper |
| :--- | :--- | :--- |
| `display` | `<id>.visible` | `val !== "none"` (reads as `<id>.visible ? "block" : "none"`) |
| `visibility` | `<id>.visible` | `val !== "hidden"` |
| `opacity` | `<id>.opacity` | `H2Q.cssNumber(val, 1)` |
| `backgroundColor` / `background` | `<id>.color` (on `Rectangle`) | `H2Q.cssColor(val)` |
| `color` | `<id>.color` (on `Text`) or `<id>Text.color` (on `Box`) | `H2Q.cssColor(val)` |
| `width` / `height` | `<id>.width` / `<id>.height` | `H2Q.cssPx(val)` (strips `"px"` suffix, parses float) |
| `minWidth` / `minHeight` / `maxWidth` / `maxHeight` | `<id>.Layout.minimumWidth` / etc. | `H2Q.cssPx(val)` |
| `left` / `top` | `<id>.x` / `<id>.y` | `H2Q.cssPx(val)` |
| `fontSize` | `<id>.font.pixelSize` | `H2Q.cssPx(val)` |
| `fontWeight` | `<id>.font.weight` / `<id>.font.bold` | `val === "bold" \|\| Number(val) >= 600` |
| `fontStyle` | `<id>.font.italic` | `val === "italic"` |
| `textAlign` | `<id>.horizontalAlignment` | `H2Q.cssTextAlign(val)` (`Text.AlignLeft`, `Text.AlignHCenter`, `Text.AlignRight`) |
| `borderRadius` | `<id>.radius` (on `Rectangle`) | `H2Q.cssPx(val)` |
| `borderColor` | `<id>.border.color` (on `Rectangle`) | `H2Q.cssColor(val)` |
| `borderWidth` | `<id>.border.width` (on `Rectangle`) | `H2Q.cssPx(val)` |
| `zIndex` | `<id>.z` | `H2Q.cssNumber(val, 0)` |

### 4.2 Style Helper Methods, `cssText`, and `getComputedStyle`

| Browser API | Lowering | Notes |
| :--- | :--- | :--- |
| `el.style.setProperty(name, val)` | `H2Q.wrap(<id>).style.setProperty(name, val)` | Supports both standard CSS properties (`background-color`, `width`, etc.) and CSS custom properties (`--*`) |
| `el.style.getPropertyValue(name)` | `H2Q.wrap(<id>).style.getPropertyValue(name)` | Reads custom properties (`--*`) or mapped inline style values |
| `el.style.removeProperty(name)` | `H2Q.wrap(<id>).style.removeProperty(name)` | Removes custom or inline property and returns previous value |
| `el.style.cssText` (get / set) | `H2Q.wrap(<id>).style.cssText` | Parses `"prop: val; ..."` declarations and applies each via `setStyle` / `setProperty` (emits `Unsupported` static diagnostic because arbitrary CSS declarations cannot be statically bound in QML) |
| `window.getComputedStyle(el)` / `getComputedStyle(el)` | `H2Q.getComputedStyle(el)` | Returns live-like style view with `getPropertyValue(name)` and camelCase properties (`display`, `visibility`, `opacity`, `width`, `height`, `color`, `backgroundColor`, `borderRadius`, `borderWidth`, `fontSize`) |

---

## 5. DOM Tree Traversal & Mutation

| Browser API | Lowering / Runtime Support |
| :--- | :--- |
| `el.parentElement` / `el.parentNode` | `H2Q.wrap(el).parentElement` (unwraps internal layout containers like `ColumnLayout`/`RowLayout`/`GridLayout` to the logical parent element) |
| `el.children` / `el.childNodes` | `H2Q.wrap(el).children` / `childNodes` (array of wrapped child elements, skipping internal `<id>Text` label children on `Box` items) |
| `el.childElementCount` | `H2Q.wrap(el).childElementCount` |
| `el.firstElementChild` / `el.lastElementChild` | `H2Q.wrap(el).firstElementChild` / `lastElementChild` |
| `el.firstChild` / `el.lastChild` | `H2Q.wrap(el).firstChild` / `lastChild` |
| `el.nextElementSibling` / `el.previousElementSibling` | `H2Q.wrap(el).nextElementSibling` / `previousElementSibling` |
| `el.nextSibling` / `el.previousSibling` | `H2Q.wrap(el).nextSibling` / `previousSibling` |
| `parent.appendChild(child)` | `H2Q.wrap(parent).appendChild(child)` (supports `DocumentFragment`) |
| `parent.removeChild(child)` | `H2Q.wrap(parent).removeChild(child)` |
| `parent.insertBefore(newNode, refNode)` | `H2Q.wrap(parent).insertBefore(newNode, refNode)` |
| `parent.replaceChild(newChild, oldChild)` | `H2Q.wrap(parent).replaceChild(newChild, oldChild)` |
| `parent.append(...nodesOrStrings)` | `H2Q.wrap(parent).append(...)` (converts string arguments to text nodes) |
| `parent.prepend(...nodesOrStrings)` | `H2Q.wrap(parent).prepend(...)` |
| `el.before(...nodesOrStrings)` / `el.after(...nodesOrStrings)` | `H2Q.wrap(el).before(...)` / `after(...)` |
| `el.replaceWith(...nodesOrStrings)` | `H2Q.wrap(el).replaceWith(...)` |
| `parent.replaceChildren(...nodesOrStrings)` | `H2Q.wrap(parent).replaceChildren(...)` |
| `el.insertAdjacentHTML(pos, html)` | `H2Q.wrap(el).insertAdjacentHTML(pos, html)` (`"beforebegin"`, `"afterbegin"`, `"beforeend"`, `"afterend"`) |
| `el.insertAdjacentElement(pos, targetEl)` | `H2Q.wrap(el).insertAdjacentElement(pos, targetEl)` |
| `el.insertAdjacentText(pos, text)` | `H2Q.wrap(el).insertAdjacentText(pos, text)` |
| `el.remove()` | `H2Q.wrap(el).remove()` (`item.parent = null; item.visible = false; item.destroy()`) |
| `el.cloneNode(deep?)` | `H2Q.wrap(el).cloneNode(deep)` (clones tag, classes, attributes, text/value, and optionally children) |

---

## 6. Focus, Blur, and Scrolling APIs

| Browser API | Static Lowering (on resolved `QmlTarget`) | Runtime Support (`h2q_runtime.js`) |
| :--- | :--- | :--- |
| `el.focus()` | `<id>.forceActiveFocus()` | `H2Q.focus(el)` (sets `focus = true`, calls `forceActiveFocus()`, updates `document.activeElement`, dispatches `focus` if not already focused) |
| `el.blur()` | `<id>.focus = false` | `H2Q.blur(el)` (sets `focus = false`, clears `document.activeElement`, dispatches `blur`) |
| `el.click()` | `H2Q.wrap(<id>).click()` | Dispatches cancelable bubbling `click` event on `el` |
| `window.scrollTo(x, y \| opts)` / `window.scroll(x, y \| opts)` | `H2Q.scrollTo(x, y)` | Updates `window.scrollX`/`scrollY` (`pageXOffset`/`pageYOffset`), syncs root `Flickable` (`contentX`/`contentY`) if present, and dispatches `scroll` |
| `window.scrollBy(dx, dy \| opts)` | `H2Q.scrollBy(dx, dy)` | Adds `(dx, dy)` to current window scroll position |
| `el.scrollTo(x, y \| opts)` / `el.scrollBy(dx, dy \| opts)` | `H2Q.wrap(<id>).scrollTo(...)` / `scrollBy(...)` | Updates element `scrollLeft`/`scrollTop` (and `contentX`/`contentY` if `Flickable`) |
| `el.scrollIntoView(arg?)` | `H2Q.wrap(<id>).scrollIntoView(arg)` | Scrolls window to element's mapped `(x, y)` coordinates |

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
| `keydown` / `keyup` / `keypress` | Any `Item` | `Keys.onPressed` / `Keys.onReleased` (`focus: true` enabled on target) | `{ type, key: H2Q.keyName(event.key, event.text), code: H2Q.keyCode(event.key), keyCode: H2Q.domKeyCode(event.key), ctrlKey, shiftKey, altKey, metaKey, repeat: event.isAutoRepeat, preventDefault() (sets event.accepted = true) }` |
| `focus` / `blur` / `focusin` / `focusout` | Any `Item` | `activeFocusChanged` and `focusChanged` (deduplicated per focus state transition) | `{ type, target, currentTarget }` |
| `mouseenter` / `mouseleave` / `mouseover` / `mouseout` | Any `Item` | `hoveredChanged` or `MouseArea { hoverEnabled: true; onEntered; onExited }` | `{ type, target, currentTarget }` |
| `DOMContentLoaded` / `load` | `document` / `window` | Executed at the end of `__h2q_init(root)` inside `Component.onCompleted` | `{ type: "DOMContentLoaded" \| "load", target: H2Q.document }` |
| `resize` | `window` | `root.widthChanged` / `root.heightChanged` | `{ type: "resize", target: H2Q.window }` |
| `scroll` | `window` / `el` | Dispatched on `scrollTo` / `scrollBy` / `scrollIntoView` | `{ type: "scroll", target }` |
| `hashchange` | `window` | Dispatched when `location.hash` changes | `{ type: "hashchange", target: H2Q.window }` |
| `popstate` | `window` | Dispatched on `history.back()` / `forward()` / `go()` | `{ type: "popstate", state, target: H2Q.window }` |
| `visibilitychange` | `document` | Dispatched if visibility state changes | `{ type: "visibilitychange", target: H2Q.document }` |

### 7.2 Event Constructors & `EventTarget`

- `new Event(type, opts?)` and `new CustomEvent(type, opts?)` (with `detail`, `bubbles`, `cancelable`, `preventDefault()`, `stopPropagation()`, `stopImmediatePropagation()`, `defaultPrevented`).
- Typed event constructors: `new KeyboardEvent(type, opts?)`, `new MouseEvent(type, opts?)`, `new PointerEvent(type, opts?)`, `new FocusEvent(type, opts?)`, `new InputEvent(type, opts?)`, `new WheelEvent(type, opts?)`.
- `new EventTarget()`: standalone event target supporting `addEventListener`, `removeEventListener`, and `dispatchEvent`.

### 7.3 Static vs. Dynamic Event Registration

1. **Inline HTML attributes** (`<button onclick="doWork(event)">`):
   - Extracted during HTML analysis (`extract_inline_handlers`).
   - Lowered into a generated function `function __h2q_attr_onclick_<id>(event) { doWork(event); }` in `page.js` and emitted as an `EventBinding`.
2. **`el.addEventListener(type, fn, opts?)` on a statically resolved `QmlTarget`**:
   - If the target has a single static listener and does not use `removeEventListener`, `dispatchEvent`, `{ once: true }`, or mixed `on<event>` + `addEventListener`, it takes the static fast path (`__h2q_on_<type>_<id>_<n>`).
   - Otherwise, all listeners on `(target, type)` are unified onto `H2Q.listen` / `H2Q.unlisten` / `H2Q.setOn` / `H2Q.dispatchEvent` with a single QML forwarding handler.
3. **`el.on<event> = fn`**:
   - Lowered to a reassignable handler slot or `H2Q.setOn(<id>, "<event>", fn)` when unified with `addEventListener`.

---

## 8. Timers, Storage, Network, Observers & Browser Globals

| Browser Global / API | Lowering in `page.js` | Implementation in `h2q_runtime.js` | Status |
| :--- | :--- | :--- | :--- |
| `setTimeout(fn, ms, ...args)` | `H2Q.setTimeout(fn, ms, ...args)` | Creates one-shot `QtQuick.Timer`, returns integer timer ID | Supported & tested |
| `clearTimeout(id)` | `H2Q.clearTimeout(id)` | Stops and destroys timer `id` | Supported & tested |
| `setInterval(fn, ms, ...args)` | `H2Q.setInterval(fn, ms, ...args)` | Creates repeating `QtQuick.Timer`, returns integer timer ID | Supported & tested |
| `clearInterval(id)` | `H2Q.clearInterval(id)` | Stops and destroys timer `id` | Supported & tested |
| `requestAnimationFrame(fn)` | `H2Q.requestAnimationFrame(fn)` | 16 ms one-shot `QtQuick.Timer` passing `H2Q.performance.now()` timestamp | Supported & tested |
| `cancelAnimationFrame(id)` | `H2Q.cancelAnimationFrame(id)` | Calls `clearTimeout(id)` | Supported & tested |
| `queueMicrotask(fn)` | `Promise.resolve().then(fn)` | Native V4 `Promise` microtask queue | Supported & tested |
| `performance.now()` | `H2Q.performance.now()` | `Date.now() - _startTime` | Supported & tested |
| `localStorage` | `H2Q.localStorage` | `getItem`, `setItem`, `removeItem`, `clear`, `key`, `length` backed by `QtQuick.LocalStorage` SQLite (with in-memory fallback) | Supported & tested |
| `sessionStorage` | `H2Q.sessionStorage` | Same API backed by in-memory object | Supported & tested |
| `fetch(url, opts?)` | `H2Q.fetch(url, opts)` | `Promise` wrapping QML `XMLHttpRequest`; supports `.ok`, `.status`, `.statusText`, `.headers.get()`, `.text()`, `.json()` | Supported & tested |
| `alert(msg)` | `H2Q.alert(msg)` | Logs `[alert] <msg>` via `console.warn` and invokes optional alert hook | Supported & tested |
| `confirm(msg)` / `prompt(msg, def?)` | `H2Q.confirm(msg)` / `H2Q.prompt(msg, def)` | Logs unsupported warning and returns `false` / `null` (emits `Unsupported` diagnostic) | Stubbed & tested |
| `atob(s)` / `btoa(s)` | `H2Q.atob(s)` / `H2Q.btoa(s)` | Standard Web API Latin1 Base64 encode/decode | Supported & tested |
| `structuredClone(val)` | `H2Q.structuredClone(val)` | Recursive structural clone preserving arrays/dates/plain objects | Supported & tested |
| `window.innerWidth` / `window.innerHeight` | `H2Q.window.innerWidth` / `H2Q.window.innerHeight` | Reads `root.width` / `root.height` (fallback `1280` / `800`) | Supported & tested |
| `window.scrollX` / `scrollY` / `pageXOffset` / `pageYOffset` | `H2Q.window.scrollX` / `scrollY` / `pageXOffset` / `pageYOffset` | Tracks window scroll offset | Supported & tested |
| `window.devicePixelRatio` | `H2Q.window.devicePixelRatio` | `Qt.styleHints.mousePressAndHoldInterval ? 1 : 1` (defaults to `1`) | Supported & tested |
| `window.getComputedStyle(el)` | `H2Q.getComputedStyle(el)` | Computed style view for `el` | Supported & tested |
| `window.matchMedia(query)` | `H2Q.matchMedia(query)` | Evaluates `(min-width)`, `(max-width)`, `(min-height)`, `(max-height)`, and `(prefers-color-scheme: dark)` against `root` | Supported & tested |
| `location` (`href`, `origin`, `protocol`, `host`, `hostname`, `port`, `pathname`, `search`, `hash`, `assign`, `replace`, `reload`) | `H2Q.location` | Parses assigned URLs, serves as document base URL for `a.href`, and dispatches `hashchange` on `window` when `hash` changes (emits `Unsupported` static diagnostic because QML has no multi-page navigation) | Shimmed & tested |
| `history` (`state`, `length`, `pushState`, `replaceState`, `back`, `forward`, `go`) | `H2Q.history` | In-memory session history stack; updates `location` and dispatches `popstate` on `back()`/`forward()`/`go()` (emits `Unsupported` static diagnostic) | Shimmed & tested |
| `navigator` (`userAgent`, `language`, `languages`, `onLine`, `platform`, `clipboard.writeText`, `clipboard.readText`) | `H2Q.navigator` | In-memory async `Promise` clipboard + environment metadata (emits `Unsupported` static diagnostic) | Shimmed & tested |
| `MutationObserver` / `ResizeObserver` / `IntersectionObserver` | `H2Q.MutationObserver` / `H2Q.ResizeObserver` / `H2Q.IntersectionObserver` | Safe no-op observer constructors with `observe()`, `unobserve()`, `disconnect()`, `takeRecords()` so scripts constructing observers do not throw `ReferenceError` (emits `Unsupported` static diagnostic) | Stubbed & tested |
| `console.log` / `warn` / `error` / `info` / `debug` | Unchanged (native in QML) | `console.table` lowered to `H2Q.consoleTable` | Supported & tested |

---

## 9. JavaScript Language Lowering for Qt 6.11 V4

Qt 6.11's V4 engine supports ES2015–ES2020 core syntax (`let`, `const`, arrow functions, template literals, destructuring declarations, `for..of`, `Map`, `Set`, `Symbol`, `Promise`, optional chaining `?.`, nullish coalescing `??`) but lacks or has quirks with several newer constructs. `h2q_js` automatically lowers the following syntax into V4-safe ES6:

1. **Object Spread (`{ ...a, x: 1 }`)** → `Object.assign({}, a, { x: 1 })`.
2. **Object Rest Destructuring (`const { a, ...rest } = obj`)** → `const __h2q_tmp = obj, a = __h2q_tmp.a, rest = H2Q.objectRest(__h2q_tmp, ["a"])`.
3. **Logical Assignment Operators (`a &&= b`, `a ||= b`, `a ??= b`)** → `a && (a = b)`, `a || (a = b)`, `(a !== null && a !== undefined) ? a : (a = b)`.
4. **Optional Catch Binding (`try { ... } catch { ... }`)** → `try { ... } catch (__h2q_err) { ... }`.
5. **ES6 `class` Declarations with Public Instance/Static Fields** → Lowered by moving field initializers into the `constructor` (or after the class for `static` fields). Private `#fields` emit an `Unsupported` diagnostic.
6. **`async function` / `await` (simple linear & try/catch forms)** → Lowered to `.then()` chains where possible; complex control flow (`await` inside loops) emits an `Unsupported` diagnostic.
7. **ES Modules (`<script type="module">`, `import`, `export`)** → Local module scripts are topologically ordered and bundled into `page.js` with per-module hygienic symbol renaming (`__h2q_m<N>_<name>`); external URL `import`s emit an `Unsupported` diagnostic.

---

## 10. Unsupported APIs & Why

The following browser APIs are not supported in QML and emit `Diagnostic` entries (`Severity::Unsupported` or `Severity::Warning`) during `analyze()` / `rewrite()`:

- **Multi-window navigation & blocking modal dialogs (`window.open`, `window.close`, `window.print`, synchronous `confirm()`, `prompt()`)**: Qt Quick runs in a single off-screen or embedded QML scene without browser tab management or synchronous event-loop-blocking modal text prompts (`confirm` returns `false`, `prompt` returns `null`).
- **Dynamic code evaluation (`eval`, `new Function`)**: Disallowed for static analysis predictability and QML `.pragma library` scope isolation.
- **Background threads & network sockets (`Worker`, `SharedWorker`, `ServiceWorker`, `WebSocket`, `EventSource`, `BroadcastChannel`)**: Qt Quick's V4 JS environment does not expose Web Workers, Service Workers, or browser `WebSocket` constructors without additional C++ plugin modules.
- **Browser storage & binary databases (`IndexedDB`, `caches`, `cookie`)**: QML provides `QtQuick.LocalStorage` (used by `H2Q.localStorage`), not IndexedDB or HTTP cookie jars.
- **Web Components & Shadow DOM (`customElements`, `attachShadow`, `ShadowRoot`, `HTMLTemplateElement.content`)**: QML uses its own component system (`Component` / `Qt.createQmlObject`) rather than HTML Shadow DOM encapsulation.
- **Rich text selection ranges (`document.createRange`, `window.getSelection`, `Range`, `Selection`)**: QML `Text` and `TextField` do not expose DOM `Node`/character offset `Range` trees.
- **2D/WebGL Canvas & Media (`canvas.getContext`, `Audio`, `Video`, `WebGLRenderingContext`, `OffscreenCanvas`)**: `h2q_convert` maps `<canvas>` to a placeholder QML `Rectangle` rather than a full HTML5 `CanvasRenderingContext2D` / WebGL surface.
- **Live layout/mutation observer callbacks (`MutationObserver`, `ResizeObserver`, `IntersectionObserver`)**: Constructor stubs are provided in `H2Q` so page scripts do not throw `ReferenceError` on `.observe()` / `.disconnect()`, and `analyze()` emits an `Unsupported` diagnostic because arbitrary DOM mutation records and intersection thresholds are not synthesized automatically.
- **Unsupported JS runtime intrinsics (`BigInt`, `WeakRef`, `FinalizationRegistry`, `Proxy`, private `#fields`)**: Not supported by Qt 6.11's V4 JavaScript engine.
