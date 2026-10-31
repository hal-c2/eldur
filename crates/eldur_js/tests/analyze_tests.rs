use eldur_js::{
    Analysis, ClassWrite, CreatedElement, CreatedId, DomQuery, ElementKey, ElementRef, ScriptInput,
    ScriptKind, Severity, analyze,
};

fn classic(name: &str, source: &str) -> ScriptInput {
    ScriptInput {
        source: source.to_string(),
        name: name.to_string(),
        kind: ScriptKind::Classic,
        line_offset: 1,
    }
}

#[test]
fn collects_and_deduplicates_all_literal_dom_queries_in_order() {
    let script = classic(
        "inline script #1",
        r#"
        const a = document.getElementById("hero");
        const b = window.document.querySelector(".card > h2");
        const c = document.querySelectorAll("li.item");
        const d = document.getElementsByClassName("pill active");
        const e = document.getElementsByTagName("button");
        const dup = document.getElementById(`hero`);
        const dynamicId = getName();
        const ignored = document.getElementById(dynamicId);
        "#,
    );

    let res = analyze(&[script]);
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    assert_eq!(
        res.queries,
        vec![
            DomQuery::ById("hero".to_string()),
            DomQuery::Selector(".card > h2".to_string()),
            DomQuery::SelectorAll("li.item".to_string()),
            DomQuery::SelectorAll(".pill.active".to_string()),
            DomQuery::SelectorAll("button".to_string()),
        ]
    );
    assert!(!res.mutates_dom);
}

