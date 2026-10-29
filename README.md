# html-2-qml

Convert a static HTML5 + CSS + JavaScript page into a runnable, readable
Qt Quick (QML, Qt 6) application.

```sh
cargo run -p h2q_cli -- convert page.html -o out/
qml6 out/Main.qml
```

See `docs/ARCHITECTURE.md`. Licence: MIT.
