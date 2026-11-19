//! An inline `<svg>` is written out as an asset and shown with an `Image`,
//! as `<img src="x.svg">` is (#5). `fill="currentColor"` takes the colour of
//! the element that holds the svg, and follows it when `:hover` changes it.
//! An `aria-hidden` icon is drawn but not read, and a link whose only visible
//! content is an icon is still a link: clicking the icon follows it, and its
//! name is its visually hidden text. Boxes, pixels, names and scroll positions
//! are run in qmltestrunner against Chromium's at 1280×800
//! (`getBoundingClientRect`, screenshots, the CDP accessibility tree).

mod support;

use std::path::Path;

use eldur_convert::{Options, RootKind, convert_str};

/// The issue's repro.
const RED: &str = r##"<!DOCTYPE html><html><body style="margin:0">
<svg width="64" height="64" viewBox="0 0 24 24" fill="#c00"><path d="M12 2 22 22H2z"/></svg>
</body></html>"##;

/// The issue comment's links: an icon-only link named by visually hidden
/// text, and text followed by an arrow icon.
const LINKS: &str = r##"<!DOCTYPE html><html><body style="margin:0;font:16px sans-serif;color:#222">
<style>.sr-only{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap}</style>
<a href="https://example.com/repo"><span class="sr-only">Source code</span><svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2 22 22H2z"/></svg></a>
<a href="#next">What is this? <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M4 11h12l-5-5 1.4-1.4L20 12l-7.6 7.4L11 18l5-5H4z"/></svg></a>
</body></html>"##;

/// The same icon in a link that changes colour on hover, and a link that is
/// a flex container.
const HOVER: &str = r##"<!DOCTYPE html><html><body style="margin:0;font:16px sans-serif;color:#222">
<style>.sr-only{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap}a:hover{color:#c00}</style>
<a href="#next"><span class="sr-only">Source code</span><svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2 22 22H2z"/></svg></a>
<a href="#next" style="display:flex;width:200px">Flex link <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M4 11h12l-5-5 1.4-1.4L20 12l-7.6 7.4L11 18l5-5H4z"/></svg></a>
<div style="height:2000px"></div><p id="next">next</p>
</body></html>"##;

/// An icon sprite: a `<use>` of a `<symbol>` kept in a hidden svg, an icon
/// filled with a gradient kept in a 0×0 svg, and an editor's prefixed
/// attributes, which HTML leaves unbound.
const SPRITE: &str = r##"<!DOCTYPE html><html><head><style>.icon{width:48px;height:48px}</style></head><body style="margin:0;color:#c00">
<svg style="display:none"><symbol id="square" viewBox="0 0 24 24"><rect x="2" y="2" width="20" height="20" fill="currentColor"/></symbol></svg><svg width="0" height="0" style="position:absolute"><linearGradient id="blue"><stop offset="0" stop-color="#00c"/><stop offset="1" stop-color="#00c"/></linearGradient></svg><svg width="48" height="48" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" inkscape:label="sprite" sodipodi:docname="x"><use href="#square"/></svg><svg width="48" height="48" viewBox="0 0 24 24"><rect width="24" height="24" fill="url(#blue)"/></svg><svg class="icon"><use href="#square"/></svg>
</body></html>"##;

/// What every test file shares: the page, its svg `Image`s in document
/// order, the boxes of items in page coordinates, and the links QML exposes
/// to assistive technology.
const HELPERS: &str = r##"
    property var page
    property var flick

    function init() {
        const component = Qt.createComponent("Main.qml")
        verify(component.status === Component.Ready, component.errorString())
        page = createTemporaryObject(component, host, { width: 1280, height: 800 })
        waitForRendering(page)
        for (const child of page.children)
            if (child.contentY !== undefined)
                flick = child
    }
    function walk(item, out) {
        out.push(item)
        for (const child of item.children)
            walk(child, out)
        return out
    }
    function icons() {
        return walk(page, []).filter(i => i.source !== undefined
                                       && String(i.source).endsWith(".svg") && i.visible)
    }
    function box(item) {
        const p = item.mapToItem(page, 0, 0)
        return [p.x, p.y, item.width, item.height]
    }
    // The item that is the icon's link: the icon, or a box around it.
    function carrier(item) {
        for (let at = item; at && at !== page; at = at.parent)
            if (at.Accessible.role === Accessible.Link)
                return at
        return null
    }
    function links() {
        return walk(page, []).filter(i => i.Accessible.role === Accessible.Link)
                             .map(i => i.Accessible.name)
    }
    // Inside the shape, away from its anti-aliased edge.
    function colourAt(item, x, y) {
        const p = item.mapToItem(page, x, y)
        const c = grabImage(page).pixel(Math.round(p.x), Math.round(p.y))
        return [Math.round(c.r * 255), Math.round(c.g * 255), Math.round(c.b * 255)]
    }
"##;

const RED_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineSvg"
    when: windowShown
HELPERS
    function test_a_64px_red_triangle_at_the_top_left() {
        const shown = icons()
        compare(shown.length, 1)
        compare(box(shown[0]), [0, 0, 64, 64])
        // Chromium: (32, 48) is #cc0000, the corners are the page's white.
        compare(colourAt(page, 32, 48), [204, 0, 0])
        compare(colourAt(page, 4, 4), [255, 255, 255])
        compare(colourAt(page, 60, 4), [255, 255, 255])
    }
}
}
"##;