#[test]
fn tracks_listeners_and_property_writes_across_variables_functions_and_loops() {
    let s1 = classic(
        "s1.js",
        r#"
        const saveBtn = document.getElementById("save");
        const items = document.querySelectorAll(".todo");
        saveBtn.addEventListener("click", () => {});
        saveBtn.disabled = true;
        "#,
    );
    let s2 = classic(
        "s2.js",
        r##"
        function render() {
            const alias = saveBtn;
            alias.textContent = "Saved";
            alias.style.color = "green";
            alias.style["backgroundColor"] = "#eee";
            alias.dataset.state = "done";
            alias.classList.toggle("is-saved");
            alias.setAttribute("aria-busy", "false");
            alias.setAttribute("class", "btn primary");
            alias.setAttribute("data-user-id", "42");

            const input = document.querySelector("#email");
            input.oninput = (e) => {};
            input.value = "a@b.c";
        }

        items.forEach((el) => {
            el.addEventListener("mouseenter", () => {});
            el.hidden = false;
        });

        for (const row of document.querySelectorAll(".row")) {
            row.textContent = "item";
        }
        "##,
    );

    let Analysis {
        queries,
        listened,
        mutates_dom,
        writes,
        diagnostics,
        class_writes,
        created,
        runtime_dom,
        mutated_parents,
        mutated_siblings,
        live_queries,
        text_writes,
        reads_attributes,
        dispatch_by_path: _,
        listens_unresolved: _,
    } = analyze(&[s1, s2]);
    // Nothing here can add, remove or rename an element, but
    // `setAttribute("class", …)` can take any class off one.
    assert_eq!(
        live_queries,
        vec![
            DomQuery::SelectorAll(".todo".to_string()),
            DomQuery::SelectorAll(".row".to_string()),
        ],
    );
    assert_eq!(
        text_writes,
        vec![
            ElementRef::Query(DomQuery::ById("save".to_string())),
            ElementRef::Query(DomQuery::SelectorAll(".row".to_string())),
        ]
    );
    assert!(!reads_attributes);
    // `textContent` replaces the children of the elements it is written to.
    assert_eq!(
        mutated_parents,
        vec![
            ElementRef::Query(DomQuery::ById("save".to_string())),
            ElementRef::Query(DomQuery::SelectorAll(".row".to_string())),
        ]
    );
    assert!(mutated_siblings.is_empty(), "{mutated_siblings:?}");

    assert!(created.is_empty(), "{created:?}");
    // `toggle` without a force argument depends on the initial classes.
    assert!(runtime_dom);
    let saved = ElementRef::Query(DomQuery::ById("save".to_string()));
    assert_eq!(
        class_writes
            .iter()
            .map(|w| (&w.target, w.names.clone(), w.open, w.assigns))
            .collect::<Vec<_>>(),
        vec![
            (&saved, names(&["is-saved"]), false, false),
            (&saved, names(&["btn", "primary"]), false, true),
        ]
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(!mutates_dom);
    assert_eq!(
        queries,
        vec![
            DomQuery::ById("save".to_string()),
            DomQuery::SelectorAll(".todo".to_string()),
            DomQuery::Selector("#email".to_string()),
            DomQuery::SelectorAll(".row".to_string()),
        ]
    );
    assert_eq!(
        listened,
        vec![
            (DomQuery::ById("save".to_string()), "click".to_string()),
            (
                DomQuery::Selector("#email".to_string()),
                "input".to_string()
            ),
            (
                DomQuery::SelectorAll(".todo".to_string()),
                "mouseenter".to_string()
            ),
        ]
    );
    assert_eq!(
        writes,
        vec![
            (DomQuery::ById("save".to_string()), "disabled".to_string()),
            (
                DomQuery::ById("save".to_string()),
                "textContent".to_string()
            ),
            (
                DomQuery::ById("save".to_string()),
                "style.color".to_string()
            ),
            (
                DomQuery::ById("save".to_string()),
                "style.backgroundColor".to_string()
            ),
            (
                DomQuery::ById("save".to_string()),
                "dataset.state".to_string()
            ),
            (DomQuery::ById("save".to_string()), "classList".to_string()),
            (DomQuery::ById("save".to_string()), "aria-busy".to_string()),
            (
                DomQuery::ById("save".to_string()),
                "dataset.userId".to_string()
            ),
            (
                DomQuery::Selector("#email".to_string()),
                "value".to_string()
            ),
            (
                DomQuery::SelectorAll(".todo".to_string()),
                "hidden".to_string()
            ),
            (
                DomQuery::SelectorAll(".row".to_string()),
                "textContent".to_string()
            ),
        ]
    );
}

#[test]
fn ignores_shadowed_document_and_reassigned_variables() {
    let script = classic(
        "shadow.js",
        r#"
        function helper(document, window) {
            const fake = document.getElementById("not-real");
            window.document.querySelector(".also-fake");
            return fake;
        }

        let target = document.getElementById("first");
        target = getOtherNode();
        target.textContent = "should not bind to #first";
        "#,
    );

    let res = analyze(&[script]);
    assert_eq!(res.queries, vec![DomQuery::ById("first".to_string())]);
    assert!(res.writes.is_empty(), "unexpected writes: {:?}", res.writes);
    assert!(res.listened.is_empty());
}

#[test]
fn detects_dom_tree_mutations_without_false_positives_on_classlist() {
    let non_mutating = classic(
        "classes.js",
        r#"
        const box = document.getElementById("box");
        box.classList.remove("hidden");
        box.classList.add("shown");
        localStorage.removeItem("token");
        "#,
    );
    assert!(!analyze(&[non_mutating]).mutates_dom);

    for snippet in [
        r#"document.createElement("div");"#,
        r#"document.getElementById("list").appendChild(node);"#,
        r#"document.getElementById("list").innerHTML = "<li>x</li>";"#,
        r#"document.getElementById("item").remove();"#,
        r#"document.getElementById("list").insertAdjacentHTML("beforeend", "<p>hi</p>");"#,
    ] {
        let res = analyze(&[classic("mut.js", snippet)]);
        assert!(res.mutates_dom, "expected mutates_dom for: {snippet}");
    }
}

#[test]
fn reports_syntax_errors_with_line_offset_and_continues_other_scripts() {
    let broken = ScriptInput {
        source: "const ok = 1;\nconst bad = (\n".to_string(),
        name: "inline script #1".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 20,
    };
    let valid_handler = ScriptInput {
        source: "doWork(this); return false;".to_string(),
        name: "onclick of <a id=link>".to_string(),
        kind: ScriptKind::HandlerAttribute {
            element: ElementKey(1),
            event: "click".to_string(),
        },
        line_offset: 45,
    };
    let valid_classic = classic(
        "app.js",
        r#"document.getElementById("status").textContent = "ok";"#,
    );

    let res = analyze(&[broken, valid_handler, valid_classic]);
    assert_eq!(res.queries, vec![DomQuery::ById("status".to_string())]);
    assert_eq!(res.diagnostics.len(), 1);
    assert_eq!(res.diagnostics[0].severity, Severity::Error);
    assert_eq!(res.diagnostics[0].source, "inline script #1");
    assert!(
        res.diagnostics[0].line.unwrap_or(0) >= 21,
        "expected line >= 21, got {:?}",
        res.diagnostics[0].line
    );
}

#[test]
fn reports_cross_script_lexical_redeclaration_as_error() {
    let s1 = classic("s1.js", "let counter = 0;");
    let s2 = ScriptInput {
        source: "const a = 1;\nlet counter = 2;".to_string(),
        name: "s2.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 10,
    };

    let res = analyze(&[s1, s2]);
    assert_eq!(res.diagnostics.len(), 1);
    assert_eq!(res.diagnostics[0].severity, Severity::Error);
    assert_eq!(res.diagnostics[0].source, "s2.js");
    assert_eq!(res.diagnostics[0].line, Some(11));
    assert!(res.diagnostics[0].message.contains("counter"));
}

#[test]
fn reports_cross_script_block_nested_var_conflict_with_lexical_binding() {
    let s1 = classic("s1.js", "let x = 1;");
    let s2 = ScriptInput {
        source: "if (true) {\n    var x = 2;\n}".to_string(),
        name: "s2.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 10,
    };

    let res = analyze(&[s1, s2]);
    assert_eq!(res.diagnostics.len(), 1);
    assert_eq!(res.diagnostics[0].severity, Severity::Error);
    assert_eq!(res.diagnostics[0].source, "s2.js");
    assert_eq!(res.diagnostics[0].line, Some(11));
    assert!(res.diagnostics[0].message.contains("x"));

    let s3 = classic("s3.js", "for (var y = 0; y < 1; ++y) {}");
    let s4 = ScriptInput {
        source: "let y = 3;".to_string(),
        name: "s4.js".to_string(),
        kind: ScriptKind::Classic,
        line_offset: 20,
    };
    let res2 = analyze(&[s3, s4]);
    assert_eq!(res2.diagnostics.len(), 1);
    assert_eq!(res2.diagnostics[0].severity, Severity::Error);
    assert_eq!(res2.diagnostics[0].source, "s4.js");
    assert_eq!(res2.diagnostics[0].line, Some(20));
    assert!(res2.diagnostics[0].message.contains("y"));
}

#[test]
fn reports_unsupported_js_and_browser_features() {
    let script = classic(
        "unsupported.js",
        r#"
        const big = 42n;
        const fmt = new Intl.NumberFormat("en-US");
        const re1 = /foo.bar/s;
        const re2 = /(?<=a)b/;
        const re3 = /(?<id>\d+)/;
        const url = location.href;
        history.pushState({}, "", "/next");
        const ua = navigator.userAgent;
        document.write("nope");
        confirm("sure?");
        prompt("name?");
        eval("1 + 1");
        class Secret {
            #hidden = 1;
            static { console.log("init"); }
        }
        "#,
    );
    let missing_mod = ScriptInput {
        source: r#"import { chart } from "https://cdn.example.com/chart.js";"#.to_string(),
        name: "mod.js".to_string(),
        kind: ScriptKind::Module,
        line_offset: 1,
    };

    let res = analyze(&[script, missing_mod]);
    let messages: Vec<&str> = res.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity == Severity::Unsupported),
        "expected all Unsupported diagnostics, got: {:?}",
        res.diagnostics
    );

    for expected in [
        "BigInt",
        "Intl",
        "RegExp flags",
        "lookbehind",
        "named capture",
        "location",
        "history",
        "navigator",
        "document.write",
        "confirm",
        "prompt",
        "eval",
        "private class fields",
        "static initialization blocks",
        "cannot inline external or missing module",
    ] {
        assert!(
            messages.iter().any(|m| m.contains(expected)),
            "missing diagnostic containing '{expected}' in {messages:#?}"
        );
    }
}

