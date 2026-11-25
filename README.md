# Eldur

Eldur (Icelandic for "fire"; a sibling of Ljós) was formerly html-2-qml.

Convert a static HTML5 + CSS + JavaScript page into a runnable, readable
Qt Quick (QML, Qt 6) application.

## Getting started

Install [mise](https://mise.jdx.dev/getting-started.html) and
[rustup](https://rustup.rs) (mise installs the pinned Rust through it), then:

```sh
mise run setup   # Rust toolchain, crates; lists optional tools it didn't find
mise run dev     # convert a sample page and run it in ljos or qml6
```

`mise run dev` needs `ljos` or Qt 6's `qml6` to run the page, and fails saying
so when neither is there; `mise run dev --runtime none` only converts.
`mise run dev path/to/page.html -o out/` converts your own page;
`mise run dev --help` lists the options. `mise tasks` lists every task; the
table in `AGENTS.md` says what each needs.

## Install

```sh
mise run install                  # eldur onto ~/.cargo/bin
mise run install --root ~/.local  # or under another prefix
eldur convert page.html -o out/
qml6 out/Main.qml                 # or: ljos run out/Main.qml
eldur convert https://example.com/blog/post.html   # into post_qml/
```

A URL's page is fetched over HTTP(S) with the style sheets, scripts and
images it references (resolved against the page, `<base href>` and each
sheet); one that fails is a warning naming its URL.

`eldur convert --help` lists the options.

## Output for programs

`eldur convert` and `eldur watch` take `--message-format <human|json>`.
`human`, the default, prints each diagnostic as a
`severity[stage]: source:line: message` line on stderr and a
`converted`/`cached` line on stdout. A program should not split that line:
a source that is a URL holds `:` itself. With `json`, stdout carries one JSON
object per line, `type` first, and stderr carries nothing but `watch`'s own
file-watching warnings:

```json
{"type":"diagnostic","severity":"warning","stage":"css","source":"site.css","line":3,"message":"approximated box-shadow"}
{"type":"done","status":"converted","output":"out","files":["Main.qml","assets/logo.png"],"main":"Main.qml"}
```

- `diagnostic`: `severity` is `info`, `warning` or `error`; `stage` is
  `html`, `css`, `js` or `qml`; `line` is left out when it is not known. A
  run served from the build cache reports the same diagnostics, field by
  field.
- `done` ends a run: `status` is `converted` or `cached`, `output` is the
  output directory as given (or defaulted), `files` are every file written
  there, relative to it with `/` between parts, and `main` is the file to
  run.
- `error`: `{"type":"error","message":"…"}` when the conversion failed; the
  exit status is then non-zero. `watch` reports every run this way; when a
  run after the first fails, it goes on watching.

`--quiet` leaves the `done` line in. Errors in the command line itself are
clap's usage text on stderr, exit status 2.

See `docs/ARCHITECTURE.md`. Licence: MIT.
