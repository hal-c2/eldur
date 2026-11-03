// Written by Eldur beside Main.qml: the lines of a block whose inline content
// holds boxes (docs/spec/layout.md). QtQuick's Text has no inline objects, so
// each run of text between boxes stays one Text, shaped whole, and this places
// its lines and the boxes on line boxes as CSS 2.2 §10.8 does. Children are the
// line's content in order; a script may add, remove or change them.
import QtQuick
import QtQuick.Layouts
import "eldur_fonts.js" as EldurFonts

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
    // Floats beside the lines, in this item's terms: `left`, `from`, `to`, `top`,
    // `bottom`. A line that passes them is shortened by them (CSS 2.2 §9.5).
    property var exclusion: null
    // Where the box of an inline element this is lies: its font's, on the first line.
    readonly property var fontExtents: EldurFonts.extents(font, cssSize > 0 ? cssSize : undefined)
    readonly property real fontTop: firstBaseline - fontExtents.ascent
    readonly property real fontHeight: fontExtents.ascent + fontExtents.descent

    property var _runs: new Map()
    property var _levels: new Map()
    property var _boxes: new Map()
    property var _watched: new Set()
    // The fractional CSS size, where the font's own is rounded to a whole one.
    property real cssSize: 0
    property var _ownLeading: new Map()
    property bool _running: false
    property bool _scheduled: false
    property bool _laidOut: false

    FontMetrics {
        id: metrics
        font: lines.font
    }

    FontMetrics {
        id: runMetrics
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

    // A run keeps its font's natural height to be placed, so what its own
    // `line-height` asks of the line is told here. Null is `normal`.
    function setRunLineHeight(item, pixels) {
        // Its text may not have changed, and nothing else would lay the line out.
        if (_ownLeading.get(item) !== (pixels === null ? undefined : pixels)) {
            schedule();
        }
        if (pixels === null) {
            _ownLeading.delete(item);
        } else {
            _ownLeading.set(item, pixels);
        }
    }

    // Whether a point in a run's own coordinates is on one of its lines: its box
    // is their bounding box, and a press beside a short line is not on it.
    function runCovers(item, x, y) {
        const run = _runs.get(item);
        if (!run || !run.place) {
            return true;
        }
        return run.place.some(function(p) {
            return x >= p.x && x < p.x + p.width && y >= p.y && y < p.y + p.height;
        });
    }

    // What a script's changes make of a run's boxes; see `_runLevels`.
    function setRunLevels(item, levels) {
        _levels.set(item, levels);
        schedule();
    }

    // What the runtime makes of a run's inline boxes when it lays the runs out
    // again: the room they take at the run's ends, in place of the converter's
    // `eldurMarginLeft`/`eldurMarginRight`, and the `EldurInlineBox`es that
    // paint them on the run's lines.
    function setRunBox(item, left, right, paints) {
        _boxes.set(item, { left: left, right: right, paints: paints });
        schedule();
    }

    // The block's own box on each line.
    function lineStrut() {
        return EldurFonts.inlineBox({ size: cssSize > 0 ? cssSize : lines.font.pixelSize,
                                      family: lines.font.family, weight: lines.font.weight,
                                      italic: lines.font.italic, line: lineHeight });
    }

    // Where a run's lines are, in its own coordinates.
    function runLines(item) {
        const run = _runs.get(item);
        return run && run.place ? run.place : [];
    }

    function _isText(item) {
        return item.lineLaidOut !== undefined && typeof item.text === "string";
    }

    function _watch(item) {
        if (_watched.has(item)) {
            return;
        }
        _watched.add(item);
        // Connected through `lines` so the signals drop with it: a created box
        // regroups its lines, and the items outlive the lines they were in.
        item.visibleChanged.connect(lines, schedule);
        item.implicitWidthChanged.connect(lines, schedule);
        item.implicitHeightChanged.connect(lines, schedule);
        if (_isText(item)) {
            // The converter leaves a run's stack for here, resolved once.
            item.font.family = EldurFonts.family(item.font.family);
            item.textChanged.connect(lines, schedule);
            item.lineLaidOut.connect(lines, function(line) { _placeLine(item, line); });
            // A `Text` lays out when its implicit size is first asked for: not in a
            // binding that asks, which would take in what the handler reads.
            void item.implicitHeight;
        } else {
            item.widthChanged.connect(lines, schedule);
            item.heightChanged.connect(lines, schedule);
        }
    }

    // The boxes of `item`'s run on a line: what the converter or the runtime said,
    // or, for text a script made, its font on what it was told.
    function _runLevels(item) {
        const told = _levels.get(item);
        if (told) {
            return told;
        }
        if (item.eldurRun) {
            const parsed = JSON.parse(item.eldurRun);
            _levels.set(item, parsed);
            return parsed;
        }
        const leading = _ownLeading.get(item);
        const own = leading !== undefined ? leading : item.eldurOwnLine;
        if (own === undefined && item.font.family === lines.font.family
            && item.font.pixelSize === lines.font.pixelSize) {
            return [];
        }
        return [{ size: item.font.pixelSize, family: item.font.family, weight: item.font.weight,
                  italic: item.font.italic, line: own === undefined ? lineHeight : own }];
    }

    function _placeLine(item, line) {
        const run = _runs.get(item);
        if (!run) {
            return;
        }
        if (line.number === 0) {
            line.width = Math.max(1, run.room);
        } else if (exclusion && !run.place) {
            const cut = _cut(run.firstLine + line.number);
            line.width = Math.max(1, run.wraps - cut.start - cut.end);
        }
        // A last word that cannot part from what comes next goes down with it.
        if (run.squeeze && run.squeeze.number === line.number) {
            line.width = Math.max(1, run.squeeze.width);
        }
        if (run.place) {
            const at = run.place[line.number];
            if (at) {
                // Qt spreads a line it justifies to this width.
                if (at.spread) {
                    line.width = at.spread;
                }
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
        // Qt's width of a line reaches past the last glyph's advance to its ink;
        // CSS puts what follows at the advance. One line in one font is known.
        const plain = _plain(item);
        if (run.measured.length === 1 && !/\n/.test(plain)
            && (item.textFormat === Text.PlainText || item.text.indexOf("<") === -1)) {
            const keeps = item.textFormat === Text.PlainText || preserveSpace
                || item.textFormat === Text.AutoText;
            runMetrics.font = item.font;
            run.measured[0].width = runMetrics.advanceWidth(
                keeps ? plain : plain.trim().replace(/\s+/g, " "));
        }
    }

    function _plain(item) {
        const text = item.text;
        return item.textFormat === Text.PlainText ? text : text.replace(/<[^>]*>/g, "");
    }

    // The spaces inside each measured line of a run. Qt says nothing about
    // which characters a line holds, so its words are walked off by width.
    function _innerSpaces(item, measured) {
        runMetrics.font = item.font;
        const gap = runMetrics.advanceWidth(" ");
        const words = _plain(item).trim().split(/\s+/);
        let at = 0;
        return measured.map(function(m) {
            let used = 0;
            let count = 0;
            while (at < words.length) {
                const next = used + (count > 0 ? gap : 0) + runMetrics.advanceWidth(words[at]);
                if (count > 0 && next > m.width + 1) {
                    break;
                }
                used = next;
                count += 1;
                at += 1;
            }
            return Math.max(0, count - 1);
        });
    }

    // Spreads what a justified line has left over across its spaces, those
    // between its items and those in its text (CSS Text 3 §7.3).
    function _justify(l, free) {
        const parts = l.texts.concat(l.boxes).sort(function(a, b) { return a.x - b.x; });
        let spaces = 0;
        for (const part of parts) {
            spaces += (part.gap ? 1 : 0) + (part.inner || 0);
        }
        if (spaces === 0) {
            return;
        }
        const extra = free / spaces;
        let moved = 0;
        for (const part of parts) {
            moved += part.gap ? extra : 0;
            part.x += moved;
            if (part.inner) {
                part.spread = part.width + part.inner * extra;
                part.perSpace = extra;
                moved += part.inner * extra;
            }
        }
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
            // A box's Text is moved to where a browser puts its half-leading, from
            // Qt's baseline, which is the font's rounded ascent as a run's is here.
            const moved = item.transform.length === 1 ? item.transform[0].y || 0 : 0;
            return moved + Math.round(item.baselineOffset) + (count - 1) * item.contentHeight / count;
        }
        // Chromium rounds a font's ascent, so a control's text sits on a whole pixel.
        if (item.contentItem && item.contentItem.baselineOffset > 0) {
            return item.topPadding + Math.round(item.contentItem.baselineOffset);
        }
        if (typeof item.selectByMouse === "boolean") {
            return Math.round(item.baselineOffset);
        }
        // A box taken out of flow (it has a slot where it would be) gives
        // its container no baseline (CSS 2.2 §10.8.1).
        const kids = Array.from(item.children).filter(k => k.visible && k.eldurSlot === undefined);
        // A flex row takes its first item's baseline, that item's bottom edge
        // when it has no text (css-flexbox §8.5); a spacer here has no height.
        if (item instanceof RowLayout) {
            const first = kids.find(k => k.height > 0);
            if (first) {
                const inside = _baselineOf(first);
                return first.y + (inside === null ? first.height : inside);
            }
            return null;
        }
        if (kind !== "first") {
            kids.reverse();
        }
        for (const kid of kids) {
            // A Text with nothing in it has no line, so gives no baseline.
            if (_isText(kid) && kid.text === "") {
                continue;
            }
            const inside = _baselineOf(kid);
            if (inside !== null) {
                return kid.y + inside;
            }
        }
        return null;
    }

    // What must stay on the line after `item` with no break: the next run's first
    // word, and the words after it while nothing in those runs allows a break.
    function _glued(children, index) {
        let width = 0;
        for (let i = index + 1; i < children.length; ++i) {
            const next = children[i];
            if (!next.visible) {
                continue;
            }
            if (!_isText(next)) {
                return width;
            }
            const plain = _plain(next);
            if (plain === "") {
                continue;
            }
            const word = /^\S*/.exec(plain)[0];
            if (word === "") {
                return width;
            }
            runMetrics.font = next.font;
            width += runMetrics.advanceWidth(word);
            if (word.length < plain.length) {
                return width;
            }
        }
        return width;
    }

    function _newLine() {
        return { texts: [], boxes: [], blanks: [], end: 0, empty: true };
    }

    // What the floats take from line `index` at its start and end. Where it
    // lies is counted in the strut's line heights, as the lines are not built yet.
    function _cut(index) {
        const ex = exclusion;
        if (!ex) {
            return { start: 0, end: 0 };
        }
        const box = lineStrut();
        const step = box.above + box.below;
        const top = index * step;
        if (top >= ex.bottom || top + step <= ex.top) {
            return { start: 0, end: 0 };
        }
        return ex.left ? { start: Math.max(0, ex.to), end: 0 } : { start: 0, end: Math.max(0, width - ex.from) };
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
        // The last thing on the line was a word with no space after it.
        let inWord = false;
        let natural = 0;
        let naturalPending = false;
        const runs = new Map();
        // The room on the line being filled, past what the floats take.
        const roomAt = function(index) {
            const cut = _cut(index);
            return room - (wraps ? cut.start + cut.end : 0);
        };

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
                if (wraps && !line.empty && start + across + item.width > roomAt(built.length)) {
                    built.push(line);
                    line = _newLine();
                    start = 0;
                }
                line.boxes.push({ item: item, x: start + margin.leftMargin,
                                  gap: pending && !line.empty,
                                  top: margin.topMargin, bottom: margin.bottomMargin });
                line.end = start + across + item.width;
                line.empty = false;
                pending = false;
                inWord = false;
                natural += (item.implicitWidth || item.width) + across + (naturalPending ? space : 0);
                naturalPending = false;
                continue;
            }
            const plain = _plain(item);
            const told = _boxes.get(item);
            const marginLeft = told ? told.left : item.eldurMarginLeft || 0;
            const marginRight = told ? told.right : item.eldurMarginRight || 0;
            natural += marginLeft + marginRight;
            if (plain === "") {
                item.width = 0;
                line.blanks.push({ item: item, x: line.end + marginLeft, box: true });
                line.end += marginLeft + marginRight;
                continue;
            }
            // A created element's `Text` is AutoText: with no tag, Qt draws its edge spaces.
            const keeps = item.textFormat === Text.PlainText || preserveSpace
                || item.textFormat === Text.AutoText && item.text.indexOf("<") === -1;
            if (!preserveSpace && /^\s*$/.test(plain)) {
                item.width = 0;
                line.blanks.push({ item: item, x: line.end + (line.empty ? 0 : space) + marginLeft, box: true });
                if (marginLeft + marginRight > 0) {
                    line.end += (line.empty ? 0 : space) + marginLeft + marginRight;
                }
                pending = true;
                naturalPending = natural > 0;
                continue;
            }
            // A `<br>` at either end is no space to take off, and one at the end
            // Qt lays out no line after, so the next run has to be told.
            const lead = !preserveSpace && /^\s/.test(plain) && plain[0] !== "\n";
            const trail = !preserveSpace && /\s$/.test(plain) && !/\n$/.test(plain);
            const leadWidth = lead && keeps ? space : 0;
            // Rich text drops a leading space but draws a trailing one, and
            // styled text draws one that a tag closes after.
            const keepsTrail = keeps || item.textFormat === Text.RichText && item.text.indexOf("<") !== -1
                || item.textFormat === Text.StyledText && /\s(<[^>]*>)+$/.test(item.text);
            const trailWidth = trail && keepsTrail ? space : 0;
            const before = (pending || lead) && !line.empty ? space : 0;
            let start = line.end + before + marginLeft;

            const justified = textAlign === Text.AlignJustify && !noWrap;
            if (justified && item.font.wordSpacing !== 0) {
                item.font.wordSpacing = 0;
            }
            const run = { room: room, measured: [], place: null, squeeze: null,
                          firstLine: built.length, wraps: room };
            runs.set(item, run);
            _runs.set(item, run);
            item.width = room;
            run.room = roomAt(built.length) - start + leadWidth;
            _measure(item, run);
            let first = run.measured[0];
            const glued = wraps && !trail ? _glued(children, children.indexOf(item)) : 0;
            const single = run.measured.length === 1;
            // No break inside a word that runs over from the last run.
            if (wraps && !line.empty && first && !(inWord && !lead && !pending)
                && start + first.width - leadWidth - (single ? trailWidth : 0)
                   + (single && !/\s/.test(plain.trim()) ? glued : 0) > roomAt(built.length) + 0.01) {
                built.push(line);
                line = _newLine();
                start = marginLeft;
                run.firstLine = built.length;
                run.room = roomAt(built.length) + leadWidth - marginLeft;
                _measure(item, run);
                first = run.measured[0];
            }
            if (glued > 0 && first) {
                const last = run.measured.length - 1;
                const left = last === 0 ? run.room : roomAt(run.firstLine + last);
                if (run.measured[last].width + glued > left + 0.01 && /\s/.test(plain.trim())) {
                    run.squeeze = { number: last, width: left - glued };
                    _measure(item, run);
                    first = run.measured[0];
                }
            }
            // RichText has no `lineLaidOut`, so nothing can move its lines: after
            // something on this line it moves as a whole, as a box does.
            if (run.measured.length === 0 && !line.empty) {
                runs.delete(item);
                _runs.delete(item);
                item.width = Math.min(item.implicitWidth, room - start);
                line.boxes.push({ item: item, x: start, gap: before > 0, top: 0, bottom: 0, wholeAscent: true });
                line.end = start + item.width + marginRight;
                pending = trail;
                inWord = !trail;
                natural += item.implicitWidth + ((naturalPending || lead) && natural > 0 ? space : 0);
                naturalPending = trail;
                continue;
            }
            const ascent = item.baselineOffset - (first ? first.y : 0);
            const inner = justified ? _innerSpaces(item, run.measured) : [];
            for (let n = 0; n < run.measured.length; ++n) {
                const m = run.measured[n];
                const last = n === run.measured.length - 1;
                const x = n === 0 ? start : 0;
                const width = m.width - (n === 0 ? leadWidth : 0) - (last ? trailWidth : 0);
                line.texts.push({ item: item, run: run, number: n, x: x, width: width,
                                  height: m.height, ascent: ascent, lead: n === 0 ? leadWidth : 0,
                                  gap: n === 0 && before > 0, inner: inner[n] || 0,
                                  trail: last ? trailWidth : 0 });
                line.end = x + width + (last ? marginRight : 0);
                line.empty = false;
                if (!last) {
                    built.push(line);
                    line = _newLine();
                }
            }
            const broken = /\n$/.test(plain);
            if (broken) {
                built.push(line);
                line = _newLine();
            }
            pending = trail;
            inWord = !trail && !broken;
            // Once a narrower room has broken a word, Qt's implicit width can be
            // well past the text's: on one line it is no wider than its lines end to end.
            let unbroken = ((lead ? 1 : 0) + (trail ? 1 : 0) - 1) * space;
            for (const m of run.measured) {
                unbroken += m.width + space;
            }
            const own = run.measured.length > 0 ? Math.min(item.implicitWidth, unbroken) : item.implicitWidth;
            // An edge space is the lines' to count: one Qt drew is taken off above,
            // and styled and rich text drop a leading one.
            natural += own - leadWidth - trailWidth
                + ((naturalPending || lead) && natural > 0 ? space : 0);
            naturalPending = trail;
        }
        built.push(line);

        const round = Math.round;
        const extents = lines.fontExtents;
        const fontAscent = extents.ascent;
        const fontDescent = extents.descent;
        // Lines add up as the block's do, so a fractional line height stays fractional.
        const strutBox = lineStrut();
        const strutAbove = strutBox.above;
        const strutBelow = strutBox.below;
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
            const groups = new Map();
            // Every inline box with something on this line takes part, its own
            // font and line height on its own baseline (layout.md §1.3).
            const reach = function(placed) {
                for (const b of placed.boxes) {
                    if (b.group) {
                        const g = groups.get(b.group.id) || { align: b.group.align, above: 0, below: 0 };
                        g.above = Math.max(g.above, b.above);
                        g.below = Math.max(g.below, b.below);
                        groups.set(b.group.id, g);
                    } else {
                        above = Math.max(above, b.above);
                        below = Math.max(below, b.below);
                    }
                }
            };
            for (const t of l.texts) {
                t.placed = EldurFonts.placeRun(_runLevels(t.item), strutBox);
                reach(t.placed);
            }
            for (const blank of l.blanks) {
                if (blank.box) {
                    blank.placed = EldurFonts.placeRun(_runLevels(blank.item), strutBox);
                    reach(blank.placed);
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
                    const own = b.wholeAscent ? round(item.baselineOffset) : _baselineOf(item);
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
            groups.forEach(function(g) {
                const missing = g.above + g.below - above - below;
                if (missing > 0) {
                    if (g.align === "top") {
                        below += missing;
                    } else {
                        above += missing;
                    }
                }
            });
            baseline = top + above;
            // The baseline a run hangs from: the line's, or its aligned box's.
            const hung = function(placed) {
                if (!placed.group) {
                    return baseline;
                }
                const g = groups.get(placed.group.id);
                return placed.group.align === "top" ? top + g.above : top + above + below - g.below;
            };
            if (index === 0) {
                firstBaseline = baseline;
            }
            const cut = _cut(index);
            let shift = cut.start;
            const free = width - cut.start - cut.end - l.end;
            // The last line stays as it is, as a browser leaves it.
            if (free > 0 && textAlign === Text.AlignJustify && !noWrap && index < built.length - 1) {
                _justify(l, free);
            }
            if (free > 0 && !(noWrap && textAlign === Text.AlignLeft)) {
                if (textAlign === Text.AlignHCenter) {
                    shift += free / 2;
                } else if (textAlign === Text.AlignRight) {
                    shift += free;
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
                blank.item.y = (blank.placed ? hung(blank.placed) + blank.placed.shift : baseline)
                    - blank.item.baselineOffset;
                // An empty element still has a box where it is, for a script to ask.
                _runs.set(blank.item, { place: [{ x: 0, y: 0, width: 0, baseline: baseline,
                                                  hung: blank.placed ? hung(blank.placed) : baseline,
                                                  lead: 0, trail: 0 }] });
            }
            for (const t of l.texts) {
                if (!t.run.place) {
                    t.run.place = [];
                }
                // A trailing space hangs at the line's end; before more it is in the box.
                const followed = t.trail > 0 && l.boxes.concat(l.texts).some(function(o) { return o.x > t.x; });
                // Qt draws a line on its own whole pixel, adding the ascent rounded to
                // the row its Text starts on, so the row must come from the same sum.
                const runBase = hung(t.placed) + t.placed.shift;
                t.run.place[t.number] = { x: t.x + shift - t.lead, y: runBase - round(t.ascent), height: t.height,
                                          baseline: baseline, hung: hung(t.placed),
                                          lead: t.lead, trail: t.trail || 0,
                                          ascent: round(t.ascent),
                                          // The box of its inline element, as far under this row.
                                          boxTop: round(t.ascent) - (t.placed.box || strutBox).ascent,
                                          boxHeight: (t.placed.box || strutBox).ascent + (t.placed.box || strutBox).descent,
                                          width: (t.spread || t.width) + t.lead + (followed ? t.trail : 0) };
                if (t.spread) {
                    // Qt justifies every line of a Text but its last, whose
                    // words word-spacing spreads when it is the only one.
                    if (t.run.measured.length === 1) {
                        t.item.font.wordSpacing = t.perSpace;
                    } else {
                        t.item.horizontalAlignment = Text.AlignJustify;
                        t.run.place[t.number].spread = t.spread + t.lead;
                    }
                }
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
            // `position: relative` moves what the line placed, and nothing else on it.
            const moved = _runLevels(item).reduce(function(sum, level) {
                return { x: sum.x + (level.relX || 0), y: sum.y + (level.relY || 0) };
            }, { x: 0, y: 0 });
            item.x = left + moved.x;
            item.y = high + moved.y;
            item.width = Math.max(0, right - left);
            item.forceLayout();
            // An inline element's own box is painted on these.
            if (item.eldurFragments !== undefined) {
                item.eldurFragments = place;
            }
            const box = _boxes.get(item);
            if (box) {
                box.paints.forEach(function(paint) { paint.fragments = place; });
            }
        });
        implicitHeight = top;
        implicitWidth = natural;
        _running = false;
    }

    onChildrenChanged: schedule()
    // Not at once: a table reads implicit sizes while working out this width.
    onWidthChanged: schedule()
    onFontChanged: schedule()
    onExclusionChanged: schedule()
    Component.onCompleted: layout()
}
