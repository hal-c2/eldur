# Design: Dynamic Styles (script-toggled classes, created elements, pseudo-classes)

Status: Proposed · Branch: `w/design-dynamic-styles`

This document says how the static QML back end styles a page after its
scripts change it: classes added and removed with `classList`/`className`,
elements built with `document.createElement`, and the dynamic
pseudo-classes (`:hover`, `:focus`, `:checked`, …). It fixes the contract
between `eldur_css`, `eldur_js` (analysis and runtime) and `eldur_convert`, so
the halves can be built in parallel (§10).

It does not cover the live-DOM renderer (`docs/design/live-dom.md`), where
the cascade runs natively on every mutation. §11 says how the two relate.

---

## 0. Decisions in brief

1. **Static cascade.** The converter runs the real cascade (`eldur_css`) for
   every class and state combination that script analysis predicts, and
   emits the results as QML. The runtime never matches CSS rules or maps CSS
   to QML. It only flips booleans and picks precomputed templates.
2. **One condition model.** A *condition* is a boolean fact about one
   element: it has class `c`, or it is in pseudo-state `s`. Script-toggled
   classes and `:hover`/`:focus`/`:checked` both use it. Today's
   `StateVariant` is the special case of a single condition.
3. **QML shape: conditional bindings, not `states`.** Each class that can
   change gets `property bool cls_<name>`. Each affected property gets a
   binding, which is a nested ternary over the conditions it depends on and
   is computed exactly for every combination
   (`opacity: cls_done ? 0.6 : 1`). Descendants and siblings read the
   condition through the condition element's id. A CSS `transition` becomes
   a `Behavior` on the bound property.
4. **Created elements use templates.** For each created-element shape that
   analysis predicts (tag, input type, class set, insertion parent), the
   converter emits a `Component` styled with the real cascade in that
   context. It also emits one registry, `root.eldurTemplates`, that maps a
   canonical key string to the Component. When an element becomes
   connected to the document, the runtime *materialises* it from the
   matching template. A fallback chain handles keys that analysis did not
   predict, and the runtime warns once per missing key.
5. **Metadata gated per page.** `eldurTag`/`eldurClasses` go on every element
   that has its own QML object, but only on pages whose scripts reach
   elements at run time. `baseUrl` goes on the root whenever the page has
   scripts.

---

## 1. Where main is today (verified)

| Area | State on `main` (3509c48) |
|---|---|
| Class toggles | `eldur_runtime.js` keeps classes in `_meta(item).classes`. `_syncClassProp` writes `item["cls_" + sanitized]` only when that property is already a `boolean`. `eldur_convert` emits no `cls_*` property and no binding, so `classList.toggle("done")` has no visible effect. |
| Analysis | `eldur_js::analyze` records `(query, "classList")` in `Analysis::writes` for `classList.add/remove/toggle/replace` and `setAttribute("class")`, and sets `mutates_dom` for `createElement`. It does **not** keep the class names, the `className =` value, the created tag, or where created elements are inserted. |
| Metadata | `eldurTag`, `eldurClasses` and `baseUrl` are never emitted (`dom-api.md` §11 "Converter Metadata Gap"). `objectName` is emitted only for elements with an HTML `id`. |
| Created elements | `Eldur.document.createElement` (`eldur_runtime.js` ≈4205) builds a fixed generic source per tag (`li` → `RowLayout { spacing: 8 }`, `span` → `Text { color: "black"; font.pixelSize: 16 }`, …), with no page CSS. Setting `input.type` already *swaps* the QML object behind a stable wrapper (`set type`, ≈3180): this design reuses that swap. |
| Pseudo-classes | `eldur_css` computes `StyleMap::variants`: one `StateVariant` per `(element, PseudoState)`, covering that element's subtree. `eldur_convert` ignores them and only reports ``info[css]: `:hover`/`:focus`/… styles are not converted to QML states yet`` (`pipeline.rs:78`). Combinations such as `.done:hover` and sibling scope (`:hover + .x`) are not computed. |
| Transitions | Parsed by `eldur_css` (`transition-*` longhands). No `Behavior` is emitted. |

Corpus impact: `16-script-todo` builds its whole list with
`createElement`, so the list renders unstyled (visual score 72).
`15-script-counter` toggles `.negative` on `#count`, and the red colour
never shows.

---

## 2. Evidence: how much is statically knowable

### 2.1 Samples

- **Corpus:** 2 pages mutate classes, 4 sites in all. All 4 class names are
  literals. `li.className = 'todo-item' + (todo.done ? ' done' : '')`
  enumerates to two sets. `count.classList.toggle('negative', n < 0)` uses
  a literal name with a dynamic `force`.
- **Hand-written pages on this machine:** 196 class-mutation sites across
  14 page scripts that are not framework code: the lemonade web UI and
  demos, the stt web pages, the mkdocs theme JS, the open-design deck
  templates, a report template and a menu mock-up. Classified with a
  regular expression over the call or assignment text:

| Shape | Sites | Share | Example |
|---|---:|---:|---|
| `classList.*` with literal names | 127 | 65 % | `classList.add('d-none')` |
| `className =` literal | 53 | 27 % | `.className = 'attachment-preview'` |
| `className =` literal concat/ternary | 2 | 1 % | `'think-tokens-container' + (expanded ? '' : ' collapsed')` |
| `className =` literal + variable | 5 | 3 % | `'chat-message ' + role` |
| `className =` template literal | 6 | 3 % | `` `toast ${type}` `` |
| Opaque (variable, copied attribute) | 3 | 1.5 % | `.className = severityClass` |

**93 % of sites name every class statically.** Another 5.6 % have a
literal part plus an *open* part (`'chat-message ' + role`), and 1.5 % are
opaque. The sample favours small, hand-written pages, which are the kind
this converter targets. It says nothing about framework-rendered pages.

### 2.2 Recovering open and unattributed classes from the style sheet

A class can only change styling if some selector names it. So the open part
of a class value is bounded by the style sheet, not by the script.

- **Open values** (`'chat-message ' + role`, `` `toast ${type}` ``): the
  candidates are the classes that appear in a compound selector together
  with the known classes or the tag. `.chat-message.user` and
  `.chat-message.bot` give `{user, bot}`. `eldur_css::class_candidates`
  (§9.2) computes them.
- **Unattributed targets** (`event.target.classList.toggle('open')`,
  `this.classList…`, `forEach(t => t.classList.remove('active'))` when the
  analysis cannot follow the callback): the class name is known. The
  candidate elements are those that would match a selector containing `.c`
  if they had `c`, meaning the selector's other parts match.
- **Opaque values** stay unstyled, with a diagnostic (§12). The runtime
  still records the classes, so selectors, `matches` and
  `getComputedStyle` see them.

### 2.3 Decision: option (a), static variants

Option (b), a rule table matched at run time, would put a second selector
matcher, a second cascade (specificity, `!important`, inheritance, `var()`,
`em`) and a second CSS→QML mapping into `eldur_runtime.js`. Every
`css-properties.md` row would then have two implementations that drift
apart. The decision also follows from where the work is: §2.1 shows the
inputs are almost always known, and the QML *form* of an element (a `Text`
or a `Rectangle`, a `RowLayout` or a `ColumnLayout`) cannot be changed by a
rule table anyway, because a QML object cannot change type.

What (a) costs: output grows with the number of predicted combinations.
Combinations are capped per element (§5.4) and per shape (§6.5), and every
cap produces a diagnostic. Pages that exceed the caps are what live-DOM
mode is for (§11).

---

## 3. The condition model

```text
Condition = Class { node, class }        // node has class `class`
          | State { node, state }        // node is :hover / :active / :focus / :focus-visible /
                                         // :focus-within / :checked / :disabled
          | Structural { node, test }    // node is :first-child / :last-child / :only-child /
                                         // :nth-child(An+B) / :nth-last-child(An+B) or an
                                         // of-type form, among its parent's element children
```

