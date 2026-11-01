.pragma library

// Which font a CSS family stack reaches, and the line it gets, at run time
// (docs/spec/layout.md §1.3). Generated pages and the runtime share this
// module, so one font has one set of numbers, and the converter reads the same
// table for the estimates it lays out with; it never looks at the machine's
// fonts, so converting gives the same output on every machine.

// The JSON between the markers is parsed by the converter too.
var TABLE = /*TABLE*/{
"families": {
  "helvetica": ["Liberation Sans", "Arimo", "Helvetica", "Arial"],
  "arial": ["Liberation Sans", "Arimo", "Arial", "Helvetica"],
  "-apple-system": ["Liberation Sans", "Arimo", "Noto Sans", "DejaVu Sans"],
  "blinkmacsystemfont": ["Liberation Sans", "Arimo", "Noto Sans", "DejaVu Sans"],
  "system-ui": ["Liberation Sans", "Arimo", "Noto Sans", "DejaVu Sans"],
  "sans-serif": ["Liberation Sans", "Arimo", "Noto Sans", "DejaVu Sans"],
  "times": ["Liberation Serif", "Tinos", "Times", "Times New Roman"],
  "times new roman": ["Liberation Serif", "Tinos", "Times New Roman", "Times"],
  "serif": ["Liberation Serif", "Tinos", "Noto Serif", "DejaVu Serif"],
  "cursive": ["Liberation Serif", "Tinos", "Noto Serif", "DejaVu Serif"],
  "fantasy": ["Liberation Serif", "Tinos", "Noto Serif", "DejaVu Serif"],
  "math": ["Liberation Serif", "Tinos", "Noto Serif", "DejaVu Serif"],
  "fangsong": ["Liberation Serif", "Tinos", "Noto Serif", "DejaVu Serif"],
  "emoji": ["Liberation Serif", "Tinos", "Noto Serif", "DejaVu Serif"],
  "courier": ["Liberation Mono", "Cousine", "Courier", "Courier New"],
  "courier new": ["Liberation Mono", "Cousine", "Courier New", "Courier"],
  "monospace": ["JetBrainsMono Nerd Font", "Liberation Mono", "Cousine", "Noto Sans Mono", "DejaVu Sans Mono"]
},
"default": ["Liberation Serif", "Tinos", "Noto Serif", "DejaVu Serif"],
"faces": {
  "liberation sans": {"x": 0.5283125, "regular": [0.905273, 0.211914, 0.032715]},
  "liberation serif": {"x": 0.459, "regular": [0.891113, 0.216309, 0.042480]},
  "liberation mono": {"x": 0.5283125, "regular": [0.832520, 0.300293, 0]},
  "nimbus sans": {"x": 0.516, "xs": [0.516, 0.526, 0.516, 0.526], "regular": [0.729, 0.271, 0.2], "italic": [0.9, 0.25, 0.2], "bolditalic": [0.9, 0.25, 0.2]},
  "nimbus roman": {"x": 0.45, "xs": [0.45, 0.459, 0.43, 0.449], "regular": [0.683, 0.317, 0.2], "bold": [0.676, 0.324, 0.2], "bolditalic": [0.699, 0.301, 0.2]},
  "nimbus mono ps": {"x": 0.417, "xs": [0.417, 0.422, 0.417, 0.422], "regular": [0.603, 0.397, 0.2]},
  "nimbus sans narrow": {"x": 0.524, "xs": [0.524, 0.532, 0.524, 0.532], "regular": [0.718, 0.282, 0.2]},
  "noto sans": {"x": 0.536, "xs": [0.536, 0.546, 0.536, 0.546], "regular": [1.069, 0.293, 0]},
  "noto serif": {"x": 0.536, "regular": [1.069, 0.293, 0]},
  "noto sans mono": {"x": 0.536, "xs": [0.536, 0.546, 0.536, 0.546], "regular": [1.069, 0.293, 0]},
  "jetbrainsmono nerd font": {"x": 0.55, "regular": [1.02, 0.3, 0]}
}
}/*TABLE*/;

