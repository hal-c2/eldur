import QtQuick

// What moves a scroller besides a drag. A notch moves it 60px, whereas
// Flickable's own wheel flings; a wheel it cannot follow goes to the scroller
// around it, and a click makes it the one the keys move.
Item {
    id: input

    required property Flickable scroller
    // Where keys come through: the page's own takes those of fixed boxes too,
    // which sit beside it.
    property Item keySource: scroller
    // A box whose children this scroller moves without holding them.
    property Item overlaidOn: null

    function moveTo(x, y) {
        const nextX = Math.max(0, Math.min(x, Math.max(0, scroller.contentWidth - scroller.width)));
        const nextY = Math.max(0, Math.min(y, Math.max(0, scroller.contentHeight - scroller.height)));
        const moved = nextX !== scroller.contentX || nextY !== scroller.contentY;
        scroller.contentX = nextX;
        scroller.contentY = nextY;
        return moved;
    }

    // The steps Chromium takes for each key: 40px an arrow, and a page less the
    // 40px it keeps in sight.
    function keyStep(key) {
        const pageY = Math.max(scroller.height - 40, 40);
        let x = scroller.contentX;
        let y = scroller.contentY;
        if (key === Qt.Key_Down) y += 40;
        else if (key === Qt.Key_Up) y -= 40;
        else if (key === Qt.Key_PageDown || key === Qt.Key_Space) y += pageY;
        else if (key === Qt.Key_PageUp) y -= pageY;
        else if (key === Qt.Key_Home) y = 0;
        else if (key === Qt.Key_End) y = scroller.contentHeight;
        else if (key === Qt.Key_Right) x += 40;
        else if (key === Qt.Key_Left) x -= 40;
        else return false;
        moveTo(x, y);
        return true;
    }

    // A click on a scroller inside belongs to that one, and one on a control
    // leaves the focus the control took.
    function holdsInner(point) {
        const held = input.Window.activeFocusItem;
        for (let at = held; at && held !== input && !(held instanceof EldurScrollInput); at = at.parent) {
            if (at === scroller) return true;
        }
        let at = scroller.contentItem;
        let p = scroller.mapToItem(at, point.x, point.y);
        for (;;) {
            const kid = at.childAt(p.x, p.y);
            if (!kid) return false;
            if (kid instanceof Flickable) return true;
            p = at.mapToItem(kid, p.x, p.y);
            at = kid;
        }
    }

    // On the scroller, because a key from a control inside bubbles up to it and
    // not to this sibling of the content.
    Connections {
        target: input.keySource.Keys
        function onPressed(event) {
            event.accepted = input.scroller.interactive && input.keyStep(event.key);
        }
    }

    // A focused box comes into view: not moved if it shows, to the nearest edge
    // if part does, centred if none does, as Chromium does.
    Connections {
        target: input.Window.window
        function onActiveFocusItemChanged() {
            // Dropped with its scroller: a created box can stop scrolling.
            if (!input) return;
            const item = input.Window.activeFocusItem;
            if (item && item !== input) input.reveal(item);
        }
    }

    function reveal(item) {
        let inside = false;
        for (let at = item; at && !inside; at = at.parent) {
            inside = at === scroller || at === overlaidOn;
        }
        if (!inside) return;
        const box = item.mapToItem(scroller, 0, 0);
        moveTo(scroller.contentX + along(box.x, item.width, scroller.width),
               scroller.contentY + along(box.y, item.height, scroller.height));
    }

    function along(start, size, port) {
        const end = start + size;
        if (start >= 0 && end <= port) return 0;
        if (end <= 0 || start >= port) return start - (port - size) / 2;
        return Math.abs(start) < Math.abs(end - port) ? start : end - port;
    }

    WheelHandler {
        parent: input.scroller
        target: null
        onWheel: (event) => {
            const dx = event.pixelDelta.x || event.angleDelta.x / 2;
            const dy = event.pixelDelta.y || event.angleDelta.y / 2;
            event.accepted = input.scroller.interactive
                && input.moveTo(input.scroller.contentX - dx, input.scroller.contentY - dy);
        }
    }

    TapHandler {
        parent: input.scroller
        gesturePolicy: TapHandler.DragThreshold
        onTapped: (eventPoint) => {
            if (!input.holdsInner(eventPoint.position)) input.forceActiveFocus();
        }
    }
}
