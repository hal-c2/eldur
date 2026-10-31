// What a step's state is, in both browsers: this function runs in Chromium for
// `gen.mjs` and inside the QML test for `apps_e2e.rs`, against each one's DOM.
// `pixel(x, y)` says what is painted there, for `paint` watches: a cell's colour
// is often its row's, which no computed style of the cell tells.
function eldurSnapshot(doc, styleOf, watch, pixel) {
    function clean(s) { return String(s === null || s === undefined ? "" : s).replace(/\s+/g, " ").trim(); }
    function two(n) { return Math.round(n * 100) / 100; }
    function shown(e) {
        const r = e.getBoundingClientRect();
        if (r.width <= 0 && r.height <= 0) return false;
        for (let a = e; a; a = a.parentElement) {
            if (styleOf(a).display === "none") return false;
        }
        return styleOf(e).visibility !== "hidden";
    }
    const out = {};
    for (const w of watch) {
        const list = doc.querySelectorAll(w.sel);
        const rows = [];
        for (let i = 0; i < list.length; i++) {
            const e = list[i];
            const r = e.getBoundingClientRect();
            const row = { box: [two(r.left), two(r.top), two(r.width), two(r.height)], shown: shown(e), text: clean(e.textContent) };
            const tag = String(e.tagName).toLowerCase();
            if (tag === "input" || tag === "textarea" || tag === "select") {
                row.text = "";
                row.value = String(e.value);
                if (e.type === "checkbox" || e.type === "radio") row.checked = Boolean(e.checked);
            }
            if (w.paint && row.shown) {
                // Inside the corner of the padding: past the border, short of the text.
                row.paint = pixel(Math.round(r.left) + 2, Math.round(r.top) + 2);
            }
            if (w.props) {
                const s = styleOf(e);
                row.props = {};
                for (const p of w.props) row.props[p] = s.getPropertyValue(p);
            }
            if (w.attrs) {
                row.attrs = {};
                for (const a of w.attrs) row.attrs[a] = e.hasAttribute(a) ? e.getAttribute(a) : null;
            }
            rows.push(row);
        }
        out[w.sel] = rows;
    }
    const active = doc.activeElement;
    out["(focus)"] = !active || !active.tagName || String(active.tagName).toLowerCase() === "body" ? "" : String(active.id || active.tagName);
    return out;
}