- **Dynamic classes** of an element `e`, written `D(e)`: classes that
  analysis says scripts may add to or remove from `e` (§9.1), plus open and
  unattributed candidates (§2.2), restricted to *styled classes* (classes
  some selector names). A class `e` has initially and that no script
  touches is not a condition.
- **States**: the condition elements of today's `StateVariant`s.
- **Positions** (S9b, §5.5): one condition per child of a container whose
  children scripts change, and per structural pseudo-class the sheets use
  (`structural_pseudos`). The cascade treats `Structural { node, test }`
  as the truth of `test` for `node` in that combination, whatever the
  document says. Children of containers no script changes keep the
  document's evaluation.
- **Scope**: a condition on `n` can affect `n`, its descendants, its
  following siblings and their subtrees (`+`, `~`), and the descendants of
  following siblings (`.a.on + .b .c`). Today `StateVariant` covers only
  `n`'s subtree. The new API covers sibling scope too.
- **`:has()`, `:nth-child(… of S)`, and conditions on later siblings or
  descendants** (`:has(.x)`): unsupported, with a diagnostic. Sibling
  combinators across a changing list (`li + li`) are still matched against
  the document.

For each element `e`, `R(e)` is the set of conditions that change some
computed value of `e` in at least one combination. `eldur_css` computes
`e`'s style for all `2^|R(e)|` combinations with the real matcher and
cascade. Specificity and source order between several toggled classes are
therefore exact by construction: `.item.done` against `.item:hover` against
`.a.b`. No precedence rule has to be reinvented in QML.

---

## 4. QML shape

### 4.1 Prototype (qml6, Qt 6.11, `/tmp/dynproto`)

| Question | Result |
|---|---|
| `property bool cls_done` and `opacity: cls_done ? 0.6 : 1`, toggled from a `.pragma library` the way `_syncClassProp` does it | Works. Undeclared classes are skipped by the existing `typeof raw[p] === "boolean"` guard. |
| Ancestor: `.list.compact .item` → `implicitHeight: … (list.cls_compact ? 2 : 8)` | Works (`item1.h` 23 → padding 2). |
| Sibling: `.item.active + .item` → `color: item1.cls_active ? "#ff0000" : "#000000"` | Works. |
| Several conditions with precedence: `opacity: cls_done ? 0.6 : hover.hovered ? 0.9 : 1` | Works. The order comes from the precomputed cascade. |
| `states` with two `when:` conditions both true (`cls_a`, `cls_b`) | **Only the first state applies** (`w = 10`, `h = 0`). A `StateGroup` has one active state, so independent classes would need one group per class, and specificity between groups is not expressible. Rejected. |
| An inline `Component` in `Main.qml`, instantiated from JS, reads document ids (`list.cls_compact`) | Works. Templates can bind to static context elements directly. |
| A template reads a *created* ancestor through `property Item eldurUp1` (`eldurUp1 && eldurUp1.cls_done`) | Works, and `qmllint` is clean. |
| `Behavior on opacity` when a class is toggled | Animates (1 → 0.6 over 200 ms). |
| `Behavior` when the template is created with `createObject(parent, { cls_done: true })` | **Does not animate** (0.5 at once). Setting `cls_done` after creation does animate. This matches the browser, where an inserted element does not transition from the initial style. Contract: the runtime passes initial `cls_*` values as initial properties (§8.3). |
| Registry `readonly property var eldurTemplates: ({ "li.done.todo-item@todoList": eldurT_li_0 })`, used from a `.pragma library` | Works. A missing key is detectable. |
| `qmllint` on all of the above | Clean. |

### 4.2 Static elements

For an element `e` with `R(e)` non-empty:

```qml
// <li class="item">   .item.done { opacity: .6 }  .item:hover { opacity: .9 }
Rectangle {
    id: item1
    property bool cls_done: false          // one per class in D(e)
    opacity: item1.cls_done ? 0.6 : item1Hover.hovered ? 0.9 : 1
    Behavior on opacity { NumberAnimation { duration: 200 } }   // only if a transition covers opacity
    HoverHandler { id: item1Hover }        // State conditions use the existing handler/control mapping
}
```

- **Binding expression.** For each QML property, take the conditions it
  actually depends on (drop each condition whose flip never changes the
  mapped value), then emit a Shannon expansion in a fixed order:
  conditions on `e` first, then ancestors, then siblings, and within each
  group in condition order. Equal branches are merged. One condition gives
  `c ? a : b`.
- **Condition expressions.**
  - Class on `e`: `<id>.cls_<name>`.
  - Class on a static ancestor or sibling: `<thatId>.cls_<name>`. That
    element gets an id, as state-variant targets already do.
  - States reuse the existing control mapping: `hovered`, `pressed`,
    `activeFocus`, `checked`, `!enabled`, or a `HoverHandler`/`TapHandler`
    id.
- **Mapping.** Each combination's `ComputedStyle` goes through the *same*
  property mapping as the base style. The emitter maps the base style and
  each variant style to QML property/value pairs and diffs them. That keeps
  one mapping (Rule 3).
- **Form changes.** Some property changes cannot be bindings: anything that
  changes which QML type an element becomes (`display` between flex, block
  and grid; `position`; `float`; a `Text` that gains a background).
  - `display: none` ↔ shown becomes `visible: <cond>`, which also leaves
    Layouts. This is the most common toggle (`d-none`, `hidden`, `open`).
  - Gaining box decoration: promote to the wrapper form in *every*
    combination ("sticky wrapper", as in live-dom §3.3), and bind
    `color`/`border` and padding margins.
  - A change of layout kind (`display: flex` ↔ `block`, `grid`,
    `position`): keep the base form and emit diagnostic `dynamic-form`
    (§12).
- **IFC splitting.** An inline element that has a condition is "a script or
  `:hover` target", so ARCHITECTURE.md already gives it a run of its own on
  an `EldurLines`. Styles inside a `StyledText` run cannot be bound.

### 4.3 Transitions

`transition-property`/`-duration`/`-timing-function`/`-delay` of the
element in its *target* style choose a `Behavior` on each bound QML
property it covers. `all` covers every bound property.
`NumberAnimation`/`ColorAnimation` get `duration`, an `easing.type` from
`Easing` (cubic-bezier → `Easing.BezierSpline` with `easing.bezierCurve`),
and a delay through a `SequentialAnimation { PauseAnimation {} … }`. Only
properties that have a conditional binding get a Behavior, because nothing
else changes. Properties that are not animatable in CSS (e.g.
`display`/`visible`) get none.

---

## 5. Static elements: which elements get conditions

### 5.1 Attribution

| Analysis says | Condition goes on |
|---|---|
| `ClassWrite { target: Query(q), names }` | every element `q` resolves to (all of them for `SelectorAll`) |
| `ClassWrite { target: Created(id) }` | the template(s) of `id` (§6) |
| `ClassWrite { target: Unknown, names }` | `eldur_css::class_candidates(doc, sheets, c)` for each name (§2.2) |
| `open: true` on any of the above | also the open candidates of that target (§2.2) |

### 5.2 Example: `15-script-counter`

`count.classList.toggle('negative', n < 0)` with `.display.negative { color: … }`
gives:

```qml
Text { id: count; property bool cls_negative: false; color: count.cls_negative ? "#c62828" : "#1f2430" }
```

### 5.3 Pseudo-classes (`12-hover-states`)

This design converts the `StateVariant`s that `eldur_css` already computes,
which today only produce the "not converted yet" diagnostic, and it
replaces that diagnostic. `.btn--primary:hover`, `.lift-card:hover`
(transform + shadow) and `.link:hover` become conditional bindings on the
element's `HoverHandler`/control state, with Behaviors from their
`transition`. Joint states (`.btn:disabled` together with `:hover`, and
`.done:hover`) are handled because combinations are computed jointly
(§3).

### 5.4 Caps

At most **6** conditions per element (64 cascades of one element). Above
that, compute each condition alone over the base style, apply them in
cascade order of their winning declarations, and emit diagnostic
`dynamic-combinations`.

---

### 5.5 Structural pseudo-classes (S9b)

