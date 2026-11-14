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
6. **Generated QML must be valid.** Every snapshot in `tests/corpus/` must
   pass `qmllint` and load in `qml6` off-screen without warnings
   (`mise run qml:check`).

## Commands

| Task | Command |
|---|---|
| First run: toolchain, crates, report of missing optional tools | `mise run setup` |
| Convert a page and run it in ljos or qml6 | `mise run dev [page.html] [-o dir] [--runtime auto\|ljos\|qml6\|none]` |
| Format check, clippy (warnings are errors) | `mise run check` |
| Rust tests | `mise run test [-- cargo test args]` |
| Build the workspace | `mise run build [--release]` |
| Install the `eldur` CLI from the checkout | `mise run install [--root dir]` |
| Validate generated QML (qmllint + qml6 off-screen) | `mise run qml:check [dir]` |
| Convert a page | `mise run convert page.html -o out/` |
| Convert every corpus case | `mise run corpus [out]` |
| Visual parity harness (Chromium vs qml6) | `mise run visual [-- run.py args]`, `mise run visual test` |

Every task takes `--help`.

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
