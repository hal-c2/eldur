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
    // A table cell moves within its table, not within its row.
    property bool cell: false

    readonly property Flickable scroller: {
        for (let at = target.parent; at; at = at.parent) {
            if (at.eldurScrollPort) return at.eldurScrollPort;
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

    // A created box's children are its own and not its scroller's content, so a
    // child's offset doesn't count the scroll, unlike a converted box's.
    function held(block) {
        return block.eldurScrollPort === scroller;
    }

    // Chromium measures the insets from inside the scroller's padding.
    function padded(inset, name) {
        return inset === undefined ? undefined : inset + (scroller[name] || 0);
    }

    // The column of the table's rows and sections, around a cell's row.
    function limit(block) {
        if (!cell || !block.parent) return block;
        let table = block.parent;
        while (table.parent && table.parent.spacing !== undefined) table = table.parent;
        return table;
    }

    function blockEnd(block, along) {
        if (cell && block.parent) return limit(block)[along];
        const box = block.parent;
        // Its children stay inside all it scrolls, not just what shows.
        if (held(block)) return along === "height" ? scroller.contentHeight : scroller.contentWidth;
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
        const lift = held(block) ? port.contentX : 0;
        const at = block.mapToItem(port.contentItem, target.x, target.y).x - lift;
        const from = limit(block).mapToItem(port.contentItem, 0, 0).x - lift;
        return shift(padded(stickLeft, "stickPadLeft"), padded(stickRight, "stickPadRight"), at, target.width, port.contentX, port.width + 0 * port.contentWidth,
                     from, from + blockEnd(block, "width"), marginLeft, marginRight);
    }
    y: {
        const port = scroller;
        const block = target.parent;
        if (!port || !block || (stickTop === undefined && stickBottom === undefined)) return 0;
        const lift = held(block) ? port.contentY : 0;
        const at = block.mapToItem(port.contentItem, target.x, target.y).y - lift;
        const from = limit(block).mapToItem(port.contentItem, 0, 0).y - lift;
        return shift(padded(stickTop, "stickPadTop"), padded(stickBottom, "stickPadBottom"), at, target.height, port.contentY, port.height + 0 * port.contentHeight,
                     from, from + blockEnd(block, "height"), marginTop, marginBottom);
    }
}
