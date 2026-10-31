use std::collections::HashSet;
use std::path::PathBuf;

use h2q_css::{Display, LoadedSheet, StylesheetLoader};
use h2q_dom::Document;
use h2q_qml::{Import, Member, Value};

use crate::diag::Diagnostics;
use crate::{
    Diagnostic, Input, Options, Output, OutputFile, Resources, Severity, metadata, scripts,
};

/// `link`ed style sheets and `@import`s come through the caller's
/// resources, relative to the document.
struct Loader<'r> {
    resources: &'r mut dyn Resources,
    base: &'r str,
}

impl StylesheetLoader for Loader<'_> {
    fn load(&mut self, href: &str) -> Option<LoadedSheet> {
        self.resources.read(href, self.base).map(|r| LoadedSheet {
            text: String::from_utf8_lossy(&r.bytes).into_owned(),
            url: r.resolved,
        })
    }
}

pub(crate) fn convert(input: &Input, options: &Options, resources: &mut dyn Resources) -> Output {
    let mut diags = Diagnostics::new(&input.url);

    // 1–2: DOM and computed styles.
    let doc = Document::parse(&input.html);
    let mut loader = Loader {
        resources,
        base: &input.url,
    };
    let (sheets, sheet_diags) = h2q_css::collect_stylesheets(&doc, &mut loader, &options.viewport);
    let resources = loader.resources;
    let styles = h2q_css::compute_styles(&doc, &sheets, &options.viewport);
    let dense_grid = doc.descendants(doc.root()).any(|n| {
        styles.style(n).is_some_and(|s| {
            s.grid_auto_flow_dense && matches!(s.display, Display::Grid | Display::InlineGrid)
        })
    });
    if dense_grid {
        diags.info(
            "css",
            None,
            "`grid-auto-flow: dense` is laid out as sparse auto-placement".to_owned(),
        );
    }
    for d in sheet_diags.iter().chain(&styles.diagnostics) {
        diags.push(css_diagnostic(d));
    }

    // 3: what scripts touch.
    let scripts = scripts::collect(&doc, &input.url, resources, &mut diags);
    for d in &scripts.analysis.diagnostics {
        diags.push(js_diagnostic(d));
    }

    // 4–5: boxes, then QML.
    let no_scripts = HashSet::new();
    let wanted = if scripts.is_empty() {
        &no_scripts
    } else {
        &scripts.wanted
    };
    let images = crate::images::collect(&doc, &input.url, resources, &mut diags);
    let runtime_dom = !scripts.is_empty() && scripts.analysis.runtime_dom;
    let emission = crate::emit::dynamic::emit(
        &crate::emit::dynamic::Inputs {
            doc: &doc,
            styles: &styles,
            sheets: &sheets,
            options,
            wanted,
            images: &images.by_node,
            runtime_dom,
            scripts: (!scripts.is_empty()).then_some(&scripts),
        },
        &mut diags,
    );
    let mut root = emission.root;
    let targets = emission.targets;
    let forms = emission.forms;
    let extra_imports = emission.imports;

    if options.animations && !styles.keyframes.is_empty() {
        diags.info(
            "css",
            None,
            "CSS animations are not converted yet".to_owned(),
        );
    }

    let mut qml = h2q_qml::Document::default();
    let mut files = images.files;

    // 6: page.js, with the ids chosen in step 5.
    if !scripts.is_empty() {
        scripts::report_missing(&doc, &scripts, &targets, &mut diags);
        let resolver = scripts::Resolver::new(&scripts, &targets);
        let js = h2q_js::rewrite(&scripts.inputs, &scripts.analysis, &resolver);
        for d in &js.diagnostics {
            diags.push(js_diagnostic(d));
        }
        scripts::wire(&mut root, &js, &forms, &scripts, &mut diags);
        scripts::declare_classes(&mut root, &doc, &scripts, &targets, &mut diags);
        let mut decls = vec![metadata::base_url_decl(&doc, &input.url)];
        if emission.gated {
            decls.push(Member::PropertyDecl {
                ty: "bool".to_owned(),
                name: "h2qAnimate".to_owned(),
                value: Some(Value::Bool(false)),
                readonly: false,
                required: false,
            });
        }
        if runtime_dom {
            let classes = h2q_css::styled_classes(&sheets);
            decls.push(metadata::styled_classes_decl(&classes));
        }
        root.members.splice(0..0, decls);
        // A page with only functions has no init function, but the runtime
        // still needs its root (`baseUrl`, queries from the document).
        let init = js.init_function.as_ref().map_or_else(
            || format!("H2Q.init({});", crate::emit::units::ROOT_ID),
            |init| format!("Page.{init}({});", crate::emit::units::ROOT_ID),
        );
        root.members.push(Member::Handler {
            name: "Component.onCompleted".to_owned(),
            params: Vec::new(),
            body: if emission.gated {
                // Transitions start once the document's classes are set.
                vec![
                    init,
                    format!("{}.h2qAnimate = true;", crate::emit::units::ROOT_ID),
                ]
            } else {
                vec![init]
            },
        });
        qml.import(Import::path("page.js", Some("Page")));
        qml.import(Import::path("h2q_runtime.js", Some("H2Q")));
        files.push(OutputFile {
            path: PathBuf::from("page.js"),
            contents: js.page_js.into_bytes(),
        });
        files.push(OutputFile {
            path: PathBuf::from("h2q_runtime.js"),
            contents: h2q_js::RUNTIME_JS.as_bytes().to_vec(),
        });
    }

    // 7: Main.qml.
    let name = std::path::Path::new(&input.url)
        .file_name()
        .map_or_else(|| input.url.clone(), |n| n.to_string_lossy().into_owned());
    qml.header = vec![format!("Generated by html-2-qml from {name}. Do not edit.")];
    for module in ["QtQuick", "QtQuick.Controls.Basic", "QtQuick.Layouts"] {
        qml.import(Import::module(module));
    }
    for module in extra_imports {
        qml.import(Import::module(module));
    }
    qml.root = root;
    files.insert(
        0,
        OutputFile {
            path: PathBuf::from("Main.qml"),
            contents: qml.to_qml().into_bytes(),
        },
    );
    Output {
        files,
        diagnostics: diags.into_vec(),
    }
}

pub(crate) fn css_diagnostic(d: &h2q_css::Diagnostic) -> Diagnostic {
    Diagnostic {
        severity: match d.severity {
            h2q_css::Severity::Unsupported => Severity::Warning,
            h2q_css::Severity::Invalid => Severity::Info,
        },
        stage: "css",
        source: d.source.clone(),
        line: d.line,
        message: d.message.clone(),
    }
}

fn js_diagnostic(d: &h2q_js::Diagnostic) -> Diagnostic {
    Diagnostic {
        severity: match d.severity {
            h2q_js::Severity::Unsupported => Severity::Warning,
            h2q_js::Severity::Error => Severity::Error,
        },
        stage: "js",
        source: d.source.clone(),
        line: d.line,
        message: d.message.clone(),
    }
}
