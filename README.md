# Eldur

Eldur (Icelandic for "fire"; a sibling of Ljós) was formerly html-2-qml.

Convert a static HTML5 + CSS + JavaScript page into a runnable, readable
Qt Quick (QML, Qt 6) application.

## Getting started

Install [mise](https://mise.jdx.dev/getting-started.html), then:

```sh
mise run setup   # Rust toolchain, crates; lists optional tools it didn't find
mise run dev     # convert a sample page and run it in ljos or qml6
```

`mise run dev path/to/page.html -o out/` converts your own page;
`mise run dev --help` lists the options. `mise tasks` lists every task.

## Install

```sh
mise run install                  # eldur onto ~/.cargo/bin
mise run install --root ~/.local  # or under another prefix
eldur convert page.html -o out/
qml6 out/Main.qml                 # or: ljos run out/Main.qml
```

See `docs/ARCHITECTURE.md`. Licence: MIT.