var _installed = null;

function _isInstalled(name) {
    if (_installed === null) {
        _installed = {};
        var all = Qt.fontFamilies();
        for (var i = 0; i < all.length; ++i) {
            // fontconfig's foundry: "Nimbus Sans [UKWN]".
            _installed[all[i].replace(/ \[[^\]]*\]$/, "").toLowerCase()] = all[i].replace(/ \[[^\]]*\]$/, "");
        }
    }
    return _installed[name.toLowerCase()];
}

function _firstInstalled(candidates) {
    for (var i = 0; i < candidates.length; ++i) {
        var found = _isInstalled(candidates[i]);
        if (found !== undefined) {
            return found;
        }
    }
    return null;
}

var _families = {};

// The family to give Qt for a CSS stack: the first name the machine can show,
// as Chromium skips a font it does not have. QML's `font` holds one family, so
// the walk is done here. If nothing is there Qt's own fallback has the first name.
function family(stack) {
    var key = String(stack || "");
    var known = _families[key];
    if (known !== undefined) {
        return known;
    }
    var names = key.split(",").map(function(n) { return n.trim().replace(/^["']|["']$/g, ""); })
        .filter(function(n) { return n !== ""; });
    var found = null;
    for (var i = 0; i < names.length && found === null; ++i) {
        var candidates = TABLE.families[names[i].toLowerCase()];
        if (candidates !== undefined) {
            found = _firstInstalled(candidates);
        } else {
            found = _isInstalled(names[i]) || null;
        }
    }
    if (found === null) {
        found = _firstInstalled(TABLE["default"]);
    }
    if (found === null) {
        found = names.length > 0 ? names[0] : "serif";
    }
    _families[key] = found;
    return found;
}

var _measured = {};

// Qt's own metrics of what it loaded: one object per font, kept for the page's life.
function _qtFont(name, pixels, weight, italic) {
    var key = name + "|" + pixels + "|" + weight + "|" + italic;
    var metrics = _measured[key];
    if (metrics === undefined) {
        metrics = Qt.createQmlObject("import QtQuick; FontMetrics {}", Qt.application, "eldur_fonts");
        metrics.font = Qt.font({ family: name, pixelSize: pixels, weight: weight, italic: italic });
        _measured[key] = metrics;
    }
    return metrics;
}

// A face's hhea ascent, descent and line gap in em, which is what Chromium lays
// out with and Qt does not expose: `null` for a face nobody measured.
function _face(name, bold, italic) {
    var face = TABLE.faces[name.toLowerCase()];
    if (face === undefined) {
        return null;
    }
    var style = bold ? (italic ? "bolditalic" : "bold") : (italic ? "italic" : "regular");
    return face[style] || face.regular;
}

// The size Chromium uses is fractional and Qt's is whole; the fractional one is
// passed along, and stands for as long as the font still has that whole size.
function _cssSize(pixels, size) {
    return size !== undefined && Math.max(1, Math.round(size)) === pixels ? size : pixels;
}

// Chromium rounds ascent, descent and gap each, at the fractional size.
function _extents(name, pixels, weight, italic, size) {
    var bold = weight >= 600;
    var face = _face(name, bold, italic);
    var css = _cssSize(pixels, size);
    if (face !== null) {
        return { ascent: Math.round(face[0] * css), descent: Math.round(face[1] * css), gap: Math.round(face[2] * css) };
    }
    var qt = _qtFont(name, pixels, weight, italic);
    var scale = css / pixels;
    return { ascent: Math.round(qt.ascent * scale), descent: Math.round(qt.descent * scale), gap: Math.round(qt.leading * scale) };
}

// A resolved `font`'s rounded ascent, descent and gap.
function extents(font, size) {
    return _extents(font.family, font.pixelSize, font.weight, font.italic, size);
}

// `line-height: normal` of a resolved `font`.
function normalLine(font, size) {
    var e = extents(font, size);
    return e.ascent + e.descent + e.gap;
}

// What moves a fixed-height Qt line's glyphs to Chromium's baseline: Qt's baseline
// is the font's rounded ascent below the line's top. `line` 0 is `normal`.
function shift(stack, size, weight, italic, line) {
    var name = family(stack);
    var pixels = Math.max(1, Math.round(size));
    var e = _extents(name, pixels, weight, italic, size);
    var leading = line > 0 ? Math.round(line * 64) / 64 - e.ascent - e.descent : e.gap;
    // In Chromium's 1/64 px units, so that 19.2 - 17 is not just under 2.2.
    var base = e.ascent + Math.floor(leading / 2 + 1e-4);
    return base - Math.round(_qtFont(name, pixels, weight, italic).ascent);
}

// Qt gives a fixed-height text's last line the font's own height when that is
// taller; CSS keeps every line box `line` tall. Whole, as Qt rounds a `Text`'s
// height up and the column carries that (layout.md §1.1).
function slack(font, line) {
    return Math.min(0, Math.ceil(line) - Math.ceil(_qtFont(font.family, font.pixelSize, font.weight, font.italic).height));
}

// Chromium's boxes are in 1/64 px, and what it works out is cut down to them.
function _unit(px) {
    return Math.floor(px * 64 + 1e-3) / 64;
}

// Bold and slanted faces of some families have another x-height, which `middle`
// sits on.
function _xHeightOf(face, box) {
    if (!face) {
        return 0.5;
    }
    return face.xs ? face.xs[((box.weight || 400) >= 600 ? 1 : 0) + (box.italic ? 2 : 0)] : face.x;
}

// One inline box on its own baseline (layout.md §1.3). `box` has the CSS size and
// stack; `line` absent is `normal`. Above and below take the half-leading, floored.
function inlineBox(box) {
    var name = family(box.family);
    var size = box.size;
    var e = _extents(name, Math.max(1, Math.round(size)), box.weight || 400, !!box.italic, size);
    var line = box.line > 0 ? _unit(box.line) : e.ascent + e.descent + e.gap;
    var above = e.ascent + Math.floor((line - e.ascent - e.descent) / 2 + 1e-4);
    var face = TABLE.faces[name.toLowerCase()];
    return { size: size, ascent: e.ascent, descent: e.descent, above: above, below: line - above,
             xHeight: _xHeightOf(face, box) * size };
}

// How far below its parent's baseline `vertical-align` puts a box. Top and bottom
// are not here: they align against the finished line.
function _alignShift(level, own, parent) {
    switch (level.align) {
    case "sub": return _unit(parent.size / 5 + 1);
    case "super": return -_unit(parent.size / 3 + 1);
    case "middle": return Math.floor(-parent.xHeight / 2 * 64) / 64 + (own.above - own.below) / 2;
    case "text-top": return own.above - parent.ascent;
    case "text-bottom": return parent.descent - own.below;
    }
    if (level.percent) {
        return -_unit((own.above + own.below) * level.percent / 100);
    }
    return level.shift ? -_unit(level.shift) : 0;
}

// Where a run's boxes reach about the baseline it hangs from, in order from the
// block's strut: `boxes` extend the line (or their `group`, for one aligned against
// the line), and `shift` is how far below that baseline the run's own text is.
function placeRun(levels, strut) {
    var parent = strut;
    var shift = 0;
    var group = null;
    var boxes = [];
    for (var i = 0; i < levels.length; ++i) {
        var own = inlineBox(levels[i]);
        if (levels[i].align === "top" || levels[i].align === "bottom") {
            group = { align: levels[i].align, id: levels[i].group, above: 0, below: 0 };
            shift = 0;
        } else {
            shift += _alignShift(levels[i], own, parent);
        }
        boxes.push({ above: own.above - shift, below: own.below + shift, group: group });
        parent = own;
    }
    return { boxes: boxes, shift: shift, group: group, box: parent };
}