`Analysis.mutated_parents` lists the elements whose children scripts
insert, remove or replace (`appendChild`, `insertBefore`, `removeChild`,
`append`, `prepend`, `replaceChildren`, `innerHTML =`, `textContent =`);
`Analysis.mutated_siblings` lists elements scripts remove or put siblings
beside (`remove()`, `before`, `after`, `replaceWith`). The containers that
get position conditions are the static `mutated_parents`, the parents of
`mutated_siblings`, and the parents of created elements (in the probe
document, §6.1). Each child of such a container gets one
`Structural { node, test }` per test the sheets use, so `li:first-child`
and `li:nth-child(even)` cost two conditions per item, within the 6 of
§5.4.

A binding reads the place the runtime keeps (§8.1):

| Test | Expression on item `X` |
|---|---|
| `:first-child` | `X.eldurIndex === 1` |
| `:last-child` | `X.eldurIndex === X.eldurCount` |
| `:only-child` | `X.eldurCount === 1` |
| `:nth-child(An+B)` | `nth(A, B, X.eldurIndex)`, e.g. `X.eldurIndex % 2 === 0` |
| `:nth-last-child(An+B)` | the same over `(X.eldurCount - X.eldurIndex + 1)` |
| of-type forms | the same over `eldurTypeIndex`/`eldurTypeCount` |

`eldurIndex`/`eldurCount` (and the type pair, only when an of-type test is
read) are declared on the item with the document's values, so the first
frame matches without the runtime. In a template the element's own tests
are read locally. A created ancestor exports
`readonly property bool eldurIs<Test>` (`eldurIsFirstChild`,
`eldurIsNthChild2n0`, `m` for a minus sign), which descendants read through
`eldurUpN`. A static ancestor is read through its id.

A combination may change which border sides are drawn (`li:first-child
{ border-top }`). Instead of a `dynamic-form`, the item gets border strips
on the union of the sides any combination draws. A side that a
combination does not draw has width 0 and a transparent colour.

At most **32** element children per container (`MAX_POSITIONED`). A
larger container keeps the document's positions, with `dynamic-structural`
(§12).

## 6. Created elements: templates

### 6.1 What analysis predicts

For every `document.createElement(tagLiteral)` site, `eldur_js::analyze`
follows the variable it is assigned to within its function, through
`return`, to call sites (one level). It records a `CreatedElement` (§9.1):

- **tag**;
- **input type**, from `x.type = "checkbox"` / `setAttribute("type", …)`;
- **class sets**: every complete class value the element can have, from
  `className =`, `setAttribute("class", …)` and `classList.*` with ternary,
  `+` and template-literal enumeration (`'todo-item' + (d ? ' done' : '')`
  gives `{todo-item}` and `{todo-item, done}`); plus `open` if part of the
  value is not literal;
  `Object.assign(el, {className, id, textContent, innerHTML})` writes as the
  assignments would, and returns `el`;
- **parents**: the targets of `appendChild`/`append`/`prepend`/
  `insertBefore`/`replaceChild`/`before`/`after` on it, each a `Query` (a
  static element), a `Created(id)` or `Unknown`.

For `16-script-todo`: `li {todo-item | todo-item done} → #todo-list`;
`input[type=checkbox] → li`; `span {todo-text} → li`;
`button {delete} → li`.

### 6.2 What the converter emits

For every predicted shape, the converter builds a *probe document*: a clone
of the page DOM with the shape appended as the last child of each predicted
parent (`eldur_dom` mutation subset, §9.3), nested for created parents. It
then runs `compute_styles` and `conditional_styles` on that document. Each
probe element is boxed and emitted with the ordinary emitter into a
`Component` at the end of the root:

```qml
// <li class="todo-item [done]"> created by script, in #todo-list
Component {
    id: eldurT_li_0
    RowLayout {
        property string eldurTag: "li"
        property string eldurClasses: ""
        property string eldurTemplate: "eldurT_li_0"
        property bool cls_done: false
        spacing: 12
        // padding, border-bottom, … from `.todo-item`
    }
}
// <span class="todo-text"> in eldurT_li_0   (.todo-item.done .todo-text)
Component {
    id: eldurT_span_2
    Text {
        property string eldurTag: "span"
        property string eldurClasses: ""
        property string eldurTemplate: "eldurT_span_2"
        property Item eldurUp1: null
        color: eldurUp1 && eldurUp1.cls_done ? "#9aa3b5" : "#1f2430"
        font.strikeout: !!(eldurUp1 && eldurUp1.cls_done)
        font.pixelSize: 16
        Layout.fillWidth: true
    }
}
readonly property var eldurTemplates: ({
    "li.todo-item@todoList": eldurT_li_0,
    "li.done.todo-item@todoList": eldurT_li_0,
    "input[type=checkbox]@eldurT_li_0": eldurT_input_1,
    "span.todo-text@eldurT_li_0": eldurT_span_2,
    "button.delete@eldurT_li_0": eldurT_button_3,
    "li.todo-item@*": eldurT_li_0
})
readonly property var eldurStyledClasses: ["delete", "done", "todo-item", "todo-text"]
```

- **Grouping.** Class sets whose probes have the same QML *form* (§4.2)
  share one Component. The classes that differ between them become
  `cls_*` bindings. This is why the two `li` keys above point to the same
  `eldurT_li_0`. Sets with different forms get different Components.
- **Conditions in templates.**
  - On the element itself: `cls_*`.
  - On a static context element: its id (works, §4.1).
  - On a created ancestor at logical distance N: `property Item eldurUpN`,
    read as `eldurUpN && eldurUpN.cls_x`.
  - On a created ancestor's state: that template exports
    `readonly property bool eldurHovered` (also `eldurPressed`, `eldurFocused`,
    `eldurFocusVisible`, `eldurFocusWithin`, `eldurChecked`, `eldurDisabled`) when
    a descendant template needs it.
- **Inherited values** (`color`, `font-*`) are already resolved in the
  probe context, so a template is exact for its key.

### 6.3 The key

```text
key     = tag [ "[type=" type "]" ] { "." class } "@" context
tag     = lower-case local name
type    = lower-case input type, only for <input>
class   = the element's classes ∩ eldurStyledClasses, deduplicated, sorted (code-point order)
context = parent's eldurTemplate     if the logical parent is a template instance
        | parent's eldurKey          if the logical parent is a static element with one
        | "*"                      otherwise
```

One level of context is enough, because the parent's template name already
encodes its own context. Static elements that analysis predicts as parents
get `property string eldurKey: "<qmlId>"`. The runtime cannot read QML ids.

### 6.4 Materialisation (runtime)

1. `createElement(tag)` behaves as today: it creates a *provisional* item
   from the generic source. Every DOM API works on it before insertion.
2. When an insertion makes a subtree **connected** (its logical root
   reaches `_root`), the runtime walks it **top-down**. For each created
   element it computes the key and looks it up (§6.6). If the result is a
   different Component from the item's current `eldurTemplate`, it
   *rematerialises*:
   - create the new item with
     `createObject(appendTarget, { cls_…: true, eldurClasses: "…" })`
     (initial properties, so no Behavior fires);
   - move the children into it;
   - replay the recorded attributes, text, value, `checked`, `disabled`,
     `hidden`, inline `style.*`, `dataset` and listeners;
   - set `eldurUpN`;
   - swap the item behind the stable wrapper and destroy the old one.

   This generalises today's `set type` swap. Children come after their
   parent, because their key depends on the parent's template. Children
   that are all text nodes go in as `appendChild` would put them: into the
   template's own Text (its inline model), not into a column below it.
   Element children stay items, since one may be a box that something is
   placed in (a relative span holding an absolute dot).
3. **Class changes after connection** (`className =` or `classList.*`) set
   `cls_*` as today. If the new key resolves to a *different* Component (a
   form change), the element is rematerialised in place. A class set
   before `appendChild` simply takes part in the step-2 lookup, so
   assigning `className` before or after insertion gives the same result.
