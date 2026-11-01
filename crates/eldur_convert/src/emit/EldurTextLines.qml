import QtQuick

// Decoration lines a font cannot draw: another colour, or a style. A child of
// its Text. Its row widths come from a hidden copy: a Text that listens to its
// own `lineLaidOut` places its fixed-height lines differently.
Canvas {
    id: lines

    property bool underline: false
    property bool overline: false
    property bool lineThrough: false
    property string lineStyle: "solid"
    property color lineColor: "black"
    property var rowWidths: []

    function place(line) {
        const widths = rowWidths.slice(0, line.number);
        widths[line.number] = line.implicitWidth;
        rowWidths = widths;
        requestPaint();
    }

    // Lines may hang below a last row's box, and an overline stands above a
    // first row's when the line height is the font's or less.
    readonly property int headroom: 8

    x: 0
    y: -headroom
    width: parent.width
    height: parent.height + 8 + headroom

    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()

    Text {
        visible: false
        text: lines.parent.text
        font: lines.parent.font
        width: lines.parent.width
        wrapMode: lines.parent.wrapMode
        textFormat: lines.parent.textFormat
        onLineLaidOut: (line) => lines.place(line)
    }

    FontMetrics {
        id: metrics

        font: lines.parent.font
    }

    function drawLine(ctx, x0, x1, y, t) {
        ctx.strokeStyle = String(lineColor);
        ctx.fillStyle = String(lineColor);
        ctx.lineWidth = t;
        // The geometry of each style is measured from Chromium at one pixel.
        if (lineStyle === "wavy") {
            const step = 4.5 * t;
            ctx.save();
            ctx.beginPath();
            ctx.rect(x0, y - 2 * step, x1 - x0, 4 * step);
            ctx.clip();
            ctx.beginPath();
            ctx.moveTo(x0, y + 2.5 * t);
            let up = true;
            for (let x = x0; x < x1; x += step) {
                ctx.quadraticCurveTo(x + step / 2, y + 2.5 * t + (up ? -4 : 4) * t, x + step, y + 2.5 * t);
                up = !up;
            }
            ctx.stroke();
            ctx.restore();
            return;
        }
        if (lineStyle === "double") {
            ctx.fillRect(x0, y - t / 2, x1 - x0, t);
            ctx.fillRect(x0, y + 2.5 * t, x1 - x0, t);
            return;
        }
        if (lineStyle === "dashed" || lineStyle === "dotted") {
            // These straddle the row a solid line fills.
            const mark = lineStyle === "dashed" ? 6 * t : t;
            const gap = lineStyle === "dashed" ? 4 * t : t;
            for (let x = x0; x < x1; x += mark + gap) {
                ctx.fillRect(x, y - t, Math.min(mark, x1 - x), t);
            }
            return;
        }
        ctx.fillRect(x0, y - t / 2, x1 - x0, t);
    }

    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        ctx.translate(0, headroom);
        const text = parent;
        const t = Math.max(1, Math.round(metrics.lineWidth));
        const rowHeight = text.lineHeightMode === Text.FixedHeight
            ? text.lineHeight : metrics.lineSpacing;
        for (let row = 0; row < rowWidths.length; ++row) {
            const w = rowWidths[row];
            if (w === undefined) {
                continue;
            }
            let x0 = 0;
            if (text.effectiveHorizontalAlignment === Text.AlignHCenter) {
                x0 = (text.width - w) / 2;
            } else if (text.effectiveHorizontalAlignment === Text.AlignRight) {
                x0 = text.width - w;
            }
            x0 = Math.round(x0);
            const x1 = x0 + Math.floor(w);
            const base = Math.round(text.baselineOffset + row * rowHeight);
            if (underline) {
                drawLine(ctx, x0, x1, base + Math.round(metrics.underlinePosition) + t / 2, t);
            }
            if (overline) {
                drawLine(ctx, x0, x1, base - Math.round(metrics.overlinePosition) - t / 2, t);
            }
            if (lineThrough) {
                drawLine(ctx, x0, x1, base - Math.round(metrics.strikeOutPosition) + t / 2, t);
            }
        }
    }
}
