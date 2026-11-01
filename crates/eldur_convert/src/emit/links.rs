//! Links: a fragment (`href="#id"`) scrolls the page's `Flickable` to its
//! target, as a browser does; any other URL opens outside.

use eldur_dom::NodeId;
use eldur_qml::{Member, Object, Value};

use super::controls::js_string;
use super::units::ROOT_ID;
use super::{Emitter, FLICKABLE_ID};

impl Emitter<'_, '_> {
    /// Claims the QML id of an element with HTML id `html_id`, and
    /// remembers it as the target of `#html_id`.
    pub(super) fn claim_html_id(&mut self, html_id: &str) -> String {
        let id = self.ids.claim(html_id);
        self.fragment_target(html_id, &id);
        id
    }

    /// `#html_id` scrolls to the item `qml_id`; the first element with an
    /// id is the target, as in HTML.
    pub(super) fn fragment_target(&mut self, html_id: &str, qml_id: &str) {
        self.fragments
            .entry(html_id.to_owned())
            .or_insert_with(|| qml_id.to_owned());
    }

    /// The statement that follows the link `href`.
    pub(super) fn follow(&mut self, href: &str) -> String {
        if href.starts_with('#') {
            self.fragment_links = true;
            format!("{ROOT_ID}.scrollToFragment({})", js_string(href))
        } else {
            format!("Qt.openUrlExternally({})", js_string(href))
        }
    }

    /// A link with a box of its own has no text run to carry it, so the box does.
    pub(super) fn block_link(&mut self, obj: &mut Object, node: NodeId) {
        let Some(element) = self.doc.element(node) else {
            return;
        };
        if element.local_name() != "a" || obj.type_name == "Text" {
            return;
        }
        let Some(href) = element.attr("href").map(str::to_owned) else {
            return;
        };
        let follow = self.follow(&href);
        // This handler may hear the tap before the one a listener is on.
        let (params, body) = if self.runtime_dom {
            (
                vec!["eventPoint".to_owned()],
                vec![format!(
                    "Eldur.afterTap(eventPoint, function () {{ {follow} }});"
                )],
            )
        } else {
            (Vec::new(), vec![follow])
        };
        let mut tap = Object::new("TapHandler");
        tap.members.push(Member::Handler {
            name: "onTapped".to_owned(),
            params,
            body,
        });
        obj.push_child(tap);
        let mut hover = Object::new("HoverHandler");
        hover.set("cursorShape", Value::expr("Qt.PointingHandCursor"));
        obj.push_child(hover);
    }

    /// `onLinkActivated` for a `Text` with links, some to `fragments`.
    pub(super) fn link_handler(&mut self, text: &mut Object, fragments: bool) {
        let body = if fragments {
            self.fragment_links = true;
            vec![
                "if (link.startsWith(\"#\"))".to_owned(),
                format!("    {ROOT_ID}.scrollToFragment(link);"),
                "else".to_owned(),
                "    Qt.openUrlExternally(link);".to_owned(),
            ]
        } else {
            vec!["Qt.openUrlExternally(link)".to_owned()]
        };
        // `preventDefault` in a listener on the link, whose tap comes first.
        let body = if self.runtime_dom {
            let mut guarded = vec!["Eldur.afterClick(function () {".to_owned()];
            guarded.extend(body);
            guarded.push("});".to_owned());
            guarded
        } else {
            body
        };
        text.members.push(Member::Handler {
            name: "onLinkActivated".to_owned(),
            params: vec!["link".to_owned()],
            body,
        });
        // Browsers show a pointing hand over a link, and only there.
        let mut hover = Object::new("HoverHandler");
        hover.set(
            "cursorShape",
            Value::expr(
                "parent.linkAt(point.position.x, point.position.y) !== \"\" \
                 ? Qt.PointingHandCursor : Qt.ArrowCursor",
            ),
        );
        text.push_child(hover);
    }

    /// `root.scrollToFragment(link)`: the target's border-box top goes to
    /// the top of the view, clamped to the end of the page; `#` and `#top`
    /// go to the top, and an unknown fragment does nothing (HTML, "scroll
    /// to the fragment"; measured in Chromium). The fragment is looked up
    /// as written, then percent-decoded; a bad escape just fails to match.
    /// `targets` has no prototype, so `#constructor` is not a target.
    pub(super) fn scroll_to_fragment(&self, root: &mut Object) {
        let mut body = vec!["let eldurName = eldurLink.slice(1);".to_owned()];
        if self.fragments.is_empty() {
            body.push("const eldurTargets = Object.create(null);".to_owned());
        } else {
            body.push("const eldurTargets = Object.assign(Object.create(null), {".to_owned());
            let last = self.fragments.len() - 1;
            for (i, (html, qml)) in self.fragments.iter().enumerate() {
                let comma = if i == last { "" } else { "," };
                body.push(format!("    {}: {qml}{comma}", js_string(html)));
            }
            body.push("});".to_owned());
        }
        // Following a fragment moves the focus off what held it.
        body.push(format!("{FLICKABLE_ID}Input.forceActiveFocus();"));
        if self.runtime_dom {
            body.push("Eldur.fragmentFollowed(eldurLink);".to_owned());
        }
        body.push("if (!(eldurName in eldurTargets)) {".to_owned());
        body.push("    try {".to_owned());
        body.push("        eldurName = decodeURIComponent(eldurName);".to_owned());
        body.push("    } catch (e) {".to_owned());
        body.push("        eldurName = null;".to_owned());
        body.push("    }".to_owned());
        body.push("}".to_owned());
        body.push(
            "const eldurTarget = eldurName === null ? undefined : eldurTargets[eldurName];"
                .to_owned(),
        );
        body.push("if (eldurTarget) {".to_owned());
        // A target in a box that scrolls comes into that box's view first.
        body.push(format!(
            "    for (let eldurAt = eldurTarget.parent; eldurAt && eldurAt !== {FLICKABLE_ID}; eldurAt = eldurAt.parent) {{"
        ));
        body.push("        if (eldurAt instanceof Flickable)".to_owned());
        body.push(
            "            eldurAt.contentY = Math.max(0, Math.min(eldurTarget.mapToItem(eldurAt.contentItem, 0, 0).y, eldurAt.contentHeight - eldurAt.height));"
                .to_owned(),
        );
        body.push("    }".to_owned());
        body.push(format!(
            "    const eldurY = eldurTarget.mapToItem({FLICKABLE_ID}.contentItem, 0, 0).y;"
        ));
        body.push(format!(
            "    {FLICKABLE_ID}.contentY = Math.max(0, Math.min(eldurY, {FLICKABLE_ID}.contentHeight - {FLICKABLE_ID}.height));"
        ));
        body.push(
            "} else if (eldurName !== null && (eldurName === \"\" || eldurName.toLowerCase() === \"top\")) {"
                .to_owned(),
        );
        body.push(format!("    {FLICKABLE_ID}.contentY = 0;"));
        body.push("}".to_owned());
        root.members.push(Member::Function {
            name: "scrollToFragment".to_owned(),
            params: vec!["eldurLink".to_owned()],
            body,
        });
    }
}
