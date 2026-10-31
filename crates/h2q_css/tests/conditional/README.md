Chromium oracle for `../conditional.rs`.

`gen.mjs` writes `cases.json`: one page per test and condition combination,
with the combination's classes written into the markup (`:hover` is written as
the class `.hov`, which has the same specificity). `chromium.jsonl` is the
output of

    node crates/h2q_css/tests/conditional/gen.mjs crates/h2q_css/tests/conditional/cases.json
    node tools/visual/computed.mjs crates/h2q_css/tests/conditional/cases.json \
        > crates/h2q_css/tests/conditional/chromium.jsonl

Regenerate it; never edit it by hand.
