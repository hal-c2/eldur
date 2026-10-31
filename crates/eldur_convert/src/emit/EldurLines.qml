// Written by Eldur beside Main.qml: the lines of a block whose inline content
// holds boxes (docs/spec/layout.md). QtQuick's Text has no inline objects, so
// each run of text between boxes stays one Text, shaped whole, and this places
// its lines and the boxes on line boxes as CSS 2.2 §10.8 does. Children are the
// line's content in order; a script may add, remove or change them.
import QtQuick
import QtQuick.Layouts

Item {
    id: lines

    property font font
    // What a text node a script adds is painted in.
    property color color: "black"
    // Pixels; 0 is `line-height: normal`.
    property real lineHeight: 0
    property bool noWrap: false
    property bool preserveSpace: false
    property int textAlign: Text.AlignLeft
    // Lets the runtime take this for a layout of the block's children.
    property real spacing: 0
    property real firstBaseline: 0
    property real lastBaseline: 0
    // Where the box of an inline element this is lies: its font's, on the first line.
    readonly property real fontTop: firstBaseline - Math.round(metrics.ascent)
    readonly property real fontHeight: Math.round(metrics.ascent) + Math.round(metrics.descent)

    property var _runs: new Map()
    property var _watched: new Set()
    property bool _running: false
    property bool _scheduled: false
    property bool _laidOut: false

    FontMetrics {
        id: metrics
        font: lines.font
    }

    function schedule() {
        if (_running || _scheduled) {
            return;
        }
        _scheduled = true;
        Qt.callLater(_scheduledLayout);
    }

    function _scheduledLayout() {
        // The page may have gone by the time a delayed call comes.
        if (typeof lines.layout === "function") {
            _scheduled = false;
            layout();
        }
    }

    // A script reading a position wants this frame's lines.
    function settleLines() {
        if (_scheduled || !_laidOut) {
            layout();
            _scheduled = false;
        }
    }

    function _isText(item) {
        return item.lineLaidOut !== undefined && typeof item.text === "string";
    }

    function _watch(item) {
        if (_watched.has(item)) {
            return;
        }
        _watched.add(item);
        item.visibleChanged.connect(schedule);
        item.implicitWidthChanged.connect(schedule);
        item.implicitHeightChanged.connect(schedule);
        if (_isText(item)) {
            item.textChanged.connect(schedule);
            item.lineLaidOut.connect(function(line) { _placeLine(item, line); });
        } else {
            item.widthChanged.connect(schedule);
            item.heightChanged.connect(schedule);
        }
    }

    function _placeLine(item, line) {
        const run = _runs.get(item);
        if (!run) {
            return;
        }
        if (line.number === 0) {
            line.width = Math.max(1, run.room);
        }
        if (run.place) {
            const at = run.place[line.number];
            if (at) {
                line.x = at.x;
                line.y = at.y;
            }
        } else {
            run.measured.push({ width: line.implicitWidth, height: line.height, y: line.y });
        }
    }

    function _measure(item, run) {
        run.measured = [];
        run.place = null;
        item.forceLayout();
    }

    function _plain(item) {
        const text = item.text;
        return item.textFormat === Text.PlainText ? text : text.replace(/<[^>]*>/g, "");
    }

    // The first line box's edge from the top of `item`, or null with none.
    function _baselineOf(item) {
        const kind = item.eldurBaseline;
        if (kind === "none") {
            return null;
        }
        // A check box or slider has no text: the line sits on the box's edge.
        if (item.checkState !== undefined || item.stepSize !== undefined && item.handle) {
            return item.height;
        }
        // A scroller's baseline is its bottom margin edge.
        if (item.textDocument !== undefined) {
            return null;
        }
        if (typeof item.lastBaseline === "number" && item.textAlign !== undefined) {
            return item.lastBaseline;
        }
        if (_isText(item)) {
            const count = Math.max(1, item.lineCount);
            // A box's Text is moved to where a browser puts its half-leading.
            const moved = item.transform.length === 1 ? item.transform[0].y || 0 : 0;
            return moved + item.baselineOffset + (count - 1) * item.contentHeight / count;
        }
        // Chromium rounds a font's ascent, so a control's text sits on a whole pixel.
        if (item.contentItem && item.contentItem.baselineOffset > 0) {
            return item.topPadding + Math.round(item.contentItem.baselineOffset);
        }
        if (typeof item.selectByMouse === "boolean") {
            return Math.round(item.baselineOffset);
        }
        const kids = Array.from(item.children).filter(k => k.visible);
        if (kind !== "first") {
            kids.reverse();
        }
        for (const kid of kids) {
            const inside = _baselineOf(kid);
            if (inside !== null) {
                return kid.y + inside;
            }
        }
        return null;
    }

    function _newLine() {
        return { texts: [], boxes: [], blanks: [], end: 0, empty: true };
    }

    function layout() {
        if (_running) {
            return;
        }
        _running = true;
        _laidOut = true;
        const children = Array.from(lines.children);
        const wraps = !noWrap && width > 0;
        const room = wraps ? width : 1e6;
        const space = metrics.advanceWidth(" ");
        const built = [];
        let line = _newLine();
        let pending = false;
        let natural = 0;
        let naturalPending = false;
        const runs = new Map();

        for (const item of children) {
            _watch(item);
            if (!item.visible) {
                continue;
            }
            if (!_isText(item)) {
                // A line a script put inside this one is as wide as its content.
                if (typeof item.firstBaseline === "number") {
                    item.noWrap = true;
                    item.width = Math.min(item.implicitWidth, room);
                }
                const margin = item.Layout;
                const across = margin.leftMargin + margin.rightMargin;
                let start = line.end + (pending && !line.empty ? space : 0);
                if (wraps && !line.empty && start + across + item.width > room) {
                    built.push(line);
                    line = _newLine();
                    start = 0;
                }
                line.boxes.push({ item: item, x: start + margin.leftMargin,
                                  top: margin.topMargin, bottom: margin.bottomMargin });
                line.end = start + across + item.width;
                line.empty = false;
                pending = false;
                natural += (item.implicitWidth || item.width) + across + (naturalPending ? space : 0);
                naturalPending = false;
                continue;
            }
            const plain = _plain(item);
            const marginLeft = item.eldurMarginLeft || 0;
            const marginRight = item.eldurMarginRight || 0;
            natural += marginLeft + marginRight;
            if (plain === "") {
                item.width = 0;
                line.blanks.push({ item: item, x: line.end + marginLeft });
                line.end += marginLeft + marginRight;
                continue;
            }
            const keeps = item.textFormat === Text.PlainText || preserveSpace;
            if (!preserveSpace && /^\s*$/.test(plain)) {
                item.width = 0;
                line.blanks.push({ item: item, x: line.end + (line.empty ? 0 : space) + marginLeft });
                if (marginLeft + marginRight > 0) {
                    line.end += (line.empty ? 0 : space) + marginLeft + marginRight;
                }
                pending = true;
                naturalPending = natural > 0;
                continue;
            }
            const lead = !preserveSpace && /^\s/.test(plain);
            const trail = !preserveSpace && /\s$/.test(plain);
            const leadWidth = lead && keeps ? space : 0;
            const trailWidth = trail && keeps ? space : 0;
            const before = (pending || lead) && !line.empty ? space : 0;
            let start = line.end + before + marginLeft;

            const run = { room: room, measured: [], place: null };
            runs.set(item, run);
            _runs.set(item, run);
            item.width = room;
            run.room = room - start + leadWidth;
            _measure(item, run);
            let first = run.measured[0];
            if (wraps && !line.empty && first
                && start + first.width - leadWidth - (run.measured.length > 1 ? trailWidth : 0) > room + 0.01) {
                built.push(line);
                line = _newLine();
                start = marginLeft;
                run.room = room + leadWidth - marginLeft;
                _measure(item, run);
                first = run.measured[0];
            }
            const ascent = item.baselineOffset - (first ? first.y : 0);
            for (let n = 0; n < run.measured.length; ++n) {
                const m = run.measured[n];
                const last = n === run.measured.length - 1;
                const x = n === 0 ? start : 0;
                const width = m.width - (n === 0 ? leadWidth : 0) - (last ? trailWidth : 0);
                line.texts.push({ item: item, run: run, number: n, x: x, width: width,
                                  height: m.height, ascent: ascent, lead: n === 0 ? leadWidth : 0 });
                line.end = x + width + (last ? marginRight : 0);
                line.empty = false;
                if (!last) {
                    built.push(line);
                    line = _newLine();
                }
            }
            pending = trail;
            natural += item.implicitWidth - leadWidth - trailWidth
                + ((naturalPending || lead && keeps) && natural > 0 ? space : 0);
            naturalPending = trail;
        }
        built.push(line);

        const round = Math.round;
        const fontAscent = round(metrics.ascent);
        const fontDescent = round(metrics.descent);
        // CSS gives the odd pixel of leading to the bottom, as Chromium rounds it.
        // Qt drops the font's line gap, which `normal` includes: 1.15 is what the
        // common fallback fonts come to, as `CONTENT_AREA` says for the converter.
        const leading = (lineHeight > 0 ? lineHeight : round(lines.font.pixelSize * 1.15))
            - fontAscent - fontDescent;
        const strutAbove = fontAscent + Math.floor(leading / 2);
        const strutBelow = fontDescent + Math.ceil(leading / 2);
        const size = lines.font.pixelSize;
        let top = 0;
        let baseline = 0;
        built.forEach(function(l, index) {
            // A line with nothing on it has no strut (CSS 2.2 §9.4.2).
            if (l.empty) {
                for (const blank of l.blanks) {
                    blank.item.x = blank.x;
                    blank.item.y = top;
                }
                return;
            }
            let above = strutAbove;
            let below = strutBelow;
            const late = [];
            // Text of the block's font is the strut already; only a larger span adds.
            for (const t of l.texts) {
                if (lineHeight === 0 && t.height > strutAbove + strutBelow + 1) {
                    above = Math.max(above, t.ascent);
                    below = Math.max(below, t.height - t.ascent);
                }
            }
            for (const b of l.boxes) {
                const item = b.item;
                const align = item.eldurVAlign || "baseline";
                const h = item.height;
                if (align === "top" || align === "bottom") {
                    late.push(b);
                    continue;
                }
                let from;
                if (align === "middle") {
                    from = -metrics.xHeight / 2 - h / 2;
                } else if (align === "text-top") {
                    from = -fontAscent;
                } else if (align === "text-bottom") {
                    from = fontDescent - h;
                } else {
                    const own = _baselineOf(item);
                    from = -(own === null ? h + b.bottom : own);
                    if (align === "sub") {
                        from += size / 5 + 1;
                    } else if (align === "super") {
                        from -= size / 3 + 1;
                    } else if (align === "length") {
                        from -= item.eldurVShift;
                    }
                }
                b.from = from;
                above = Math.max(above, b.top - from);
                below = Math.max(below, from + h + b.bottom);
            }
            for (const b of late) {
                const missing = b.item.height + b.top + b.bottom - above - below;
                if (missing > 0) {
                    if (b.item.eldurVAlign === "top") {
                        below += missing;
                    } else {
                        above += missing;
                    }
                }
            }
            baseline = top + above;
            if (index === 0) {
                firstBaseline = baseline;
            }
            let shift = 0;
            const free = width - l.end;
            if (free > 0 && !(noWrap && textAlign === Text.AlignLeft)) {
                if (textAlign === Text.AlignHCenter) {
                    shift = free / 2;
                } else if (textAlign === Text.AlignRight) {
                    shift = free;
                }
            }
            for (const b of l.boxes) {
                const align = b.item.eldurVAlign;
                b.item.x = b.x + shift;
                if (align === "top") {
                    b.item.y = top + b.top;
                } else if (align === "bottom") {
                    b.item.y = top + above + below - b.item.height - b.bottom;
                } else {
                    b.item.y = baseline + b.from;
                }
            }
            for (const blank of l.blanks) {
                blank.item.x = blank.x + shift;
                blank.item.y = baseline - blank.item.baselineOffset;
            }
            for (const t of l.texts) {
                if (!t.run.place) {
                    t.run.place = [];
                }
                t.run.place[t.number] = { x: t.x + shift - t.lead, y: baseline - t.ascent, width: t.width + t.lead };
            }
            top += above + below;
        });
        lastBaseline = baseline;

        // A Text covers only what it draws, so it neither catches a press
        // beside its words nor measures wider than they are.
        runs.forEach(function(run, item) {
            const place = run.place || [];
            run.place = place;
            let left = 1e9;
            let right = 0;
            let high = 1e9;
            place.forEach(function(p) {
                left = Math.min(left, p.x);
                right = Math.max(right, p.x + p.width);
                high = Math.min(high, p.y);
            });
            if (left === 1e9) {
                left = 0;
                high = 0;
            }
            place.forEach(function(p) {
                p.x -= left;
                p.y -= high;
            });
            item.x = left;
            item.y = high;
            item.width = Math.max(0, right - left);
            item.forceLayout();
        });
        implicitHeight = top;
        implicitWidth = natural;
        _running = false;
    }

    onChildrenChanged: schedule()
    // Not at once: a table reads implicit sizes while working out this width.
    onWidthChanged: schedule()
    onFontChanged: schedule()
    Component.onCompleted: layout()
}
