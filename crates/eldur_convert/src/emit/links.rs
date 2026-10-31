//! Links: a fragment (`href="#id"`) scrolls the page's `Flickable` to its
//! target, as a browser does; any other URL opens outside.

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
        let mut body = vec!["let name = link.slice(1);".to_owned()];
        if self.fragments.is_empty() {
            body.push("const targets = Object.create(null);".to_owned());
        } else {
            body.push("const targets = Object.assign(Object.create(null), {".to_owned());
            let last = self.fragments.len() - 1;
            for (i, (html, qml)) in self.fragments.iter().enumerate() {
                let comma = if i == last { "" } else { "," };
                body.push(format!("    {}: {qml}{comma}", js_string(html)));
            }
            body.push("});".to_owned());
        }
        body.push("if (!(name in targets)) {".to_owned());
        body.push("    try {".to_owned());
        body.push("        name = decodeURIComponent(name);".to_owned());
        body.push("    } catch (e) {".to_owned());
        body.push("        name = null;".to_owned());
        body.push("    }".to_owned());
        body.push("}".to_owned());
        body.push("const target = name === null ? undefined : targets[name];".to_owned());
        body.push("if (target) {".to_owned());
        body.push(format!(
            "    const y = target.mapToItem({FLICKABLE_ID}.contentItem, 0, 0).y;"
        ));
        body.push(format!(
            "    {FLICKABLE_ID}.contentY = Math.max(0, Math.min(y, {FLICKABLE_ID}.contentHeight - {FLICKABLE_ID}.height));"
        ));
        body.push(
            "} else if (name !== null && (name === \"\" || name.toLowerCase() === \"top\")) {"
                .to_owned(),
        );
        body.push(format!("    {FLICKABLE_ID}.contentY = 0;"));
        body.push("}".to_owned());
        root.members.push(Member::Function {
            name: "scrollToFragment".to_owned(),
            params: vec!["link".to_owned()],
            body,
        });
    }
}
