//! `Options::animations`: what the visual harness converts with, so a captured
//! frame is never one caught part way through.

use eldur_convert::{Options, convert_str};

const CLOSED: &str = r"<!DOCTYPE html><html><head><style>
@keyframes f{from{opacity:0}to{opacity:1}}
#a{height:20px;background:#000;animation:f 1s;transition:opacity 1s}
#a:hover{opacity:.5}
</style></head><body><div id=a></div></body></html>";

const OPEN: &str = r#"<!DOCTYPE html><html><head><style>
@keyframes f{from{opacity:0}to{opacity:1}}
.a{height:20px;background:#000;animation:f 1s}
</style></head><body><div class="a"></div><script>
document.documentElement.classList.add("ready");
</script></body></html>"#;

fn main_qml(html: &str, animations: bool) -> String {
    let options = Options {
        animations,
        ..Options::default()
    };
    convert_str(html, &options)
        .main_qml()
        .expect("Main.qml is always produced")
        .to_owned()
}

#[test]
fn closed_page_runs_its_animation_unless_told_not_to() {
    let moving = main_qml(CLOSED, true);
    assert!(moving.contains("SequentialAnimation"), "{moving}");
    assert!(moving.contains("Behavior on"), "{moving}");
    let still = main_qml(CLOSED, false);
    assert!(!still.contains("SequentialAnimation"), "{still}");
    assert!(!still.contains("Behavior on"), "{still}");
}

#[test]
fn open_page_tells_its_runtime_not_to_move() {
    let moving = main_qml(OPEN, true);
    assert!(
        moving.contains("eldurOpenElements"),
        "the page must be open"
    );
    assert!(moving.contains("eldurKeyframes"));
    assert!(!moving.contains("eldurNoMotion"));
    let still = main_qml(OPEN, false);
    assert!(still.contains("eldurNoMotion"));
    assert!(!still.contains("eldurKeyframes"));
}

#[test]
fn scroll_chrome_that_does_not_scroll_says_so() {
    let page = r"<!DOCTYPE html><html><head><style>
#a{overflow:auto;height:20px;overscroll-behavior:contain;scroll-snap-type:y mandatory}
</style></head><body><div id=a>x</div></body></html>";
    let output = convert_str(page, &Options::default());
    let said: Vec<String> = output.diagnostics.iter().map(ToString::to_string).collect();
    assert!(
        said.iter().any(|d| d.contains("overscroll-behavior")),
        "{said:?}"
    );
    assert!(said.iter().any(|d| d.contains("scroll-snap")), "{said:?}");
}
