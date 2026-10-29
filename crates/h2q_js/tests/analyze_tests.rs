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
