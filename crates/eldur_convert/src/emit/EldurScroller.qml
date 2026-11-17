import QtQuick
import QtQuick.Controls.Basic

// A box with `overflow: auto` or `scroll`. The bars float over the content and
// take no layout width, as they do in the browser the pages are compared with;
// Basic shows them while the box scrolls.
Flickable {
    id: scrollBox

    // Whether an axis's overflow can be scrolled by the user.
    property bool eldurSmooth: false
    property bool scrollsX: true
    property bool scrollsY: true
    // Whether the user can scroll at all, or only scripts can.
    property bool userScrolls: true
    // A created box's scroller lies under the box's children, which stay the
    // box's: its bars go above them, on the box.
    property Item overlaidOn: null
    readonly property bool eldurChrome: true
    // The box's padding, which position: sticky keeps clear of.
    property real stickPadTop
    property real stickPadRight
    property real stickPadBottom
    property real stickPadLeft

    boundsBehavior: Flickable.StopAtBounds
    clip: true
    interactive: userScrolls
    flickableDirection: scrollsX && scrollsY ? Flickable.AutoFlickIfNeeded
        : scrollsX ? Flickable.HorizontalFlick : Flickable.VerticalFlick
    ScrollBar.vertical: ScrollBar {
        readonly property bool eldurChrome: true
        enabled: scrollBox.scrollsY
        visible: scrollBox.userScrolls
        parent: scrollBox.overlaidOn ?? scrollBox
        z: scrollBox.overlaidOn ? 1000 : 0
        x: scrollBox.overlaidOn ? scrollBox.x + scrollBox.width - width : 0
        y: scrollBox.overlaidOn ? scrollBox.y : 0
        height: scrollBox.height
    }
    ScrollBar.horizontal: ScrollBar {
        readonly property bool eldurChrome: true
        enabled: scrollBox.scrollsX
        visible: scrollBox.userScrolls
        parent: scrollBox.overlaidOn ?? scrollBox
        z: scrollBox.overlaidOn ? 1000 : 0
        x: scrollBox.overlaidOn ? scrollBox.x : 0
        y: scrollBox.overlaidOn ? scrollBox.y + scrollBox.height - height : 0
        width: scrollBox.width
    }

    EldurScrollInput {
        scroller: scrollBox
        enabled: scrollBox.userScrolls
        overlaidOn: scrollBox.overlaidOn
    }
}
