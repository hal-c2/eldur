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
    if !scripts.is_empty()
        && selectors_outrun_templates(
            &doc,
            &scripts,
            &eldur_css::runtime_rules(&sheets, &options.viewport),
        )
    {
        scripts.analysis.open_elements = true;
        scripts.analysis.adopts_for_inline = true;
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
        all.extend(crate::scripts::text_areas(&doc));
        // `getElementById` finds items by the name only a wanted element gets.
        if scripts.analysis.computed_lookups {
            all.extend(
                doc.descendants(doc.root())
                    .filter(|&n| doc.element(n).is_some_and(|e| e.id().is_some())),
            );
        }
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

    let mut qml = eldur_qml::Document::default();
    let mut files = images.files;

    // 6: page.js, with the ids chosen in step 5.
    if !scripts.is_empty() {
        scripts::report_missing(&doc, &scripts, &targets, &inline_covered, &mut diags);
        let pseudo = eldur_css::pseudo_rules(&sheets, &options.viewport);
        // Numbers in generated text move when a script adds or removes an element.
        let counts = pseudo.iter().any(|r| {
            r.content.iter().any(|p| {
                matches!(
                    p,
                    eldur_css::PseudoPart::Counter(..) | eldur_css::PseudoPart::Counters(..)
                )
            })
        });
        let follows_generated = (!scripts.analysis.class_writes.is_empty()
            && pseudo.iter().any(|r| r.sets_content))
            || (counts && scripts.analysis.mutates_dom);
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
            if follows_generated {
                decls.push(metadata::pseudo_rules_decl(&pseudo));
                if counts {
                    let counters = eldur_css::counter_rules(&sheets, &options.viewport);
                    decls.push(metadata::counter_rules_decl(&counters));
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
                for text in crate::paint_values::flattened_transforms(&rules) {
                    diags.info(
                        "css",
                        None,
                        format!(
                            "`{text}`, where a script applies it, is flattened to the plane: \
                             it has no depth here, and what it overlaps stays in document order"
                        ),
                    );
                }
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
                if !options.animations {
                    decls.push(metadata::no_motion_decl());
                } else if !styles.keyframes.is_empty() {
                    decls.push(metadata::keyframes_decl(&styles.keyframes));
                }
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

    // The font module: every font in the page goes through it.
    qml.import(Import::path("eldur_fonts.js", Some("EldurFonts")));
    files.push(OutputFile {
        path: PathBuf::from("eldur_fonts.js"),
        contents: eldur_js::FONTS_JS.as_bytes().to_vec(),
    });

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
    crate::emit::name_runs(&mut qml.root);
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
        files.push(OutputFile {
            path: PathBuf::from("EldurRun.qml"),
            contents: crate::emit::ELDUR_RUN_QML.as_bytes().to_vec(),
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
    if main.contains("EldurRoundClip {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
        files.push(OutputFile {
            path: PathBuf::from("EldurRoundClip.qml"),
            contents: crate::emit::ELDUR_ROUND_CLIP_QML.as_bytes().to_vec(),
        });
    }
    if main.contains("EldurDisclosure {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
        files.push(OutputFile {
            path: PathBuf::from("EldurDisclosure.qml"),
            contents: crate::emit::ELDUR_DISCLOSURE_QML.as_bytes().to_vec(),
        });
    }
    // The page's own scroller uses it as well as every box's.
    files.push(OutputFile {
        path: PathBuf::from("EldurScrollInput.qml"),
        contents: crate::emit::ELDUR_SCROLL_INPUT_QML.as_bytes().to_vec(),
    });
    if main.contains("EldurSticky {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
        files.push(OutputFile {
            path: PathBuf::from("EldurSticky.qml"),
            contents: crate::emit::ELDUR_STICKY_QML.as_bytes().to_vec(),
        });
    }
    if main.contains("EldurScroller {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
        files.push(OutputFile {
            path: PathBuf::from("EldurScroller.qml"),
            contents: crate::emit::ELDUR_SCROLLER_QML.as_bytes().to_vec(),
        });
    }
    if main.contains("EldurTextLines {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
        files.push(OutputFile {
            path: PathBuf::from("EldurTextLines.qml"),
            contents: crate::emit::ELDUR_TEXT_LINES_QML.as_bytes().to_vec(),
        });
    }
    if main.contains("EldurBorder {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
        files.push(OutputFile {
            path: PathBuf::from("EldurBorder.qml"),
            contents: crate::emit::ELDUR_BORDER_QML.as_bytes().to_vec(),
        });
    }
    if main.contains("EldurFilter {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
        files.push(OutputFile {
            path: PathBuf::from("EldurFilter.qml"),
            contents: crate::emit::ELDUR_FILTER_QML.as_bytes().to_vec(),
        });
        files.push(OutputFile {
            path: PathBuf::from("eldur_filter.frag.qsb"),
            contents: crate::emit::ELDUR_FILTER_QSB.to_vec(),
        });
    }
    if main.contains("EldurTextShadow {") {
        files.push(OutputFile {
            path: PathBuf::from("EldurTextShadow.qml"),
            contents: crate::emit::ELDUR_TEXT_SHADOW_QML.as_bytes().to_vec(),
        });
    }
    // The runtime paints the boxes of inline elements it lays out again.
    if main.contains("EldurInlineBox {")
        || files
            .iter()
            .any(|f| f.path.as_os_str() == "eldur_runtime.js")
    {
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

/// A template is written for a tag and the classes the analysis saw. Everything
/// else a rule asks is known only once the page runs: an id or attribute a script
/// writes, a class it computes, and, for what kind of box the element is, anything
/// but its own tag and classes.
fn selectors_outrun_templates(
    doc: &Document,
    scripts: &scripts::Scripts,
    rules: &[eldur_css::RuntimeRule],
) -> bool {
    let analysis = &scripts.analysis;
    rules.iter().filter(|r| !r.user_agent).any(|rule| {
        let selector = rule.selector.as_str();
        names_written_id(selector, analysis)
            || names_written_attribute(selector, analysis)
            || class_takes_out_of_flow(rule, analysis)
            || class_gives_pseudo_box(rule, analysis)
            || (changes_form(rule) && form_depends_on_page(doc, scripts, selector))
    })
}

/// A template's conditions reach paint, height and margins. Its width and what
/// kind of box it is are one answer per template, whatever its place in the list.
fn changes_form(rule: &eldur_css::RuntimeRule) -> bool {
    rule.declarations.iter().any(|(p, _, _)| {
        p.ends_with("width") && !p.starts_with("border") && !p.starts_with("outline")
            || matches!(p.as_str(), "display" | "position" | "float")
            || p.starts_with("flex")
            || p.starts_with("grid")
    })
}

/// A class variant changes what a form paints, and can't move it out of a layout.
fn class_takes_out_of_flow(rule: &eldur_css::RuntimeRule, analysis: &eldur_js::Analysis) -> bool {
    rule.declarations
        .iter()
        .any(|(p, v, _)| p == "position" && (v == "absolute" || v == "fixed"))
        && names_after(&rule.selector, '.').any(|name| {
            analysis
                .class_writes
                .iter()
                .any(|w| w.names.iter().any(|n| n == name))
        })
}

/// A box a class gives `::before` or `::after` is laid out by the runtime, which
/// does that only on an open page; a template has no place to put it.
fn class_gives_pseudo_box(rule: &eldur_css::RuntimeRule, analysis: &eldur_js::Analysis) -> bool {
    let selector = rule.selector.trim_end();
    let pseudo = [":before", ":after"].iter().any(|s| selector.ends_with(s));
    pseudo
        && rule
            .declarations
            .iter()
            .any(|(p, _, _)| matches!(p.as_str(), "display" | "position" | "float"))
        && names_after(selector, '.').any(|name| {
            analysis
                .class_writes
                .iter()
                .any(|w| w.names.iter().any(|n| n == name))
        })
}

fn names_after(selector: &str, mark: char) -> impl Iterator<Item = &str> {
    selector.match_indices(mark).map(move |(at, _)| {
        let rest = &selector[at + mark.len_utf8()..];
        let rest = rest.trim_start();
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(rest.len());
        &rest[..end]
    })
}

fn names_written_id(selector: &str, analysis: &eldur_js::Analysis) -> bool {
    names_after(selector, '#')
        .any(|name| analysis.writes_computed_id || analysis.written_ids.contains(name))
}

/// The converter's variants answer for an attribute on a page's own elements; a
/// template is written before anyone has set one on it.
fn names_written_attribute(selector: &str, analysis: &eldur_js::Analysis) -> bool {
    !analysis.created.is_empty()
        && names_after(selector, '[').any(|name| {
            analysis.writes_computed_attribute
                || analysis
                    .written_attributes
                    .contains(&name.to_ascii_lowercase())
        })
}

/// Whether a selector could style an element a script creates, by more than the
/// tag and classes its template was written for.
fn form_depends_on_page(doc: &Document, scripts: &scripts::Scripts, selector: &str) -> bool {
    let analysis = &scripts.analysis;
    let subject = last_compound(selector);
    // An id no script writes is on no element a script creates.
    if names_after(subject, '#')
        .any(|name| !analysis.writes_computed_id && !analysis.written_ids.contains(name))
    {
        return false;
    }
    let (tag, classes) = compound_names(subject);
    // A state or an id, or an attribute the template has no value for. Written
    // attributes open the page on their own (`names_written_attribute`).
    let own = |created: &eldur_js::CreatedElement| {
        subject.contains([':', '#'])
            || names_after(subject, '[').any(|name| {
                name.eq_ignore_ascii_case("type")
                    && created.tag == "input"
                    && created.input_type.is_none()
            })
    };
    let context = &selector.trim()[..selector.trim().len() - subject.len()];
    analysis.created.iter().any(|created| {
        (tag.is_empty() || tag == "*" || tag.eq_ignore_ascii_case(&created.tag))
            && (created.open_classes
                || classes.iter().all(|class| {
                    created
                        .class_sets
                        .iter()
                        .any(|set| set.iter().any(|c| c == class))
                }))
            && match context_fit(doc, scripts, context, created) {
                Fit::Never => false,
                Fit::Later => true,
                Fit::Always => own(created) || (created.open_classes && !classes.is_empty()),
            }
    })
}

/// What an element a script creates may have around it. A template is probed
/// at each place the analysis knows, so what is there when the page is converted
/// is in it; what a script writes there later, or a place it can't name, is not.
enum Around {
    Anything,
    Known {
        tag: String,
        id: Option<String>,
        /// On it in every copy of the page.
        classes: Vec<String>,
        /// A script may add these.
        written: Vec<String>,
        any_class: bool,
    },
}

/// How a compound of a selector's context can fit the ancestors of an element.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fit {
    Never,
    /// Only once a script writes it, or where the analysis lost the place.
    Later,
    Always,
}

impl Around {
    fn fit(&self, compound: &str, analysis: &eldur_js::Analysis) -> Fit {
        let Around::Known {
            tag,
            id,
            classes,
            written,
            any_class,
        } = self
        else {
            return Fit::Later;
        };
        let (want_tag, want_classes) = compound_names(compound);
        if !(want_tag.is_empty() || want_tag == "*" || want_tag.eq_ignore_ascii_case(tag)) {
            return Fit::Never;
        }
        let mut fit = Fit::Always;
        for want in &want_classes {
            if !classes.contains(want) {
                if *any_class || written.contains(want) {
                    fit = Fit::Later;
                } else {
                    return Fit::Never;
                }
            }
        }
        for want in names_after(compound, '#') {
            if id.as_deref() != Some(want) {
                if analysis.writes_computed_id || analysis.written_ids.contains(want) {
                    fit = Fit::Later;
                } else {
                    return Fit::Never;
                }
            }
        }
        fit
    }
}

/// How a selector's context fits a created element: its worst compound against
/// the best of the ancestors the element may have.
fn context_fit(
    doc: &Document,
    scripts: &scripts::Scripts,
    context: &str,
    created: &eldur_js::CreatedElement,
) -> Fit {
    let (ancestral, siblings) = context_compounds(context);
    if ancestral.is_empty() && !siblings {
        return Fit::Always;
    }
    let mut around = Vec::new();
    ancestors_of(doc, scripts, created, 0, &mut around);
    let worst = ancestral
        .iter()
        .map(|compound| {
            around
                .iter()
                .map(|a| a.fit(compound, &scripts.analysis))
                .max()
                .unwrap_or(Fit::Later)
        })
        .min()
        .unwrap_or(Fit::Always);
    // A sibling is where it lands among others, known only when it runs.
    if siblings {
        worst.min(Fit::Later)
    } else {
        worst
    }
}

/// A context's compounds that are ancestors of the subject, and whether any are
/// siblings (of it or of an ancestor). Each compound's role is the combinator to
/// its right.
fn context_compounds<'a>(context: &'a str) -> (Vec<&'a str>, bool) {
    let mut ancestral = Vec::new();
    let mut siblings = false;
    let mut pending: Option<&str> = None;
    let mut combinator = None;
    let mut push = |compound: Option<&'a str>, combinator: Option<char>| {
        if let Some(compound) = compound {
            match combinator {
                Some('+' | '~') => siblings = true,
                _ => ancestral.push(compound),
            }
        }
    };
    let mut rest = context.trim();
    while !rest.is_empty() {
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '>' | '+' | '~'))
            .unwrap_or(rest.len());
        if end > 0 {
            push(pending.take(), combinator.take());
            pending = Some(&rest[..end]);
            combinator = Some(' ');
            rest = &rest[end..];
        } else {
            let c = rest.chars().next().unwrap_or(' ');
            if !c.is_whitespace() {
                combinator = Some(c);
            }
            rest = &rest[c.len_utf8()..];
        }
    }
    push(pending, combinator);
    (ancestral, siblings)
}

fn ancestors_of(
    doc: &Document,
    scripts: &scripts::Scripts,
    created: &eldur_js::CreatedElement,
    depth: u32,
    out: &mut Vec<Around>,
) {
    let analysis = &scripts.analysis;
    // Writes to an element the analysis can't name may land on any.
    let written = |target: &dyn Fn(&eldur_js::ElementRef) -> bool| {
        let mut names = Vec::new();
        let mut open = false;
        for w in &analysis.class_writes {
            if matches!(w.target, eldur_js::ElementRef::Unknown) || target(&w.target) {
                names.extend(w.names.iter().cloned());
                open |= w.open;
            }
        }
        (names, open)
    };
    if created.parents.is_empty() || depth > 16 {
        out.push(Around::Anything);
        return;
    }
    for parent in &created.parents {
        match parent {
            eldur_js::ElementRef::Unknown => out.push(Around::Anything),
            eldur_js::ElementRef::Created(id) => {
                let Some(made) = analysis.created.iter().find(|c| c.id == *id) else {
                    out.push(Around::Anything);
                    continue;
                };
                let (mut written, open) = written(&|t| t == parent);
                written.extend(made.class_sets.iter().flatten().cloned());
                let classes = made
                    .class_sets
                    .iter()
                    .map(|set| set.to_vec())
                    .reduce(|mut all, set| {
                        all.retain(|c| set.contains(c));
                        all
                    })
                    .unwrap_or_default();
                out.push(Around::Known {
                    tag: made.tag.clone(),
                    id: None,
                    classes,
                    written,
                    any_class: open || made.open_classes,
                });
                ancestors_of(doc, scripts, made, depth + 1, out);
            }
            eldur_js::ElementRef::Query(query) => {
                let nodes = scripts.matches(query);
                if nodes.is_empty() {
                    out.push(Around::Anything);
                }
                for &node in nodes {
                    for at in std::iter::once(node).chain(doc.ancestors(node)) {
                        let Some(element) = doc.element(at) else {
                            continue;
                        };
                        let (written, open) = written(&|t| match t {
                            eldur_js::ElementRef::Query(q) => scripts.matches(q).contains(&at),
                            _ => false,
                        });
                        out.push(Around::Known {
                            tag: element.local_name().to_owned(),
                            id: element.id().map(str::to_owned),
                            classes: element.classes().map(str::to_owned).collect(),
                            written,
                            any_class: open,
                        });
                    }
                }
            }
        }
    }
}

fn last_compound(selector: &str) -> &str {
    let mut depth = 0_u32;
    let mut start = 0;
    for (at, c) in selector.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ' ' | '>' | '+' | '~' if depth == 0 => start = at + c.len_utf8(),
            _ => {}
        }
    }
    &selector[start..]
}

/// A compound's type and required classes. What is inside `:not()` and the like
/// says nothing about what the element has to be.
fn compound_names(compound: &str) -> (String, Vec<String>) {
    let mut outer = String::new();
    let mut depth = 0_u32;
    for c in compound.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => outer.push(c),
            _ => {}
        }
    }
    let tag_end = outer.find(['.', '#', ':']).unwrap_or(outer.len());
    let classes = names_after(&outer, '.').map(str::to_owned).collect();
    (outer[..tag_end].to_owned(), classes)
}
