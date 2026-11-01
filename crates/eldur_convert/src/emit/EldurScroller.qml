import QtQuick
import QtQuick.Controls.Basic

// A box with `overflow: auto` or `scroll`. The bars float over the content and
// take no layout width, as they do in the browser the pages are compared with;
// Basic shows them while the box scrolls.
Flickable {
    id: scroller

    // Whether an axis's overflow can be scrolled by the user.
    property bool scrollsX: true
    property bool scrollsY: true

    boundsBehavior: Flickable.StopAtBounds
    clip: true
    flickableDirection: scrollsX && scrollsY ? Flickable.AutoFlickIfNeeded
        : scrollsX ? Flickable.HorizontalFlick : Flickable.VerticalFlick
    ScrollBar.vertical: ScrollBar {
        enabled: scroller.scrollsY
    }
    ScrollBar.horizontal: ScrollBar {
        enabled: scroller.scrollsX
    }

    EldurScrollInput {
        scroller: scroller
    }
}
