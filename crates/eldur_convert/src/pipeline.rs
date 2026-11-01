use std::collections::HashSet;
use std::path::PathBuf;

use eldur_css::{Display, LoadedSheet, StylesheetLoader};
use eldur_dom::Document;
use eldur_qml::{Import, Member, Value};

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
    let (sheets, sheet_diags) =
        eldur_css::collect_stylesheets(&doc, &mut loader, &options.viewport);
    let resources = loader.resources;
    let styles = eldur_css::compute_styles(&doc, &sheets, &options.viewport);
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
    let mut scripts = scripts::collect(&doc, &input.url, resources, &mut diags);
    for d in &scripts.analysis.diagnostics {
        diags.push(js_diagnostic(d));
    }

    // 4–5: boxes, then QML.
    let no_scripts = HashSet::new();
    // A tap on an inline element looks for its node, and goes up from there.
    if !scripts.is_empty()
        && scripts
            .wanted
            .iter()
            .any(|&n| crate::boxes::is_inline_node(&doc, &styles, n))
    {
        scripts.analysis.dispatch_by_path = true;
    }
    let wanted = if scripts.is_empty() {
        &no_scripts
    } else {
        &scripts.wanted
    };
    let mut images = crate::images::collect(&doc, &input.url, resources, &mut diags);
    crate::images::collect_backgrounds(
        &mut images,
        &doc,
        &styles,
        &sheets,
        &input.url,
        resources,
        &mut diags,
    );
    // An inline element or option has no object for a script's static target,
    // so a script that wants one finds it at run time.
    let runtime_dom = !scripts.is_empty()
        && (scripts.analysis.runtime_dom
            || scripts.analysis.open_elements
            || wanted
                .iter()
                .any(|&n| crate::boxes::is_inline_node(&doc, &styles, n)));
    // A script that walks or looks up at run time can reach anything, and a
    // hidden element has no object unless it is kept.
    let reachable = if runtime_dom {
        let mut all = wanted.clone();
        all.extend(crate::boxes::hidden_elements(&doc, &styles));
        all
    } else {
        HashSet::new()
    };
    let wanted = if runtime_dom { &reachable } else { wanted };
    let inputs = crate::emit::dynamic::Inputs {
        doc: &doc,
        styles: &styles,
        sheets: &sheets,
        options,
        wanted,
        images: &images.by_node,
        background_images: &images.backgrounds,
        runtime_dom,
        scripts: (!scripts.is_empty()).then_some(&scripts),
        probe: None,
    };
    let emission = crate::emit::dynamic::emit(&inputs, &mut diags);
    let mut root = emission.root;
    let targets = emission.targets;
    let inline_covered = emission.inline_covered;
    let forms = emission.forms;
    let mut extra_imports = emission.imports;
    let mut gated = emission.gated;

    if options.animations && !styles.keyframes.is_empty() {
        diags.info(
            "css",
            None,
            "CSS animations are not converted yet".to_owned(),
        );
    }

    let mut qml = eldur_qml::Document::default();
    let mut files = images.files;

    // 6: page.js, with the ids chosen in step 5.
    if !scripts.is_empty() {
        scripts::report_missing(&doc, &scripts, &targets, &inline_covered, &mut diags);
        let follows_generated = !scripts.analysis.class_writes.is_empty()
            && !eldur_css::pseudo_rules(&sheets, &options.viewport).is_empty();
        let resolver = scripts::Resolver::new(&scripts, &targets, &inline_covered)
            .with_generated_text(follows_generated);
        let js = eldur_js::rewrite(&scripts.inputs, &scripts.analysis, &resolver);
        for d in &js.diagnostics {
            diags.push(js_diagnostic(d));
        }
        let attribute_handlers = scripts::wire(&mut root, &js, &forms, &scripts, &mut diags);
        scripts::declare_classes(&mut root, &doc, &scripts, &targets, &mut diags);
        // A created element may land in a block of text the script holds.
        let holds_text = wanted.iter().any(|&n| {
            doc.children(n).any(|c| {
                matches!(&doc.node(c).data, eldur_dom::NodeData::Text(t) if !t.trim().is_empty())
            })
        });
        // The page's links and spans may be an event's target, named or not.
        let delegated = scripts.analysis.dispatch_by_path && has_inline_model(&root);
        let open_elements = scripts.analysis.open_elements;
        let inline_rules = runtime_dom
            && (delegated
                || !inline_covered.is_empty()
                || (scripts.analysis.mutates_dom && holds_text));
        if let Some(added) = crate::emit::templates::add(
            &inputs,
            &scripts,
            &mut root,
            &targets,
            inline_rules,
            &mut diags,
        ) {
            gated |= added.gated;
            extra_imports.extend(added.imports);
        }
        let mut decls = vec![
            metadata::base_url_decl(&doc, &input.url),
            metadata::document_url_decl(&input.url),
        ];
        if gated {
            decls.push(Member::PropertyDecl {
                ty: "bool".to_owned(),
                name: "eldurAnimate".to_owned(),
                value: Some(Value::Bool(false)),
                readonly: false,
                required: false,
            });
        }
        if runtime_dom {
            let classes = eldur_css::styled_classes(&sheets);
            decls.push(metadata::styled_classes_decl(&classes));
            if scripts.analysis.reads_template_content {
                decls.extend(metadata::html_templates_decl(&doc));
            }
            if !scripts.analysis.class_writes.is_empty() {
                let pseudo = eldur_css::pseudo_rules(&sheets, &options.viewport);
                if !pseudo.is_empty() {
                    decls.push(metadata::pseudo_rules_decl(&pseudo));
                }
            }
            if scripts.analysis.needs_cascade() {
                let rules = eldur_css::runtime_rules(&sheets, &options.viewport);
                let mut with_rules = crate::images::Images {
                    backgrounds: images.backgrounds.clone(),
                    files: std::mem::take(&mut files),
                    ..Default::default()
                };
                crate::images::add_background_files(
                    &mut with_rules,
                    crate::paint_values::background_urls(&rules, &options.viewport),
                    &sheets,
                    &input.url,
                    resources,
                    &mut diags,
                );
                files = with_rules.files;
                decls.push(crate::paint_values::paint_values_decl(
                    &rules,
                    &options.viewport,
                    &with_rules.backgrounds,
                ));
                if !open_elements {
                    decls.push(metadata::cascade_rules_decl(&rules));
                }
            }
            if open_elements {
                let rules = eldur_css::runtime_rules(&sheets, &options.viewport);
                // Any element of the page may take a rule like this at run time,
                // and leaves the flow for the block the runtime needs here.
                let leaves_flow = rules.iter().any(|r| {
                    r.declarations
                        .iter()
                        .any(|(p, v, _)| p == "position" && (v == "absolute" || v == "fixed"))
                });
                if leaves_flow
                    && !root.members.iter().any(|m| {
                        matches!(m, Member::PropertyDecl { name, .. } if name == "eldurInitialBlock")
                    })
                {
                    crate::emit::declare_initial_block(&mut root);
                }
                decls.push(metadata::runtime_rules_decl(&rules));
                let literal = scripts
                    .analysis
                    .class_writes
                    .iter()
                    .flat_map(|w| w.names.iter().map(String::as_str))
                    .collect();
                decls.push(metadata::literal_classes_decl(&literal));
                decls.push(metadata::open_elements_decl());
                decls.push(metadata::control_fonts_decl());
            } else if inline_rules {
                decls.push(metadata::inline_rules_decl(&eldur_css::inline_rules(
                    &sheets,
                    &options.viewport,
                )));
            }
        }
        root.members.splice(0..0, decls);
        // A page with only functions has no init function, but the runtime
        // still needs its root (`baseUrl`, queries from the document).
        let init = js.init_function.as_ref().map_or_else(
            || format!("Eldur.init({});", crate::emit::units::ROOT_ID),
            |init| format!("Page.{init}({});", crate::emit::units::ROOT_ID),
        );
        root.members.push(Member::Handler {
            name: "Component.onCompleted".to_owned(),
            params: Vec::new(),
            body: if gated {
                // Transitions start once the document's classes are set.
                let mut body = attribute_handlers;
                body.extend([
                    init,
                    format!("{}.eldurAnimate = true;", crate::emit::units::ROOT_ID),
                ]);
                body
            } else {
                let mut body = attribute_handlers;
                body.push(init);
                body
            },
        });
        qml.import(Import::path("page.js", Some("Page")));
        qml.import(Import::path("eldur_runtime.js", Some("Eldur")));
        files.push(OutputFile {
            path: PathBuf::from("page.js"),
            contents: js.page_js.into_bytes(),
        });
        files.push(OutputFile {
            path: PathBuf::from("eldur_runtime.js"),
            contents: eldur_js::RUNTIME_JS.as_bytes().to_vec(),
        });
    }

    // 7: Main.qml.
    let name = std::path::Path::new(&input.url)
        .file_name()
        .map_or_else(|| input.url.clone(), |n| n.to_string_lossy().into_owned());
    qml.header = vec![format!("Generated by Eldur from {name}. Do not edit.")];
    for module in ["QtQuick", "QtQuick.Controls.Basic", "QtQuick.Layouts"] {
        qml.import(Import::module(module));
    }
    for module in extra_imports {
        qml.import(Import::module(module));
    }
    qml.root = root;
    let main = qml.to_qml();
    // A script makes lines of its own; a page without one needs it only if it uses it.
    if main.contains("EldurLines {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
        files.push(OutputFile {
            path: PathBuf::from("EldurLines.qml"),
            contents: crate::emit::ELDUR_LINES_QML.as_bytes().to_vec(),
        });
    }
    if main.contains("EldurBackgroundLayer {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
        files.push(OutputFile {
            path: PathBuf::from("EldurBackgroundLayer.qml"),
            contents: crate::emit::ELDUR_BACKGROUND_LAYER_QML.as_bytes().to_vec(),
        });
    }
    if main.contains("EldurInlineBox {") {
        files.push(OutputFile {
            path: PathBuf::from("EldurInlineBox.qml"),
            contents: crate::emit::ELDUR_INLINE_BOX_QML.as_bytes().to_vec(),
        });
    }
    files.insert(
        0,
        OutputFile {
            path: PathBuf::from("Main.qml"),
            contents: main.into_bytes(),
        },
    );
    Output {
        files,
        diagnostics: diags.into_vec(),
    }
}

fn has_inline_model(obj: &eldur_qml::Object) -> bool {
    obj.members.iter().any(|member| match member {
        Member::PropertyDecl {
            name,
            value: Some(Value::String(model)),
            ..
        } => name == "eldurInline" && model.contains("\"tag\":"),
        Member::Child(child) => has_inline_model(child),
        _ => false,
    })
}

pub(crate) fn css_diagnostic(d: &eldur_css::Diagnostic) -> Diagnostic {
    Diagnostic {
        severity: match d.severity {
            eldur_css::Severity::Unsupported => Severity::Warning,
            eldur_css::Severity::Invalid => Severity::Info,
        },
        stage: "css",
        source: d.source.clone(),
        line: d.line,
        message: d.message.clone(),
    }
}

fn js_diagnostic(d: &eldur_js::Diagnostic) -> Diagnostic {
    Diagnostic {
        severity: match d.severity {
            eldur_js::Severity::Unsupported => Severity::Warning,
            eldur_js::Severity::Error => Severity::Error,
        },
        stage: "js",
        source: d.source.clone(),
        line: d.line,
        message: d.message.clone(),
    }
}
