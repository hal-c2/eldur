# JavaScript, DOM and events mapping

**Spec:** `docs/spec/dom-api.md`
**Target:** Qt 6.11 with QtQuick, QtQuick.Controls (Basic), QtQuick.Layouts and QtQuick.Shapes. `QtQuick.Dialogs` (`MessageDialog`) and `QtQuick.LocalStorage` are imported only when a page uses `alert`/`confirm` or `localStorage`.
**Out of scope:** QtWebEngine, WebView, and `Text.RichText` used as a whole-page shortcut.

**Support levels**

| Level | Meaning |
|---|---|
| Full | Same observable behavior as the browser |
| Approx | Visible result close but not identical |
| Emulated | Needs generated helper code in `dom.js` |
| Ignored | No visual effect, safe to drop |
| Unsupported | Converter emits a warning |

**JS engine baseline.** The QML engine (checked against Qt 6.11 with `qml6`) is roughly ES2017 plus `?.` and `??`. It has `Promise`, generators, arrow functions, classes, template literals, `Map`/`Set`/`Proxy`, `XMLHttpRequest`, `URL`, `URLSearchParams`, typed arrays and ES modules (`.mjs`). It lacks `async`/`await`, object spread/rest, class fields, optional `catch` binding, `??=`/`||=`/`&&=`, BigInt, regex named groups, lookbehind and the `s` flag, `Intl`, `fetch`, `setTimeout`/`setInterval`, `queueMicrotask`, `structuredClone`, `performance`, `window`/`document`/`globalThis` and the DOM `Event` classes. Missing syntax is lowered in §5; missing library functions are polyfilled in `dom.js` (§3).

## Conventions

- Every element with an `id` becomes a QML object whose `id` is the sanitized HTML id. Characters outside `[A-Za-z0-9_]` become `_`. An id gets an `el_` prefix when it starts with a digit or an uppercase letter (QML rejects both), or is a QML/JS reserved word or one of our own names (`root`, `Dom`, `parent`, `model`, `index`). Elements without an `id` that a script targets statically get a generated id (`<class-or-tag>_<n>`).
- Generated helpers live in `dom.js`, imported per file as `import "dom.js" as Dom`. Use a plain (non-`.pragma library`) file so it can see the importing file's ids. Every importing file gets its own copy, so state shared across files (timer table, event bus, `sessionStorage`) is stored on the root object, not in module variables. `dom.js` cannot declare QML objects; `Timer`, `FrameAnimation` and dialogs are instantiated from helper components via `Component.createObject`.
- Top-level script statements that are not declarations are collected, in document order, into the single `Component.onCompleted` of the root object. Declarations are emitted into the root object body (§5).
- Signal handlers use the Qt 6 arrow form with explicit parameters, for example `onTapped: (eventPoint, button) => ...`. Injected, undeclared handler parameters are deprecated.

## Section 1: Statically resolvable DOM access

The converter rewrites these at compile time to direct ids and properties.

