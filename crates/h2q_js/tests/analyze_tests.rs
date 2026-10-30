use h2q_js::{Analysis, DomQuery, ElementKey, ScriptInput, ScriptKind, Severity, analyze};

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
    } = analyze(&[s1, s2]);

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
