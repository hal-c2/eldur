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
  "liberation sans": {"regular": [0.905273, 0.211914, 0.032715]},
  "liberation serif": {"regular": [0.891113, 0.216309, 0.042480]},
  "liberation mono": {"regular": [0.832520, 0.300293, 0]},
  "nimbus sans": {"regular": [0.729, 0.271, 0.2], "italic": [0.9, 0.25, 0.2], "bolditalic": [0.9, 0.25, 0.2]},
  "nimbus roman": {"regular": [0.683, 0.317, 0.2], "bold": [0.676, 0.324, 0.2], "bolditalic": [0.699, 0.301, 0.2]},
  "nimbus mono ps": {"regular": [0.603, 0.397, 0.2]},
  "nimbus sans narrow": {"regular": [0.718, 0.282, 0.2]},
  "noto sans": {"regular": [1.069, 0.293, 0]},
  "noto serif": {"regular": [1.069, 0.293, 0]},
  "noto sans mono": {"regular": [1.069, 0.293, 0]},
  "jetbrainsmono nerd font": {"regular": [1.02, 0.3, 0]}
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