#[test]
fn reports_window_qualified_unsupported_globals_and_nested_arguments() {
    let bare = classic(
        "bare.js",
        r#"
        BigInt(10);
        eval("1 + 1");
        confirm("sure?");
        prompt("name?");
        Function("return 1")();
        RegExp("(?<=a)b");
        new MutationObserver(() => {});
        new IntersectionObserver(() => {});
        new ResizeObserver(() => {});
        new Worker("worker.js");
        new Function("return 2");
        new Intl.NumberFormat("en-US");
        new RegExp("foo", "s");
        const mBig = BigInt;
        const mEval = eval;
        const mConfirm = confirm;
        const mPrompt = prompt;
        const mFunc = Function;
        const mMo = MutationObserver;
        const mIo = IntersectionObserver;
        const mRo = ResizeObserver;
        const mWorker = Worker;
        const mIntl = Intl;
        const mLoc = location.href;
        const mHist = history.state;
        const mNav = navigator.userAgent;
        "#,
    );

    let win = classic(
        "win.js",
        r#"
        window.BigInt(10);
        window.eval("1 + 1");
        window.confirm("sure?");
        window.prompt("name?");
        window.Function("return 1")();
        window.RegExp("(?<=a)b");
        new window.MutationObserver(() => {});
        new window.IntersectionObserver(() => {});
        new window.ResizeObserver(() => {});
        new window.Worker("worker.js");
        new window.Function("return 2");
        new window.Intl.NumberFormat("en-US");
        new window.RegExp("foo", "s");
        const mBig = window.BigInt;
        const mEval = window.eval;
        const mConfirm = window.confirm;
        const mPrompt = window.prompt;
        const mFunc = window.Function;
        const mMo = window.MutationObserver;
        const mIo = window.IntersectionObserver;
        const mRo = window.ResizeObserver;
        const mWorker = window.Worker;
        const mIntl = window.Intl;
        const mLoc = window.location.href;
        const mHist = window.history.state;
        const mNav = window.navigator.userAgent;
        "#,
    );

    let bare_res = analyze(&[bare]);
    let win_res = analyze(&[win]);
    assert_eq!(
        bare_res.diagnostics.len(),
        26,
        "unexpected bare diagnostics: {:#?}",
        bare_res.diagnostics
    );
    assert_eq!(
        win_res.diagnostics.len(),
        bare_res.diagnostics.len(),
        "window-qualified diagnostics should match bare count without duplicates: {:#?}",
        win_res.diagnostics
    );
    assert!(
        win_res
            .diagnostics
            .iter()
            .all(|d| d.severity == Severity::Unsupported)
    );

    let nested = classic(
        "nested.js",
        r#"
        window.setTimeout(() => window.BigInt(1), 10);
        window.alert(window.location.href);
        window.eval(window.navigator.userAgent);
        window.BigInt(2).toString();
        new window.MutationObserver(() => window.confirm("ok?"));
        "#,
    );
    let nested_res = analyze(&[nested]);
    assert_eq!(
        nested_res.diagnostics.len(),
        7,
        "expected 7 diagnostics across nested constructs, got: {:#?}",
        nested_res.diagnostics
    );
}

