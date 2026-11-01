import QtQuick

// A border a Rectangle cannot draw: a side per style and colour, joined on the
// diagonal of its corners. Lists go top, right, bottom, left; radii start at the
// top left. The `outset`/`inset` shades were measured from Chromium: the largest
// channel moves 84 down or up, the others keep their proportion to it, and black
// goes to two greys.
Item {
    id: box

    property var sideWidths: [0, 0, 0, 0]
    property var sideColors: ["", "", "", ""]
    property var sideStyles: ["none", "none", "none", "none"]
    property var radii: [0, 0, 0, 0]

    onSideWidthsChanged: canvas.requestPaint()
    onSideColorsChanged: canvas.requestPaint()
    onSideStylesChanged: canvas.requestPaint()
    onRadiiChanged: canvas.requestPaint()

    // A browser puts a box's edges on whole pixels; a Canvas on a fractional
    // position would be resampled, and a thin line would smear over two rows.
    function snap() {
        const at = mapToItem(null, 0, 0);
        const left = Math.round(at.x);
        const top = Math.round(at.y);
        canvas.x = left - at.x;
        canvas.y = top - at.y;
        canvas.width = Math.round(at.x + width) - left;
        canvas.height = Math.round(at.y + height) - top;
    }

    onWidthChanged: snap()
    onHeightChanged: snap()
    Component.onCompleted: snap()

    Canvas {
        id: canvas

        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()

        function css(c) {
            return "rgba(" + Math.round(c.r * 255) + "," + Math.round(c.g * 255) + "," + Math.round(c.b * 255) + "," + c.a + ")";
        }

        function shade(text, up) {
            const c = Qt.color(text);
            const most = Math.max(c.r, c.g, c.b) * 255;
            if (most === 0) {
                const grey = (up ? 168 : 84) / 255;
                return css({ r: grey, g: grey, b: grey, a: c.a });
            }
            const k = Math.max(0, Math.min(255, most + (up ? 84 : -84))) / most;
            return css({ r: Math.min(1, c.r * k), g: Math.min(1, c.g * k), b: Math.min(1, c.b * k), a: c.a });
        }

        // A point `along` a side and `depth` into the box, whichever side it is.
        function point(side, along, depth) {
            switch (side) {
            case 0: return [along, depth];
            case 1: return [width - depth, along];
            case 2: return [width - along, height - depth];
            default: return [depth, height - along];
            }
        }

        function sideLength(side) {
            return side % 2 === 0 ? width : height;
        }

        function polygon(ctx, side, points) {
            ctx.beginPath();
            points.forEach((p, i) => {
                const at = point(side, p[0], p[1]);
                if (i === 0) {
                    ctx.moveTo(at[0], at[1]);
                } else {
                    ctx.lineTo(at[0], at[1]);
                }
            });
            ctx.closePath();
        }

        // The side's share of the box, cut on the line through each outer corner and
        // its inner one, run on far enough to take in a rounded corner's curve.
        function clipToSide(ctx, side) {
            const length = sideLength(side);
            const deep = sideWidths[side];
            const before = sideWidths[(side + 3) % 4];
            const after = sideWidths[(side + 1) % 4];
            const run = (r, across) => r > 0 ? 1 + r / Math.max(Math.min(across, deep), 1) : 1;
            const a = run(radii[side], before);
            const b = run(radii[(side + 1) % 4], after);
            polygon(ctx, side, [[0, 0], [length, 0], [length - after * b, deep * b], [before * a, deep * a]]);
            ctx.clip();
        }

        function roundedBox(ctx, top, right, bottom, left) {
            const k = 0.5523;
            const x0 = left;
            const y0 = top;
            const x1 = width - right;
            const y1 = height - bottom;
            const corner = (r, across, down) => [Math.max(0, r - across), Math.max(0, r - down)];
            const a = corner(radii[0], left, top);
            const b = corner(radii[1], right, top);
            const c = corner(radii[2], right, bottom);
            const d = corner(radii[3], left, bottom);
            ctx.moveTo(x0 + a[0], y0);
            ctx.lineTo(x1 - b[0], y0);
            ctx.bezierCurveTo(x1 - b[0] + k * b[0], y0, x1, y0 + b[1] - k * b[1], x1, y0 + b[1]);
            ctx.lineTo(x1, y1 - c[1]);
            ctx.bezierCurveTo(x1, y1 - c[1] + k * c[1], x1 - c[0] + k * c[0], y1, x1 - c[0], y1);
            ctx.lineTo(x0 + d[0], y1);
            ctx.bezierCurveTo(x0 + d[0] - k * d[0], y1, x0, y1 - d[1] + k * d[1], x0, y1 - d[1]);
            ctx.lineTo(x0, y0 + a[1]);
            ctx.bezierCurveTo(x0, y0 + a[1] - k * a[1], x0 + a[0] - k * a[0], y0, x0 + a[0], y0);
            ctx.closePath();
        }

        // The part of the border between two depths, each a share of its side's width.
        function ring(ctx, from, to, fill) {
            const w = sideWidths;
            ctx.beginPath();
            roundedBox(ctx, w[0] * from, w[1] * from, w[2] * from, w[3] * from);
            roundedBox(ctx, w[0] * to, w[1] * to, w[2] * to, w[3] * to);
            ctx.fillRule = Qt.OddEvenFill;
            ctx.fillStyle = fill;
            ctx.fill();
        }

        // The middle of the border along a side, from the middle of one corner to the
        // middle of the next.
        function centreLine(side) {
            const length = sideLength(side);
            const mid = sideWidths[side] / 2;
            const out = [];
            const arc = (r, cx, from, to) => {
                for (let i = 0; i <= 12; ++i) {
                    const t = from + (to - from) * i / 12;
                    out.push([cx + (r - mid) * Math.cos(t), r - (r - mid) * Math.sin(t)]);
                }
            };
            const before = radii[side];
            const after = radii[(side + 1) % 4];
            if (before > mid) {
                arc(before, before, Math.PI * 3 / 4, Math.PI / 2);
            } else {
                out.push([0, mid]);
            }
            if (after > mid) {
                arc(after, length - after, Math.PI / 2, Math.PI / 4);
            } else {
                out.push([length, mid]);
            }
            return out;
        }

        function along(points, distance) {
            let left = distance;
            for (let i = 1; i < points.length; ++i) {
                const dx = points[i][0] - points[i - 1][0];
                const dy = points[i][1] - points[i - 1][1];
                const step = Math.hypot(dx, dy);
                if (left <= step || i === points.length - 1) {
                    const t = step > 0 ? left / step : 0;
                    return [points[i - 1][0] + dx * t, points[i - 1][1] + dy * t];
                }
                left -= step;
            }
            return points[0];
        }

        function lengthOf(points) {
            let total = 0;
            for (let i = 1; i < points.length; ++i) {
                total += Math.hypot(points[i][0] - points[i - 1][0], points[i][1] - points[i - 1][1]);
            }
            return total;
        }

        // Marks start and end on the corners: the gap keeps its size and the dash takes
        // up what is left over, the way Chromium's measure. A closed line has as many
        // gaps as marks.
        function drawMarks(ctx, line, w, dotted, closed, color) {
            const total = lengthOf(line);
            const mark = dotted ? w : 2 * w;
            const room = closed ? total : total + w;
            const count = Math.max(1, Math.round(room / (mark + w)));
            const period = room / count;
            ctx.fillStyle = color;
            ctx.strokeStyle = color;
            ctx.lineWidth = w;
            for (let i = 0; i < count; ++i) {
                const from = i * period;
                ctx.beginPath();
                if (dotted) {
                    const at = along(line, closed ? from : from + w / 2);
                    ctx.arc(at[0], at[1], w / 2, 0, Math.PI * 2);
                    ctx.fill();
                    continue;
                }
                const end = from + period - w;
                let at = along(line, from);
                ctx.moveTo(at[0], at[1]);
                for (let d = from + 3; d < end; d += 3) {
                    at = along(line, d);
                    ctx.lineTo(at[0], at[1]);
                }
                at = along(line, end);
                ctx.lineTo(at[0], at[1]);
                ctx.stroke();
            }
        }

        function sideLine(side) {
            return centreLine(side).map(p => point(side, p[0], p[1]));
        }

        function drawSide(ctx, side) {
            const style = sideStyles[side];
            const color = sideColors[side];
            const upperLeft = side === 0 || side === 3;
            const w = sideWidths[side];
            if (style === "dashed" || style === "dotted") {
                drawMarks(ctx, sideLine(side), w, style === "dotted", false, color);
            } else if (style === "double" && w >= 3) {
                const line = Math.round(w / 3) / w;
                ring(ctx, 0, line, color);
                ring(ctx, 1 - line, 1, color);
            } else if (style === "groove" || style === "ridge") {
                const up = (style === "ridge") === upperLeft;
                ring(ctx, 0, 0.5, shade(color, up));
                ring(ctx, 0.5, 1, shade(color, !up));
            } else if (style === "inset" || style === "outset") {
                ring(ctx, 0, 1, shade(color, (style === "outset") === upperLeft));
            } else {
                ring(ctx, 0, 1, color);
            }
        }

        onPaint: {
            box.snap();
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);
            // Marks all round a box that is the same all round run on past its corners.
            const same = [1, 2, 3].every(i => sideWidths[i] === sideWidths[0] && sideStyles[i] === sideStyles[0]
                    && sideColors[i] === sideColors[0]);
            if (same && radii.some(r => r > 0) && (sideStyles[0] === "dashed" || sideStyles[0] === "dotted")) {
                let line = [];
                for (let side = 0; side < 4; ++side) {
                    line = line.concat(sideLine(side));
                }
                // Chromium's marks start where the top side's straight part does.
                const start = radii[0] > sideWidths[0] / 2 ? 12 : 0;
                line = line.slice(start).concat(line.slice(0, start + 1));
                drawMarks(ctx, line, sideWidths[0], sideStyles[0] === "dotted", true, sideColors[0]);
                return;
            }
            for (let side = 0; side < 4; ++side) {
                if (sideWidths[side] > 0 && sideStyles[side] !== "none" && sideStyles[side] !== "hidden") {
                    ctx.save();
                    clipToSide(ctx, side);
                    drawSide(ctx, side);
                    ctx.restore();
                }
            }
        }
    }
}