const SPRITE_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "InlineSvgSprite"
    when: windowShown
HELPERS
    function test_a_sprite_and_an_outside_gradient_draw() {
        // The 0×0 svg that only holds the gradient draws nothing.
        const shown = icons().filter(i => i.width > 0)
        compare(shown.length, 3)
        compare(box(shown[0]), [0, 0, 48, 48])
        compare(box(shown[1]), [48, 0, 48, 48])
        compare(box(shown[2]), [96, 0, 48, 48])
        // Chromium: the symbol's square in the body's #cc0000, scaled to
        // the svg, the gradient's #0000cc, and white around the square and
        // past them all. The third icon is sized by CSS alone and draws the
        // symbol as the first does.
        compare(colourAt(page, 24, 24), [204, 0, 0])
        compare(colourAt(page, 42, 42), [204, 0, 0])
        compare(colourAt(page, 2, 2), [255, 255, 255])
        compare(colourAt(page, 46, 46), [255, 255, 255])
        compare(colourAt(page, 72, 24), [0, 0, 204])
        compare(colourAt(page, 120, 24), [204, 0, 0])
        compare(colourAt(page, 98, 2), [255, 255, 255])
        compare(colourAt(page, 142, 46), [255, 255, 255])
        compare(colourAt(page, 160, 24), [255, 255, 255])
    }
}
}
"##;

const LINKS_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "IconLinks"
    when: windowShown
HELPERS
    function test_icons_sit_where_chromium_has_them() {
        const shown = icons()
        compare(shown.length, 2)
        compare(box(shown[0]), [0, 0, 16, 16])
        // After "What is this? " in the platform's sans-serif: 116.48 in
        // Chromium; the two engines shape the text a little differently.
        const arrow = box(shown[1])
        fuzzyCompare(arrow[0], 116.48, 3)
        compare([arrow[1], arrow[2], arrow[3]], [0, 16, 16])
    }

    function test_current_color_is_the_links_colour() {
        const shown = icons()
        compare(colourAt(shown[0], 8, 12), [0, 0, 238])
        // In the arrow's head: its shaft is 1.33px tall, all edge.
        compare(colourAt(shown[1], 11, 8), [0, 0, 238])
    }

    function test_hidden_icons_are_read_only_as_their_links() {
        for (const icon of icons()) {
            const link = carrier(icon)
            verify(link, String(icon.source))
            // An ignored item hides the link it carries.
            verify(!link.Accessible.ignored, String(icon.source))
            if (link !== icon)
                verify(icon.Accessible.ignored, String(icon.source))
        }
    }

    function test_each_link_is_named_as_chromium_names_it() {
        compare(links(), ["Source code", "What is this?"])
    }
}
}
"##;

const HOVER_TEST: &str = r##"
import QtQuick
import QtTest

Item {
    id: host
    width: 1280
    height: 800

TestCase {
    name: "IconHover"
    when: windowShown
HELPERS
    function test_the_icon_repaints_when_its_link_is_hovered() {
        const icon = icons()[0]
        const arrow = icons()[1]
        compare(colourAt(icon, 8, 12), [0, 0, 238])
        const p = icon.mapToItem(page, 8, 12)
        mouseMove(page, p.x, p.y)
        waitForRendering(page)
        // Chromium: the hovered link's icon is #cc0000; the other link's
        // stays blue.
        tryVerify(() => colourAt(icon, 8, 12)[0] === 204, 2000)
        compare(colourAt(icon, 8, 12), [204, 0, 0])
        compare(colourAt(arrow, 11, 8), [0, 0, 238])
        mouseMove(page, 1279, 799)
        tryVerify(() => colourAt(icon, 8, 12)[2] === 238, 2000)
    }

    function test_clicking_an_icon_only_link_follows_it() {
        verify(flick)
        const icon = icons()[0]
        const p = icon.mapToItem(page, 8, 12)
        mouseClick(page, p.x, p.y)
        // Chromium: scrollY 1288 after the click.
        tryCompare(flick, "contentY", 1288, 2000)
    }

    function test_clicking_the_icon_of_a_flex_link_follows_it() {
        verify(flick)
        const arrow = icons()[1]
        // Chromium: at (58.69, 20), after "Flex link ".
        const at = box(arrow)
        fuzzyCompare(at[0], 58.69, 3)
        fuzzyCompare(at[1], 20, 0.01)
        // Its size attributes are its height: the row doesn't stretch it.
        compare([at[2], at[3]], [16, 16])
        const p = arrow.mapToItem(page, 11, 8)
        mouseClick(page, p.x, p.y)
        tryCompare(flick, "contentY", 1288, 2000)
    }

    function test_links_are_named_as_chromium_names_them() {
        compare(links(), ["Source code", "Flex link"])
    }

    function test_return_on_an_icon_only_link_follows_it() {
        verify(flick)
        // As in a browser, Return on the focused link follows it, to the
        // scrollY 1288 a click there gives.
        const link = carrier(icons()[0])
        verify(link.activeFocusOnTab)
        link.forceActiveFocus()
        keyClick(Qt.Key_Return)
        tryCompare(flick, "contentY", 1288, 2000)
    }
}
}
"##;

