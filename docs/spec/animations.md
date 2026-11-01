# What moves

How scrolling, transitions and animations convert. Expected values in the tests come
from Chromium (`crates/eldur_convert/tests/apps/`, `gen.mjs`), never from reading this.

## Which engine

| Page | Transitions | `@keyframes` | Clock |
|---|---|---|---|
| Closed, no script | `Behavior`, with a `PauseAnimation` for a delay | `SequentialAnimation`, worked out in `emit/keyframes.rs` | Qt's |
| Closed, script without motion | as above | as above | Qt's |
| Open (`eldurOpenElements`) | the runtime's engine | the runtime's engine | one `FrameAnimation` |

A page without scripts never gains the runtime for motion. Which scripted pages open
is the converter's analysis of what their scripts do; motion events are among it.

`getComputedStyle` gives the value on the way in both: on open pages from the
engine's sample, on closed ones from the item the `Behavior` moves (opacity, width,
height, background-color only).

## Keyframes without a runtime

- Paused: the value at `t = -delay`, written as plain values, no animation at all.
- Running: `PropertyAction` for a backwards fill, `PauseAnimation`, then one track per
  property with `loops`, then a `PropertyAction` restoring the value unless `forwards`.
  `alternate` is unrolled; `steps()` is a near-upright polyline in `easing.bezierCurve`.
- Sizes and margins that move layout go through `eldurMotionWidth/Height` and
  `eldurMotionMargin*`, bound after flow has set them; `left` is an added `Translate`.

Not converted, each with an info diagnostic: `right`, `top`, `bottom`, padding and other
layout properties; a running negative delay starts from the beginning; a fractional
iteration count is floored; transforms other than translate, scale and rotate, or whose
keyframes differ in shape.

Also not done, without a diagnostic: a box hidden by a class and shown again on a closed
page does not restart its animation; a negative transition delay is dropped; two running
animations on one property both act.

## The run-time engine

`_aScan` starts what the cascade asks for, `_aFrame` samples, `_aRead` brings values up
to date before a script reads. Gaps: `filter` transitions jump; transforms whose function
lists differ flip at 50%; inline spans and controls do not animate; `commitStyles` and
`persist` do nothing; `linear()` works here only.

## Scrolling

- **Where a Flickable appears.** Only on a box that can overflow. A created box with
  overflow gets an empty `EldurScroller` under its children, once its content does
  overflow, and a `Translate` moves the children: they stay the box's in the DOM.
- **Page root.** A plain `Flickable` in both `--root window` and `--root item`, with
  fixed boxes beside it. The viewport takes `overflow` and `scroll-behavior` from the
  root element. No horizontal page scrolling.
- **Scrollbars** overlay, as Playwright's Chromium's do, and show while active.
- **Input.** A wheel notch is 60px, at once, and chains at the bound. Arrow keys 40px;
  a page is `max(0.875h, h - 40)`; Home, End, Space. A mouse drag scrolls and flicks,
  which Chromium does only for touch.
- **`scroll-behavior: smooth`**, declared or asked for by a script: 300ms `InOutQuad`.
  A fragment link on a scriptless page jumps.
- **`overscroll-behavior`, `scroll-snap-*`** warn as unknown properties.

## Timers

One queue ordered by due time, then by when they were made: a QML `Timer` fires on the
animation clock's tick, and those due on a tick in creation order. One callback a turn
of the event loop, so a promise's reactions run between them. Reactions that queue
further reactions can still lose that race: the engine's queue cannot be drained or seen.

## The visual harness

Motion is off on both sides, so that a frame is stable: `render_html.mjs` injects
`*{animation:none!important;transition:none!important}`, and the QML side converts with
`--no-animations` (no Behaviors, no keyframes, and open pages declare `eldurNoMotion`).
Mid-flight values are checked by e2e pages against Chromium, not by pixels.