4. **Detaching** (`removeChild`, `remove`) does not rematerialise.
   Reinsertion runs step 2 again with the new context.

### 6.5 Caps

At most **16** keys per `CreatedElement`. Beyond that, keep the first 16 in
enumeration order plus the `@*` fallbacks, and emit diagnostic
`dynamic-templates`.

### 6.6 Unknown context and missing keys

The converter emits, for each predicted `(tag, type, class set)`, an `@*`
entry computed with the probe appended to `<body>`. The runtime looks keys
up in this order, and the first hit wins:

1. `tag[type].classes@context`, the exact key;
2. `tag[type].classes@*`;
3. `tag[type]@context`;
4. `tag[type]@*`;
5. the generic source (today's behaviour).

A miss at step 1 logs once per key:
`console.warn("eldur: no style template for li.foo@todoList; using <step>")`.
An element of a line's model that a box splits into items (`_vToReal`) does
not log. The model showed only its font and colour, which the item is given,
unless the model also painted a background or edges.
At conversion time, `Unknown` parents and `open` class values produce
diagnostic `dynamic-context` (§12). `@*` templates are not exact for
inherited properties, since they inherit from `<body>`, and the diagnostic
says so.

### 6.7 `innerHTML` / `insertAdjacentHTML` with literal markup

The same mechanism applies to markup assigned to `innerHTML` (`=` and
`+=`) or passed to `insertAdjacentHTML`: analysis parses each alternative
value with `eldur_dom::Document::parse_fragment` and reports its elements as
`CreatedElement`s whose parent is the target (`afterbegin`/`beforeend`;
`beforebegin`/`afterend` give an `Unknown` parent). Nested elements are
parented to their fragment parent. `Eldur.setInnerHTML` then materialises
through the registry (S9a).

- A value is literal when its structure is: concatenations and template
  literals may put dynamic parts in text and attribute values. A dynamic
  part in a `class` value makes the classes open (§6.1); in `type`, the
  input type unknown.
- Each distinct `(parent, tag, type, class)` is one `CreatedElement` with
  `ClassOp::Assign` of that class value, so `li.item` and `li.done.item`
  get their own templates and a later `classList` toggle swaps keys.
- A dynamic part at the top level of the fragment, or in a tag or
  attribute name, makes that part opaque (`dynamic-markup`): its elements
  are not predicted and fall back at run time. The literal elements
  around it still are.
- `<script>`, `<style>`, `<template>` and non-HTML (`<svg>`) elements are
  left out: the runtime does not make items for them.
- The runtime still builds the nodes with its own small parser, so markup
  that relies on HTML's implied end tags or foster parenting can differ
  from what analysis predicted (the fallback chain then applies).
- `getInnerHTML` returns the last string assigned, not a serialisation;
  `outerHTML` and markup built by `Array.prototype.join` are opaque.

### 6.8 Created inline elements

Inline content a script makes is broken into lines by the same layouter as
the page's own: `EldurLines` (`docs/spec/layout.md` §1.3, §2). That covers
`createElement` with `appendChild`/`insertBefore`/`replaceChild`, `innerHTML`
and `insertAdjacentHTML` markup, `textContent` and text nodes, and
inline-block children. There is no `Flow` of inline items any more.

- **The line.** A created block's inline content is an `EldurLines` with a
  `readonly property Component eldurTextRun` (`inline.rs inline_flow`).
  `_appendTarget` appends to it, because a line has `spacing`.
- **Text and inline elements are nodes, not items.** On a closed page, the
  created inline elements and text nodes of a template's line become nodes of an
  inline model (`_vModelTemplateLine` after `_materializeSubtree`, and
  `_vNewLinesTail` for content appended later, through `_vHostOf`). The model
  is cut into runs of one font and line box (`_vRunsFor`, `_vPutRuns`), and the
  line places them as it places a written paragraph's: words break across
  lines, baselines line up, mixed sizes and `vertical-align` get their own line
  boxes, and `text-align` moves runs, not glyphs inside a run. The template a
  converter wrote for such an element is its look as an item. On a line, the
  rules style it as a node (`eldurInlineRules`, which now also carry
  `vertical-align`, `overflow-wrap` and `word-break`).
- **Boxes stay items.** An atomic inline (inline-block, a control, `img`) is an
  item on the line. A made one gets its CSS `height` as well as its width
  (`inline.rs atomic`), because its empty `Text` would otherwise give its line box
  a text line's height. Runs on either side of a made box stay separate runs even
  before its style has sized it (`_vLineModels`).
- **A line that replaces a `Text`** keeps that `Text`'s alignment, wrapping and
  `white-space: pre*` (`_linesLike`). On a closed page there is no cascade to
  say how the paragraph aligns and wraps, and the converter set the `Text`
  from the page's style. The converter marks a preserving `Text` with
  `eldurPre`.
- **Open pages.** The run-time cascade lays the line out (`_gLayOutRun`). It
  sets the line's font, CSS size, colour, `text-align` and wrap mode
  (`_gWrapMode`: `overflow-wrap`/`word-break`), sets each run's line height
  and line box, and gives each box its `vertical-align` through
  `eldurVAlign`/`eldurVShift` (`_gBoxLevel`). A box made on a written
  paragraph's line, which no rule run lays out, gets its level in
  `_gLayoutRoots`. A `_vRender` that changes how many lines a run takes marks
  the layout dirty, because on an open page the rules size the boxes, not
  Qt's layouts.
- **Text nodes.** A text node that is its own item (`_materializeTextRun`)
  collapses its spaces as `white-space` says, and they stay breakable spaces
  (`_runText`). The line places a run's edge spaces itself, so U+00A0 is no
  longer needed there. A hand-written template whose line is still a `Flow`
  keeps U+00A0, because Qt does not measure an item's trailing space.
- **Element rectangles.** A node's `getBoundingClientRect` comes from the
  line's placement of its run (`runLines`). An edge space the line put beside a
  run counts inside its element when CSS keeps it there: a leading space that
  collapsed into nothing before it, and a trailing one with more after it on the
  line (`ownLead`/`ownTrail`).
- **Colours.** A model's colours are CSS strings (`rgb(34, 34, 34)`), and Qt 6
  reads that as black when it is assigned to a `color`. Every colour from a
  model or the cascade goes through `_gQtColor`. A run's colour is bound to its
  line's, because the rules may set the line's colour after the runs are made.
- **Decorations and states around a line.** A block's `text-decoration` is
  drawn through all its inline content: a run follows its line's
  `font.underline`/`font.strikeout`, which the converter binds to the block's
  classes (`_vRunText`). The nodes are markup, which no template binding
  redraws, so an element around the line that a state rule's compound matches
  (`.item:hover .label`) is watched (`_vWatchStates` with `_gWatch`), and a
  change of its state redraws the models.
- **Class toggles** on created elements work as on blocks (§6.4 step 3).
  The keys include every set a toggle makes. A toggle through a query, or on
  a target the analysis cannot attribute, counts when the selector matches the
  probe or when the class could style it (§2.2).
- **Shared keys.** Two probes with the same key are two templates. The first
  wins the key, and the second's children are also registered under the winner
  (`span.tag@eldurT_li_0`). The runtime finds them by the template the parent
  actually has.

Limits (measured against Chromium, `tests/created_rules`, `tests/apps`):

- `text-indent` is not converted anywhere, written or made (the converter
  reports it). A made indented paragraph starts flush.
- A pre-wrap word longer than the line breaks inside the word, where Chromium
  lets it overflow.
- Qt Layouts round some boxes to whole pixels, so a made box can be up to 1px
  off in y (made chat bubbles). A made `<mark>` on a wrapping line is ~2px off
  in x and ~3px in width.
- A created `<a>` is probed with `href=""`. Analysis records only `type`
  and `class`, and an `<a>` without `href` would not match `a:link` or the
  user-agent link style.

---

## 7. Runtime metadata

| Property | On | When | Why |
|---|---|---|---|
| `eldurTag: "<tag>"` | every element with its own QML object | page has `Analysis::runtime_dom` (§9.1) | tag selectors, `tagName`, `matches`, `closest` |
| `eldurClasses: "<class attr>"` | same, when the element has classes | same | class selectors, `className`, `classList` initial state |
| `objectName: "<html id>"` | elements with an HTML `id` | always (as today) | `getElementById`, `#id` selectors |
| `property bool cls_<c>` | elements and templates with `c ∈ D(e)` | when there are conditions | styling (§4) |
| `eldurKey: "<qmlId>"` | static elements predicted as insertion parents | when there are templates | template context (§6.3) |
| `property string baseUrl` | root | page has any script | `a.href`/`img.src` resolution, `location` |

- **Size.** About 100 bytes per element. The largest snapshots have 94–104
  elements, so this adds 4.5–7.7 % (≈10 KB of 135–207 KB). Static-only
  pages pay nothing.
- **Why not "only elements reachable by script"?** `querySelectorAll`,
  `children`, `closest`, `matches` and `event.target` reach elements that
  no literal query names, and an element without `eldurTag` gives a wrong
  answer silently.
- **Gating.** The flag is per page: `runtime_dom` is false only when every
  DOM access is a statically resolved literal query with no traversal, no
  `matches`/`closest`, and no `mutates_dom`.
- **Elements merged into a `StyledText` run** have no object, so selectors
  cannot reach them at run time. This is already a known limit.
- **`baseUrl` value.**
  - If `<base href>` resolves to an absolute URL, use it.
  - Otherwise, if `Input::url` is an absolute `http(s):`/`file:` URL, use
    that.
  - Otherwise emit `Qt.resolvedUrl("<file name of Input::url>")`, so
    snapshots stay machine-independent. Relative links then resolve next
    to the converted app, which is a documented divergence.
- **Sync.** The runtime keeps `eldurClasses` equal to the class list on every
  change (when the property exists). `_meta(item).classes` stays the source
  of truth.

---

## 8. Contract between `eldur_convert` and `eldur_js` (runtime)

### 8.1 Names

| Name | Type | Owner (writes) | Reader |
|---|---|---|---|
| `eldurTag` | `string`, lower-case | converter (static, templates) | runtime |
| `eldurClasses` | `string`, space-separated | converter (initial), runtime (on change) | runtime |
| `eldurTemplate` | `string`, the Component id | converter (in each template) | runtime (key context, swap check) |
| `eldurKey` | `string`, the QML id | converter | runtime (key context) |
| `cls_<s>` | `bool`; `s` = class with `[^A-Za-z0-9_]` → `_` (today's `_sanitizeClassProp`) | converter declares and sets the initial value; runtime sets it | bindings |
| `eldurUp<N>` | `Item`, N ≥ 1 = logical-ancestor distance | runtime, at materialisation | template bindings |
| `eldurHovered`, `eldurPressed`, `eldurFocused`, `eldurFocusVisible`, `eldurFocusWithin`, `eldurChecked`, `eldurDisabled` | `readonly bool` | converter (in templates) | descendant templates' bindings |
| `eldurIndex`, `eldurCount` | `int`, 1-based place among element siblings, and their number | converter (document value), runtime (`_renumber`) | structural bindings (§5.5) |
| `eldurTypeIndex`, `eldurTypeCount` | `int`, the same among siblings with the same tag | converter (only when an of-type test reads it), runtime | structural bindings |
| `eldurIs<Test>` | `readonly bool`, e.g. `eldurIsFirstChild`, `eldurIsNthChild2n1` | converter (in templates) | descendant templates' bindings |
| `eldurTextRun` | `readonly Component` on a created block's `EldurLines`: a `Text` with the block's text style | converter (§6.8) | runtime (text nodes in that line) |
| `root.eldurTemplates` | `var` object: key → `Component` | converter | runtime |
| `root.eldurStyledClasses` | `var` array of strings, sorted | converter | runtime (key building) |
| `root.baseUrl` | `string` | converter | runtime (`init`, already supported) |
| Component ids | `eldurT_<tag>_<n>` (n = emission order, from 0) | converter | snapshots only (the runtime goes through the registry) |

Two classes whose sanitised names collide (`a-b`, `a_b`) are a conversion
diagnostic. Both map to the same property.

### 8.2 Converter owns

The cascade, which classes and states are conditions, the binding
expressions, Behaviors, the choice of forms, the Components, the registry,
the `@*` entries, the metadata gating, and every conversion-time
diagnostic.

### 8.3 Runtime owns

- Class bookkeeping (as today), plus writing back to `eldurClasses`.
- Key building (§6.3) and the fallback chain (§6.6), with one warning per
  missing key.
- Provisional items, detecting connection, and top-down rematerialisation:
  initial properties carry `cls_*` and `eldurClasses`; recorded state is
  replayed; `eldurUpN` is set.
- Rematerialisation on a form-changing class change.
- Created text nodes in a line with `eldurTextRun` (§6.8):
  `_materializeTextRun` remakes the node's `Text` from that Component, in
  place, with its spaces collapsed as `white-space` says. `textContent` reads and writes the text
  as the script set it, and `_layoutChanged` follows each swap.
- Positions (S9b): `_renumber(container)` rewrites `eldurIndex`/`eldurCount`
  (and the type pair where declared) on the container's element children
  after every insert, remove, reorder, template swap, `innerHTML` and
  `textContent` write. `_settleLayouts` also renumbers each dirty item and
  its parent. Element children are the raw children minus created text
  runs and layout spacers (`_isLayoutSpacer`: a bare, childless
  `QQuickItem` the converter puts at the end of a flex row). Appending
  inserts before such spacers. `textContent = ''` on a layout child
  container detaches its static element items (as `removeChild` does)
  rather than destroying them, because pending bindings may still read
  them.

The runtime contains **no CSS matching for styling and no CSS→QML mapping**
beyond the existing `el.style.*` setters.

### 8.4 Each half's tests stand alone

- Runtime tests (`crates/eldur_js/tests/qml_runtime.rs`) use **hand-written
  QML fixtures that follow §8.1**: a root with `eldurTemplates`, a few
  Components and a static parent with `eldurKey`. They assert lookup,
  fallback, the warning, rematerialisation (children, listeners, `checked`,
  text kept), `eldurUpN`, no Behavior on insertion, and a Behavior on toggle.
- Converter tests (insta snapshots, plus `qml:check`) assert the emitted
  names and registry. The end-to-end tests in §13 join the halves.

---

## 9. Crate API additions (add-only)

### 9.1 `eldur_js` (`src/lib.rs`)

```rust
/// A created element (`document.createElement` call site) as analysis predicts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CreatedId(pub u32);

/// An element a script refers to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElementRef {
    Query(DomQuery),
    Created(CreatedId),
    /// `event.target`, `this`, untracked variables.
    Unknown,
}

/// Classes a script adds, removes or assigns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassWrite {
    pub target: ElementRef,
    /// Literal class names involved (each may end up present or absent).
    pub names: Vec<String>,
    /// Part of the value is not a literal (`'chat ' + role`, `` `toast ${t}` ``, a variable).
    pub open: bool,
    /// `className =` / `setAttribute("class")`: replaces the whole set.
    pub assigns: bool,
    pub source: String,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedElement {
    pub id: CreatedId,
    pub tag: String,
    /// `<input>` type set by the script, if literal.
    pub input_type: Option<String>,
    /// Every complete class set the element can have when inserted (sorted, deduplicated).
    pub class_sets: Vec<Vec<String>>,
    pub open_classes: bool,
    /// Where it is inserted.
    pub parents: Vec<ElementRef>,
    pub source: String,
    pub line: Option<u32>,
}

// New fields on `Analysis` (it derives `Default`):
//   pub class_writes: Vec<ClassWrite>,
//   pub created: Vec<CreatedElement>,
//   /// Scripts reach elements by runtime selectors, traversal, `matches`/`closest`,
//   /// event targets, or mutate the DOM: every element needs `eldurTag`/`eldurClasses`.
//   pub runtime_dom: bool,
// Added in S9b (§5.5):
//   pub mutated_parents: Vec<ElementRef>,
//   pub mutated_siblings: Vec<ElementRef>,
```

Today's `writes` entries `(q, "classList")` are kept.

### 9.2 `eldur_css` (`src/lib.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Condition {
    Class { node: NodeId, class: String },
    State { node: NodeId, state: PseudoState },
    /// Added in S9b: `node` passes `test` (§5.5).
    Structural { node: NodeId, test: Structural },
}

/// S9b: a structural pseudo-class without `of S`.
pub enum Structural {
    FirstChild, LastChild, OnlyChild,
    NthChild { a: i32, b: i32 }, NthLastChild { a: i32, b: i32 },
    FirstOfType, LastOfType, OnlyOfType,
    NthOfType { a: i32, b: i32 }, NthLastOfType { a: i32, b: i32 },
}
// impl Structural { pub fn of_type(self) -> bool; pub fn matches(self, index: usize, count: usize) -> bool }

/// S9b: the structural pseudo-classes some selector uses.
#[must_use]
pub fn structural_pseudos(sheets: &[Stylesheet]) -> BTreeSet<Structural>;

/// The styles of every element some condition affects, for each combination of the
/// conditions it depends on.
#[derive(Debug, Clone, Default)]
pub struct ConditionalStyles {
    pub conditions: Vec<Condition>,
    pub elements: Vec<ElementConditional>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElementConditional {
    pub node: NodeId,
    /// Indices into `conditions` (at most 6; see `diagnostics` when capped).
    pub depends_on: Vec<u16>,
    /// `2^depends_on.len()` styles; bit `i` of the index = `depends_on[i]` holds.
    /// Index 0 is not the base style when a class is present initially:
    /// bits are "holds", not "differs from the document".
    pub by_mask: Vec<Box<ComputedStyle>>,
}

/// Cascade under every combination of `conditions` (scope: node, subtree,
/// following siblings and their subtrees).
#[must_use]
pub fn conditional_styles(
    doc: &Document, sheets: &[Stylesheet], viewport: &Viewport, conditions: &[Condition],
) -> ConditionalStyles;

/// Classes some selector in `sheets` names.
#[must_use]
pub fn styled_classes(sheets: &[Stylesheet]) -> BTreeSet<String>;

/// Elements on which having `class` would change which rules match (§2.2), and,
/// for an element known to have classes `known`, the classes that co-occur with
/// them in a compound selector (`open` completion).
#[must_use]
pub fn class_candidates(doc: &Document, sheets: &[Stylesheet], class: &str) -> Vec<NodeId>;
#[must_use]
pub fn open_class_candidates(sheets: &[Stylesheet], tag: &str, known: &[String]) -> BTreeSet<String>;
```

Inside the crate, `MatchContext.active_state: Option<(NodeId, PseudoState)>`
becomes a set of active states plus a per-node class override (added and
removed). `StyleMap::variants` and `StateVariant` stay as they are.

### 9.3 `eldur_dom`

The probe document needs a subset of live-dom §2.2, with **the same names
and signatures**, so live mode later extends it rather than replacing it:
`create_html_element(&mut self, local: &str) -> NodeId`,
`set_attribute(&mut self, node, name: QualName, value) -> Result<(), DomError>`,
`append_child(&mut self, parent, child) -> Result<NodeId, DomError>`, and
`DomError`. No mutation log is needed for this design. `Document` already
derives `Clone`.

### 9.4 `eldur_convert`

No public API change. `Options::state_variants` now also gates class
conditions and templates (one switch for "dynamic styles").

---

## 10. Implementation slices

| # | Slice | Crate(s) | Depends on | Parallel with |
|---|---|---|---|---|
| S1 | Runtime metadata emission: `eldurTag`/`eldurClasses` gated by `runtime_dom` (temporarily approximated by `mutates_dom` ∨ any runtime-fallback query), `baseUrl` on the root. Update `dom-api.md` §11. Fixes problem 2 on its own. | `eldur_convert` | — (after the current `eldur_convert` worker lands) | S2, S3, S4, S5 |
| S2 | `Condition`, `conditional_styles` (joint combinations, sibling scope), `styled_classes`, `class_candidates`, `open_class_candidates`, with unit tests whose values come from Chromium. | `eldur_css` | — | S3, S4, S5 |
| S3 | `eldur_dom` mutation subset (§9.3). | `eldur_dom` | — | all |
| S4 | Analysis: `ClassWrite`, `CreatedElement`, `runtime_dom`. Dataflow through locals, one-level returns and `querySelectorAll(...).forEach` callback parameters; enumeration of literal, ternary, `+` and template values. | `eldur_js` (`analyze.rs`) | — | S2, S5 |
| S5 | Runtime: registry lookup, fallback chain and warning, provisional → rematerialise on connect (generalising `set type`), replay of recorded state, `eldurUpN`, initial-property `cls_*`, `eldurClasses` write-back, form-change swap. qml6 tests on hand-written fixtures (§8.4). | `eldur_js` (`eldur_runtime.js`) | — | S2, S4 |
| S6 | Conditional bindings for static elements: classes and `:hover`/`:focus`/`:checked`/…, `visible` for `display:none`, sticky wrapper, `Behavior`s from `transition`. Removes the "not converted to QML states yet" diagnostic. Corpus 12, 15. | `eldur_convert` | S1, S2, S4 | S5 |
| S7 | Templates: probe document, Components, grouping by form, registry, `@*` entries, `eldurKey`, `eldurUpN`/`eldur<State>` exports. Corpus 16. | `eldur_convert` | S3, S6 | — |
| S8 | Oracles and tests: a `run` step in `computed.mjs` (and `--run` in `geometry.mjs`), corpus case `26-class-toggles`, end-to-end qml6 behaviour tests (§13). | `tools/visual`, `tests/corpus`, `eldur_convert/tests` | — (the e2e tests are written first and fail until S6/S7) | all |
| S9 | Later: `innerHTML` literal markup through the registry (§6.7); structural pseudo-classes for mutated containers (S9b, §5.5). | `eldur_js`, `eldur_convert` | S7 | — |

`eldur_convert` slices (S1 → S6 → S7) are sequential. The rest can run in
parallel.

---

## 11. Relation to live-DOM mode

`docs/design/live-dom.md` runs the real DOM and `StyleEngine` at run time,
and restyles natively on each `Attribute("class")` mutation. It needs none
of `cls_*`, templates or the registry. This design is the **static back
end's** answer, and it is bounded by what analysis can predict.

The two share:

- the selector matcher and cascade in `eldur_css`. `conditional_styles` is
  the batch form of what `StyleEngine` does incrementally;
- the `eldur_dom` mutation API names (§9.3);
- the sticky-wrapper rule (live-dom §3.3);
- the form choice in the emitter. A conditional binding is a
  `BindingExpr::Cond` over `Read(ObjectProp{… cls_x …})` in live-dom's
  `SceneTree`, so when the `SceneTree` refactor lands, S6's output is
  expressed there rather than as text.

Pages that exceed this design's limits (§12) are the ones to run in live
mode. The two are not alternatives for the same page.

---

## 12. Limits and diagnostics

All are `info[css]`/`info[js]` diagnostics with the element and the source
line.

| Code | When | Effect |
|---|---|---|
| `dynamic-opaque-class` | `ClassWrite` with no literal names and no open candidates (`el.className = x`) | Classes are tracked at run time (selectors work) but not styled. |
| `dynamic-context` | `CreatedElement` with an `Unknown` parent or open classes, on a closed page (an open page's cascade styles it where it lands), other than a table's rows and sections | `@*` templates are used. Inherited values come from `<body>`. |
| `dynamic-form` | A combination changes layout kind (`display` flex/block/grid, `position`, `float`) | The base form is kept. Bound properties still apply. |
| `dynamic-combinations` | More than 6 conditions on one element | Conditions are applied independently, in cascade order. |
| `dynamic-templates` | More than 16 keys for one created element; or, on a closed page, a probe with no item of its own other than a table's row or section | Extra keys use the fallback chain. |
| `dynamic-markup` (`warning[js]`) | Markup given to `innerHTML`/`insertAdjacentHTML` is not literal (§6.7) | Its elements are not predicted and use the fallback chain. |
| `dynamic-unsupported-selector` | The condition is inside `:has()`, or on a later sibling or a descendant | Not converted. |
| `dynamic-structural` (`warning[css]`) | A container scripts change has more than 32 element children | Its children keep the document's `:first-child`/`:nth-child()` results. |
| `dynamic-class-collision` | Two classes sanitise to the same `cls_` name | They share one property. |
| *(runtime)* `no style template for …` | A key was not predicted | The fallback chain is used. |

Structural pseudo-classes on children of script-mutated containers follow
the runtime's `eldurIndex`/`eldurCount` (S9b, §5.5). Still static: sibling
combinators (`li + li`, `li ~ li`) across a changing list,
`:nth-child(… of S)`, and containers analysis cannot name (a removal
through `e.target.closest('li').remove()` does not mark its list).
Attribute selectors on script-written attributes (`[aria-expanded=true]`,
`[hidden]`) follow the same condition model and are out of scope here.

---

## 13. Test plan

1. **`eldur_css` unit tests (S2).** For each case (ancestor
   `.list.compact .item`, sibling `.tab.active + .tab`, conflicting
   `.a.b`/`.b`/source order, `.done:hover`, `!important`, `var()` under a
   class, `display:none` ↔ shown, `em` under a font-size class), the
   expected values come from Chromium: `computed.mjs` with a new
   `"run": "<js>"` field that is evaluated before reading (S8). Expected
   values are stored as JSON next to the test and never written by hand.
2. **Runtime tests (S5).** qml6 fixtures as in §8.4.
3. **Corpus snapshots.** `12-hover-states` (diagnostic gone, bindings and
   Behaviors), `15-script-counter` (`cls_negative`), `16-script-todo`
   (templates and registry), and the new **`26-class-toggles`**, which has:
   - a list with a "compact" toggle on the ancestor
     (`.list.compact .item`);
   - tabs that toggle `.active`, with a sibling rule
     (`.tab.active + .tab`);
   - two conflicting toggled classes on one element;
   - a `d-none` toggle;
   - `.item.done:hover`;
   - a `transition: opacity .2s`;
   - created toasts with `` className = `toast ${type}` `` (open-class
     completion);
   - one element appended to an unknown parent (the `@*` path).

   Its `DOMContentLoaded` handler toggles classes, so the visual harness
   (initial state) measures the toggled state. Every snapshot passes
   `mise run qml:check`.
4. **Behaviour tests (S8, `crates/eldur_convert/tests/dynamic_styles_e2e.rs`).**
   Convert a page, load it in qml6 off-screen with a probe that calls page
   functions (click, toggle) and prints QML properties after Behaviors
   settle. Compare with Chromium's computed style after the same `run`
   step (`computed.mjs`) and geometry (`geometry.mjs --run`), mapped as
   follows:
   - `opacity` → `opacity`;
   - `color` → `color`;
   - `text-decoration-line: line-through` → `font.strikeout`;
   - `display: none` → `!visible`;
   - box size → `width`/`height` within 1 px.
5. **Visual parity.** Track `16-script-todo` (now 72) and `26-class-toggles`
   in `tools/visual`.

---

## 14. Lead decisions (2026-10-31)

1. **One switch.** `Options::state_variants` turns on classes and
   templates too. They are one feature, precomputing styles for runtime
   conditions, and a page that wants :hover variants but not class
   toggles has no use case. Rename it only with the next breaking options
   change; its doc comment is updated now.
2. **`baseUrl` follows the converted app, not the source.**
   - **Local inputs:** `Qt.resolvedUrl("Main.qml")`. Assets are copied
     next to the output, so this is where relative links actually
     resolve. A browser would show `file:///…/source/…` instead, which is
     the one visible difference, and it is accepted.
   - **`http(s)` inputs:** the source URL.
   - **`<base href>`:** overrides both, resolved against them, as in
     HTML.

## 15. Where a template stops: what makes a page open

A template is probed at each place the analysis knows (§6): appended under each
static element a query names and under each created parent, and cascaded there.
What the page holds when it is converted is therefore in the template. What it
can't hold is what only running the script decides, and a page whose stylesheet
asks about that is converted open (`selectors_outrun_templates` in
`pipeline.rs`), so the runtime cascade answers instead. There is one rule, not a
list of selector shapes: **a rule outruns templates when what it matches can
change after conversion.** That is the case when the rule:

- names an id a script writes (or the script computes an id);
- names an attribute a script writes, on a page that creates elements. A page's
  own elements keep the converter's variants, so a theme switch on
  `<html data-theme>` stays closed;
- gives an element a class that takes it out of flow;
- gives `::before`/`::after` a box (`display`, `float` or `position`) through a
  class a script writes. The runtime makes, shows and hides such boxes only on an
  open page, and a template has no place to put one;
- names a class at all, when a script writes a computed class name on an element
  the analysis can't name (it can be any class on any element);
- changes a created element's width or kind of box, and matches that element by
  more than its tag and the classes it is created with. Paint, margins and height
  stay template conditions (§5.5). "More than" is decided per created element:
  - the subject has a state, an id, or an `input[type]` the analysis didn't see;
  - the subject's classes are computed (`open_classes`);
  - the context can come to fit later. Each compound left of the subject is
    checked against the ancestors the element can have (`Around`): a static
    element and its ancestors, with the classes a script may add to it; a created
    parent, with the classes all its class sets share as fixed and the rest as
    written; or anything at all where the analysis lost the place. A compound
    that no ancestor can fit means the rule never matches that element. A
    compound fitted only through a written class or id, an unknown place, or a
    sibling compound means the answer comes at run time. When every compound is
    fitted by what is already there, the probe saw it and the page stays closed.

A class name computed on a known element doesn't open the page by itself any more
(the "is not followed" diagnostic is gone): the runtime cascade runs on the
elements a computed lookup finds, and the converter emits an item for each
element with an id when the script looks ids up by computed names. An open
class write opens the page in the analysis; a target it can't name came from a
query it can't follow, which stays live, so an item a class swaps is found again
by any lookup (`rule_computed_mixed`).

`el.style.position` also opens the page: it is no longer written in place.

At run time:

- **A script-filled container is laid out by the runtime.** Stretch and alignment
  between items whose sizes come from a pass can't be said to a QML layout, and a
  static block's column is measured by no one once a box holds it, so `_gFillsLine`
  adopts the container, unless the page wrote text in it that a box would lose. A
  page's own items in it are adopted with it, since they share lines and tracks with
  the created ones. Clearing such a box (`textContent = ""`) detaches every element
  in it, as clearing a layout did.
- **Out-of-flow is a state of any element.** `_leaveFlow` keeps a slot where the
  element was. A template's block is `"containing"` and is resolved when it connects,
  because the template is written before its parent is known. A control keeps its item
  (`_G_MOVES`), with the slot held in its meta, as a control's type has no property for
  it. `_gSyncFlowForm` and `_gSyncControlFlow` keep the slot in DOM order.
- **An id or attribute that a selector names is noticed on a static element**:
  `_gAttrsSig` is part of what `_gUncovered` compares. Attributes no author selector
  names don't count, so `hidden` leaves an element in its converted form.
- **`innerHTML` with inline elements on a created box makes elements**, not one styled
  `Text`, so they answer to selectors and `getElementById`.
- **An element is its record, not its item.** A swap of forms (`_gReplace`) moves
  the element's record to the new item, and a wrapper resolves through the record
  (`_unwrap`), so a page's variable that holds the old item still reaches the
  element at any later time. `_gReplaced` (old item to new) is only for a raw item
  captured during one read: it is cleared when the outermost read starts, and when
  a settle starts outside a read, so a page that swaps an element's form over and
  over keeps nothing of the old items. It is a `Map`: Qt's `WeakMap` crashes in a
  lookup once a key has been collected. The record table (`_itemMeta`) drops the
  records of destroyed items when it has doubled since its last sweep (Qt destroys
  an item at the next turn of the event loop, after which its `parent` reads as
  undefined). Earlier the map was kept forever, which grew by one entry per swap.
- **Floats a script makes are laid out as the page's own** (layout.md §5.5).
  `float` and `clear` are in the runtime's rule table. As CSS 2 §9.7 says, an
  absolute box or a flex or grid item doesn't float, and a float is blockified.
  `_gLayoutFlow` places a same-side run of floats along a line from its side,
  wrapping when one doesn't fit, and cuts the lines of each following block's own
  text beside them (`_gBeside`, which moves Qt's lines in `lineLaidOut`). A
  container with a float in it is adopted even when the page wrote elements in it,
  since what flows beside a float is placed by a pass, not by a column. Margins,
  clearance and line cutting follow the rules in layout.md §5.5, as the static
  layout does. A block in the same formatting context gets the floats in its own
  coordinates (`gAround`), and line boxes get them as `exclusion`. A float in a
  block that is no formatting context of its own (a clearfix row a script fills)
  is not held by it: the block's pass hands its floats up (`floats` on its
  placement, `_gJoinFloats`) and its height leaves them out, and a later float on
  the same side lines up beside them (`prior` in `_gPlaceFloats`). That is done
  only where the parent is a box the runtime lays out; under a column a page wrote
  (the body, say) the block still holds its floats. A created element a
  template paints is adopted when it holds floats, as a written one is. A text the
  script filled before the element was in the page takes the cascade's font into
  its inline model when the element is painted (`_vInheritFont`).
- **Generated text is the element's for a read.** A class change that gives an
  element `::before`/`::after` content is applied at the next turn
  (`_pseudoTouch`), but a read (`getBoundingClientRect`, `offsetWidth`,
  `getComputedStyle`) applies it first, as a browser lays out before it answers.
  A table's column widths read right after a sort are measured with the arrow the
  sorted header gains.
- **Generated text is drawn as the rules say and counts as they count.** The
  converter emits `eldurPseudoRules` (text parts, and the paint declarations of
  each `::before`/`::after` rule; a rule that only paints is `drawOnly`) when a
  class write can change generated text, or when the page counts and changes
  its DOM; `eldurCounterRules` then carries `counter-reset`, `counter-increment`
  and `counter-set`. `_pseudoFlush` lets the rules compete per side by
  importance, specificity and order, and works counters out over the whole tree
  (`_counterValues`: a reset nests a counter, an increment or a set goes to the
  innermost one) before each flush; an insert or a removal asks for one
  (`_counterTouch`). A list item increments `list-item`. An `<ol>`, `<ul>` or
  `<menu>` resets it to `start` − 1, or 0, unless its own `counter-reset`
  names `list-item`; another counter's reset does not stop it (measured). A
  rule on `::before`/`::after` carries its side (`after`). Where the element
  has that generated box, its changes apply inside the element's scope, after
  its own and before or after its children. The static numbering in
  `boxes/counters.rs` applies the same list reset. Text drawn other than the element's own font or colour is a
  `gen` node of the element's inline model (in its own `<span>` font when its
  size or family differs); a plain `Text` takes a model only then. An element
  whose content is on lines holds it as a run at that side of the lines whose
  model has only the `gen` node, so a script sees no node: the converter's run
  is reused, and a box the runtime lays out gets one (`_pseudoNewRun`) that
  `_gFlowGroups` keeps first or last on its lines. The converter's own
  generated text is adopted as the rules' until they say otherwise.
  A pseudo-element laid out as a box (block-level, atomic inline, floated or
  absolutely positioned) is a box of the converted tree like an element of
  that style; an absolutely positioned one leaves its line's baseline to the
  rest (CSS 2.2 §10.8.1). A class change or an insertion that gives an element
  such a box makes it (`_pseudoBoxes`); one that takes it away hides it. The
  converter marks its own boxes `eldurPseudo`, on the item a line holds when
  the box is atomic inline (its wrapper), since that is what is shown and
  hidden and kept at the element's end. The runtime's boxes carry their owner
  in their record (`pseudoOf`), which still holds when being out of flow moves
  one under its gate, and a swap to or from out-of-flow form carries the box
  over. A script's reads of the element's children and text skip them. Not
  followed: `content`
  other than strings, `attr()` and counters, and `counter(list-item)` in a `reversed` list or after `<li value>` (in Chromium the
  `::before` widths of a three-item reversed list whose last item has
  `value=7` fit 0, −1, −1. That is not yet understood, so it is not copied).
- **Inline boxes have one path, written or created.** There is no static/run-time
  split for an inline element on a page with scripts: the converter's runs are
  where the cascade starts, and a class change, an insertion or a removal runs
  the whole cascade over the line's inline model and cuts its runs again
  (`_vRender`). What a rule sets on an inline element (padding, border, margin,
  radius, background, generated text, pseudo boxes) is drawn the same way
  whether the element was written or made:
  - An element the cascade makes inline is a node of the model whatever its tag
    (an `li` in a menu), and one appended to a line already cut into runs joins
    the last run's model (`_vLinesTail`) instead of standing beside it as an
    item. Run `Text`s the runtime makes carry `eldurInline`, so they are DOM runs
    and structural rules (`:last-child::after`, `a + a::before`, `:nth-child`)
    match again after each cut.
  - A run's box (margins, borders, padding of the elements open at its ends) is
    the room it takes on its line and the `EldurInlineBox`es that paint it
    (`setRunBox`), in place of the converter's `eldurMarginLeft`/`Right`. The
    room of generated text is read from its style: the converter's run margin is
    the sum with the element's own padding. `content: ""` with margin, padding
    or a border is a box that takes room.
  - Generated text with a box of its own is a run of its own and takes its own
    font there, so a 10px badge in a 14px line stays styled text. Whether a run
    needs rich text is derived again at each cut (the converter's `html: true`
    describes the content it cut, not what a run holds now): `Text.RichText`
    lays out no lines, so `EldurLines` would have nothing to place and the line
    would collapse.
  - The converter's `::after` run of an element split across runs (marked
    `under`) records the element it closes (`afterOf`), so a removal before it
    closes the right element's box.
  - A control's pseudo boxes are looked for in its label too, where what a
    script inserts goes, so a made button has its `::before` icon once. A button
    template holding lines draws its label from the button's `text`.
  - Lines nested in a box on a line (a button's label) tell the line holding them
    when their baseline moves, since the box's size need not change with it.
- **A table a script makes is a live table.** On an open page it takes the
  form `emit/table.rs` gives a written table a script fills, with no rows
  (`_TABLE_MADE_SOURCE`): captions in a column above a box whose grid the
  runtime lays out with the static tables' solver. `_tableStyleMade` writes
  what the cascade says each settle pass: `width`, `min-width`, `max-width`
  (percentages of the box it is in), `border-spacing`, `table-layout: fixed`
  (first-row cell widths), the background and the border. Collapsed, the
  table's border wins where it is wider than its cells' (CSS 2.1 §17.6.2.1):
  it is painted over them, and each outer cell gets half the difference as
  padding (`_tableOuterPads`). A template for a table is used only on a
  closed page, where nothing else would style it, since its spacing is fixed
  in it; its probe gets no placeholder text, which would make it a static
  grid. Row heights and column widths are whole pixels, as the static
  tables' are, so borders stay sharp.
- **An inline model first built after a write has no baseline.** It can't tell which
  matches changed, so it runs the cascade rather than trusting the converter's values.


Two rules every box the runtime lays out now keeps, because a script-filled grid
or flex container brings its items onto this path. Paint the runtime makes as a
child (`_gReplaceShape`) is no child of the element: a script does not see it and
a box's layout does not put it on a line. A grid item, a flex item and an
out-of-flow box each start a block formatting context, so their children's margins
stay inside them.