fn convert(page: &str) -> eldur_convert::Output {
    let options = Options {
        root: RootKind::Item,
        ..Options::default()
    };
    convert_str(page, &options)
}

fn svg_assets(out: &eldur_convert::Output) -> Vec<String> {
    out.files
        .iter()
        .filter(|f| f.path.starts_with("assets") && f.path.extension().is_some_and(|e| e == "svg"))
        .map(|f| String::from_utf8(f.contents.clone()).expect("svg is text"))
        .collect()
}

#[test]
fn an_inline_svg_is_an_asset_image() {
    let out = convert(RED);
    let assets = svg_assets(&out);
    assert_eq!(assets.len(), 1, "{:?}", out.files);
    let svg = &assets[0];
    assert!(
        svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
        "{svg}"
    );
    // Attribute names keep their case: Qt's loader ignores `viewbox`.
    assert!(svg.contains("viewBox=\"0 0 24 24\""), "{svg}");
    assert!(svg.contains("<path d=\"M12 2 22 22H2z\"/>"), "{svg}");
    let qml = out.main_qml().expect("Main.qml");
    assert!(qml.contains("source: \"assets/svg-"), "{qml}");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("not converted yet")),
        "{:?}",
        out.diagnostics
    );
}

#[test]
fn current_color_is_the_holders_colour() {
    let out = convert(LINKS);
    let assets = svg_assets(&out);
    // Two icons, both in the links' blue.
    assert_eq!(assets.len(), 2, "{assets:?}");
    for svg in &assets {
        assert!(!svg.to_ascii_lowercase().contains("currentcolor"), "{svg}");
        assert!(svg.contains("fill=\"#0000ee\""), "{svg}");
    }
}

#[test]
fn a_hover_colour_is_another_asset() {
    let out = convert(HOVER);
    let assets = svg_assets(&out);
    assert!(
        assets.iter().any(|s| s.contains("fill=\"#0000ee\"")),
        "{assets:?}"
    );
    assert!(
        assets.iter().any(|s| s.contains("fill=\"#cc0000\"")),
        "{assets:?}"
    );
}

#[test]
fn inline_svg_draws_like_chromium() {
    run("red", RED, RED_TEST);
}

#[test]
fn a_sprite_asset_holds_what_it_uses() {
    let out = convert(SPRITE);
    let assets = svg_assets(&out);
    let sprite = assets
        .iter()
        .find(|s| s.contains("<use href=\"#square\"/>"))
        .expect("the sprite's asset");
    assert!(sprite.contains("<defs><symbol id=\"square\""), "{sprite}");
    assert!(sprite.contains("fill=\"#cc0000\""), "{sprite}");
    assert!(!sprite.contains("inkscape"), "{sprite}");
    assert!(!sprite.contains("sodipodi"), "{sprite}");
    let filled = assets
        .iter()
        .find(|s| s.contains("fill=\"url(#blue)\""))
        .expect("the gradient icon's asset");
    assert!(
        filled.contains("<defs><linearGradient id=\"blue\">"),
        "{filled}"
    );
}

#[test]
fn a_sprite_draws_like_chromium() {
    run("sprite", SPRITE, SPRITE_TEST);
}

#[test]
fn icon_links_read_and_paint_like_chromium() {
    run("links", LINKS, LINKS_TEST);
}

#[test]
fn icon_links_hover_and_click_like_chromium() {
    run("hover", HOVER, HOVER_TEST);
}

fn run(name: &str, page: &str, test: &str) {
    if support::no_qt() {
        return;
    }
    let out = convert(page);
    let dir = std::env::temp_dir().join(format!("eldur-inline-svg-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    write(&dir, &out);
    let test = test.replace("HELPERS", HELPERS);
    std::fs::write(dir.join("tst_inline_svg.qml"), test).expect("write test");
    let output = support::qmltestrunner()
        .args(["-platform", "offscreen", "-input"])
        .arg(dir.join("tst_inline_svg.qml"))
        .env("QT_FORCE_STDERR_LOGGING", "1")
        .output()
        .expect("run qmltestrunner");
    let text = format!(
        "{}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        out.main_qml().unwrap_or_default()
    );
    std::fs::remove_dir_all(&dir).expect("remove temp dir");
    assert!(output.status.success(), "{text}");
}

fn write(dir: &Path, out: &eldur_convert::Output) {
    for file in &out.files {
        let path = dir.join(&file.path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("asset dir");
        std::fs::write(path, &file.contents).expect("write output");
    }
}