#[test]
fn ignores_shadowed_window_and_supported_window_apis() {
    let script = classic(
        "supported_and_shadowed.js",
        r#"
        function useShadowed(window, self, globalThis) {
            window.BigInt(1);
            window.eval("1 + 1");
            window.confirm("ok?");
            window.prompt("name?");
            window.Function("return 1")();
            window.RegExp("(?<=a)b");
            new window.MutationObserver(() => {});
            new window.IntersectionObserver(() => {});
            new window.ResizeObserver(() => {});
            new window.Worker("w.js");
            new window.Function("return 2");
            new window.Intl.NumberFormat("en-US");
            new window.RegExp("foo", "s");
            const a = [
                window.BigInt,
                window.eval,
                window.confirm,
                window.prompt,
                window.MutationObserver,
                window.ResizeObserver,
                window.Worker,
                window.location.href,
                window.history.state,
                window.navigator.userAgent,
                self.BigInt(2),
                globalThis.eval("2"),
            ];
            return a;
        }

        function useShadowedGlobals(BigInt, Intl, MutationObserver, ResizeObserver, Worker, eval, confirm, prompt, Function, location, history, navigator) {
            BigInt(1);
            eval("1");
            confirm("ok");
            prompt("p");
            new Function("return 1");
            new MutationObserver(() => {});
            new ResizeObserver(() => {});
            new Worker("w.js");
            return [Intl, location.href, history.state, navigator.userAgent];
        }

        const id = window.setTimeout(() => window.alert("tick"), 50);
        window.clearTimeout(id);
        const iv = window.setInterval(() => {}, 100);
        window.clearInterval(iv);
        const raf = window.requestAnimationFrame(() => {});
        window.cancelAnimationFrame(raf);
        window.localStorage.setItem("k", "v");
        window.sessionStorage.getItem("k");
        const now = window.performance.now();
        const encoded = window.btoa("abc");
        const decoded = window.atob(encoded);
        const mql = window.matchMedia("(min-width: 600px)");
        const dims = [window.innerWidth, window.innerHeight];
        const re1 = window.RegExp("abc", "gi");
        const re2 = new window.RegExp("xyz", "m");
        window.addEventListener("load", () => {});
        window.fetch("/api").then((r) => r.json());
        "#,
    );

    let res = analyze(&[script]);
    assert!(
        res.diagnostics.is_empty(),
        "expected no diagnostics for shadowed window or supported window APIs, got: {:#?}",
        res.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Dynamic styles (docs/design/dynamic-styles.md §9.1): class writes, created
// elements and `runtime_dom`.
// ---------------------------------------------------------------------------

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

fn sets(list: &[&[&str]]) -> Vec<Vec<String>> {
    list.iter().map(|s| names(s)).collect()
}

fn by_id(id: &str) -> ElementRef {
    ElementRef::Query(DomQuery::ById(id.to_string()))
}

/// The inline `<script>` elements of a corpus page, as the converter extracts
/// them: source after the start tag, `line_offset` the line of that tag.
fn corpus_scripts(case: &str) -> Vec<ScriptInput> {
    let path = format!(
        "{}/../../tests/corpus/{case}/index.html",
        env!("CARGO_MANIFEST_DIR")
    );
    let html = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut out = Vec::new();
    let mut rest = html.as_str();
    let mut consumed = 0usize;
    while let Some(start) = rest.find("<script>") {
        let body_start = start + "<script>".len();
        let end = rest[body_start..]
            .find("</script>")
            .expect("unclosed script")
            + body_start;
        let line = html[..consumed + start].matches('\n').count() + 1;
        out.push(ScriptInput {
            source: rest[body_start..end].to_string(),
            name: format!("inline script #{}", out.len() + 1),
            kind: ScriptKind::Classic,
            line_offset: u32::try_from(line).expect("line fits u32"),
        });
        consumed += end;
        rest = &rest[end..];
    }
    out
}

fn line_of(case: &str, needle: &str) -> u32 {
    let path = format!(
        "{}/../../tests/corpus/{case}/index.html",
        env!("CARGO_MANIFEST_DIR")
    );
    let html = std::fs::read_to_string(path).expect("corpus page");
    let at = html.find(needle).expect("needle in page");
    u32::try_from(html[..at].matches('\n').count() + 1).expect("line fits u32")
}

fn write_summary(w: &ClassWrite) -> (ElementRef, Vec<String>, bool, bool) {
    (w.target.clone(), w.names.clone(), w.open, w.assigns)
}

#[test]
fn corpus_12_hover_states_has_no_scripts_and_no_runtime_dom() {
    let scripts = corpus_scripts("12-hover-states");
    assert!(scripts.is_empty());
    let res = analyze(&scripts);
    assert!(res.class_writes.is_empty());
    assert!(res.created.is_empty());
    assert!(!res.runtime_dom);
}

#[test]
fn corpus_15_counter_toggles_negative_on_count() {
    let case = "15-script-counter";
    let res = analyze(&corpus_scripts(case));
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    assert_eq!(
        res.class_writes,
        vec![ClassWrite {
            target: by_id("count"),
            names: names(&["negative"]),
            open: false,
            assigns: false,
            source: "inline script #1".to_string(),
            line: Some(line_of(case, "count.classList.toggle")),
        }]
    );
    assert!(res.created.is_empty());
    // The input and change listeners read `event.target`.
    assert!(res.runtime_dom);
    // The pre-existing summary is unchanged.
    assert!(
        res.writes
            .contains(&(DomQuery::ById("count".to_string()), "classList".to_string()))
    );
}

#[test]
fn corpus_16_todo_predicts_every_created_element() {
    let case = "16-script-todo";
    let res = analyze(&corpus_scripts(case));
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    assert!(res.mutates_dom);
    assert!(res.runtime_dom);

    let li = CreatedId(0);
    let src = "inline script #1".to_string();
    assert_eq!(
        res.created,
        vec![
            CreatedElement {
                id: li,
                tag: "li".to_string(),
                input_type: None,
                class_sets: sets(&[&["done", "todo-item"], &["todo-item"]]),
                open_classes: false,
                parents: vec![by_id("todo-list")],
                source: src.clone(),
                line: Some(line_of(case, "document.createElement('li')")),
            },
            CreatedElement {
                id: CreatedId(1),
                tag: "input".to_string(),
                input_type: Some("checkbox".to_string()),
                class_sets: sets(&[&[]]),
                open_classes: false,
                parents: vec![ElementRef::Created(li)],
                source: src.clone(),
                line: Some(line_of(case, "document.createElement('input')")),
            },
            CreatedElement {
                id: CreatedId(2),
                tag: "span".to_string(),
                input_type: None,
                class_sets: sets(&[&["todo-text"]]),
                open_classes: false,
                parents: vec![ElementRef::Created(li)],
                source: src.clone(),
                line: Some(line_of(case, "document.createElement('span')")),
            },
            CreatedElement {
                id: CreatedId(3),
                tag: "button".to_string(),
                input_type: None,
                class_sets: sets(&[&["delete"]]),
                open_classes: false,
                parents: vec![ElementRef::Created(li)],
                source: src.clone(),
                line: Some(line_of(case, "document.createElement('button')")),
            },
        ]
    );
    assert_eq!(
        res.class_writes
            .iter()
            .map(write_summary)
            .collect::<Vec<_>>(),
        vec![
            (
                ElementRef::Created(li),
                names(&["done", "todo-item"]),
                false,
                true
            ),
            (
                ElementRef::Created(CreatedId(2)),
                names(&["todo-text"]),
                false,
                true
            ),
            (
                ElementRef::Created(CreatedId(3)),
                names(&["delete"]),
                false,
                true
            ),
        ]
    );
    assert_eq!(
        res.class_writes[0].line,
        Some(line_of(case, "li.className ="))
    );
}

#[test]
fn class_values_enumerate_literals_and_mark_open_parts() {
    let res = analyze(&[classic(
        "values.js",
        r#"
        const el = document.getElementById("el");
        const base = "card";
        let later = "x";
        later = pick();
        el.className = "a  b";
        el.className = "item" + (on ? " on" : "");
        el.className = `tab ${active ? "active" : "idle"}`;
        el.className = "chat-message " + role;
        el.className = `toast ${type}`;
        el.className = "btn-" + kind;
        el.className = base + " big";
        el.className = later;
        el.setAttribute("class", flag ? "p" : "q r");
        el.removeAttribute("class");
        el.className = (on && "lit") || "dflt";
        "#,
    )]);
    let el = by_id("el");
    assert_eq!(
        res.class_writes
            .iter()
            .map(write_summary)
            .collect::<Vec<_>>(),
        vec![
            (el.clone(), names(&["a", "b"]), false, true),
            (el.clone(), names(&["item", "on"]), false, true),
            (el.clone(), names(&["active", "idle", "tab"]), false, true),
            (el.clone(), names(&["chat-message"]), true, true),
            (el.clone(), names(&["toast"]), true, true),
            // `btn-` is only part of a name.
            (el.clone(), names(&[]), true, true),
            (el.clone(), names(&["big", "card"]), false, true),
            // Reassigned: its value is not known.
            (el.clone(), names(&[]), true, true),
            (el.clone(), names(&["p", "q", "r"]), false, true),
            (el.clone(), names(&[]), false, true),
            // `on && "lit"` can be `false`.
            (el, names(&["dflt", "lit"]), true, true),
        ]
    );
    assert!(res.created.is_empty());
}

#[test]
fn class_list_methods_record_each_name() {
    let res = analyze(&[classic(
        "classlist.js",
        r#"
        const el = document.querySelector(".menu");
        el.classList.add("open", "wide");
        el.classList.remove("closed");
        el.classList.replace("old", "new");
        el.classList.toggle("dark", true);
        el.classList.toggle("is-" + state, x);
        el.classList.add(...extra);
        el.classList.contains("open");
        "#,
    )]);
    let el = ElementRef::Query(DomQuery::Selector(".menu".to_string()));
    assert_eq!(
        res.class_writes
            .iter()
            .map(write_summary)
            .collect::<Vec<_>>(),
        vec![
            (el.clone(), names(&["open", "wide"]), false, false),
            (el.clone(), names(&["closed"]), false, false),
            (el.clone(), names(&["new", "old"]), false, false),
            (el.clone(), names(&["dark"]), false, false),
            (el.clone(), names(&[]), true, false),
            (el, names(&[]), true, false),
        ]
    );
}

#[test]
fn class_write_targets_follow_locals_returns_and_foreach_parameters() {
    let res = analyze(&[classic(
        "targets.js",
        r#"
        function panel() { return document.getElementById("panel"); }
        const getTab = () => document.querySelector(".tab");
        function twoLevels() { return panel(); }
        function either(x) { return x ? document.getElementById("a") : document.getElementById("b"); }

        panel().classList.add("open");
        getTab().classList.add("current");
        twoLevels().classList.add("deep");
        either(1).classList.add("ab");

        document.querySelectorAll(".tab").forEach((t) => t.classList.remove("active"));
        const items = document.querySelectorAll(".item");
        items.forEach(function (it) { it.classList.add("seen"); });

        document.addEventListener("click", function (event) {
            event.target.classList.toggle("hit", true);
            this.classList.add("clicked");
        });
        function style(node) { node.classList.add("styled"); }
        let moving = document.getElementById("m");
        moving = other();
        moving.classList.add("moved");
        "#,
    )]);
    assert_eq!(
        res.class_writes
            .iter()
            .map(|w| (w.target.clone(), w.names.clone()))
            .collect::<Vec<_>>(),
        vec![
            (by_id("panel"), names(&["open"])),
            (
                ElementRef::Query(DomQuery::Selector(".tab".to_string())),
                names(&["current"])
            ),
            // Only one level of return is followed.
            (ElementRef::Unknown, names(&["deep"])),
            // Two different elements: never one of them.
            (ElementRef::Unknown, names(&["ab"])),
            (
                ElementRef::Query(DomQuery::SelectorAll(".tab".to_string())),
                names(&["active"])
            ),
            (
                ElementRef::Query(DomQuery::SelectorAll(".item".to_string())),
                names(&["seen"])
            ),
            (ElementRef::Unknown, names(&["hit"])),
            (ElementRef::Unknown, names(&["clicked"])),
            (ElementRef::Unknown, names(&["styled"])),
            (ElementRef::Unknown, names(&["moved"])),
        ]
    );
    assert!(res.runtime_dom);
}

#[test]
fn created_elements_record_class_sets_type_and_parents() {
    let res = analyze(&[classic(
        "created.js",
        r#"
        const list = document.getElementById("list");
        function toast(kind, urgent) {
            const t = document.createElement("DIV");
            t.className = `toast ${kind}`;
            if (urgent) {
                t.classList.add("urgent");
            }
            document.body.appendChild(t);
            setTimeout(() => t.classList.add("show"), 10);
            t.classList.remove("toast");
            return t;
        }
        function field() {
            const box = document.createElement("input");
            box.setAttribute("type", "Range");
            box.classList.add("slider");
            box.classList.toggle("big", wide);
            list.append("label", box);
            list.prepend(box);
            list.insertBefore(box, list.firstChild);
            list.replaceChildren(box);
            list.append(box);
            mystery().appendChild(box);
        }
        const hr = document.createElement("hr");
        hr.className = "rule";
        // No force: flips, so a fresh element gains it.
        hr.classList.toggle("thin");
        list.insertAdjacentElement("beforeend", hr);
        list.insertAdjacentElement("afterend", hr);
        const kind = pickTag();
        document.createElement(kind);
        const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
        list.appendChild(document.createElement("p"));
        "#,
    )]);
    let t = &res.created[0];
    assert_eq!(t.tag, "div");
    // Only the classes set before the first insertion; `show` comes later.
    assert_eq!(t.class_sets, sets(&[&["toast"], &["toast", "urgent"]]));
    assert!(t.open_classes);
    assert_eq!(
        t.parents,
        vec![ElementRef::Query(DomQuery::Selector("body".to_string()))]
    );

    let b = &res.created[1];
    assert_eq!(b.tag, "input");
    assert_eq!(b.input_type, Some("range".to_string()));
    assert_eq!(b.class_sets, sets(&[&["big", "slider"], &["slider"]]));
    assert!(!b.open_classes);
    assert_eq!(b.parents, vec![by_id("list"), ElementRef::Unknown]);

    let hr = &res.created[2];
    assert_eq!(hr.tag, "hr");
    assert_eq!(hr.class_sets, sets(&[&["rule", "thin"]]));
    assert_eq!(hr.parents, vec![by_id("list"), ElementRef::Unknown]);

    let p = &res.created[3];
    assert_eq!(p.tag, "p");
    assert_eq!(p.class_sets, sets(&[&[]]));
    assert_eq!(p.parents, vec![by_id("list")]);
    // Non-literal tags and namespaced elements are not predicted.
    assert_eq!(res.created.len(), 4);
    assert_eq!(
        res.created.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![CreatedId(0), CreatedId(1), CreatedId(2), CreatedId(3)]
    );

    let toast_writes: Vec<_> = res
        .class_writes
        .iter()
        .filter(|w| w.target == ElementRef::Created(CreatedId(0)))
        .map(|w| (w.names.clone(), w.open, w.assigns))
        .collect();
    assert_eq!(
        toast_writes,
        vec![
            (names(&["toast"]), true, true),
            (names(&["urgent"]), false, false),
            (names(&["show"]), false, false),
            (names(&["toast"]), false, false),
        ]
    );
    assert!(res.runtime_dom);
}

#[test]
fn created_element_through_two_returns_is_not_attributed() {
    let res = analyze(&[classic(
        "returns.js",
        r#"
        function make() { return document.createElement("li"); }
        function wrap() { return make(); }
        document.getElementById("a").appendChild(make());
        document.getElementById("b").appendChild(wrap());
        "#,
    )]);
    assert_eq!(res.created.len(), 1);
    assert_eq!(res.created[0].parents, vec![by_id("a")]);
}

#[test]
fn created_element_with_unknown_class_value_is_open() {
    let res = analyze(&[classic(
        "opaque.js",
        r#"
        const row = document.createElement("tr");
        row.className = severityClass;
        document.getElementById("t").appendChild(row);
        "#,
    )]);
    assert_eq!(res.created[0].class_sets, sets(&[&[]]));
    assert!(res.created[0].open_classes);
}

#[test]
fn runtime_dom_is_false_for_literal_queries_only() {
    let res = analyze(&[classic(
        "static.js",
        r#"
        const count = document.getElementById("count");
        const all = document.querySelectorAll(".row");
        let n = 0;
        document.getElementById("inc").addEventListener("click", () => {
            n += 1;
            count.textContent = String(n);
            count.classList.toggle("negative", n < 0);
            all.forEach((r) => { r.hidden = n > 3; });
        });
        const config = { title: "x" };
        console.log(config.title);
        "#,
    )]);
    assert!(!res.runtime_dom);
    assert!(!res.mutates_dom);
}

#[test]
fn runtime_dom_is_set_by_each_dynamic_access() {
    for snippet in [
        r#"document.getElementById(name).hidden = true;"#,
        r#"document.querySelector(sel);"#,
        r#"document.getElementById("a").querySelector(".b");"#,
        r#"document.getElementById("a").parentElement.hidden = true;"#,
        r#"document.getElementById("a").children[0];"#,
        r#"document.getElementById("a").nextElementSibling;"#,
        r#"document.getElementById("a").closest(".card");"#,
        r#"document.getElementById("a").matches(".on");"#,
        r#"document.getElementById("a").tagName;"#,
        r#"document.getElementById("a").classList.contains("on");"#,
        r#"const c = document.getElementById("a").className;"#,
        r#"document.activeElement.blur();"#,
        r#"document.addEventListener("click", (e) => e.target.focus());"#,
        r#"document.getElementById("a").onclick = function () { this.hidden = true; };"#,
        r#"document.createElement("div");"#,
        r#"document.getElementById("a").remove();"#,
    ] {
        let res = analyze(&[classic("snippet.js", snippet)]);
        assert!(res.runtime_dom, "expected runtime_dom for: {snippet}");
    }
}

#[test]
fn runtime_dom_ignores_this_in_methods_and_top_level_scripts() {
    let res = analyze(&[classic(
        "this.js",
        r#"
        class Counter { constructor() { this.n = 0; } inc() { this.n += 1; } }
        const store = { items: [], add(x) { this.items.push(x); } };
        const top = this;
        "#,
    )]);
    assert!(!res.runtime_dom);
}

#[test]
fn handler_attribute_this_is_a_runtime_element() {
    let res = analyze(&[ScriptInput {
        source: "this.classList.add('pressed')".to_string(),
        name: "onclick of <button>".to_string(),
        kind: ScriptKind::HandlerAttribute {
            element: ElementKey(3),
            event: "click".to_string(),
        },
        line_offset: 4,
    }]);
    assert!(res.runtime_dom);
    assert_eq!(
        res.class_writes
            .iter()
            .map(write_summary)
            .collect::<Vec<_>>(),
        vec![(ElementRef::Unknown, names(&["pressed"]), false, false)]
    );
    assert_eq!(res.class_writes[0].line, Some(4));
}

// ---------------------------------------------------------------------------
// Markup assigned to `innerHTML` or passed to `insertAdjacentHTML`
// (docs/design/dynamic-styles.md §6.7): its elements are created elements.
// ---------------------------------------------------------------------------

/// `(tag, input type, class sets, open, parents)` of each created element.
type Shape = (
    String,
    Option<String>,
    Vec<Vec<String>>,
    bool,
    Vec<ElementRef>,
);

fn shapes(res: &Analysis) -> Vec<Shape> {
    res.created
        .iter()
        .map(|c| {
            (
                c.tag.clone(),
                c.input_type.clone(),
                c.class_sets.clone(),
                c.open_classes,
                c.parents.clone(),
            )
        })
        .collect()
}

fn shape(tag: &str, classes: &[&[&str]], open: bool, parents: Vec<ElementRef>) -> Shape {
    (tag.to_string(), None, sets(classes), open, parents)
}

fn created(id: u32) -> ElementRef {
    ElementRef::Created(CreatedId(id))
}

fn markup_diagnostics(res: &Analysis) -> Vec<(Severity, Option<u32>, String)> {
    res.diagnostics
        .iter()
        .filter(|d| d.message.contains("dynamic-markup"))
        .map(|d| (d.severity, d.line, d.message.clone()))
        .collect()
}

#[test]
fn inner_html_literal_elements_are_created_in_the_target() {
    let res = analyze(&[classic(
        "markup.js",
        r#"
        const list = document.getElementById("list");
        list.innerHTML = '<li class="item"><span class="label">First</span></li>'
            + '<li class="done item"><span class="label">Second</span><input type="CheckBox"></li>';
        "#,
    )]);
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    assert!(res.mutates_dom);
    assert!(res.runtime_dom);
    // Siblings with different classes are different elements, as separate
    // `createElement` calls are; their children are parented to each.
    assert_eq!(
        shapes(&res),
        vec![
            shape("li", &[&["item"]], false, vec![by_id("list")]),
            shape("span", &[&["label"]], false, vec![created(0)]),
            shape("li", &[&["done", "item"]], false, vec![by_id("list")]),
            shape("span", &[&["label"]], false, vec![created(2)]),
            (
                "input".to_string(),
                Some("checkbox".to_string()),
                sets(&[&[]]),
                false,
                vec![created(2)],
            ),
        ]
    );
    let line = Some(3);
    assert!(
        res.created.iter().all(|c| c.line == line),
        "{:?}",
        res.created
    );
    assert!(res.created.iter().all(|c| c.source == "markup.js"));
    assert_eq!(
        res.created.iter().map(|c| c.id).collect::<Vec<_>>(),
        (0..5).map(CreatedId).collect::<Vec<_>>()
    );
}

#[test]
fn identical_sibling_elements_are_one_created_element() {
    let res = analyze(&[classic(
        "same.js",
        r#"
        document.getElementById("list").innerHTML =
            "<li class='item'>a</li><li class='item'>b</li><li class=' item '>c</li>";
        "#,
    )]);
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    assert_eq!(
        shapes(&res),
        vec![shape("li", &[&["item"]], false, vec![by_id("list")])]
    );
}

#[test]
fn insert_adjacent_html_parents_follow_the_position() {
    let res = analyze(&[classic(
        "adjacent.js",
        r#"
        const list = document.getElementById("list");
        list.insertAdjacentHTML("beforeend", "<li class='a'>x</li>");
        list.insertAdjacentHTML("AfterBegin", "<li class='b'>y</li>");
        list.insertAdjacentHTML("beforebegin", "<p class='c'>z</p>");
        list.insertAdjacentHTML("afterend", "<p class='d'>w</p>");
        list.insertAdjacentHTML(where, "<p class='e'>v</p>");
        "#,
    )]);
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    assert_eq!(
        shapes(&res),
        vec![
            shape("li", &[&["a"]], false, vec![by_id("list")]),
            shape("li", &[&["b"]], false, vec![by_id("list")]),
            // Beside the target: its parent, which analysis does not know.
            shape("p", &[&["c"]], false, vec![ElementRef::Unknown]),
            shape("p", &[&["d"]], false, vec![ElementRef::Unknown]),
            shape("p", &[&["e"]], false, vec![ElementRef::Unknown]),
        ]
    );
}

#[test]
fn markup_with_dynamic_text_and_class_values_keeps_its_literal_structure() {
    let res = analyze(&[classic(
        "dynamic.js",
        r#"
        const list = document.getElementById("list");
        function add(item) {
            list.insertAdjacentHTML(
                "beforeend",
                '<li class="item' + (item.done ? ' done' : '') + '"><span class="label">'
                    + item.text + '</span></li>'
            );
            list.innerHTML += `<li class="item ${item.kind}" data-id="${item.id}">${item.text}</li>`;
        }
        "#,
    )]);
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    // Each class value of the ternary is an element of its own, in
    // enumeration order.
    assert_eq!(
        shapes(&res),
        vec![
            shape("li", &[&["done", "item"]], false, vec![by_id("list")]),
            shape("span", &[&["label"]], false, vec![created(0)]),
            shape("li", &[&["item"]], false, vec![by_id("list")]),
            shape("span", &[&["label"]], false, vec![created(2)]),
            // `item ${kind}`: `item` is literal, the rest is open.
            shape("li", &[&["item"]], true, vec![by_id("list")]),
        ]
    );
}

#[test]
fn markup_inside_a_created_element_is_parented_to_it() {
    let res = analyze(&[classic(
        "nested.js",
        r#"
        const li = document.createElement("li");
        li.className = "row";
        li.innerHTML = '<span class="label">x</span>';
        document.getElementById("list").appendChild(li);
        "#,
    )]);
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    assert_eq!(
        shapes(&res),
        vec![
            shape("li", &[&["row"]], false, vec![by_id("list")]),
            shape("span", &[&["label"]], false, vec![created(0)]),
        ]
    );
}

#[test]
fn empty_and_text_only_markup_creates_nothing() {
    let res = analyze(&[classic(
        "empty.js",
        r#"
        const list = document.getElementById("list");
        list.innerHTML = "";
        list.innerHTML = "no elements &amp; no tags";
        document.getElementById("msg").innerHTML = "<!-- a comment -->";
        "#,
    )]);
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    assert!(res.created.is_empty(), "{:?}", res.created);
    assert!(res.mutates_dom);
}

#[test]
fn non_literal_markup_is_a_diagnostic() {
    let res = analyze(&[classic(
        "opaque.js",
        r#"
        const list = document.getElementById("list");
        list.innerHTML = rows.map((r) => `<li>${r}</li>`).join("");
        list.insertAdjacentHTML("beforeend", html);
        list.innerHTML = '<li class="a">x</li>' + more;
        list.innerHTML = `<${tag} class="b">y</${tag}>`;
        list.innerHTML = `<li ${attrs}>z</li>`;
        "#,
    )]);
    let diags = markup_diagnostics(&res);
    assert_eq!(
        diags.iter().map(|d| (d.0, d.1)).collect::<Vec<_>>(),
        vec![
            (Severity::Unsupported, Some(3)),
            (Severity::Unsupported, Some(4)),
            (Severity::Unsupported, Some(5)),
            (Severity::Unsupported, Some(6)),
            (Severity::Unsupported, Some(7)),
        ],
        "{diags:?}"
    );
    assert!(
        diags.iter().all(|d| d.2.contains("not literal")),
        "{diags:?}"
    );
    // The literal elements beside an unknown part are still predicted; an
    // element whose tag or attribute names are not literal is not.
    assert_eq!(
        shapes(&res),
        vec![shape("li", &[&["a"]], false, vec![by_id("list")])]
    );
}

#[test]
fn markup_elements_the_runtime_does_not_create_are_left_out() {
    let res = analyze(&[classic(
        "skipped.js",
        r#"
        document.getElementById("box").innerHTML =
            '<svg class="icon"><path d="M0 0"/></svg><p class="x">a</p>';
        "#,
    )]);
    assert!(res.diagnostics.is_empty(), "{:?}", res.diagnostics);
    assert_eq!(
        shapes(&res),
        vec![shape("p", &[&["x"]], false, vec![by_id("box")])]
    );
}

#[test]
fn containers_whose_children_scripts_change() {
    let res = analyze(&[classic(
        "lists.js",
        r##"
        const list = document.getElementById("list");
        const done = document.querySelector("#done");
        const log = document.getElementById("log");
        const first = document.getElementById("first");
        const gone = document.getElementById("gone");
        function render() {
            list.textContent = "";
            const li = document.createElement("li");
            li.appendChild(document.createTextNode("x"));
            list.appendChild(li);
        }
        done.removeChild(done.firstElementChild);
        log.insertAdjacentHTML("beforeend", "<p>hi</p>");
        first.before(document.createElement("hr"));
        gone.remove();
        document.querySelector(".x").classList.remove("y");
        event.target.closest("li").remove();
        "##,
    )]);
    assert_eq!(
        res.mutated_parents,
        vec![
            by_id("list"),
            created(0),
            ElementRef::Query(DomQuery::Selector("#done".to_string())),
            by_id("log")
        ]
    );
    assert_eq!(res.mutated_siblings, vec![by_id("first"), by_id("gone")]);
}
