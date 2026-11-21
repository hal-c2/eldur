//! Work counters for tests that guard how much the cascade does, which unlike
//! time is the same on every machine.

use std::cell::Cell;

thread_local! {
    static STYLE_COMPUTES: Cell<usize> = const { Cell::new(0) };
    static VALUE_PARSES: Cell<usize> = const { Cell::new(0) };
    static BASE_PASSES: Cell<usize> = const { Cell::new(0) };
    static ANCESTOR_STEPS: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn count_style_compute() {
    STYLE_COMPUTES.with(|c| c.set(c.get() + 1));
}

pub(crate) fn count_value_parse() {
    VALUE_PARSES.with(|c| c.set(c.get() + 1));
}

pub(crate) fn count_base_pass() {
    BASE_PASSES.with(|c| c.set(c.get() + 1));
}

pub(crate) fn count_ancestor_step() {
    ANCESTOR_STEPS.with(|c| c.set(c.get() + 1));
}

/// How many ancestors the custom property pass grouping walked while `f` ran
/// on this thread.
pub(crate) fn ancestor_steps<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let before = ANCESTOR_STEPS.with(Cell::get);
    let out = f();
    (out, ANCESTOR_STEPS.with(Cell::get) - before)
}

/// How many extra cascade passes over the document `f` ran on this thread.
pub(crate) fn base_passes<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let before = BASE_PASSES.with(Cell::get);
    let out = f();
    (out, BASE_PASSES.with(Cell::get) - before)
}

/// What `f` did on this thread: (elements cascaded, value texts parsed).
pub(crate) fn measure<T>(f: impl FnOnce() -> T) -> (T, usize, usize) {
    let computes = STYLE_COMPUTES.with(Cell::get);
    let parses = VALUE_PARSES.with(Cell::get);
    let out = f();
    (
        out,
        STYLE_COMPUTES.with(Cell::get) - computes,
        VALUE_PARSES.with(Cell::get) - parses,
    )
}