| JS pattern | QML rewrite | Condition for static rewrite | Support |
|---|---|---|---|
| `document.getElementById('x')` | `x` | `id="x"` occurs once and the element is not created at runtime | Full |
| `document.querySelector('#x')` | `x` | Same as `getElementById`; simple id selector only | Full |
| `document.querySelector('.c')` | `c_0` (one match) | Exactly one element has class `c` | Full |
| `document.querySelectorAll('.c')` | `[c_0, c_1]` | Matches are known at compile time | Full |
| `document.querySelector('tag')` | first matching id | Tag is unique, or the first in document order is intended | Approx |
| Combinators, pseudo-classes, attribute selectors | none | Not resolvable at compile time | Emulated (§2) |
| Any property access on a receiver of unknown type (function parameter, array element) | `Dom.wrap(item).<prop>` | Receiver is not statically an element | Emulated (§2) |
| `el.textContent = s` | `el.text = s` | `el` maps to `Text`, `Label`, `Button`, `TextField`, `TextArea`, `CheckBox` or `RadioButton` | Full |
| `el.textContent = s` on a container | generated child `Text { text: s }` | `el` is a plain `Rectangle` or `Item` | Approx |
| `el.innerText = s` | `el.text = s` | Same as `textContent` on text items; `\n` renders as a line break | Full |
| `el.value` (read/write) | `el.text` (`TextField`, `TextArea`), `el.value` (`Slider`, `SpinBox`), `el.currentValue` (`ComboBox` with `valueRole`) | `el` is an `<input>`, `<select>` or `<textarea>` mapped by type | Full |
| `select.selectedIndex` | `el.currentIndex` | `el` is a `ComboBox` | Full |
| `el.checked` | `el.checked` | `CheckBox`, `Switch` or `RadioButton` | Full |
| `el.checked` on a radio group | same, exclusive via `ButtonGroup` (attached `ButtonGroup.group`) | All radios share `name` and the group is generated | Full |
| `el.disabled = b` | `el.enabled = !b` | Always | Full |
| `el.hidden = b` | `el.visible = !b` | Always; invisible items are skipped by `Layout` items too | Full |
| `el.style.<prop> = v` | target per the table below | Property is in the CSS-to-QML table | Approx |
| `el.style.cssText = s` | none | Not statically parseable | Unsupported |
| `el.classList.add/remove/toggle/contains(c)`, `el.className = s` | one `bool` property per toggled class on the element (`property bool cls_open`), with the `.c` rule compiled into conditional bindings (`color: cls_open ? "#f00" : "#fff"`) | Class is known at compile time. `states` are used only when the classes are mutually exclusive, because an item has one active state. | Emulated |
| `el.setAttribute(n, v)` / `getAttribute(n)` | mapped property (see below) | `n` is in the attribute map | Full |
| `el.setAttribute(n, v)` / `getAttribute(n)` | `el.attrs[n]` (`property var attrs`, reassigned on write so bindings update) | `n` not in the map | Emulated |
| `el.addEventListener('click', f)` | `TapHandler { onTapped: ... }`, or `onClicked` on a Controls button (`Connections { target: btn }` when it is declared elsewhere) | Event is in §4 | Full |
| `el.onclick = f` | same as `addEventListener`; a later assignment replaces the earlier one | Event is in §4 | Full |
| `el.focus()` | `el.forceActiveFocus()` | Element is a Controls item or has `tabindex`; HTML ignores `focus()` on other elements | Full |
| `el.blur()` | `el.focus = false` (clears `activeFocus`; the window's content item takes over) | Always | Full |

**CSS-to-QML property map for `el.style.<prop>`**

| `style.` property | QML target | Support |
|---|---|---|
| `backgroundColor` | `Rectangle.color` (or `background.color` on a Controls item) | Full |
| `color` | `Text.color` | Full |
| `fontSize` | `font.pixelSize` | Full |
| `width`, `height` (px) | `width`, `height` | Full |
| `width`, `height` (%) | binding on the parent, e.g. `width: parent.width * 0.5` | Approx |
| `opacity` | `opacity` | Full |
| `display` = `none` | `visible = false` | Full |
| `visibility` = `hidden` | `opacity = 0; enabled = false` (keeps its layout slot) | Approx |
| `transform` (translate) | `transform: Translate { x: ...; y: ... }` (`Item.transform` is a list) | Approx |

**Attribute map for `setAttribute` and `getAttribute`**

| Attribute | QML target | Support |
|---|---|---|
| `aria-label` | `Accessible.name` | Full |
| `role` | `Accessible.role` | Approx |
| `disabled` | `enabled` (inverted) | Full |
| `hidden` | `visible` (inverted) | Full |
| `value` | value property of the control | Full |
| `data-*` | `dataset` (§2) | Emulated |

## Section 2: Runtime DOM shim

Used when a target cannot be resolved statically. `dom.js` provides a minimal `Element` wrapper `{ _item, attrs, _data }` over QML items.

**Creation.** `document.createElement(tag)` looks up a precompiled component for the tag (`Elements/Div.qml`, `Elements/Span.qml`, and so on), created with `Qt.createComponent`, and instantiates it with `Component.createObject(null, props)`. The node has no parent until `appendChild`/`insertBefore` sets one, as in the DOM. `Qt.createQmlObject` is a fallback for tags with no precompiled component, since it parses at runtime and needs a parent.

**Child order.** `appendChild` sets `child.parent`, and a `Layout` places reparented items in child order. `Item.stackBefore` is not exposed to QML, so `insertBefore` detaches `ref` and every later sibling and re-attaches them after the new node. Containers that the converter can see are filled from a JS array in a loop (render-list pattern) are generated as a `Repeater` over a `ListModel` (`append`, `insert`, `remove`) instead, with a `Loader` delegate picked by tag.

| Shim API | QML mechanism | Support |
|---|---|---|
| `document.createElement(tag)` | `Component.createObject(null, props)` from a per-tag component | Emulated |
| `document.createElement(unknownTag)` | `Qt.createQmlObject` fallback | Emulated |
| `parent.appendChild(child)` | Set `child.parent`; for a `Repeater`-backed container, `model.append` | Emulated (layout differs: Approx) |
| `parent.removeChild(child)`, `el.remove()` | `child.parent = null`; the node stays alive and can be re-attached | Emulated |
| `parent.insertBefore(new, ref)` | Re-attach `new`, `ref` and later siblings in order; `model.insert` for `Repeater` containers | Emulated |
| `parent.replaceChild(new, old)` | Detach `old`, insert `new` at the same position | Emulated |
| `el.innerHTML = s` | Minimal parser in `dom.js` for elements and text; existing children are detached and `destroy()`ed | Emulated (subset) |
| `el.innerHTML = s` with `<script>`, `<style>` or `on*` attributes | none | Unsupported |
| `el.innerHTML` (read) | Serialize the shim tree | Emulated |
| `document.querySelectorAll(sel)` (runtime) | Walk the shim tree; supports tag, `#id`, `.class`, `tag.class` | Emulated |
| Complex selectors at runtime | none | Unsupported |
| `el.dataset.k` | `_data.k`; `data-foo-bar` is `dataset.fooBar` (kebab-case to camelCase) | Emulated |
| `el.children` | `item.children` (Items only) with helper items such as `background` filtered out | Emulated (read-only) |
| `el.parentNode`, `nextSibling` | Parent wrapper; sibling by index | Emulated |
| `el.cloneNode(deep)` | Recreate from tag, attrs and properties | Approx |
| `el.getBoundingClientRect()` | `mapToItem(null, 0, 0)` (null maps to the scene root) plus `width`/`height` | Approx |
| `el.offsetWidth`, `offsetHeight` | `width`, `height` | Approx |
| `el.scrollTop` | `Flickable.contentY` (only inside a `Flickable`) | Approx |
| `document.write` | none | Unsupported |
| `MutationObserver` | none | Unsupported |

## Section 3: Browser globals

| Global / API | QML mapping | Support |
|---|---|---|
| `window` | Rewritten to the root object id | Full |
| `window.innerWidth`, `innerHeight` | `root.width`, `root.height` | Full |
| `window.scrollY` | `Flickable.contentY` | Approx |
| `window.close()` | `root.close()` when the root is a `Window`, else `Qt.quit()` | Full |
| `window.open(url)` | `Qt.openUrlExternally(url)` | Approx |
| `document.title` (read/write) | `Window.title` | Full |
| `document.body`, `documentElement` | Root item | Approx |
| `document.hidden` | `root.visibility === Window.Hidden \|\| root.visibility === Window.Minimized` | Approx |
| `console.log/info/debug/warn/error/assert/count/time/timeEnd/trace` | Same QML `console` methods | Full |
| `console.table`, `group`, `groupEnd`, `dir`, `timeLog`, `countReset` | Not defined in QML; mapped to `console.log` or a helper | Emulated |
| `setTimeout(f, ms)` | `Timer { interval: ms; repeat: false; running: true; onTriggered: f() }` (not defined in the engine) | Full |
| `setInterval(f, ms)` | `Timer { repeat: true }` | Full |
| `clearTimeout`, `clearInterval` | `running = false` and destroy (ids held in a map) | Full |
| `setTimeout(f, 0)` | `Qt.callLater(f)` only when `f` is not scheduled twice per turn; `callLater` coalesces repeated calls of the same function. Otherwise `Timer { interval: 0 }` | Approx |
| `requestAnimationFrame(cb)` | One shared `FrameAnimation` in the helper; each `onTriggered` drains the queued callbacks with a millisecond timestamp (`Date.now()` minus start) and stops when the queue is empty. Callbacks run once, unlike a raw `FrameAnimation`. | Approx |
| `cancelAnimationFrame(id)` | Remove the callback from the queue | Full |
| `performance.now()` | `Date.now()` minus application start | Emulated |
| `fetch(url, opts)` | `XMLHttpRequest` wrapped in a Promise | Emulated |
| `XMLHttpRequest` (text GET/POST) | QML engine `XMLHttpRequest` | Full |
| `XMLHttpRequest` with `responseType` `blob` | none (no `Blob` type) | Unsupported |
| `localStorage` | `QtQuick.LocalStorage` (`openDatabaseSync`, `db.transaction` with `tx.executeSql`) behind `getItem`/`setItem`/`removeItem` wrappers | Emulated (synchronous, strings only) |
| `sessionStorage` | In-memory object on the root (lost on exit) | Emulated |
| `location.href` | `Qt.resolvedUrl(...)` of the source | Approx |
| `location.reload()` | none | Unsupported |
| `history.pushState`, `back()` | `StackView` push/pop when the app uses `StackView` | Emulated |
| `history.pushState` without `StackView` | none | Unsupported |
| `navigator.language` | `Qt.locale().name` with `_` replaced by `-` (`en_US` to `en-US`) | Approx |
| `navigator.platform` | `Qt.platform.os` | Approx |
| `navigator.userAgent` | Constant `html-2-qml/<version>` | Emulated |
| `alert(msg)` | `MessageDialog { buttons: MessageDialog.Ok; text: msg }` from QtQuick.Dialogs | Emulated (non-blocking) |
| `confirm(msg)` (sync return) | none; the call cannot block | Unsupported |
| `confirm(msg, cb)` (callback form) | `MessageDialog` with `MessageDialog.Ok \| MessageDialog.Cancel`; `onAccepted`/`onRejected` call `cb` | Emulated |
| `prompt(msg)` (sync return) | none | Unsupported |
| `prompt(msg, cb)` (callback form) | Controls `Dialog { modal: true; standardButtons: Dialog.Ok \| Dialog.Cancel }` with a `TextField` | Emulated |
| `JSON.parse`, `JSON.stringify` | QML JS engine | Full |
| `Math.*` | QML JS engine | Full |
| `Date` | QML JS engine; local time zone from the system | Full |
| `Date.prototype.toLocaleString/DateString/TimeString` with `(locale, options)` | `date.toLocaleString(Qt.locale(tag), format)` with the options object translated to a Qt format string; the engine's own `toLocale*` takes a Qt format, not an options object, and its no-argument output is the long format | Approx |
| `Intl.NumberFormat`, `Intl.DateTimeFormat` (basic) | `Intl` is not defined; helper over `Number.toLocaleString(Qt.locale(tag), 'f', n)` and `Qt.formatDateTime` | Emulated (subset) |
| Other `Intl.*` | none | Unsupported |
| `Promise` (`then`, `catch`, `all`, `race`, `resolve`, `reject`) | QML JS engine | Full |
| `Promise.prototype.finally`, `Promise.allSettled`, `Promise.any` | Not defined in the engine; polyfilled in `dom.js` | Emulated |
| `async` / `await` | Lowered by oxc to generator plus Promise driver; the engine has generators and `Promise` but not the syntax | Emulated |
| `Array.prototype.flat/flatMap/at`, `Object.fromEntries`, `String.prototype.replaceAll/trimStart/trimEnd` | Not defined in the engine; polyfilled in `dom.js` | Emulated |
| `queueMicrotask(f)`, `structuredClone(v)` | `Promise.resolve().then(f)`; JSON round trip (no Map/Set/Date fidelity) | Emulated / Approx |
| `atob`, `btoa` | `Qt.atob`, `Qt.btoa` | Full |
| `matchMedia(q)` | Evaluates `min-width`/`max-width` against `root.width`; change listeners through `Connections` | Emulated |
| `Event`, `CustomEvent`, `EventTarget` | Plain objects with `type` and `detail`, dispatched through the shim | Emulated |
| `ResizeObserver` | `onWidthChanged`/`onHeightChanged` | Emulated |
| `IntersectionObserver` | none | Unsupported |
| `import` / `export` in inline code | none | Unsupported |
| `import` of a local `.js` file that does not touch the DOM | Emitted as `.mjs`; `import "x.mjs" as X` (QML supports ES modules in `.mjs` files) | Approx |
| `import` of a file that touches the DOM, or of a bare/URL specifier | none; modules cannot see the importing file's ids | Unsupported |

## Section 4: Events

**Handler choice.** Prefer pointer handlers (`TapHandler`, `HoverHandler`, `WheelHandler`, `DragHandler`) over `MouseArea`. Handlers on an item also see events over its children and do not block handlers on ancestors, which is closest to DOM bubbling. A `MouseArea` that accepts a press hides it from items below it.

**DOM event to QML handler**

| DOM event | QML handler | Support |
|---|---|---|
| `click` | `TapHandler.onTapped` (left button), `MouseArea.onClicked`, `Button.onClicked` | Full |
| `dblclick` | `TapHandler.onDoubleTapped`, `MouseArea.onDoubleClicked` | Full |
| `contextmenu` | `TapHandler { acceptedButtons: Qt.RightButton }`; `onLongPressed` on touch | Approx |
| `mousedown`, `mouseup` | `TapHandler.onPressedChanged` (`pressed` true or false) or `MouseArea.onPressed`/`onReleased` | Full |
| `mousemove` | `HoverHandler.onPointChanged` or `MouseArea.onPositionChanged` (needs `hoverEnabled: true` to fire without a pressed button) | Full |
| `mouseenter`, `pointerenter` | `HoverHandler.onHoveredChanged` (`hovered` true) or `MouseArea.onEntered` | Full |
| `mouseleave`, `pointerleave` | `HoverHandler.onHoveredChanged` (`hovered` false) or `MouseArea.onExited` | Full |
| `mouseover`, `mouseout` | Same as enter and leave | Approx (no bubbling between handlers) |
| `wheel` | `WheelHandler.onWheel` or `MouseArea.onWheel` | Approx (`deltaY = -angleDelta.y`, scaled from 120 per notch to about 100 px; `pixelDelta` is used when non-zero. Qt's sign is opposite to the DOM's.) |
| `pointerdown`, `pointermove`, `pointerup`, `pointercancel` | `PointHandler`, `TapHandler` | Approx |
| `touchstart`, `touchmove`, `touchend`, `touchcancel` | `MultiPointTouchArea.onPressed`, `onUpdated`, `onReleased`, `onCanceled` (each receives the list of touch points) | Approx |
| `keydown` | `Keys.onPressed` (attached, on a focused item) | Full |
| `keyup` | `Keys.onReleased` | Full |
| `keypress` | `Keys.onPressed` filtered on non-empty `event.text` | Emulated |
| `input` on `TextField` | `TextField.onTextEdited` (user input only) | Full |
| `input` on `TextArea` | `TextArea.onTextChanged`, skipping writes made by the shim (`TextArea` has no `textEdited`) | Emulated |
| `input` on sliders | `Slider.onMoved` | Approx |
| `change` on text inputs | `TextField.onEditingFinished`, `TextArea.onEditingFinished` | Approx |
| `change` on checkboxes | `CheckBox.onToggled` | Full |
| `change` on selects | `ComboBox.onActivated` | Full |
| `submit` | `TextField.onAccepted`, dispatched to the form wrapper | Emulated |
| `focus`, `focusin` | `Item.onActiveFocusChanged` (`activeFocus` true) | Full (focus); Approx (focusin, no bubbling) |
| `blur`, `focusout` | `Item.onActiveFocusChanged` (`activeFocus` false) | Full (blur); Approx (focusout) |
| `scroll` | `Flickable.onContentYChanged`, `onContentXChanged` | Full on `Flickable`; Approx elsewhere |
| `resize` | `Window.onWidthChanged`, `onHeightChanged` | Full |
| `load` (document) | End of the root `Component.onCompleted`, after the top-level statements | Approx |
| `load` (image) | `Image.onStatusChanged` with `Image.Ready` | Full |
| `error` (image) | `Image.onStatusChanged` with `Image.Error` | Full |
| `DOMContentLoaded` | End of the root `Component.onCompleted`, after the top-level statements | Full |
| `animationend` | `Animation.onFinished` (since Qt 5.12) | Approx |
| `transitionend` | `Transition.onRunningChanged` (read-only `running`) with `running` false, or `onFinished` of the animation inside a `Behavior` | Approx |
| `dragstart`, `drag` | `DragHandler.onActiveChanged`, `onTranslationChanged` | Approx |
| `dragenter`, `dragover`, `dragleave`, `drop` | `DropArea.onEntered`, `onPositionChanged`, `onExited`, `onDropped` | Approx |
| `beforeunload` | `Window.onClosing` (`close.accepted = false` cancels) | Approx |
| `unload`, `pagehide` | `Connections { target: Qt.application; function onAboutToQuit() }` | Approx |
| `visibilitychange` | `Window.onVisibilityChanged`; hidden when `visibility` is `Window.Hidden` or `Window.Minimized` | Approx |
| `popstate`, `hashchange` | Emulated through the `history` shim | Emulated |
| Custom `dispatchEvent` | Shim event bus | Emulated |

**Event object fields**

| Field | Emulation | Support |
|---|---|---|
| `type` | String literal from the rewrite | Full |
| `target` | Item id from the static rewrite; wrapper for shim-dispatched events | Approx |
| `currentTarget` | Item the listener is registered on | Full |
| `key` | `event.text` for printable keys; otherwise `KeyEvent.key` mapped through a generated `Qt.Key` to DOM name table | Full |
| `code` | `KeyEvent.key` mapped through a table to a US-layout physical key name (`KeyA`, `Enter`). `nativeScanCode` is platform-specific and is not used. | Approx |
| `keyCode` | Legacy mapping table | Emulated |
| `repeat` | `KeyEvent.isAutoRepeat` | Full |
| `altKey`, `ctrlKey`, `shiftKey`, `metaKey` | Bit tests on `modifiers` (`Qt.AltModifier`, `Qt.ControlModifier`, `Qt.ShiftModifier`, `Qt.MetaModifier`) | Emulated |
| `button` | `Qt.LeftButton` to 0, `Qt.MiddleButton` to 1, `Qt.RightButton` to 2 | Emulated |
| `buttons` | `MouseEvent.buttons` unchanged; the left (1), right (2) and middle (4) bits match the DOM | Full |
| `offsetX`, `offsetY` | `mouse.x`, `mouse.y` (`MouseArea`); `eventPoint.position` (handlers) | Full |
| `clientX`, `clientY` | `item.mapToItem(null, x, y)` (null maps to the scene root), or `eventPoint.scenePosition` in handlers | Full |
| `pageX`, `pageY` | Same as `clientX` (no page scroll) | Approx |
| `deltaX`, `deltaY`, `deltaMode` | From `WheelEvent.angleDelta`/`pixelDelta` (see `wheel`); `deltaMode` is 0 (pixels) | Approx |
| `timeStamp` | Milliseconds since application start (same clock as `performance.now()`) | Approx |
| `preventDefault()` | Key events: `accepted = true`; `Keys` handlers run before the item (`Keys.priority` default `BeforeItem`), so the item does not process the key. This also stops propagation to parent items. Other events: no-op. | Approx |
| `stopPropagation()` | Key events: `accepted = true`. Pointer events: a flag on the shim event that the generated ancestor handlers check before running their listeners. | Approx |
| `stopImmediatePropagation()` | All listeners of one element are called by one generated dispatcher in registration order; the flag stops the loop | Emulated |
| `bubbles`, `cancelable`, `defaultPrevented` | Constants or derived from `accepted` | Emulated |

## Section 5: Rewrite rules for the oxc pass

Rules run in the order listed. Each rule emits a diagnostic when it changes the output.

| # | Rule | Before | After | Support |
|---|---|---|---|---|
| 1 | Collect top-level declarations | `let n = 0; function bump() {}` | Symbol table entries for `n` (state) and `bump` (function) | Full |
| 2 | Hoist top-level functions | `function bump() { ... }` | `function bump() { ... }` in the root object body | Full |
| 3 | Promote mutated top-level state to properties | `let count = 0;` (written in a handler) | `property int count: 0`. The type is `int`, `real`, `string` or `bool` only when every assignment has that type, otherwise `var`. | Full |
| 4 | Keep read-only top-level constants as readonly | `const MAX = 10;` | `readonly property int _MAX: 10` (non-literal initializers use `readonly property var`) | Full |
| 5 | Rewrite DOM access to ids (§1) | `document.getElementById('n').textContent = count` | `n.text = count` | Full |
| 6 | Rewrite `window` references | `window.innerWidth` | `root.width` | Full |
| 7 | Wrap DOMContentLoaded and load listeners | `document.addEventListener('DOMContentLoaded', init);` | Call `init()` at the end of the root `Component.onCompleted`, after the top-level statements | Full |
| 8 | Convert `addEventListener` and `on*` assignments to handlers | `btn.addEventListener('click', bump);` | `Button { id: btn; onClicked: bump() }` | Full |
| 9 | Convert inline `on*` attributes | `<button onclick="bump(this)">` | `onClicked: bump(btn)` | Full |
| 10 | Replace `this` in handlers | `function () { this.text = 'x'; }` | `this` replaced by the target id (`btn.text = 'x'`); arrow functions keep their lexical `this` | Approx |
| 11 | Pass event objects through the shim | `e => console.log(e.clientX)` | `onTapped: (eventPoint, button) => _h_click_1(Dom.event("click", eventPoint, button))` | Emulated |
| 12 | Lower known class toggles to class properties | `box.classList.toggle('open')` | `box.cls_open = !box.cls_open` | Emulated |
| 13 | Static timers when called once at top level and never cleared | `setTimeout(tick, 500);` | `Timer { interval: 500; running: true; onTriggered: tick() }` | Full |
| 14 | Dynamic timers otherwise | `id = setInterval(f, 100);` | `id = Dom.setInterval(f, 100)` (likewise `Dom.setTimeout`) | Emulated |
| 15 | Flag constructs that bypass the static rewrite | `eval(s)`, `new Function(s)`, `document.write(x)` | Compile warning. `eval` and `new Function` do run in the engine, but their strings are not rewritten, so DOM access inside them fails. `document.write` is not supported. | Approx (eval); Unsupported (write) |
| 16 | Handle module syntax | `export function f() {}` | Inline scripts: `export` removed, `import` flagged. Local DOM-free modules: emitted as `.mjs` (§3). | Ignored (export); Unsupported (import of DOM-touching or bare modules) |
| 17 | Rename identifiers that collide with QML names or that QML forbids | `let state = 1;`, `let Count = 0;` | `property int state_: 1`, `property int _Count: 0` (property, function and id names cannot start with an uppercase letter, and `state`, `width`, `data`, `children` and so on shadow `Item` members) | Full |
| 18 | Lower syntax the engine lacks | `({ ...a, b })`, `x ??= 1`, `try {} catch {}`, class fields | `Object.assign({}, a, { b })`, `x = x ?? 1`, `catch (_e) {}`, constructor assignments. `?.` and `??` pass through unchanged. Regex named groups, lookbehind, the `s` flag, BigInt and `for await` cannot be lowered and are flagged. | Approx |
| 19 | Lower `async`/`await` | `async function f() { await g(); }` | Generator plus Promise driver | Approx |

## Review notes

- Verified against the 6.11 `.qmltypes` and probed with `qml6`: `FrameAnimation` (`triggered`, `elapsedTime`), `Animation.finished` (revision 2.12), `Transition.running`, `Window.closing`/`visibilityChanged`/`close()`, `Accessible.name`, `MessageDialog` (`buttons`, `text`, `accepted`/`rejected` from `QQuickAbstractDialog`), `TapHandler.tapped(eventPoint, button)`, `DropArea` signals, `MultiPointTouchArea` signals. All `(verify)` markers are resolved.
- Rewrote the JS-engine claims after probing: `async`/`await`, object spread, class fields, `??=`, BigInt, `Intl`, `Promise.finally/allSettled/any`, `flat`, `at`, `fromEntries`, `replaceAll`, regex named groups/lookbehind/`s` flag are missing. `?.` and `??` work, so rule 18 no longer lowers them.
- `setTimeout`, `setInterval`, `fetch`, `queueMicrotask`, `structuredClone`, `performance` and `window`/`document` are undefined in the engine; the globals table now says so.
- `console.table/group/groupEnd/dir/timeLog/countReset` are undefined; `assert`, `count`, `time`, `timeEnd` and `trace` are native (`time`/`timeEnd` were wrongly listed as emulated).
- `requestAnimationFrame` is one-shot, so the single shared `FrameAnimation` queue replaces a raw `FrameAnimation` that would fire every frame. `elapsedTime` is not in DOM milliseconds, so the timestamp comes from `Date.now()`.
- `setTimeout(f, 0)` via `Qt.callLater` is Approx because `callLater` coalesces repeated calls of the same function (probed).
- Id sanitizing now also prefixes ids that start with an uppercase letter; rule 17 and the `const` rule do the same for property and function names (QML rejects uppercase-initial ids and property names at load time, although `qmllint` accepts them).
- `classList` no longer maps to `state`: states are exclusive, so independent classes became per-class `bool` properties with conditional bindings.
- `insertBefore`: `Item.stackBefore` is not callable from QML (probed), so the shim re-attaches `ref` and later siblings; verified that a `Layout` follows the reparenting order. `removeChild` no longer destroys the node, since the DOM allows re-attaching it.
- Dropped the always-`Repeater` plan for dynamic children; imperative `createObject` is the default and `Repeater`/`ListModel` is for recognized render-list loops. `createElement` creates a parentless object because the parent is unknown at that point.
- `dom.js` cannot declare QML objects and each importing file gets its own copy, so shared state lives on the root.
- `el.focus()` condition was wrong (`focus: true` is irrelevant); `blur()` verified to clear `activeFocus`, so it is Full. `mapToItem(null, ...)` verified.
- `select.value` maps to `currentValue` (with `valueRole`) rather than `currentIndex`; added `selectedIndex`.
- `%` sizes map to a parent-relative binding, not `Layout.fillWidth`/`anchors.fill`; added `visibility: hidden`.
- Wheel: Qt's `angleDelta.y` sign is opposite to DOM `deltaY`; the old "angleDelta / 120" mapping was wrong. Added `deltaX`/`deltaY`/`deltaMode` fields.
- `code` no longer derives from `nativeScanCode` (platform-specific); `buttons` needs no translation (bit values match), while `button` does; `key` uses `event.text` for printable keys; added `repeat`.
- `preventDefault`/`stopPropagation`/`stopImmediatePropagation` rewritten: `Keys` handlers run before the item, so `accepted = true` is a real preventDefault; multiple listeners per element are one dispatcher, so `stopImmediatePropagation` is emulable (it was wrongly "one handler per signal").
- Event table: `dragenter/dragover/dragleave` split to `onEntered/onPositionChanged/onExited`; `TextArea` has no `textEdited`; touch events are Approx; added `unload` (`Qt.application.aboutToQuit`), `touchcancel`, `mousemove` via `HoverHandler.onPointChanged`.
- `DOMContentLoaded`/`load` run at the end of the root `Component.onCompleted` (Qt does not guarantee `onCompleted` order across objects); top-level statements are collected there.
- Rule 11 used the undeclared name `point`; `TapHandler.tapped` provides `(eventPoint, button)`, and undeclared injected handler parameters are deprecated in Qt 6, so the arrow form is mandatory.
- Rule 13 now requires a single top-level call (a `Timer` declared statically cannot restart per call); rule 15: `eval`/`new Function` run in the engine, they just bypass the rewrite.
- ES modules: `.mjs` imports work in QML (probed); the old `.import` directive claim for ES `import` in `.js` files was wrong.
- `navigator.language` needs `_` to `-`; `Date.toLocaleString` takes a Qt format, not an options object; `atob`/`btoa` map to `Qt.atob`/`Qt.btoa`; `localStorage` uses `db.transaction`.
- `alert`/`confirm` need `import QtQuick.Dialogs`; `prompt` uses a Controls `Dialog`.
