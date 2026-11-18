# Working on Eldur

Eldur converts a static HTML5 + CSS + JavaScript page into a runnable
Qt Quick (QML, Qt 6.11) application. Read `docs/ARCHITECTURE.md` first; the
mapping specifications are in `docs/spec/`.

## Rules

1. **Contracts.** Each crate's `src/lib.rs` public API is the contract other
   crates are written against. Add to it; never change or remove an item
   without the lead's agreement.
2. **Licences.** Only permissively licensed dependencies (MIT, Apache-2.0,
   BSD, ISC, Zlib, CC0). No MPL (servo `selectors`, `cssparser`, `stylo`,
   `lightningcss`), no GPL/LGPL. Never copy code from Servo, Blink, WebKit,
   Gecko or Qt; specifications (WHATWG, W3C/CSSWG, doc.qt.io) and Qt's
   installed `.qmltypes` files are the references.
3. **Don't reinvent.** Use html5ever, swc_css_parser, oxc, csscolorparser,
   blake3 for what they do. Write our own code only for what no
   permissively licensed crate does (the cascade, the QML mapping).
4. **Behaviour first.** A change starts as a test: a unit test, or a corpus
   case in `tests/corpus/` with its expected QML snapshot.
5. **Honest results.** Never weaken, `#[ignore]`, or delete a test to make
   it pass, never hard-code an expected output in the implementation, never
   leave `unimplemented!()`/`todo!()` in code you report as done. Report
   what does not work.
6. **Generated QML must be valid.** The QML of every `tests/corpus/` case
   must pass `qmllint` and load in `qml6` off-screen without warnings
   (`mise run corpus`, then `mise run qml:check`).

## Commands

| Task | Command |
|---|---|
| First run: toolchain (needs rustup), crates, report of missing optional tools; `--check` installs nothing and fails on a Rust that is not the pinned one; a Qt other than the pinned 6.11 is a note | `mise run setup [--offline] [--check]` |
| Convert a page and run it in ljos or qml6 (fails if neither is found) | `mise run dev [page.html] [-o dir] [--runtime auto\|ljos\|qml6\|none] [--viewport WxH]` |
| Format every crate in place | `mise run fmt` |
| Format check, clippy (warnings are errors) | `mise run check` |
| Rust tests; the QML end-to-end tests skip, saying so, without Qt 6's qmltestrunner | `mise run test [-- cargo test args]` |
| Build the workspace and print the `eldur` binary's path | `mise run build [--release]` |
| API docs of every crate | `mise run docs [--open]` |
| Install the `eldur` CLI from the checkout | `mise run install [--root dir]` |
| Convert a page | `mise run convert page.html -o out/` |
| Convert every corpus case (default `<cargo target dir>/corpus`) | `mise run corpus [out]` |
| Validate QML (qmllint + qml6 off-screen; default: what `mise run corpus` wrote) | `mise run qml:check [dir]` |
| Visual parity harness (Chromium vs qml6; needs Playwright's Chromium, and sway unless `--backend software`) | `mise run visual [-- run.py args]` |
| The visual harness's own tests | `mise run visual:test` |
| Run every task quickly, as CI does | `mise run tasks:smoke [--list] [--only task]` |
| The tasks' own tests, with stand-in tools (setup's Qt pin) | `mise run tasks:test` |

Every task takes `--help`. Paths you pass are relative to the directory you
run mise in. Outputs go under cargo's target directory (`CARGO_TARGET_DIR` is
respected). CI (`.github/workflows/ci.yml`) runs `setup`, `check`, `test` and
`tasks:smoke`, without Qt, so the QML end-to-end tests skip there.

## Conventions

- Rust 1.98.1, edition 2024, resolver 3. Versions are pinned in
  `[workspace.dependencies]`; crates use `dep.workspace = true`.
- `thiserror` in libraries, `anyhow` only in the binary.
- Snapshot tests use `insta`. Review new snapshots by reading them; accept
  with `cargo insta accept` only when they are right.
- Comments explain why, not what.
- Build cache: `.cargo/config.toml` (machine-local, not committed) routes
  rustc through the shared sccache, as in `~/dev/ljos-ui`. Parallel
  checkouts use their own `CARGO_TARGET_DIR`.
