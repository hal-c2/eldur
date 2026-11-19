import QtQuick

// A `Flow` whose lines are `rowSpacing` apart while the items on a line are
// `spacing` apart: a wrapping flex container keeps its row gap and its
// column gap apart (CSS Box Alignment 3 §8), and `Flow` has one spacing for
// both. The items stay this item's own children, so the runtime's DOM sees
// them as the box's, and it knows a layout by its `spacing`.
//
// Lines break as `Flow`'s do (measured in qml6): an item that would end past
// the width starts a new line unless it is the line's first, an item that is
// hidden or has no width or height takes no place, and with no width the
// items stay on one line. `implicitWidth` is the widest line and
// `implicitHeight` the last line's bottom; both are only defaults, so a box
// with a set width keeps it.
Item {
    id: eldurFlow

    property real spacing: 0
    property real rowSpacing: 0
    property int layoutDirection: Qt.LeftToRight
    property real eldurLinesWidth: 0
    property real eldurLinesHeight: 0
    property var eldurWatched: []
    property bool eldurPlaced: false
    property bool eldurProbing: false
    // Tells a line of inline boxes this lays out a flex row's lines.
    readonly property bool eldurFlowLines: true

    implicitWidth: eldurLinesWidth
    implicitHeight: eldurLinesHeight

    // One pass for however many sizes change before the next frame.
    function eldurQueue() {
        Qt.callLater(eldurPlace);
    }

    function eldurWatch() {
        const kids = Array.from(children);
        // Sets, so a box built one child at a time stays linear per pass.
        const now = new Set(kids);
        const before = new Set(eldurWatched);
        for (const old of eldurWatched) {
            // A destroyed item has no signals left to leave.
            if (!now.has(old) && old && old.widthChanged) {
                old.widthChanged.disconnect(eldurQueue);
                old.heightChanged.disconnect(eldurQueue);
                old.visibleChanged.disconnect(eldurQueue);
            }
        }
        for (const kid of kids) {
            if (!before.has(kid)) {
                kid.widthChanged.connect(eldurQueue);
                kid.heightChanged.connect(eldurQueue);
                kid.visibleChanged.connect(eldurQueue);
            }
        }
        eldurWatched = kids;
        // Placed now, so whoever lays this box out next sees its lines.
        eldurPlace();
    }

    // The width the lines break at. A width nobody set follows
    // `implicitWidth`, the last pass's widest line, and must not bound the
    // next pass: `Flow` keeps its items on one line then. QML cannot ask
    // whether a width was set, so move `implicitWidth` by one and see
    // whether the width moves with it.
    function eldurBound() {
        if (width <= 0) {
            return Infinity;
        }
        if (width !== implicitWidth) {
            return width;
        }
        const set = width;
        eldurProbing = true;
        eldurLinesWidth = set + 1;
        const follows = width !== set;
        eldurLinesWidth = set;
        eldurProbing = false;
        return follows ? Infinity : set;
    }

    function eldurPlace() {
        // `visible` is the effective one, so while this box is hidden every
        // kid reads hidden and a kid hidden on its own cannot be told apart.
        // A hidden box keeps the lines it last had, as `Flow` keeps a hidden
        // kid out of them; shown again, it is placed at once.
        if (!visible && eldurPlaced) {
            return;
        }
        // Unbounded without a width, as `Flow` is.
        const bound = eldurBound();
        const lines = [];
        let line = [];
        let x = 0;
        let y = 0;
        let tallest = 0;
        let widest = 0;
        for (const kid of children) {
            // Never placed and hidden: every kid counts, as none can be told
            // to be hidden on its own.
            if ((visible && !kid.visible) || kid.width <= 0 || kid.height <= 0) {
                continue;
            }
            // A hair of slack: a line whose items sum to the width fits.
            if (line.length > 0 && x + kid.width > bound + 0.0001) {
                lines.push({ items: line, y: y });
                y += tallest + rowSpacing;
                line = [];
                x = 0;
                tallest = 0;
            }
            line.push({ item: kid, x: x });
            widest = Math.max(widest, x + kid.width);
            tallest = Math.max(tallest, kid.height);
            x += kid.width + spacing;
        }
        if (line.length > 0) {
            lines.push({ items: line, y: y });
        }
        const rtl = layoutDirection === Qt.RightToLeft;
        const span = bound === Infinity ? widest : bound;
        for (const l of lines) {
            for (const at of l.items) {
                at.item.x = rtl ? span - at.x - at.item.width : at.x;
                at.item.y = l.y;
            }
        }
        eldurLinesWidth = widest;
        eldurLinesHeight = lines.length > 0 ? y + tallest : 0;
        eldurPlaced = visible;
    }

    onChildrenChanged: eldurWatch()
    onWidthChanged: {
        if (!eldurProbing) {
            eldurQueue();
        }
    }
    onSpacingChanged: eldurQueue()
    onRowSpacingChanged: eldurQueue()
    onLayoutDirectionChanged: eldurQueue()
    // Placed at once, so a box shown again has its lines in its first frame
    // and a kid hidden on its own drops out of them.
    onVisibleChanged: eldurPlace()
    Component.onCompleted: eldurWatch()
}
