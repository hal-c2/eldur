import QtQuick

// `position: sticky`: the move that keeps a box inside its nearest scroller,
// by its insets, without leaving its containing block. Worked out from where
// layout put the box, which this transform doesn't change.
Translate {
    id: sticky

    required property Item target
    property var stickTop
    property var stickRight
    property var stickBottom
    property var stickLeft
    property real marginTop
    property real marginRight
    property real marginBottom
    property real marginLeft

    readonly property Flickable scroller: {
        for (let at = target.parent; at; at = at.parent) {
            if (at instanceof Flickable) return at;
        }
        return null;
    }

    function shift(near, far, start, size, viewStart, viewSize, blockStart, blockEnd, marginNear, marginFar) {
        let by = 0;
        if (far !== undefined) {
            const over = viewStart + viewSize - far - start - size;
            if (over < 0) by = Math.max(over, Math.min(0, blockStart + marginNear - start));
        }
        if (near !== undefined) {
            const under = viewStart + near - start;
            if (under > by) by = Math.min(under, Math.max(by, blockEnd - marginFar - start - size));
        }
        return by;
    }

    function blockEnd(block, along) {
        const box = block.parent;
        // A block column is as tall as its content, and the box round it may be taller.
        if (along === "height" && box && box.eldurInsetBottom !== undefined) {
            return box.height - box.eldurInsetBottom - block.y;
        }
        return block[along];
    }

    x: {
        const port = scroller;
        const block = target.parent;
        if (!port || !block || (stickLeft === undefined && stickRight === undefined)) return 0;
        const at = block.mapToItem(port.contentItem, target.x, target.y).x;
        const from = block.mapToItem(port.contentItem, 0, 0).x;
        return shift(stickLeft, stickRight, at, target.width, port.contentX, port.width + 0 * port.contentWidth,
                     from, from + blockEnd(block, "width"), marginLeft, marginRight);
    }
    y: {
        const port = scroller;
        const block = target.parent;
        if (!port || !block || (stickTop === undefined && stickBottom === undefined)) return 0;
        const at = block.mapToItem(port.contentItem, target.x, target.y).y;
        const from = block.mapToItem(port.contentItem, 0, 0).y;
        return shift(stickTop, stickBottom, at, target.height, port.contentY, port.height + 0 * port.contentHeight,
                     from, from + blockEnd(block, "height"), marginTop, marginBottom);
    }
}
