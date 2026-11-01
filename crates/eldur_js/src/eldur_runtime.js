.pragma library
.import QtQuick.LocalStorage 2.0 as QmlLocalStorage

// Eldur JavaScript runtime shim (eldur_runtime.js)
//
// Imported by generated page.js files via:
//   .import "eldur_runtime.js" as Eldur
//
// Responsibilities:
// 1. Polyfills for ES2019–ES2022 standard library methods missing from Qt 6.11's
//    V4 JavaScript engine (Array.prototype.at/flat/flatMap, String.prototype.at/
//    replaceAll/trimStart/trimEnd, Object.fromEntries/hasOwn, Promise.prototype.finally,
//    Promise.allSettled, Promise.any), plus a generator driver for lowered async/await.
// 2. Browser globals absent in QML: setTimeout/setInterval/clearTimeout/clearInterval,
//    requestAnimationFrame/cancelAnimationFrame, localStorage (QtQuick.LocalStorage),
//    sessionStorage, fetch (XMLHttpRequest + Promise), alert, performance, window,
//    location, history, navigator, matchMedia.
// 3. Dynamic DOM fallback shim (Eldur.document, Eldur.classList, Eldur.dataset,
//    Eldur.setStyle, Eldur.setAttribute, Eldur.setInnerHTML, Eldur.listen) and DOM-like
//    event normalization (Eldur.event).
//
// Converter assumption for dynamic DOM lookups:
//   Eldur.document.getElementById(id) and Eldur.document.querySelector("#" + id)
//   walk the QML item tree from the root passed to Eldur.init(rootItem) and match
//   items whose `objectName` equals the HTML element's `id`. Tag and class
//   queries match optional `eldurTag` / `eldurClasses` properties or runtime
//   classList state recorded through Eldur.classList(item).

// ---------------------------------------------------------------------------
// 1. Standard library polyfills for Qt 6.11
// ---------------------------------------------------------------------------

(function installPolyfills() {
    if (!Array.prototype.at) {
        Object.defineProperty(Array.prototype, "at", {
            configurable: true,
            writable: true,
            value: function(n) {
                var i = Math.trunc(n) || 0;
                if (i < 0) {
                    i += this.length;
                }
                if (i < 0 || i >= this.length) {
                    return undefined;
                }
                return this[i];
            }
        });
    }

    if (!Array.prototype.flat) {
        Object.defineProperty(Array.prototype, "flat", {
            configurable: true,
            writable: true,
            value: function(depth) {
                var maxDepth = depth === undefined ? 1 : Number(depth);
                var result = [];
                function flattenInto(arr, currentDepth) {
                    for (var i = 0; i < arr.length; ++i) {
                        if (!(i in arr)) {
                            continue;
                        }
                        var el = arr[i];
                        if (Array.isArray(el) && currentDepth > 0) {
                            flattenInto(el, currentDepth - 1);
                        } else {
                            result.push(el);
                        }
                    }
                }
                flattenInto(this, maxDepth);
                return result;
            }
        });
    }

    if (!Array.prototype.flatMap) {
        Object.defineProperty(Array.prototype, "flatMap", {
            configurable: true,
            writable: true,
            value: function(callback, thisArg) {
                return this.map(callback, thisArg).flat(1);
            }
        });
    }

    if (!String.prototype.at) {
        Object.defineProperty(String.prototype, "at", {
            configurable: true,
            writable: true,
            value: function(n) {
                var str = String(this);
                var i = Math.trunc(n) || 0;
                if (i < 0) {
                    i += str.length;
                }
                if (i < 0 || i >= str.length) {
                    return undefined;
                }
                return str.charAt(i);
            }
        });
    }

    if (!String.prototype.replaceAll) {
        Object.defineProperty(String.prototype, "replaceAll", {
            configurable: true,
            writable: true,
            value: function(search, replacement) {
                var str = String(this);
                if (search instanceof RegExp) {
                    if (String(search.flags).indexOf("g") === -1) {
                        throw new TypeError("String.prototype.replaceAll called with a non-global RegExp");
                    }
                    return str.replace(search, replacement);
                }
                var needle = String(search);
                if (typeof replacement === "function") {
                    if (needle === "") {
                        var out = replacement("", 0, str);
                        for (var i = 0; i < str.length; ++i) {
                            out += str.charAt(i) + replacement("", i + 1, str);
                        }
                        return out;
                    }
                    var res = "";
                    var pos = 0;
                    var idx = str.indexOf(needle, pos);
                    while (idx !== -1) {
                        res += str.slice(pos, idx) + replacement(needle, idx, str);
                        pos = idx + needle.length;
                        idx = str.indexOf(needle, pos);
                    }
                    return res + str.slice(pos);
                }
                return str.split(needle).join(String(replacement));
            }
        });
    }

    if (!String.prototype.trimStart) {
        Object.defineProperty(String.prototype, "trimStart", {
            configurable: true,
            writable: true,
            value: function() {
                return String(this).replace(/^\s+/, "");
            }
        });
    }

    if (!String.prototype.trimEnd) {
        Object.defineProperty(String.prototype, "trimEnd", {
            configurable: true,
            writable: true,
            value: function() {
                return String(this).replace(/\s+$/, "");
            }
        });
    }

    if (!Object.fromEntries) {
        Object.fromEntries = function(entries) {
            if (entries === null || entries === undefined) {
                throw new TypeError("Object.fromEntries requires an iterable");
            }
            var obj = {};
            for (var entry of entries) {
                if (entry === null || typeof entry !== "object") {
                    throw new TypeError("Iterator value is not an entry object");
                }
                obj[entry[0]] = entry[1];
            }
            return obj;
        };
    }

    if (!Object.hasOwn) {
        Object.hasOwn = function(obj, prop) {
            return Object.prototype.hasOwnProperty.call(Object(obj), prop);
        };
    }

    if (typeof Promise !== "undefined") {
        if (!Promise.prototype.finally) {
            Object.defineProperty(Promise.prototype, "finally", {
                configurable: true,
                writable: true,
                value: function(onFinally) {
                    var P = this.constructor || Promise;
                    return this.then(
                        function(value) {
                            return P.resolve(typeof onFinally === "function" ? onFinally() : undefined)
                                .then(function() { return value; });
                        },
                        function(reason) {
                            return P.resolve(typeof onFinally === "function" ? onFinally() : undefined)
                                .then(function() { throw reason; });
                        }
                    );
                }
            });
        }

        if (!Promise.allSettled) {
            Promise.allSettled = function(iterable) {
                var items = Array.from(iterable);
                return Promise.all(items.map(function(p) {
                    return Promise.resolve(p).then(
                        function(value) { return { status: "fulfilled", value: value }; },
                        function(reason) { return { status: "rejected", reason: reason }; }
                    );
                }));
            };
        }

        if (!Promise.any) {
            Promise.any = function(iterable) {
                var items = Array.from(iterable);
                return new Promise(function(resolve, reject) {
                    if (items.length === 0) {
                        reject(new Error("All promises were rejected"));
                        return;
                    }
                    var errors = new Array(items.length);
                    var remaining = items.length;
                    items.forEach(function(p, index) {
                        Promise.resolve(p).then(resolve, function(err) {
                            errors[index] = err;
                            remaining -= 1;
                            if (remaining === 0) {
                                var agg = new Error("All promises were rejected");
                                agg.errors = errors;
                                reject(agg);
                            }
                        });
                    });
                });
            };
        }
    }
})();

// Drives a generator function lowered from an ES2017 `async` function.
function asyncRun(thisArg, genFn) {
    return new Promise(function(resolve, reject) {
        var gen = genFn.call(thisArg);
        function step(nextMethod, arg) {
            var result;
            try {
                result = nextMethod.call(gen, arg);
            } catch (err) {
                reject(err);
                return;
            }
            if (result.done) {
                resolve(result.value);
            } else {
                Promise.resolve(result.value).then(
                    function(val) { step(gen.next, val); },
                    function(err) { step(gen["throw"], err); }
                );
            }
        }
        step(gen.next, undefined);
    });
}

// Helper for lowered object rest destructuring: `const { a, ...rest } = src`.
function objectRest(source, excludedKeys) {
    var target = {};
    if (source === null || source === undefined) {
        return target;
    }
    var obj = Object(source);
    var keys = Object.keys(obj);
    for (var i = 0; i < keys.length; ++i) {
        var key = keys[i];
        if (excludedKeys.indexOf(key) === -1) {
            target[key] = obj[key];
        }
    }
    return target;
}

// ---------------------------------------------------------------------------
// 2. Root context and lifecycle
// ---------------------------------------------------------------------------

var _root = null;
var _holder = null;
var _startTime = Date.now();
var _fallbackTitle = "";
var _alertHook = null;
var _activeElement = null;
var _clipboardText = "";
var _explicitBaseUrl = "";
var _explicitDocumentUrl = "";

// Per-item sidecar storage keyed by raw QML QObject (or synthetic node).
// QML QObjects reject ad-hoc JS property assignments, whereas ES6 Map keys
// preserve QObject identity across direct id references and tree walks.
var _itemMeta = new Map();
var _warnedMissingTemplates = {};
var _componentProbeCache = new Map();

function init(rootItem, baseUrl, documentUrl) {
    if (rootItem) {
        _root = rootItem;
    }
    _warnedMissingTemplates = {};
    _componentProbeCache = new Map();
    _explicitBaseUrl = "";
    _explicitDocumentUrl = "";
    if (typeof documentUrl === "string" && documentUrl !== "") {
        _explicitDocumentUrl = documentUrl;
    } else if (_root && typeof _root.documentUrl === "string" && _root.documentUrl !== "") {
        _explicitDocumentUrl = _root.documentUrl;
    }
    if (typeof baseUrl === "string" && baseUrl !== "") {
        _explicitBaseUrl = baseUrl;
    } else if (_root && typeof _root.baseUrl === "string" && _root.baseUrl !== "") {
        _explicitBaseUrl = _root.baseUrl;
    }
    var docUrl = _explicitDocumentUrl !== "" ? _explicitDocumentUrl : _explicitBaseUrl;
    if (docUrl !== "") {
        _updateLocationFromHref(docUrl, false);
    }
    _vModels = [];
    _vRules = null;
    _gCascadeRules = null;
    _gPaintTable = null;
    _gWarned = {};
    _gMarginHelper = null;
    _gStateRules = null;
    if (_vHasRules() && _root) {
        // What the converter wrote for a `Text` holds until the rules that
        // match its nodes change, so they must be read before any write.
        _walkItems(_root, function() { return false; });
        _gLive = [];
        _gStaticBase = null;
        _gSnapshotStatics();
    }
    if (_gHasCascade() && !_gActive()) {
        _gSnapshotClosedHidden();
    }
    _startTime = Date.now();
    if (_root) {
        _connectControls(_root);
        _walkItems(_root, function(item) {
            if (item.eldurTag === "details") {
                _setUpDetails(item);
            }
            return false;
        });
        _tableSetUp();
        _pseudoLoad();
    }
}

// A `<details>` shows its summary, and the rest when open.
function _setUpDetails(raw) {
    var kids = _rawElementChildren(raw);
    var summary = kids.filter(function(k) { return _nodeTag(k) === "summary"; })[0];
    _detailsShow(raw, raw.eldurOpen === true);
    if (summary) {
        wrap(summary).addEventListener("click", function() {
            summary.forceActiveFocus();
            wrap(raw).open = !_meta(raw).detailsOpen;
        });
    }
}

function _detailsShow(raw, open) {
    _meta(raw).detailsOpen = open;
    if (open) {
        setAttribute(raw, "open", "");
    } else {
        removeAttribute(raw, "open");
    }
    if (_gHasCascade()) {
        _gTouch(raw);
    }
    _rawElementChildren(raw).forEach(function(kid) {
        if (_nodeTag(kid) !== "summary") {
            kid.visible = open && _gStyleAllowsShow(kid);
        }
    });
    _layoutChanged(raw);
}

function _gStyleAllowsShow(kid) {
    return !_gHasCascade() || _gStyle(kid).display !== "none";
}

function _isQObject(obj) {
    if (!obj || typeof obj !== "object") {
        return false;
    }
    if (obj.__eldurWrapper || obj.__eldurRaw) {
        return true;
    }
    if (_itemMeta.has(obj)) {
        return true;
    }
    if (typeof obj.objectName === "string" && typeof obj.destroy === "function") {
        return true;
    }
    var s = "";
    try {
        s = Object.prototype.toString.call(obj);
    } catch (_e) {
        return false;
    }
    if (s !== "[object Object]" && s !== "[object Array]" && s !== "[object Function]"
        && s !== "[object Error]" && s !== "[object RegExp]" && s !== "[object Date]"
        && s !== "[object Map]" && s !== "[object Set]" && s !== "[object Promise]") {
        return true;
    }
    return false;
}

function _isDomTarget(obj) {
    if (!obj || typeof obj !== "object") {
        return false;
    }
    if (obj === window || obj === document || obj === _root || obj.__eldurWrapper || obj.__eldurRaw || obj.__eldurEventTarget) {
        return true;
    }
    return _isQObject(obj);
}

// An `HTMLCollection` follows the document: every read asks again. The cost is
// one tree walk per read, so an indexed loop over one is quadratic, where a
// browser caches; a script that holds one across a mutation needs the answer
// more than the speed.
function _liveCollection(elements, truncate) {
    function items() {
        return elements();
    }
    function index(key) {
        if (typeof key !== "string" || !/^(0|[1-9][0-9]*)$/.test(key)) {
            return -1;
        }
        return Number(key);
    }
    var methods = {
        item: function(idx) {
            var n = Number(idx);
            var list = items();
            return (n >= 0 && n < list.length) ? wrap(list[n]) : null;
        },
        namedItem: function(name) {
            var key = String(name);
            if (!key) {
                return null;
            }
            var list = items();
            for (var i = 0; i < list.length; ++i) {
                if (_nodeId(list[i]) === key || getAttribute(list[i], "name") === key) {
                    return wrap(list[i]);
                }
            }
            return null;
        }
    };
    return new Proxy({}, {
        set: function(_target, key, value) {
            if (key === "length" && truncate) {
                truncate(Number(value));
                return true;
            }
            return false;
        },
        get: function(_target, key) {
            if (key === "length") {
                return items().length;
            }
            if (key === Symbol.iterator) {
                return function() {
                    return items().map(wrap)[Symbol.iterator]();
                };
            }
            if (Object.prototype.hasOwnProperty.call(methods, key)) {
                return methods[key];
            }
            var i = index(key);
            if (i >= 0) {
                var list = items();
                return i < list.length ? wrap(list[i]) : undefined;
            }
            // `col.name` is `col.namedItem("name")`.
            if (typeof key === "string") {
                var named = methods.namedItem(key);
                return named === null ? undefined : named;
            }
            return undefined;
        },
        has: function(_target, key) {
            var i = index(key);
            return key === "length" || Object.prototype.hasOwnProperty.call(methods, key)
                || (i >= 0 && i < items().length);
        },
        ownKeys: function() {
            return items().map(function(_e, i) { return String(i); });
        },
        getOwnPropertyDescriptor: function(_target, key) {
            var i = index(key);
            var list = items();
            if (i >= 0 && i < list.length) {
                return { value: wrap(list[i]), writable: false, enumerable: true, configurable: true };
            }
            return undefined;
        }
    });
}

// What compile-time resolution found, in the shape the method returns. The
// analysis proved the list cannot change.
function staticNodeList(items) {
    return _asNodeList(items.map(function(item) { return wrap(item); }));
}

function staticCollection(items) {
    return _asHTMLCollection(items.map(function(item) { return wrap(item); }));
}

function _asNodeList(arr) {
    Object.defineProperty(arr, "item", {
        configurable: true,
        writable: true,
        enumerable: false,
        value: function(idx) {
            var n = Number(idx);
            if (isNaN(n) || n < 0 || n >= arr.length) {
                return null;
            }
            return arr[n] || null;
        }
    });
    return arr;
}

function _asHTMLCollection(arr) {
    var col = arr.slice();
    Object.defineProperty(col, "forEach", {
        configurable: true,
        writable: true,
        enumerable: false,
        value: undefined
    });
    Object.defineProperty(col, "item", {
        configurable: true,
        writable: true,
        enumerable: false,
        value: function(idx) {
            var n = Number(idx);
            if (isNaN(n) || n < 0 || n >= col.length) {
                return null;
            }
            return col[n] || null;
        }
    });
    Object.defineProperty(col, "namedItem", {
        configurable: true,
        writable: true,
        enumerable: false,
        value: function(name) {
            var key = String(name);
            if (!key) {
                return null;
            }
            for (var i = 0; i < col.length; ++i) {
                var el = col[i];
                if (el && (el.id === key || el.getAttribute("name") === key)) {
                    return el;
                }
            }
            return null;
        }
    });
    for (var i = 0; i < col.length; ++i) {
        var el = col[i];
        if (el) {
            var idKey = el.id;
            if (idKey && !(idKey in col)) {
                col[idKey] = el;
            }
            var nameKey = el.getAttribute("name");
            if (nameKey && !(nameKey in col)) {
                col[nameKey] = el;
            }
        }
    }
    return col;
}

function _qobjectParent() {
    if (_root) {
        return _root;
    }
    if (!_holder) {
        _holder = Qt.createQmlObject("import QtQuick; QtObject {}", Qt.application, "eldur_holder");
    }
    return _holder;
}

// ---------------------------------------------------------------------------
// 3. Timers and animation frames
// ---------------------------------------------------------------------------

var _nextTimerId = 1;
var _timers = {};

function _scheduleTimer(callback, delay, repeat, extraArgs) {
    if (typeof callback !== "function") {
        return 0;
    }
    var id = _nextTimerId++;
    var ms = Math.max(0, Number(delay) || 0);
    var timer = Qt.createQmlObject(
        "import QtQuick; Timer { running: false }",
        _qobjectParent(),
        "eldur_timer_" + id
    );
    timer.interval = ms;
    timer.repeat = Boolean(repeat);
    _timers[id] = timer;
    timer.triggered.connect(function() {
        if (!_timers[id]) {
            return;
        }
        if (!repeat) {
            delete _timers[id];
            timer.destroy();
        }
        callback.apply(null, extraArgs);
    });
    timer.start();
    return id;
}

function setTimeout(callback, delay) {
    var extra = Array.prototype.slice.call(arguments, 2);
    return _scheduleTimer(callback, delay, false, extra);
}

function setInterval(callback, delay) {
    var extra = Array.prototype.slice.call(arguments, 2);
    return _scheduleTimer(callback, delay, true, extra);
}

function clearTimeout(id) {
    var timer = _timers[id];
    if (!timer) {
        return;
    }
    delete _timers[id];
    timer.running = false;
    timer.destroy();
}

function clearInterval(id) {
    clearTimeout(id);
}

var _nextRafId = 1;
var _rafQueue = [];
var _rafAnim = null;

function _ensureRafAnim() {
    if (_rafAnim) {
        return _rafAnim;
    }
    _rafAnim = Qt.createQmlObject(
        "import QtQuick; FrameAnimation { running: false }",
        _qobjectParent(),
        "eldur_raf"
    );
    _rafAnim.triggered.connect(function() {
        var batch = _rafQueue.slice();
        _rafQueue.length = 0;
        if (_rafQueue.length === 0) {
            _rafAnim.running = false;
        }
        var timestamp = Date.now() - _startTime;
        for (var i = 0; i < batch.length; ++i) {
            batch[i].cb(timestamp);
        }
        if (_rafQueue.length > 0) {
            _rafAnim.running = true;
        }
    });
    return _rafAnim;
}

function requestAnimationFrame(callback) {
    if (typeof callback !== "function") {
        return 0;
    }
    var id = _nextRafId++;
    _rafQueue.push({ id: id, cb: callback });
    var anim = _ensureRafAnim();
    anim.running = true;
    return id;
}

function cancelAnimationFrame(id) {
    for (var i = 0; i < _rafQueue.length; ++i) {
        if (_rafQueue[i].id === id) {
            _rafQueue.splice(i, 1);
            break;
        }
    }
    if (_rafQueue.length === 0 && _rafAnim) {
        _rafAnim.running = false;
    }
}

function queueMicrotask(callback) {
    Promise.resolve().then(callback);
}

function structuredClone(value) {
    if (value === undefined) {
        return undefined;
    }
    return JSON.parse(JSON.stringify(value));
}

var performance = {
    now: function() {
        return Date.now() - _startTime;
    }
};

var _B64_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

function btoa(raw) {
    var str = String(raw);
    var out = "";
    for (var i = 0; i < str.length; i += 3) {
        var c1 = str.charCodeAt(i);
        var has2 = i + 1 < str.length;
        var has3 = i + 2 < str.length;
        var c2 = has2 ? str.charCodeAt(i + 1) : 0;
        var c3 = has3 ? str.charCodeAt(i + 2) : 0;
        if (c1 > 255 || c2 > 255 || c3 > 255) {
            throw new Error("InvalidCharacterError: btoa input out of Latin1 range");
        }
        var triplet = (c1 << 16) | (c2 << 8) | c3;
        out += _B64_CHARS.charAt((triplet >> 18) & 63);
        out += _B64_CHARS.charAt((triplet >> 12) & 63);
        out += has2 ? _B64_CHARS.charAt((triplet >> 6) & 63) : "=";
        out += has3 ? _B64_CHARS.charAt(triplet & 63) : "=";
    }
    return out;
}

function atob(encoded) {
    var str = String(encoded).replace(/\s+/g, "");
    if (str.length % 4 === 1) {
        throw new Error("InvalidCharacterError: invalid base64 length");
    }
    var out = "";
    var buf = 0;
    var bits = 0;
    for (var i = 0; i < str.length; ++i) {
        var ch = str.charAt(i);
        if (ch === "=") {
            break;
        }
        var val = _B64_CHARS.indexOf(ch);
        if (val === -1) {
            throw new Error("InvalidCharacterError: invalid base64 character");
        }
        buf = (buf << 6) | val;
        bits += 6;
        if (bits >= 8) {
            bits -= 8;
            out += String.fromCharCode((buf >> bits) & 255);
        }
    }
    return out;
}

// ---------------------------------------------------------------------------
// 4. Storage (localStorage via QtQuick.LocalStorage, sessionStorage in-memory)
// ---------------------------------------------------------------------------

function _createMemoryStorage() {
    var store = {};
    var order = [];
    var api = {
        getItem: function(key) {
            var k = String(key);
            return Object.prototype.hasOwnProperty.call(store, k) ? store[k] : null;
        },
        setItem: function(key, value) {
            var k = String(key);
            if (!Object.prototype.hasOwnProperty.call(store, k)) {
                order.push(k);
            }
            store[k] = String(value);
        },
        removeItem: function(key) {
            var k = String(key);
            if (Object.prototype.hasOwnProperty.call(store, k)) {
                delete store[k];
                var idx = order.indexOf(k);
                if (idx !== -1) {
                    order.splice(idx, 1);
                }
            }
        },
        clear: function() {
            store = {};
            order.length = 0;
        },
        key: function(index) {
            var i = Number(index) || 0;
            return i >= 0 && i < order.length ? order[i] : null;
        }
    };
    Object.defineProperty(api, "length", {
        enumerable: true,
        get: function() {
            return order.length;
        }
    });
    return api;
}

var sessionStorage = _createMemoryStorage();

var localStorage = (function() {
    var fallback = _createMemoryStorage();
    var db = null;
    var dbFailed = false;

    function getDb() {
        if (db) {
            return db;
        }
        if (dbFailed) {
            return null;
        }
        try {
            db = QmlLocalStorage.LocalStorage.openDatabaseSync(
                "eldur_local_storage",
                "1.0",
                "Eldur localStorage",
                1048576
            );
            db.transaction(function(tx) {
                tx.executeSql("CREATE TABLE IF NOT EXISTS eldur_kv (k TEXT PRIMARY KEY, v TEXT NOT NULL)");
            });
            return db;
        } catch (err) {
            console.warn("eldur: LocalStorage unavailable, falling back to memory:", err);
            dbFailed = true;
            return null;
        }
    }

    var api = {
        getItem: function(key) {
            var k = String(key);
            var handle = getDb();
            if (!handle) {
                return fallback.getItem(k);
            }
            var found = null;
            handle.readTransaction(function(tx) {
                var rs = tx.executeSql("SELECT v FROM eldur_kv WHERE k = ?", [k]);
                if (rs.rows.length > 0) {
                    found = String(rs.rows.item(0).v);
                }
            });
            return found;
        },
        setItem: function(key, value) {
            var k = String(key);
            var v = String(value);
            var handle = getDb();
            if (!handle) {
                fallback.setItem(k, v);
                return;
            }
            handle.transaction(function(tx) {
                tx.executeSql("INSERT OR REPLACE INTO eldur_kv (k, v) VALUES (?, ?)", [k, v]);
            });
        },
        removeItem: function(key) {
            var k = String(key);
            var handle = getDb();
            if (!handle) {
                fallback.removeItem(k);
                return;
            }
            handle.transaction(function(tx) {
                tx.executeSql("DELETE FROM eldur_kv WHERE k = ?", [k]);
            });
        },
        clear: function() {
            var handle = getDb();
            if (!handle) {
                fallback.clear();
                return;
            }
            handle.transaction(function(tx) {
                tx.executeSql("DELETE FROM eldur_kv");
            });
        },
        key: function(index) {
            var i = Number(index) || 0;
            if (i < 0) {
                return null;
            }
            var handle = getDb();
            if (!handle) {
                return fallback.key(i);
            }
            var found = null;
            handle.readTransaction(function(tx) {
                var rs = tx.executeSql("SELECT k FROM eldur_kv ORDER BY rowid ASC LIMIT 1 OFFSET ?", [i]);
                if (rs.rows.length > 0) {
                    found = String(rs.rows.item(0).k);
                }
            });
            return found;
        }
    };

    Object.defineProperty(api, "length", {
        enumerable: true,
        get: function() {
            var handle = getDb();
            if (!handle) {
                return fallback.length;
            }
            var count = 0;
            handle.readTransaction(function(tx) {
                var rs = tx.executeSql("SELECT COUNT(*) AS c FROM eldur_kv");
                if (rs.rows.length > 0) {
                    count = Number(rs.rows.item(0).c) || 0;
                }
            });
            return count;
        }
    });

    return api;
})();

// ---------------------------------------------------------------------------
// 5. Fetch (Promise wrapper around QML's XMLHttpRequest)
// ---------------------------------------------------------------------------

function fetch(input, initOpts) {
    var url = String(input);
    var opts = initOpts || {};
    var method = opts.method ? String(opts.method).toUpperCase() : "GET";

    return new Promise(function(resolve, reject) {
        var xhr = new XMLHttpRequest();
        xhr.open(method, url, true);

        if (opts.headers && typeof opts.headers === "object") {
            var headerNames = Object.keys(opts.headers);
            for (var i = 0; i < headerNames.length; ++i) {
                var h = headerNames[i];
                xhr.setRequestHeader(h, String(opts.headers[h]));
            }
        }

        xhr.onreadystatechange = function() {
            if (xhr.readyState !== XMLHttpRequest.DONE) {
                return;
            }
            var bodyText = xhr.responseText || "";
            var rawStatus = Number(xhr.status) || 0;
            var isLocalOk = rawStatus === 0 && url.indexOf("http://") !== 0 && url.indexOf("https://") !== 0 && bodyText.length > 0;
            var status = isLocalOk ? 200 : rawStatus;
            var ok = status >= 200 && status < 300;
            resolve({
                ok: ok,
                status: status,
                statusText: xhr.statusText || (ok ? "OK" : ""),
                url: url,
                headers: {
                    get: function(name) {
                        return xhr.getResponseHeader(String(name));
                    }
                },
                text: function() {
                    return Promise.resolve(bodyText);
                },
                json: function() {
                    return new Promise(function(res, rej) {
                        try {
                            res(JSON.parse(bodyText));
                        } catch (err) {
                            rej(err);
                        }
                    });
                }
            });
        };

        xhr.onerror = function() {
            reject(new TypeError("Network request failed for " + url));
        };

        if (opts.body !== undefined && opts.body !== null) {
            xhr.send(opts.body);
        } else {
            xhr.send();
        }
    });
}

// ---------------------------------------------------------------------------
// 6. Browser globals: alert, title, window, location, history, navigator
// ---------------------------------------------------------------------------

function alert(message) {
    var text = message === undefined ? "" : String(message);
    console.warn("[alert]", text);
    if (typeof _alertHook === "function") {
        _alertHook(text);
    }
}

function setAlertHandler(fn) {
    _alertHook = typeof fn === "function" ? fn : null;
}

function confirm(message) {
    console.warn("eldur: synchronous confirm() is unsupported:", message);
    return false;
}

function prompt(message, defaultValue) {
    console.warn("eldur: synchronous prompt() is unsupported:", message, defaultValue);
    return null;
}

function setTitle(newTitle) {
    var text = String(newTitle);
    _fallbackTitle = text;
    if (_root && typeof _root.title === "string") {
        _root.title = text;
    }
}

function getTitle() {
    if (_root && typeof _root.title === "string") {
        return _root.title;
    }
    return _fallbackTitle;
}

function consoleTable(data) {
    console.log(JSON.stringify(data));
}

function unsupported(feature) {
    console.warn("eldur: unsupported browser API:", feature);
    return undefined;
}

// Note on document base URL resolution for `a.href`:
// `Eldur.init(rootItem)` receives only the root QML Item (`Main.qml` calls
// `Page.__eldur_init(root)`), and the HTML-to-QML converter does not embed a
// source document URL into `Main.qml` or `page.js`. Consequently,
// `location.href` starts as `""`.
// - When `rawHref` is an absolute URL (e.g. `https://example.com` or
//   `https://example.com/a/b/../c`), `_resolveUrl` normalizes it per the
//   WHATWG URL / DOM `HTMLAnchorElement.href` rules (ensuring a trailing `/`
//   on bare origins and collapsing `.` / `..` path segments, which Qt 6.11's
//   built-in `new URL()` does not do).
// - When `rawHref` is a relative URL and a base URL is available via
//   `Eldur.init(rootItem, baseUrl)` or `_root.baseUrl`, `_resolveUrl` resolves `rawHref`
//   against that base URL.
// - When `rawHref` is a relative URL and no base URL is known,
//   `_resolveUrl` returns the raw attribute value unchanged because the runtime
//   has no document base URL to resolve against.
function _normalizeUrlPath(path) {
    var parts = String(path || "/").split("/");
    var stack = [];
    for (var i = 0; i < parts.length; ++i) {
        var seg = parts[i];
        if (seg === "" || seg === ".") {
            continue;
        }
        if (seg === "..") {
            if (stack.length > 0) {
                stack.pop();
            }
        } else {
            stack.push(seg);
        }
    }
    var trailingSlash = path.length > 1 && path.charAt(path.length - 1) === "/" ||
        path.slice(-2) === "/." || path.slice(-3) === "/..";
    return "/" + stack.join("/") + (trailingSlash && stack.length > 0 ? "/" : "");
}

function _parseHierarchicalUrl(urlStr) {
    var m = /^([a-zA-Z][a-zA-Z0-9+.-]*:)\/\/([^\/?#]*)([^?#]*)(\?[^#]*)?(#.*)?$/.exec(String(urlStr));
    if (!m) {
        return null;
    }
    var protocol = m[1].toLowerCase();
    var host = m[2];
    var portIdx = host.indexOf(":");
    var hostname = portIdx === -1 ? host : host.slice(0, portIdx);
    var port = portIdx === -1 ? "" : host.slice(portIdx + 1);
    var rawPath = m[3] || "/";
    var pathname = _normalizeUrlPath(rawPath);
    var search = m[4] || "";
    var hash = m[5] || "";
    var origin = (protocol === "http:" || protocol === "https:") ? (protocol + "//" + host) : "null";
    return {
        protocol: protocol,
        host: host,
        hostname: hostname,
        port: port,
        pathname: pathname,
        search: search,
        hash: hash,
        origin: origin,
        href: protocol + "//" + host + pathname + search + hash
    };
}

function _getDocumentUrl() {
    if (_locState.href !== "") {
        return _locState.href;
    }
    if (_explicitDocumentUrl !== "") {
        return _explicitDocumentUrl;
    }
    if (_root && typeof _root.documentUrl === "string" && _root.documentUrl !== "") {
        return _root.documentUrl;
    }
    if (_explicitBaseUrl !== "") {
        return _explicitBaseUrl;
    }
    if (_root && typeof _root.baseUrl === "string" && _root.baseUrl !== "") {
        return _root.baseUrl;
    }
    return "";
}

function _findBaseElementHref() {
    if (!_root) {
        return null;
    }
    var foundHref = null;
    _walkItems(_root, function(node) {
        if (node === _root) {
            return false;
        }
        var tag = typeof node.eldurTag === "string"
            ? node.eldurTag.toLowerCase()
            : (_meta(node).createdTag || "");
        if (tag === "base") {
            var h = getAttribute(node, "href");
            if (h !== null) {
                foundHref = String(h).trim();
                return true;
            }
        }
        return false;
    });
    return foundHref;
}

function _getDocumentBaseUrl() {
    var docUrl = _getDocumentUrl();
    var baseElemHref = _findBaseElementHref();
    if (baseElemHref !== null) {
        return baseElemHref === "" ? docUrl : _resolveUrlWithBase(baseElemHref, docUrl);
    }
    if (_explicitBaseUrl !== "") {
        return docUrl !== "" ? _resolveUrlWithBase(_explicitBaseUrl, docUrl) : _explicitBaseUrl;
    }
    if (_root && typeof _root.baseUrl === "string" && _root.baseUrl !== "") {
        return docUrl !== "" ? _resolveUrlWithBase(_root.baseUrl, docUrl) : _root.baseUrl;
    }
    return docUrl;
}

function _resolveUrl(rawHref) {
    return _resolveUrlWithBase(rawHref, _getDocumentBaseUrl());
}

function _resolveUrlWithBase(rawHref, baseStr) {
    if (rawHref === null || rawHref === undefined || rawHref === "") {
        return baseStr;
    }
    var str = String(rawHref).trim();
    if (str === "") {
        return baseStr;
    }
    var parsedAbs = _parseHierarchicalUrl(str);
    if (parsedAbs) {
        return parsedAbs.href;
    }
    if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(str)) {
        return str;
    }
    var base = baseStr ? _parseHierarchicalUrl(baseStr) : null;
    if (!base) {
        // Runtime has no document base URL when converted statically unless
        // `Eldur.init(root, baseUrl)` or `root.baseUrl` was populated; return the raw
        // relative attribute unchanged.
        return String(rawHref);
    }
    if (str.indexOf("//") === 0) {
        var protoRel = _parseHierarchicalUrl(base.protocol + str);
        return protoRel ? protoRel.href : (base.protocol + str);
    }
    if (str.charAt(0) === "#") {
        return base.protocol + "//" + base.host + base.pathname + base.search + str;
    }
    if (str.charAt(0) === "?") {
        var qHashIdx = str.indexOf("#");
        var qPart = qHashIdx === -1 ? str : str.slice(0, qHashIdx);
        var hPart = qHashIdx === -1 ? "" : str.slice(qHashIdx);
        return base.protocol + "//" + base.host + base.pathname + qPart + hPart;
    }
    var relMatch = /^([^?#]*)(\?[^#]*)?(#.*)?$/.exec(str);
    var relPath = relMatch ? relMatch[1] : str;
    var relSearch = relMatch && relMatch[2] ? relMatch[2] : "";
    var relHash = relMatch && relMatch[3] ? relMatch[3] : "";
    var combinedPath;
    if (relPath.charAt(0) === "/") {
        combinedPath = _normalizeUrlPath(relPath);
    } else {
        var lastSlash = base.pathname.lastIndexOf("/");
        var baseDir = lastSlash === -1 ? "/" : base.pathname.slice(0, lastSlash + 1);
        combinedPath = _normalizeUrlPath(baseDir + relPath);
    }
    return base.protocol + "//" + base.host + combinedPath + relSearch + relHash;
}

var _locState = {
    href: "",
    protocol: "file:",
    host: "",
    hostname: "",
    port: "",
    pathname: "/",
    search: "",
    hash: "",
    origin: "null"
};

function _updateLocationFromHref(newHref, fireHashChange) {
    var oldHash = _locState.hash;
    var s = String(newHref);
    var parsed = _parseHierarchicalUrl(s);
    if (parsed) {
        _locState.href = parsed.href;
        _locState.protocol = parsed.protocol;
        _locState.host = parsed.host;
        _locState.hostname = parsed.hostname;
        _locState.port = parsed.port;
        _locState.pathname = parsed.pathname;
        _locState.search = parsed.search;
        _locState.hash = parsed.hash;
        _locState.origin = parsed.origin;
    } else {
        _locState.href = s;
        var hashIdx = s.indexOf("#");
        var qIdx = s.indexOf("?");
        if (hashIdx !== -1) {
            _locState.hash = s.slice(hashIdx);
        } else {
            _locState.hash = "";
        }
        if (qIdx !== -1 && (hashIdx === -1 || qIdx < hashIdx)) {
            _locState.search = hashIdx === -1 ? s.slice(qIdx) : s.slice(qIdx, hashIdx);
        } else {
            _locState.search = "";
        }
        var pathEnd = qIdx !== -1 ? qIdx : (hashIdx !== -1 ? hashIdx : s.length);
        var p = s.slice(0, pathEnd);
        if (p) {
            _locState.pathname = p.charAt(0) === "/" ? p : "/" + p;
        }
    }
    if (fireHashChange && _locState.hash !== oldHash && _root) {
        _dispatchDynamic(_root, "hashchange", null);
    }
}

function _warnUnsupportedNavigation(method, targetUrl) {
    var s = String(targetUrl === undefined ? "" : targetUrl);
    if (s.charAt(0) === "#") {
        location.hash = s;
        return;
    }
    console.warn("eldur: " + method + " navigation is unsupported in QML: " + s);
}

var location = {
    get href() { return _locState.href; },
    set href(v) { _warnUnsupportedNavigation("location.href", v); },
    get protocol() { return _locState.protocol; },
    set protocol(v) { _warnUnsupportedNavigation("location.protocol", v); },
    get host() { return _locState.host; },
    set host(v) { _warnUnsupportedNavigation("location.host", v); },
    get hostname() { return _locState.hostname; },
    set hostname(v) { _warnUnsupportedNavigation("location.hostname", v); },
    get port() { return _locState.port; },
    set port(v) { _warnUnsupportedNavigation("location.port", v); },
    get pathname() { return _locState.pathname; },
    set pathname(v) { _warnUnsupportedNavigation("location.pathname", v); },
    get search() { return _locState.search; },
    set search(v) { _warnUnsupportedNavigation("location.search", v); },
    get hash() { return _locState.hash; },
    set hash(v) {
        var s = String(v);
        var normalized = s === "" ? "" : (s.charAt(0) === "#" ? s : "#" + s);
        var changed = _locState.hash !== normalized;
        _locState.hash = normalized;
        if (_locState.href) {
            var baseHref = _locState.href.split("#")[0];
            _locState.href = baseHref + normalized;
        }
        if (changed && _root) {
            _dispatchDynamic(_root, "hashchange", null);
        }
    },
    get origin() { return _locState.origin; },
    assign: function(url) {
        _warnUnsupportedNavigation("location.assign()", url);
    },
    replace: function(url) {
        _warnUnsupportedNavigation("location.replace()", url);
    },
    reload: function() {
        console.warn("eldur: location.reload() is unsupported");
    },
    toString: function() {
        return _locState.href;
    }
};

var _historyEntries = [{ state: null, url: "" }];
var _historyIndex = 0;

var history = {
    get length() { return _historyEntries.length; },
    get state() { return _historyEntries[_historyIndex].state; },
    pushState: function(state, _unused, url) {
        var resolvedUrl = (url !== undefined && url !== null) ? _resolveUrl(String(url)) : _locState.href;
        _historyEntries = _historyEntries.slice(0, _historyIndex + 1);
        _historyEntries.push({ state: state, url: resolvedUrl });
        _historyIndex = _historyEntries.length - 1;
        if (url !== undefined && url !== null) {
            _updateLocationFromHref(resolvedUrl, false);
        }
    },
    replaceState: function(state, _unused, url) {
        var resolvedUrl = (url !== undefined && url !== null) ? _resolveUrl(String(url)) : _locState.href;
        _historyEntries[_historyIndex] = { state: state, url: resolvedUrl };
        if (url !== undefined && url !== null) {
            _updateLocationFromHref(resolvedUrl, false);
        }
    },
    back: function() {
        this.go(-1);
    },
    forward: function() {
        this.go(1);
    },
    go: function(delta) {
        var step = delta === undefined ? 0 : (Number(delta) || 0);
        if (step === 0) {
            return;
        }
        var nextIdx = _historyIndex + step;
        if (nextIdx >= 0 && nextIdx < _historyEntries.length) {
            _historyIndex = nextIdx;
        }
        var current = _historyEntries[_historyIndex];
        if (current && current.url) {
            _updateLocationFromHref(current.url, false);
        }
        if (_root) {
            var popEv = Event("popstate", { bubbles: false, cancelable: false });
            popEv.state = current ? current.state : null;
            _dispatchDynamic(_root, "popstate", popEv);
        }
    }
};

var navigator = {
    userAgent: "Eldur/0.1.0",
    onLine: true,
    clipboard: {
        writeText: function(text) {
            _clipboardText = String(text === undefined ? "" : text);
            return Promise.resolve();
        },
        readText: function() {
            return Promise.resolve(_clipboardText);
        }
    }
};
Object.defineProperty(navigator, "language", {
    enumerable: true,
    get: function() {
        return String(Qt.locale().name).replace("_", "-");
    }
});
Object.defineProperty(navigator, "languages", {
    enumerable: true,
    get: function() {
        return [String(Qt.locale().name).replace("_", "-")];
    }
});
Object.defineProperty(navigator, "platform", {
    enumerable: true,
    get: function() {
        return String(Qt.platform.os);
    }
});

function matchMedia(query) {
    var q = String(query);
    var width = _root && typeof _root.width === "number" ? _root.width : 0;
    var matches = true;
    var minMatch = /min-width\s*:\s*(\d+(?:\.\d+)?)px/i.exec(q);
    if (minMatch && width < Number(minMatch[1])) {
        matches = false;
    }
    var maxMatch = /max-width\s*:\s*(\d+(?:\.\d+)?)px/i.exec(q);
    if (maxMatch && width > Number(maxMatch[1])) {
        matches = false;
    }
    return {
        matches: matches,
        media: q,
        onchange: null,
        addListener: function() {},
        removeListener: function() {},
        addEventListener: function() {},
        removeEventListener: function() {}
    };
}

// ---------------------------------------------------------------------------
// 7. DOM element helpers, dynamic queries, and Element wrapper
// ---------------------------------------------------------------------------

function _unwrap(item) {
    if (item && typeof item === "object") {
        if (item.__eldurWrapper) {
            return item._item;
        }
        if ((item === window || item === document) && _root) {
            return _root;
        }
        // The script's id still names what the box replaced.
        if (_gAdopted.size > 0 && _gAdopted.has(item)) {
            return _gAdopted.get(item);
        }
    }
    return item;
}

function _isQObject(obj) {
    return Boolean(
        obj
        && typeof obj === "object"
        && typeof obj.objectName === "string"
        && typeof obj.objectNameChanged === "function"
    );
}

function _isPlainJsObject(obj) {
    if (!obj || typeof obj !== "object" || obj.__eldurWrapper || _isQObject(obj)) {
        return false;
    }
    var proto = Object.getPrototypeOf(obj);
    return proto === Object.prototype || proto === null;
}

function _meta(item) {
    var raw = _unwrap(item);
    if (!raw || (typeof raw !== "object" && typeof raw !== "function")) {
        return {
            classes: [],
            attrs: {},
            dataset: {},
            styles: {},
            listeners: {},
            onceListeners: {},
            onHandlers: {},
            connected: {},
            wrapper: null
        };
    }
    var entry = _itemMeta.get(raw);
    if (!entry) {
        var initialClasses = [];
        if (typeof raw.eldurClasses === "string" && raw.eldurClasses.length > 0) {
            var parts = raw.eldurClasses.trim().split(/\s+/);
            for (var i = 0; i < parts.length; ++i) {
                if (parts[i] && initialClasses.indexOf(parts[i]) === -1) {
                    initialClasses.push(parts[i]);
                }
            }
        }
        var initialAttrs = {};
        var initialData = {};
        if (raw.eldurAttrs) {
            var attrNames = Object.keys(raw.eldurAttrs);
            for (var n = 0; n < attrNames.length; ++n) {
                var attrName = attrNames[n];
                initialAttrs[attrName] = raw.eldurAttrs[attrName];
                if (attrName.indexOf("data-") === 0 && attrName.length > 5) {
                    initialData[_kebabToCamel(attrName.slice(5))] = raw.eldurAttrs[attrName];
                }
            }
        }
        entry = {
            classes: initialClasses,
            attrs: initialAttrs,
            dataset: initialData,
            styles: {},
            listeners: {},
            onceListeners: {},
            onHandlers: {},
            connected: {},
            wrapper: null
        };
        _itemMeta.set(raw, entry);
    }
    return entry;
}

// A boolean property is its attribute's presence, which `[hidden]` reads.
function _reflectFlag(m, name, on) {
    if (on) {
        m.attrs[name] = "";
    } else {
        delete m.attrs[name];
    }
}

function _kebabToCamel(attrName) {
    return attrName.replace(/-([a-z])/g, function(_all, ch) {
        return ch.toUpperCase();
    });
}

function _sanitizeClassProp(cls) {
    return "cls_" + String(cls).replace(/[^A-Za-z0-9_]/g, "_");
}

function _syncClassProp(raw, cls, active) {
    if (!raw || typeof raw !== "object") {
        return;
    }
    var prop = _sanitizeClassProp(cls);
    if (typeof raw[prop] === "boolean") {
        raw[prop] = Boolean(active);
    }
}

function _syncEldurClasses(raw, m) {
    if (raw && typeof raw === "object" && typeof raw.eldurClasses === "string") {
        var joined = m.classes.join(" ");
        if (raw.eldurClasses !== joined) {
            raw.eldurClasses = joined;
        }
    }
}

// Generated text in the element's `Text`, kept out of what its content reads:
// a generated box there would be one more thing a line or a column measures apart.
var _pseudoRules = null;
var _pseudoPending = false;

function _pseudoLoad() {
    _pseudoRules = [];
    if (!_root || typeof _root.eldurPseudoRules !== "string") {
        return;
    }
    _pseudoRules = JSON.parse(_root.eldurPseudoRules);
    // What the converter already wrote is matched here, before any class moves.
    _pseudoFlush(true);
}

function _pseudoTouch() {
    if (!_pseudoRules || _pseudoRules.length === 0 || _pseudoPending) {
        return;
    }
    _pseudoPending = true;
    Qt.callLater(function() {
        _pseudoPending = false;
        _pseudoFlush(false);
    });
}

function _pseudoHolder(raw) {
    return typeof raw.text === "string" ? raw : _findTextChild(raw);
}

// `str` without what generated text the element holds.
function _pseudoOwn(raw, str) {
    var p = _meta(raw).pseudo;
    if (!p || p.model) {
        return str;
    }
    var out = String(str);
    if (p.before && out.indexOf(p.before) === 0) {
        out = out.slice(p.before.length);
    }
    if (p.after && out.slice(-p.after.length) === p.after) {
        out = out.slice(0, -p.after.length);
    }
    return out;
}

function _pseudoWrapped(raw, str) {
    var p = _meta(raw).pseudo;
    return p && !p.model ? p.before + str + p.after : str;
}

function _pseudoFlush(baseline) {
    var seen = new Map();
    _pseudoRules.forEach(function(rule, order) {
        var found = document.querySelectorAll(rule.sel);
        for (var i = 0; i < found.length; ++i) {
            var raw = _unwrap(found[i]);
            if (!raw || raw.__eldurInline || !_pseudoHolder(raw)) {
                continue;
            }
            var entry = seen.get(raw) || { before: null, after: null };
            var side = rule.after ? "after" : "before";
            var rank = rule.spec[0] * 1e6 + rule.spec[1] * 1e3 + rule.spec[2];
            if (!entry[side] || rank > entry[side].rank
                || (rank === entry[side].rank && order > entry[side].order)) {
                var text = rule.parts.map(function(part) {
                    return typeof part === "string" ? part : (getAttribute(raw, part.attr) || "");
                }).join("");
                entry[side] = { rank: rank, order: order, text: text };
            }
            seen.set(raw, entry);
        }
    });
    var next = new Set();
    seen.forEach(function(entry, raw) {
        next.add(raw);
        _pseudoSet(raw, entry.before ? entry.before.text : "", entry.after ? entry.after.text : "", baseline);
    });
    _pseudoShown.forEach(function(raw) {
        if (!next.has(raw)) {
            _pseudoSet(raw, "", "", false);
        }
    });
    _pseudoShown = next;
}
var _pseudoShown = new Set();

// A `Text` with a model renders from it: generated text is a node there, which
// no script reads, and a leaf the converter wrote it into gives it back.
function _pseudoInModel(model, raw, before, after, baseline) {
    var m = _meta(raw);
    if (baseline) {
        var leaves = model.kids.filter(function(k) { return k.vtext; });
        var first = leaves[0], last = leaves[leaves.length - 1];
        if ((before && (!first || first.data.indexOf(before) !== 0))
            || (after && (!last || last.data.slice(-after.length) !== after))) {
            return;
        }
        if (before) {
            first.data = first.data.slice(before.length);
        }
        if (after) {
            last.data = last.data.slice(0, -after.length);
        }
    }
    var kids = model.kids.filter(function(k) { return !k.pseudo; });
    if (before) {
        kids.unshift({ gen: { text: before, c: {} }, pseudo: true });
    }
    if (after) {
        kids.push({ gen: { text: after, c: {} }, pseudo: true });
    }
    model.kids = kids;
    m.pseudo = { before: before, after: after, model: true };
    if (!baseline) {
        _vRender(model);
    }
}

function _pseudoSet(raw, before, after, baseline) {
    var m = _meta(raw);
    var was = m.pseudo || { before: "", after: "" };
    if (was.before === before && was.after === after) {
        return;
    }
    var holder = _pseudoHolder(raw);
    var model = _vOwnModel(raw) || _vExisting(raw);
    if (model) {
        _pseudoInModel(model, raw, before, after, baseline);
        return;
    }
    if (baseline) {
        // The converter's text: it holds this only if it is plain.
        var written = String(holder.text);
        if ((before && written.indexOf(before) !== 0)
            || (after && written.slice(-after.length) !== after)) {
            return;
        }
        m.pseudo = { before: before, after: after };
        return;
    }
    var own = _pseudoOwn(raw, holder.text);
    m.pseudo = { before: before, after: after };
    holder.text = before + own + after;
    _layoutChanged(raw);
}

function _afterClassMutation(raw, m) {
    var curRaw = (m && m.raw) ? m.raw : raw;
    if (curRaw && curRaw.__eldurInline) {
        if (curRaw.attrOrder.indexOf("class") === -1) {
            curRaw.attrOrder.push("class");
        }
        _vChanged(curRaw.model);
        return;
    }
    _vChanged(null);
    _layoutChanged(curRaw);
    _gTouch();
    _pseudoTouch();
    _syncEldurClasses(curRaw, m);
    if (_isCreatedElement(curRaw) && _isConnected(curRaw)) {
        _connectSubtree(curRaw);
    }
}

function classList(item) {
    var raw = _unwrap(item);
    var m = _meta(raw);
    // A class's variant may take the focus as it is set.
    _holdFocus();
    return {
        add: function() {
            var curRaw = m.raw || raw;
            for (var i = 0; i < arguments.length; ++i) {
                var cls = String(arguments[i]);
                if (cls && m.classes.indexOf(cls) === -1) {
                    m.classes.push(cls);
                }
                _syncClassProp(curRaw, cls, true);
            }
            _afterClassMutation(curRaw, m);
        },
        remove: function() {
            var curRaw = m.raw || raw;
            for (var i = 0; i < arguments.length; ++i) {
                var cls = String(arguments[i]);
                var idx = m.classes.indexOf(cls);
                if (idx !== -1) {
                    m.classes.splice(idx, 1);
                    // The attribute stays, empty; one that was never there does not come.
                    m.classAttr = true;
                }
                _syncClassProp(curRaw, cls, false);
            }
            _afterClassMutation(curRaw, m);
        },
        toggle: function(clsName, force) {
            var curRaw = m.raw || raw;
            var cls = String(clsName);
            var idx = m.classes.indexOf(cls);
            var shouldHave = force !== undefined ? Boolean(force) : idx === -1;
            if (shouldHave) {
                if (idx === -1) {
                    m.classes.push(cls);
                }
            } else if (idx !== -1) {
                m.classes.splice(idx, 1);
                m.classAttr = true;
            }
            _syncClassProp(curRaw, cls, shouldHave);
            _afterClassMutation(curRaw, m);
            return shouldHave;
        },
        contains: function(clsName) {
            var curRaw = m.raw || raw;
            var cls = String(clsName);
            if (m.classes.indexOf(cls) !== -1) {
                return true;
            }
            if (curRaw && typeof curRaw === "object") {
                var prop = _sanitizeClassProp(cls);
                if (typeof curRaw[prop] === "boolean") {
                    return curRaw[prop];
                }
            }
            return false;
        },
        replace: function(oldCls, newCls) {
            var curRaw = m.raw || raw;
            var oldStr = String(oldCls);
            var newStr = String(newCls);
            var idx = m.classes.indexOf(oldStr);
            if (idx === -1) {
                return false;
            }
            var existingNewIdx = m.classes.indexOf(newStr);
            if (existingNewIdx !== -1 && existingNewIdx !== idx) {
                m.classes.splice(idx, 1);
            } else {
                m.classes[idx] = newStr;
            }
            if (oldStr !== newStr) {
                _syncClassProp(curRaw, oldStr, false);
            }
            _syncClassProp(curRaw, newStr, true);
            _afterClassMutation(curRaw, m);
            return true;
        },
        get value() {
            return m.classes.join(" ");
        },
        get length() {
            return m.classes.length;
        },
        item: function(index) {
            var i = Number(index) || 0;
            return i >= 0 && i < m.classes.length ? m.classes[i] : null;
        },
        toString: function() {
            return m.classes.join(" ");
        }
    };
}

function setClassName(item, value) {
    var raw = _unwrap(item);
    var m = _meta(raw);
    var curRaw = m.raw || raw;
    for (var i = 0; i < m.classes.length; ++i) {
        _syncClassProp(curRaw, m.classes[i], false);
    }
    m.classes.length = 0;
    var trimmed = String(value).trim();
    if (trimmed.length > 0) {
        var parts = trimmed.split(/\s+/);
        for (var j = 0; j < parts.length; ++j) {
            if (parts[j] && m.classes.indexOf(parts[j]) === -1) {
                m.classes.push(parts[j]);
                _syncClassProp(curRaw, parts[j], true);
            }
        }
    }
    _afterClassMutation(curRaw, m);
}

function getClassName(item) {
    return _meta(item).classes.join(" ");
}

function dataset(item) {
    var m = _meta(item);
    if (!_gHasCascade()) {
        return m.dataset;
    }
    // Plain writes, which the rules' `[data-*]` have to hear of.
    if (!m.datasetView) {
        m.datasetView = new Proxy(m.dataset, {
            set: function(target, key, value) {
                target[key] = String(value);
                _gTouch();
                return true;
            },
            deleteProperty: function(target, key) {
                delete target[key];
                _gTouch();
                return true;
            }
        });
    }
    return m.datasetView;
}

// The item appended children go in. A box's inner layout, when it has one.
// An empty box has only the `Text` for its `textContent`, and the column
// made here takes its place in the box's height.
function _appendTarget(raw) {
    if (_gIsBox(raw)) {
        return raw;
    }
    if (_meta(raw).tableGrid) {
        return _meta(raw).tableGrid;
    }
    if (raw.contentItem) {
        return raw.contentItem;
    }
    if (typeof raw.spacing === "number") {
        return raw;
    }
    var m = _meta(raw);
    if (m.flow) {
        return m.flow;
    }
    for (var i = 0; raw.children && i < raw.children.length; ++i) {
        var boxChild = raw.children[i];
        if (boxChild && (typeof boxChild.spacing === "number" || typeof boxChild.columnSpacing === "number")
            && !boxChild.objectName && !_meta(boxChild).createdTag && typeof boxChild.eldurTag !== "string") {
            _meta(boxChild).hostBox = raw;
            return boxChild;
        }
    }
    var text = _findTextChild(raw);
    if (!text) {
        return _isPlainTextItem(raw) ? _flowInText(raw) : raw;
    }
    var flow = Qt.createQmlObject("import QtQuick; import QtQuick.Layouts; ColumnLayout { spacing: 0 }", raw, "eldur_flow");
    var extra = raw.implicitHeight - text.implicitHeight;
    flow.x = Qt.binding(function() { return text.x; });
    flow.y = Qt.binding(function() { return text.y + (text.text === "" ? 0 : text.height); });
    flow.width = Qt.binding(function() { return text.width; });
    text.visible = Qt.binding(function() { return text.text !== ""; });
    raw.implicitHeight = Qt.binding(function() {
        return extra + (text.text === "" ? 0 : text.implicitHeight) + flow.implicitHeight;
    });
    // `textContent = x` replaces every child.
    text.textChanged.connect(function() { _clearAppended(raw); });
    _meta(flow).hostBox = raw;
    m.flow = flow;
    return flow;
}

// A block of text that is its own item, which no script was seen to fill (one
// reached by traversal or an event's target): the column goes under the text,
// and the size a layout gives the item says how much room both take.
function _flowInText(text) {
    var m = _meta(text);
    var flow = Qt.createQmlObject("import QtQuick; import QtQuick.Layouts; ColumnLayout { spacing: 0 }", text, "eldur_flow");
    var shift = text.transform.length > 0 && typeof text.transform[0].y === "number" ? text.transform[0].y : 0;
    var lines = function() { return text.text === "" ? 0 : text.contentHeight; };
    flow.y = Qt.binding(function() { return lines() - shift; });
    flow.width = Qt.binding(function() { return text.width; });
    var sizer = Qt.createQmlObject("import QtQuick; import QtQuick.Layouts; QtObject { "
        + "function hold(item, height) { item.Layout.preferredHeight = Qt.binding(height); } }", flow, "eldur_flow_size");
    sizer.hold(text, function() { return lines() + flow.implicitHeight; });
    // Rendering a model's runs is not a script replacing the text.
    text.textChanged.connect(function() {
        if (!_vRendering) {
            _clearAppended(text);
        }
    });
    _meta(flow).hostBox = text;
    m.flow = flow;
    return flow;
}

// Qt delivers its once-per-frame hover before it polishes layouts, so an
// item a script puts under a resting cursor is hit-tested at its size
// before layout and stays unhovered until the mouse moves. A browser lays
// out before it hit-tests: when the script's turn ends, polish the changed
// subtrees (children first: a layout's implicit size waits for its polish)
// and their ancestors, ahead of the frame.
var _layoutDirty = [];
var _tableHosts = [];

var _TABLE_PARTS = { table: true, thead: true, tbody: true, tfoot: true, tr: true };
var _TABLE_SECTION = { thead: 0, tbody: 1, tfoot: 2 };

function _tablePartTag(item) {
    var tag = _meta(item).createdTag || item.eldurTag;
    return typeof tag === "string" ? tag.toLowerCase() : "";
}

// What a script's rows belong to: a static table's grid, whose solver then counts
// them too, or a table the script made.
function _tableHost(item) {
    for (var cur = item; cur; cur = cur.parent) {
        // A table with no static cell has no grid, and is laid out like a created one.
        if (_meta(cur).tableGrid) {
            return _meta(cur).tableGrid;
        }
        if (cur.columnSpecs !== undefined || _tablePartTag(cur) === "table") {
            return cur;
        }
    }
    return null;
}

// The rows under a section in order; a static section's flow sits a level down.
function _tableRowsIn(item, out) {
    var kids = item.children || [];
    for (var i = 0; i < kids.length; ++i) {
        var tag = item.eldurTag ? _tablePartTag(kids[i]) : _meta(kids[i]).createdTag;
        if (tag === "tr") {
            if (kids[i].visible !== false || !kids[i].parent) {
                out.push(kids[i]);
            }
        } else if (!_isCreatedElement(kids[i]) && kids[i].tableSlot === undefined && !kids[i].eldurTag
            || tag in _TABLE_SECTION || _TABLE_SECTION[kids[i].eldurTag] !== undefined) {
            _tableRowsIn(kids[i], out);
        }
    }
    return out;
}

function _tablePartsOf(raw, tag) {
    return _rawElementChildren(raw).filter(function(kid) { return _nodeTag(kid) === tag; });
}

function _tableCellsOf(row) {
    return _rawElementChildren(row).filter(function(kid) { return /^t[dh]$/.test(_nodeTag(kid)); });
}

// HTML's `table.rows`: header sections' rows, then the body's and those straight
// in the table in tree order, then the footer's.
function _tableAllRows(table) {
    var head = [];
    var body = [];
    var foot = [];
    _rawElementChildren(table).forEach(function(kid) {
        var tag = _nodeTag(kid);
        if (tag === "tr") {
            body.push(kid);
        } else if (tag === "thead") {
            head = head.concat(_tablePartsOf(kid, "tr"));
        } else if (tag === "tbody") {
            body = body.concat(_tablePartsOf(kid, "tr"));
        } else if (tag === "tfoot") {
            foot = foot.concat(_tablePartsOf(kid, "tr"));
        }
    });
    return head.concat(body, foot);
}

function _tableFirst(raw, ownTag, tag) {
    if (_nodeTag(raw) !== ownTag) {
        return undefined;
    }
    var found = _tablePartsOf(raw, tag)[0];
    return found ? wrap(found) : null;
}

function _tableOwner(raw) {
    for (var up = _logicalParent(raw); up; up = _logicalParent(up)) {
        if (_nodeTag(up) === "table") {
            return up;
        }
    }
    return null;
}

function _tableInsertRow(raw, index) {
    var isTable = _nodeTag(raw) === "table";
    var list = isTable ? _tableAllRows(raw) : _tablePartsOf(raw, "tr");
    if (index < -1 || index > list.length) {
        throw new Error("IndexSizeError: no row " + index);
    }
    var row = document.createElement("tr");
    if (index === -1 || index === list.length) {
        var into = raw;
        if (isTable) {
            var bodies = _tablePartsOf(raw, "tbody");
            if (list.length > 0) {
                into = _logicalParent(list[list.length - 1]);
            } else if (bodies.length > 0) {
                into = bodies[bodies.length - 1];
            } else {
                into = _unwrap(_tableMakePart(raw, "tbody"));
            }
        }
        wrap(into).appendChild(row);
    } else {
        wrap(_logicalParent(list[index])).insertBefore(row, wrap(list[index]));
    }
    return row;
}

function _tableMakePart(table, tag) {
    if (tag !== "tbody") {
        var have = _tablePartsOf(table, tag)[0];
        if (have) {
            return wrap(have);
        }
    }
    var part = document.createElement(tag);
    var kids = _rawElementChildren(table);
    var before = null;
    if (tag === "thead") {
        before = kids.filter(function(k) { return !/^(caption|colgroup)$/.test(_nodeTag(k)); })[0];
    } else if (tag === "tbody") {
        var bodies = _tablePartsOf(table, "tbody");
        var last = bodies.length > 0 ? bodies[bodies.length - 1] : null;
        before = last ? kids[kids.indexOf(last) + 1] : kids.filter(function(k) { return /^(tfoot)$/.test(_nodeTag(k)); })[0];
    } else if (tag === "caption") {
        before = kids[0];
    }
    if (before) {
        wrap(table).insertBefore(part, wrap(before));
    } else {
        wrap(table).appendChild(part);
    }
    if (tag === "tbody" && last && before) {
        // It goes right after the last body, ahead of the whitespace there.
        _wsMove(_wsBefore(_unwrap(part)), before, "ws", true);
    }
    return part;
}

function _tableDropPart(table, tag) {
    var have = _tablePartsOf(table, tag)[0];
    if (have) {
        wrap(have).remove();
    }
}

function _tableIsPart(raw) {
    return /^(table|caption|thead|tbody|tfoot|tr|td|th)$/.test(_nodeTag(raw));
}

function _tableSpan(cell, name) {
    var n = parseInt(getAttribute(cell, name), 10);
    return n > 0 ? n : 1;
}

var _tableMetrics = null;

// As the converter's `min_content`: the longest word, and what is around the text.
function _tableMinContent(cell, text) {
    if (!text) {
        return cell.implicitWidth;
    }
    if (!_tableMetrics) {
        _tableMetrics = Qt.createQmlObject("import QtQuick; FontMetrics {}", _root, "eldur_table_metrics");
    }
    _tableMetrics.font = text.font;
    var words = String(text.text).split(/\s+/);
    var longest = 0;
    for (var i = 0; i < words.length; ++i) {
        longest = Math.max(longest, _tableMetrics.advanceWidth(words[i]));
    }
    return Math.ceil(longest) + cell.implicitWidth - text.implicitWidth;
}

// The HTML forming algorithm's slots, for what a script made: rows are indexes
// into `rows`, and a rowspan stops at its section's end as the converter's does.
function _tablePlace(rows) {
    var cells = [];
    var taken = {};
    var columns = 0;
    for (var r = 0; r < rows.length; ++r) {
        var kids = rows[r].item.children;
        var c = 0;
        for (var i = 0; i < kids.length; ++i) {
            if (!/^t[dh]$/.test(_tablePartTag(kids[i]))) {
                continue;
            }
            while (taken[r + ":" + c]) {
                ++c;
            }
            var across = _tableSpan(kids[i], "colspan");
            var down = Math.min(_tableSpan(kids[i], "rowspan"), rows[r].end - r);
            for (var y = r; y < r + down; ++y) {
                for (var x = c; x < c + across; ++x) {
                    taken[y + ":" + x] = true;
                }
            }
            cells.push({ item: kids[i], row: r, column: c, rowSpan: down, columnSpan: across });
            c += across;
            columns = Math.max(columns, c);
        }
    }
    return { cells: cells, columns: columns };
}

function _tableSpacing(host) {
    if (host.columnSpacing !== undefined) {
        return [host.columnSpacing, host.rowSpacing];
    }
    var styles = _meta(host).styles || {};
    var won = _gHasCascade() ? _gStyle(host).won : {};
    var written = function(camel, kebab) {
        return String(styles[camel] || styles[kebab] || (won[kebab] ? won[kebab].value : ""));
    };
    if (written("borderCollapse", "border-collapse").trim() === "collapse") {
        return [0, 0];
    }
    var parts = written("borderSpacing", "border-spacing").split(/\s+/).map(parseFloat).filter(function(v) { return !isNaN(v); });
    if (parts.length === 0) {
        return [2, 2];
    }
    return [parts[0], parts.length > 1 ? parts[1] : parts[0]];
}

function _tableCollapses(host) {
    if (host.eldurOuterEdge > 0) {
        return true;
    }
    var styles = _meta(host).styles || {};
    var written = styles.borderCollapse || styles["border-collapse"];
    if (written === undefined && _gHasCascade()) {
        written = _gComputed(host, "border-collapse");
    }
    return String(written).trim() === "collapse";
}

// A cell's CSS width is a floor on its column, as its text is.
function _tableCellWidth(item, exact) {
    var wanted = exact ? item.implicitWidth : Math.ceil(item.implicitWidth);
    if (_gHasCascade()) {
        var s = _gStyle(item);
        var px = /px$/.test(String(s.width)) ? _gPx(s.width, 0, s.fontSize, s.rootSize) : null;
        if (px !== null) {
            var edges = _gEdges(item, s, 0);
            var frame = s["box-sizing"] === "border-box" ? 0 : edges.pl + edges.pr + edges.bl + edges.br;
            wanted = Math.max(wanted, Math.ceil(px + frame));
        }
    }
    return wanted;
}

// A live table's sections show head first and foot last, whichever way a script
// put them in; a layout goes by its children's order.
function _tableOrderSections(host) {
    var kids = [];
    for (var i = 0; i < host.children.length; ++i) {
        kids.push(host.children[i]);
    }
    var rank = function(item) {
        var tag = _tablePartTag(item);
        if (tag === "colgroup" || tag === "col") {
            return -1;
        }
        return tag === "thead" ? 0 : tag === "tfoot" ? 2 : 1;
    };
    var sorted = true;
    for (var k = 1; k < kids.length; ++k) {
        sorted = sorted && rank(kids[k - 1]) <= rank(kids[k]);
    }
    if (sorted) {
        return;
    }
    var order = [-1, 0, 1, 2];
    order.forEach(function(r) {
        kids.forEach(function(kid) {
            if (rank(kid) === r) {
                kid.parent = null;
                kid.parent = host;
            }
        });
    });
}

// A live static table has its own objects for rows and sections, and hands the
// solver every cell as a created table does; this says where its columns come
// from: what the page declared, and as many more as its rows now hold.
function _tableLiveSpecs(host, columns) {
    var m = _meta(host);
    if (!m.tableSpecs) {
        m.tableSpecs = host.columnSpecs.slice();
        var declared = 0;
        var count = function(item) {
            var span = parseInt(getAttribute(item, "span"), 10);
            return span > 0 ? span : 1;
        };
        _rawElementChildren(host).forEach(function(part) {
            var tag = _tablePartTag(part);
            if (tag === "col") {
                declared += count(part);
            } else if (tag === "colgroup") {
                var cols = _rawElementChildren(part);
                declared += cols.length === 0 ? count(part)
                    : cols.reduce(function(sum, col) { return sum + count(col); }, 0);
            }
        });
        m.tableColumns = declared;
    }
    var n = Math.max(columns, m.tableColumns, 1);
    var same = host.columnSpecs.length === n;
    if (!same) {
        var specs = [];
        for (var c = 0; c < n; ++c) {
            specs.push(c < m.tableSpecs.length ? m.tableSpecs[c] : null);
        }
        host.columnSpecs = specs;
    }
}

function _tableIsLive(host) {
    return host.columnSpecs !== undefined && host.rowHeights === undefined;
}

function _tableLayout(host) {
    var live = _tableIsLive(host);
    if (live) {
        // Nothing of a hidden table has a size to read; it lays out when shown.
        if (!host.visible) {
            return;
        }
        var lm = _meta(host);
        if (lm.tablePlacing) {
            lm.tableAgain = true;
            return;
        }
        lm.tablePlacing = true;
        _tableOrderSections(host);
        _tablePlaceAll(host, live);
        lm.tablePlacing = false;
        if (lm.tableAgain) {
            lm.tableAgain = false;
            _tableLayout(host);
        }
        return;
    }
    _tablePlaceAll(host, live);
}

function _tablePlaceAll(host, live) {
    var isStatic = host.columnSpecs !== undefined;
    var sections = [];
    if (live) {
        var hostParts = host.children;
        for (var hp = 0; hp < hostParts.length; ++hp) {
            var hpTag = _tablePartTag(hostParts[hp]);
            if (_TABLE_SECTION[hpTag] !== undefined) {
                sections.push({ rows: _tableRowsIn(hostParts[hp], []), item: hostParts[hp] });
            } else if (hpTag === "tr" && hostParts[hp].visible !== false) {
                sections.push({ rows: [hostParts[hp]], item: null });
            }
        }
    } else if (isStatic) {
        for (var g = 0; g < host.children.length; ++g) {
            if (_TABLE_SECTION[host.children[g].eldurTag] !== undefined) {
                sections.push({ rows: _tableRowsIn(host.children[g], []) });
            }
        }
    } else {
        var parts = host.children;
        for (var o = 0; o < 3; ++o) {
            for (var p = 0; p < parts.length; ++p) {
                if (_TABLE_SECTION[_tablePartTag(parts[p])] === o) {
                    sections.push({ rows: _tableRowsIn(parts[p], []), item: parts[p] });
                }
            }
        }
        // Rows put straight in a table are a body of their own.
        var loose = [];
        for (var q = 0; q < parts.length; ++q) {
            if (_tablePartTag(parts[q]) === "tr") {
                loose.push(parts[q]);
            }
        }
        if (loose.length > 0) {
            sections.push({ rows: loose });
        }
    }
    var rows = [];
    sections.forEach(function(group) {
        var end = rows.length + group.rows.length;
        group.rows.forEach(function(item) {
            rows.push({ item: item, end: end });
        });
    });
    var placed = _tablePlace(rows);
    // What a cell needs changes as its text wraps in the width given below, and
    // whenever a script writes to it afterwards.
    placed.cells.forEach(function(c) {
        var cm = _meta(c.item);
        if (!cm.tableWatched) {
            cm.tableWatched = true;
            var again = function() { _layoutChanged(c.item); };
            c.item.implicitHeightChanged.connect(again);
            c.item.implicitWidthChanged.connect(again);
        }
    });
    var measure = function() {
        return placed.cells.map(function(c) {
            var text = _findTextChild(c.item);
            return {
                tableSlot: [c.row, c.column, c.rowSpan, c.columnSpan],
                // Columns come out whole pixels; a text must not wrap by a fraction.
                implicitWidth: _tableCellWidth(c.item),
                implicitHeight: c.item.implicitHeight,
                minContentWidth: _tableCellMinimum(c.item, text)
            };
        });
    };
    var spacing = _tableSpacing(host);
    // Collapsed, a cell made from a template shares its side borders with the cells
    // beside it and its top border with the row above. It has all four of its own,
    // where a static cell paints the sides it won.
    var collapsed = _tableCollapses(host);
    var sharedAcross = function(c) {
        return collapsed && c.column > 0 && c.item.eldurTemplate !== undefined && c.item.border
            ? c.item.border.width : 0;
    };
    var sharedRight = function(c) {
        return collapsed && c.item.eldurTemplate !== undefined && c.item.border ? c.item.border.width : 0;
    };
    var adapters = measure().map(function(a, i) {
        var over = sharedAcross(placed.cells[i]);
        return Object.assign({}, a, {
            implicitWidth: a.implicitWidth - over,
            minContentWidth: a.minContentWidth - over
        });
    });
    var widths;
    if (isStatic) {
        host.eldurCreatedCells = adapters;
        if (live) {
            _tableLiveSpecs(host, placed.columns);
            _tableWatchHost(host);
            var specsNow = host.columnSpecs;
            // Written, not bound: the width follows from these, and these from
            // cells this function is about to resize.
            host.maxContentWidth = _root.tableMaxContentWidth(host, specsNow, host.eldurFixed);
            host.minContentWidth = _root.tableMinContentWidth(host, specsNow, host.eldurFixed);
            // Spare width is shared by content width, on the text's exact width and
            // without the outer collapsed border; whole pixels only keep a table with
            // none to spare from wrapping.
            if (host.width > host.maxContentWidth) {
                host.eldurCreatedCells = placed.cells.map(function(c, i) {
                    var exact = Object.assign({}, adapters[i]);
                    var last = c.column + c.columnSpan === host.columnSpecs.length;
                    exact.implicitWidth = _tableCellWidth(c.item, true)
                        - (sharedRight(c) || (last ? host.eldurOuterEdge : 0));
                    exact.minContentWidth = Math.min(exact.minContentWidth, exact.implicitWidth);
                    return exact;
                });
            }
            host.columnWidths = _root.tableColumnWidths(host, specsNow, host.eldurFixed);
        }
        widths = host.columnWidths;
    } else {
        var specs = [];
        for (var n = 0; n < placed.columns; ++n) {
            specs.push(null);
        }
        var solver = { children: adapters, columnSpacing: spacing[0], rowSpacing: spacing[1], width: 0 };
        // Auto width is max-content, up to what the table is in.
        var room = host.parent && host.parent.width > 0 ? host.parent.width - 2 * spacing[0] : Infinity;
        var given = /^[\d.]+px$/.test(String(_meta(host).styles.width)) ? parseFloat(_meta(host).styles.width) : NaN;
        solver.width = !isNaN(given) ? given - 2 * spacing[0]
            : Math.min(_root.tableMaxContentWidth(solver, specs, false), room);
        widths = _root.tableColumnWidths(solver, specs, false);
    }
    var left = [];
    var edge = 0;
    for (var w = 0; w < widths.length; ++w) {
        left.push(edge);
        edge += widths[w] + spacing[0];
    }
    var total = Math.max(edge - spacing[0], 0);
    if (live) {
        host.eldurEmpty = rows.length === 0;
    }
    var acrossFrom = function(c) {
        var sum = spacing[0] * (c.columnSpan - 1);
        for (var k = c.column; k < c.column + c.columnSpan && k < widths.length; ++k) {
            sum += widths[k];
        }
        return sum;
    };
    placed.cells.forEach(function(c) {
        // Its right border lies on the next cell's left one, which static cells paint.
        var last = c.column + c.columnSpan >= widths.length;
        c.item.x = left[c.column] || 0;
        _meta(c.item).tableBorderOver = last ? 0 : sharedRight(c);
        c.item.width = acrossFrom(c) + _meta(c.item).tableBorderOver;
    });
    // Text wraps in the width it now has.
    var zeros = rows.map(function() { return 0; });
    rows.forEach(function(row, r) {
        zeros[r] = row.item.eldurMinHeight || 0;
    });
    var shared = function(c) {
        return collapsed && c.row > 0 && c.item.eldurTemplate !== undefined && c.item.border
            ? c.item.border.width : 0;
    };
    var heights = rows.length > 0
        ? _root.tableRowHeights({
            children: measure().map(function(a, i) {
                return Object.assign({}, a, { implicitHeight: a.implicitHeight - shared(placed.cells[i]) });
            }),
            rowSpacing: spacing[1]
        }, zeros)
        : [];
    placed.cells.forEach(function(c) {
        var sum = spacing[1] * (c.rowSpan - 1);
        for (var k = c.row; k < c.row + c.rowSpan; ++k) {
            sum += heights[k];
        }
        var over = shared(c);
        c.item.y = -over;
        c.item.height = sum + over;
    });
    rows.forEach(function(row, r) {
        row.item.implicitWidth = total;
        row.item.implicitHeight = heights[r];
        if (row.item.parent && row.item.parent !== host && row.item.parent.spacing !== undefined) {
            row.item.parent.spacing = spacing[1];
        }
    });
    if (live) {
        // Border-spacing between what shows: the table's outer spacing is
        // around all this, and an empty section takes no room.
        var shown = false;
        sections.forEach(function(group) {
            if (group.item) {
                var filled = group.rows.length > 0;
                group.item.eldurTop = shown && filled ? spacing[1] : 0;
                shown = shown || filled;
            } else {
                group.rows[0].eldurTop = shown ? spacing[1] : 0;
                shown = true;
            }
        });
    } else if (!isStatic) {
        // A table's border-spacing runs around its rows too.
        host.spacing = spacing[1];
        host.implicitWidth = total + 2 * spacing[0];
        for (var e = 0; e < host.children.length; ++e) {
            var kid = host.children[e];
            if (kid.eldurSide !== undefined) {
                kid.eldurSide = spacing[0];
                kid.eldurTop = e === 0 ? spacing[1] : 0;
                kid.eldurBottom = e === host.children.length - 1 ? spacing[1] : 0;
            }
        }
    }
}

// The width a live table has comes from its parent's polish, after the cells it
// was first given: a new width lays them out again, and so does being shown.
function _tableWatchHost(host) {
    var m = _meta(host);
    if (m.tableHostWatched) {
        return;
    }
    m.tableHostWatched = true;
    var lastWidth = -1;
    host.widthChanged.connect(function() {
        if (host.width !== lastWidth) {
            lastWidth = host.width;
            _tableLayout(host);
        }
    });
    host.visibleChanged.connect(function() {
        if (host.visible) {
            _layoutChanged(host);
        }
    });
}

// Text a script can rewrite has to be measured as it is now; a cell with anything
// else inside holds what the converter worked out.
function _tableCellMinimum(cell, text) {
    if (cell.minContentWidth !== undefined && !text && typeof cell.text !== "string") {
        return cell.minContentWidth;
    }
    if (typeof cell.text === "string") {
        if (cell.textFormat === 1 || String(cell.text).indexOf("<") >= 0) {
            return cell.minContentWidth !== undefined ? cell.minContentWidth : cell.implicitWidth;
        }
        return _tableMinContent(cell, cell);
    }
    return _tableMinContent(cell, text);
}

function _tableLiveHosts() {
    if (!_root || _root.tableColumnWidths === undefined) {
        return [];
    }
    var hosts = [];
    _walkItems(_root, function(item) {
        if (item.columnSpecs !== undefined && _tableIsLive(item)) {
            hosts.push(item);
        }
        return false;
    });
    return hosts;
}

// The object scripts see as the `<table>` holds its grid a level or two down:
// under the box with its border, and beside the captions.
function _tableSetUp() {
    _tableHosts = _tableLiveHosts();
    _tableHosts.forEach(function(host) {
        for (var cur = host; cur; cur = cur.parent) {
            if (cur.eldurTag === "table") {
                if (cur !== host) {
                    _meta(cur).tableGrid = host;
                    _meta(host).hostBox = cur;
                }
                break;
            }
        }
        _tableBaseline(host);
        _layoutChanged(host);
    });
    _tableHosts.forEach(function(host) { _tableLayout(host); });
}

// What a cell's own rule, its row's, its section's and its column's add up to,
// as the converter flattened them (CSS 2.1 §17.5.1).
function _tableBackground(cell, column, layers) {
    var r = 0, g = 0, b = 0, a = 0;
    layers.concat([cell]).forEach(function(part) {
        if (!part) {
            return;
        }
        var value = _gStyle(part)["background-color"];
        var c = value ? Qt.color(_gQtColor(value)) : null;
        if (!c || c.a === 0) {
            return;
        }
        var out = c.a + a * (1 - c.a);
        r = (c.r * c.a + r * a * (1 - c.a)) / out;
        g = (c.g * c.a + g * a * (1 - c.a)) / out;
        b = (c.b * c.a + b * a * (1 - c.a)) / out;
        a = out;
    });
    return Qt.rgba(r, g, b, a);
}

function _tableColumns(host) {
    var columns = [];
    var span = function(item) {
        var n = parseInt(getAttribute(item, "span"), 10);
        return n > 0 ? n : 1;
    };
    _rawElementChildren(host).forEach(function(part) {
        var tag = _nodeTag(part);
        if (tag === "col") {
            for (var i = 0; i < span(part); ++i) {
                columns.push({ group: null, col: part });
            }
        } else if (tag === "colgroup") {
            var cols = _rawElementChildren(part);
            if (cols.length === 0) {
                for (var j = 0; j < span(part); ++j) {
                    columns.push({ group: part, col: null });
                }
            }
            cols.forEach(function(col) {
                for (var k = 0; k < span(col); ++k) {
                    columns.push({ group: part, col: col });
                }
            });
        }
    });
    return columns;
}

function _tableCellColor(cell, color) {
    if (cell.color !== undefined && typeof cell.text !== "string") {
        cell.color = color;
        return;
    }
    // A cell that painted nothing has no rectangle: it gets one, behind it, that
    // is no element.
    var m = _meta(cell);
    if (!m.tableBacking) {
        m.tableBacking = Qt.createQmlObject(
            "import QtQuick; Rectangle { z: -1; anchors.fill: parent; property var eldurBox: null }",
            cell, "eldur_cell_backing");
    }
    m.tableBacking.color = color;
}

function _tableDisplay(item, baseline) {
    var m = _meta(item);
    var none = _gStyle(item).display === "none" || m.explicitHidden === true;
    if (baseline) {
        m.tableNone = none;
        return;
    }
    if (none && item.visible) {
        _setShown(item, false);
    } else if (!none && m.tableNone) {
        _setShown(item, true);
    }
    m.tableNone = none;
}

// Which rules hold for a row or its cells changes with its class, its place
// among its siblings and the pointer. A cell paints all of it: a row is no
// rectangle, and it could not be under cells that span out of it. The first pass
// takes what the converter painted as read and a pass writes only what moved
// since, so what's bound or written otherwise is left alone until then.
function _tableRepaint(host, baseline) {
    var columns = _tableColumns(host);
    var paintRow = function(row, section, taken, r) {
        _tableDisplay(row, baseline);
        var c = 0;
        _tableCellsOf(row).forEach(function(cell) {
            while (taken[r + ":" + c]) {
                ++c;
            }
            var across = _tableSpan(cell, "colspan");
            var down = _tableSpan(cell, "rowspan");
            for (var y = r; y < r + down; ++y) {
                for (var x = c; x < c + across; ++x) {
                    taken[y + ":" + x] = true;
                }
            }
            _tableDisplay(cell, baseline);
            var column = columns[c] || {};
            var color = _tableBackground(cell, c, [column.group, column.col, section, row]);
            var text = _gStyle(cell).color;
            var key = String(color) + "/" + text;
            var m = _meta(cell);
            var known = m.tablePaint;
            m.tablePaint = key;
            if (!(baseline && !_isCreatedElement(cell)) && (known !== key)) {
                if (known !== undefined || color.a > 0) {
                    _tableCellColor(cell, color);
                }
                var label = typeof cell.text === "string" ? cell : _findTextChild(cell);
                if (label && label.color !== undefined && text) {
                    label.color = _gQtColor(text);
                }
            }
            c += across;
        });
    };
    _rawElementChildren(host).forEach(function(part) {
        var tag = _nodeTag(part);
        if (tag === "tr") {
            paintRow(part, null, {}, 0);
        } else if (_TABLE_SECTION[tag] !== undefined) {
            _tableDisplay(part, baseline);
            var taken = {};
            _tablePartsOf(part, "tr").forEach(function(row, r) {
                paintRow(row, part, taken, r);
            });
        }
    });
}

function _tableBaseline(host) {
    if (_gHasCascade()) {
        _tableRepaint(host, true);
    }
}

function _tableRepaintAll() {
    _tableHosts.forEach(function(host) {
        if (_isConnected(host)) {
            _tableRepaint(host, false);
        }
    });
}

function _tableSettle(dirty) {
    var hosts = [];
    for (var i = 0; i < dirty.length; ++i) {
        var host = null;
        var pair = [dirty[i], dirty[i].parent];
        for (var q = 0; q < pair.length && !host; ++q) {
            if (pair[q] && (_TABLE_PARTS[_tablePartTag(pair[q])] || /^t[dh]$/.test(_tablePartTag(pair[q])))) {
                host = _tableHost(pair[q]);
            }
        }
        if (host && hosts.indexOf(host) < 0) {
            hosts.push(host);
        }
    }
    hosts.forEach(_tableLayout);
}

function _layoutChanged(item) {
    if (!item || typeof item !== "object") {
        return;
    }
    if (item.__eldurInline) {
        _layoutChanged(item.model && item.model.text);
        return;
    }
    if (_layoutDirty.length === 0) {
        Qt.callLater(_settleLayouts);
    }
    _layoutDirty.push(item);
}

function _polishSubtree(item) {
    var kids = item.children || [];
    for (var i = 0; i < kids.length; ++i) {
        _polishSubtree(kids[i]);
    }
    if (typeof item.settleLines === "function") {
        item.settleLines();
    }
    if (typeof item.ensurePolished === "function") {
        item.ensurePolished();
    }
}

// Structural pseudo-classes (`:first-child`, `:nth-child()`, …) read an
// element's place among its siblings from `eldurIndex`/`eldurCount` (and the
// same-tag `eldurTypeIndex`/`eldurTypeCount`), which the converter declares
// only on the children of containers scripts change.
// A flex container the page left empty: its spacers follow what it now holds
// (`spacer_slots` in the converter's flex.rs). `spacing` is the gap unless
// spacers sit between the items, which then hold the gap open themselves.
function _packFlex(layout, force) {
    var kids = [];
    var spacers = [];
    for (var i = 0; i < layout.children.length; ++i) {
        (_isLayoutSpacer(layout.children[i]) ? spacers : kids).push(layout.children[i]);
    }
    var m = _meta(layout);
    if (!force && m.packedFor === kids.length) {
        return;
    }
    m.packedFor = kids.length;
    var n = kids.length;
    var pack = layout.eldurPack;
    if (pack === "between" && n <= 1) {
        pack = "start";
    }
    var slot = function(at) {
        switch (pack) {
        case "none": return 0;
        case "start": return at === n ? 1 : 0;
        case "end": return at === 0 ? 1 : 0;
        case "center": return at === 0 || at === n ? 1 : 0;
        case "between": return at === 0 || at === n ? 0 : 1;
        case "around": return at === 0 || at === n ? 1 : 2;
        default: return 1;
        }
    };
    var axis = layout.eldurPackRow ? "Width" : "Height";
    var source = "import QtQuick; import QtQuick.Layouts; Item { Layout.fill" + axis + ": true; Layout.minimum" + axis + ": ";
    for (var d = 0; d < spacers.length; ++d) {
        spacers[d].parent = null;
        spacers[d].destroy();
    }
    for (var k = 0; k < n; ++k) {
        kids[k].parent = null;
    }
    for (var at = 0; at <= n; ++at) {
        for (var c = slot(at); c > 0; --c) {
            var between = at > 0 && at < n;
            Qt.createQmlObject(source + (between ? layout.eldurPackGap / slot(at) : 0) + " }", layout);
        }
        if (at < n) {
            kids[at].parent = layout;
        }
    }
}

// A grid the page left empty: each item takes the next cell and that column's
// size, which only attached properties can say.
function _placeGrid(grid) {
    var tracks = grid.eldurTracks;
    var n = tracks.length;
    var m = _meta(grid);
    if (!m.gridPlacer) {
        m.gridPlacer = Qt.createQmlObject("import QtQuick; import QtQuick.Layouts; QtObject { "
            + "function place(item, row, column, size, cellOnly) { "
            + "item.Layout.row = row; item.Layout.column = column; if (cellOnly) return; "
            + "if (size >= 0) { item.Layout.fillWidth = false; item.Layout.minimumWidth = size; "
            + "item.Layout.preferredWidth = size; item.Layout.maximumWidth = size; } "
            + "else { item.Layout.fillWidth = true; item.Layout.minimumWidth = 0; item.Layout.preferredWidth = 0; "
            + "item.Layout.horizontalStretchFactor = -size; } } }", grid);
    }
    var kids = _rawElementChildren(grid);
    for (var i = 0; i < kids.length; ++i) {
        var size = tracks[i % n];
        // A box reads its width from its own rules, within this track.
        if (size >= 0 && kids[i].eldurTrack !== undefined) {
            kids[i].eldurTrack = size;
            m.gridPlacer.place(kids[i], Math.floor(i / n), i % n, -1, true);
        } else {
            m.gridPlacer.place(kids[i], Math.floor(i / n), i % n, size);
        }
    }
}

// A page's grid holds its free cells with placeholders: an item moved in takes the
// next one, and its column's width unless it has its own.
function _placeInStaticGrid(grid) {
    var kids = Array.prototype.filter.call(grid.children, function(kid) {
        return typeof kid.eldurTag === "string";
    });
    var free = Array.prototype.filter.call(grid.children, function(kid) {
        return typeof kid.eldurTag !== "string" && kid.visible !== false;
    });
    if (free.length === 0) {
        return;
    }
    var m = _meta(grid);
    if (!m.gridTaker) {
        m.gridTaker = Qt.createQmlObject("import QtQuick; import QtQuick.Layouts; QtObject { "
            + "function same(a, b) { return a.Layout.row === b.Layout.row && a.Layout.column === b.Layout.column; } "
            + "function isCell(cell, last) { return cell.Layout.column < last; } "
            + "function take(item, cell) { item.Layout.row = cell.Layout.row; item.Layout.column = cell.Layout.column; "
            + "if (item.Layout.preferredWidth < 0) { item.Layout.preferredWidth = cell.Layout.preferredWidth; "
            + "item.Layout.maximumWidth = cell.Layout.maximumWidth; item.Layout.fillWidth = cell.Layout.fillWidth; } "
            + "else { item.Layout.rightMargin = Math.max(0, cell.Layout.preferredWidth - item.Layout.preferredWidth); } } }",
            grid);
    }
    // A Layout's row reads 0 unset, so what has not been placed shares a taken cell.
    for (var i = 0; i < kids.length; ++i) {
        var taken = false;
        for (var o = 0; o < kids.length && !taken; ++o) {
            taken = o !== i && m.gridTaker.same(kids[i], kids[o]) && o < i;
        }
        for (var e = 0; e < free.length && !taken; ++e) {
            taken = free[e].visible !== false && m.gridTaker.same(kids[i], free[e]);
        }
        if (taken) {
            for (var c = 0; c < free.length; ++c) {
                if (free[c].visible !== false && m.gridTaker.isCell(free[c], grid.columns - 1)) {
                    free[c].visible = false;
                    m.gridTaker.take(kids[i], free[c]);
                    break;
                }
            }
        }
    }
}

function _reflowFilled(container, force) {
    if (typeof container.eldurPack === "string") {
        _packFlex(container, force);
    } else if (container.eldurTracks) {
        _placeGrid(container);
    } else if (typeof container.columnSpacing === "number" && typeof container.columns === "number") {
        _placeInStaticGrid(container);
    }
}

function _renumber(container) {
    if (!container || typeof container !== "object" || !container.children) {
        return;
    }
    var kids = _rawElementChildren(container).filter(function(kid) {
        return _meta(kid).createdTag !== "#text";
    });
    var tags = [];
    var typeCounts = {};
    for (var i = 0; i < kids.length; ++i) {
        tags.push(_inferTagName(kids[i]));
        typeCounts[tags[i]] = (typeCounts[tags[i]] || 0) + 1;
    }
    var seen = {};
    for (var k = 0; k < kids.length; ++k) {
        var kid = kids[k];
        seen[tags[k]] = (seen[tags[k]] || 0) + 1;
        if (typeof kid.eldurIndex === "number") {
            if (kid.eldurIndex !== k + 1) {
                kid.eldurIndex = k + 1;
            }
            if (kid.eldurCount !== kids.length) {
                kid.eldurCount = kids.length;
            }
        }
        if (typeof kid.eldurTypeIndex === "number") {
            if (kid.eldurTypeIndex !== seen[tags[k]]) {
                kid.eldurTypeIndex = seen[tags[k]];
            }
            if (kid.eldurTypeCount !== typeCounts[tags[k]]) {
                kid.eldurTypeCount = typeCounts[tags[k]];
            }
        }
    }
}

// A Qt layout left with no visible item keeps its last implicit size, so an
// emptied one holds an empty item until something goes in it.
function _keepCollapsing(layout) {
    // Lines keep no size for what is gone.
    if (typeof layout.firstBaseline === "number") {
        return;
    }
    if (typeof layout.spacing !== "number" && typeof layout.columnSpacing !== "number") {
        return;
    }
    if (layout.background !== undefined || layout.contentItem !== undefined) {
        return;
    }
    var m = _meta(layout);
    var kids = layout.children;
    var empty = true;
    for (var i = 0; i < kids.length && empty; ++i) {
        empty = kids[i] === m.collapser || !kids[i].visible;
    }
    if (empty && !m.collapser) {
        m.collapser = Qt.createQmlObject("import QtQuick; Item {}", layout, "eldur_collapser");
    } else if (!empty && m.collapser) {
        m.collapser.parent = null;
        m.collapser.destroy();
        m.collapser = null;
    }
}

function _settleLayouts() {
    _gSettle();
    var dirty = _layoutDirty;
    _layoutDirty = [];
    _tableSettle(dirty);
    var done = [];
    var numbered = [];
    for (var n = 0; n < dirty.length; ++n) {
        // A dirty item is a container that lost or gained children, or an
        // item that was swapped or refilled in its own container.
        var pair = [dirty[n], dirty[n].parent];
        for (var q = 0; q < pair.length; ++q) {
            if (pair[q] && numbered.indexOf(pair[q]) < 0) {
                numbered.push(pair[q]);
                _reflowFilled(pair[q], false);
                _renumber(pair[q]);
            }
        }
    }
    for (var i = 0; i < dirty.length; ++i) {
        _keepCollapsing(dirty[i]);
        if (dirty[i].parent) {
            _polishSubtree(dirty[i]);
        }
        for (var p = dirty[i].parent; p && done.indexOf(p) < 0; p = p.parent) {
            done.push(p);
            if (typeof p.ensurePolished === "function") {
                p.ensurePolished();
            }
        }
    }
}

// A browser lays out when a script asks where things are: a size read right
// after an insert already counts it. The settle above works upwards, so the
// widths the top hands back down are polished afterwards.
function _layoutForRead() {
    if (_layoutDirty.length === 0) {
        // Lines lay out once when complete, which may be after a page's script.
        if (!_linesLaidOut) {
            _linesLaidOut = true;
            _polishDown(_root);
        }
        return;
    }
    _settleLayouts();
    _polishDown(_root);
    _gAfterPolish();
    _polishDown(_root);
}

var _linesLaidOut = false;

function _polishDown(item) {
    if (typeof item.settleLines === "function") {
        item.settleLines();
    }
    if (!item) {
        return;
    }
    if (typeof item.ensurePolished === "function") {
        item.ensurePolished();
    }
    var kids = item.children || [];
    for (var i = 0; i < kids.length; ++i) {
        _polishDown(kids[i]);
    }
    // A line below may have just found its height from the width given here.
    if (typeof item.ensurePolished === "function") {
        item.ensurePolished();
    }
}

// Takes a node out of the DOM. Never destroys it: a script may hold it and put
// it back, as with a modal, and bindings of the page may read it by id.
function _detachNode(raw) {
    if (!raw.__eldurInline) {
        _wsLeave(raw);
    }
    if (raw.__eldurInline) {
        _vRemove(raw);
        return;
    }
    _gTouch();
    var box = raw.eldurBox || raw;
    var place = box.eldurSlot || box;
    if (place.parent) {
        _layoutChanged(place.parent);
        place.parent = null;
    }
    if (box.eldurSlot && box.eldurBlock !== undefined) {
        box.parent = null;
    }
}

// Sets whether a node shows of itself. A box painted away from its DOM parent
// still goes when that parent does.
function _setShown(raw, shown) {
    _holdFocus();
    _layoutChanged(raw.parent);
    var slot = raw.eldurSlot;
    if (slot && raw.eldurBlock === undefined) {
        raw.visible = Qt.binding(function() {
            return shown && (slot.parent ? slot.parent.visible : false);
        });
    } else {
        _inlineHold(raw, raw, "visible", shown);
    }
}

// An imperative write ends the binding the converter wrote, and with it the
// class variants that binding reads. A `Binding` object sits over the property
// instead and gives the binding back when it lets go, re-evaluated: that is
// what clearing an inline declaration means.

function _inlineFind(raw, target, prop) {
    var m = _meta(raw);
    // An item swapped for another form took its holds with it.
    m.inlineHolds = (m.inlineHolds || []).filter(function(h) { return Boolean(h.bind); });
    return m.inlineHolds.find(function(h) { return h.target === target && h.prop === prop; });
}

function _inlineHold(raw, target, prop, value) {
    var hold = _inlineFind(raw, target, prop);
    if (!hold) {
        var bind = Qt.createQmlObject("import QtQml; Binding { when: false; restoreMode: Binding.RestoreBindingOrValue }",
            raw, "eldur_inline");
        bind.target = target;
        bind.property = prop;
        hold = { target: target, prop: prop, bind: bind };
        _meta(raw).inlineHolds.push(hold);
    }
    hold.bind.value = value;
    hold.bind.when = true;
}

function _inlineRelease(raw, target, prop) {
    var hold = _inlineFind(raw, target, prop);
    if (hold) {
        hold.bind.when = false;
    }
}

function _clearAppended(raw) {
    _layoutChanged(raw);
    var flow = _meta(raw).flow;
    while (flow && flow.children.length > 0) {
        var child = flow.children[0];
        _detachNode(child);
        child.parent = null;
    }
    // The page's own elements in a laid-out container (a list's items) go
    // too; spacers, decoration and text runs are not elements.
    var container = _childContainer(raw);
    if (container && container !== flow
        && (typeof container.spacing === "number" || typeof container.columnSpacing === "number")) {
        var kids = _rawElementChildren(raw);
        for (var i = 0; i < kids.length; ++i) {
            var kid = kids[i];
            if (typeof kid.eldurTag === "string" || _meta(kid).createdTag) {
                _detachNode(kid);
            }
        }
    }
}

function _isPlainTextItem(item) {
    return Boolean(
        item
        && typeof item.text === "string"
        && item.background === undefined
        && typeof item.readOnly !== "boolean"
        && typeof item.clicked !== "function"
        && typeof item.toggled !== "function"
    );
}

function _findTextChild(raw) {
    if (!raw || !raw.children) {
        return null;
    }
    for (var i = 0; i < raw.children.length; ++i) {
        var child = raw.children[i];
        if (_isPlainTextItem(child) && !child.objectName && !_meta(child).createdTag) {
            return child;
        }
    }
    return null;
}

function _childContainer(raw) {
    if (!raw) {
        return null;
    }
    var base = (typeof raw.contentHeight === "number" && raw.contentItem) ? raw.contentItem : raw;
    var m = _meta(raw);
    if (m.tableGrid) {
        return m.tableGrid;
    }
    if (typeof base.spacing === "number" || typeof base.columnSpacing === "number") {
        return base;
    }
    if (m.flow) {
        return m.flow;
    }
    var isContentItem = Boolean(raw.parent && raw.parent.contentItem === raw);
    // A box's lines are made at layout; its children are still its own.
    if (_gIsBox(raw)) {
        return base;
    }
    if (base.children) {
        for (var i = 0; i < base.children.length; ++i) {
            var ch = base.children[i];
            if (ch && (typeof ch.spacing === "number" || typeof ch.columnSpacing === "number")
                && !ch.objectName && !_meta(ch).createdTag && typeof ch.eldurTag !== "string") {
                _meta(ch).hostBox = raw;
                return ch;
            }
        }
    }
    return base;
}

// The spacers a flex layout packs its items with (eldur_convert flex.rs):
// bare `Item`s (no declared properties, so no `eldurTag`) without children.
// They are not elements, and appended elements go before the trailing
// ones. `Layout.fillWidth` cannot be read here: this library does not
// import QtQuick.Layouts.
function _isLayoutSpacer(item) {
    return Boolean(
        item
        && item.eldurBox === undefined
        && String(item).indexOf("QQuickItem(") === 0
        && !item.objectName
        && item.children && item.children.length === 0
        && !_meta(item).createdTag
    );
}

function _rawElementChildren(raw) {
    if (raw && raw.__eldurInline) {
        return raw.children;
    }
    var own = raw ? _vModelOf(raw) : null;
    if (own && (own.select || _vIsTagged(raw))) {
        return own.kids.filter(_vIsElement);
    }
    return _rawKids(raw, false);
}

function _tableCaptions(raw) {
    var out = [];
    if (_meta(raw).tableGrid && typeof raw.spacing === "number") {
        for (var i = 0; i < raw.children.length; ++i) {
            if (raw.children[i].eldurTag === "caption") {
                out.push(raw.children[i]);
            }
        }
    }
    return out;
}

function _rawKids(raw, withText) {
    var container = _childContainer(raw);
    if (!container || !container.children) {
        return [];
    }
    var captions = raw ? _tableCaptions(raw) : [];
    if (captions.length > 0) {
        var below = container.children;
        for (var bi = 0; bi < below.length; ++bi) {
            captions.push(below[bi]);
        }
        return captions;
    }
    var isLayout = typeof container.spacing === "number" || typeof container.columnSpacing === "number";
    var out = [];
    for (var i = 0; i < container.children.length; ++i) {
        var child = container.children[i];
        if (!child) {
            continue;
        }
        // An out-of-flow box is its slot's child in the DOM, wherever it is
        // painted.
        if (child.eldurSlot !== undefined) {
            continue;
        }
        // A layer's source and effect are made beside the item it belongs to.
        if (child.sourceItem !== undefined || child.fragmentShader !== undefined || child.blurMax !== undefined) {
            continue;
        }
        if (_isAnonLines(child)) {
            var inLines = _rawKids(child, withText);
            for (var al = 0; al < inLines.length; ++al) {
                out.push(inLines[al]);
            }
            continue;
        }
        if (child.eldurBox !== undefined) {
            if (child.eldurBox) {
                out.push(child.eldurBox);
            }
            continue;
        }
        // A control's own parts (Button.background, .contentItem, …) are
        // children of it but not elements; a Button also has `spacing`.
        if (child === container.background || child === container.contentItem
            || child === container.indicator) {
            continue;
        }
        if (!withText && _meta(child).createdTag === "#text" && _isLines(container)) {
            continue;
        }
        if (_vAnonText(child)) {
            var runKids = _vModelOf(child).kids.filter(withText ? _vIsRendered : _vIsElement);
            for (var r = 0; r < runKids.length; ++r) {
                out.push(runKids[r]);
            }
            continue;
        }
        if (!isLayout && _isPlainTextItem(child) && !child.objectName && !_meta(child).createdTag) {
            if (withText && _vHasRules() && !_meta(child).vSplit) {
                out.push(child);
            }
            continue;
        }
        // A run of a line is a text node to a script, not an element.
        if (!withText && _isLines(container) && _isPlainTextItem(child) && !child.objectName) {
            continue;
        }
        if (isLayout && _isLayoutSpacer(child)) {
            continue;
        }
        out.push(child);
    }
    // A flex or grid container's out-of-flow boxes wait beside its items.
    if (container !== raw && raw.children) {
        for (var c = 0; c < raw.children.length; ++c) {
            var corner = raw.children[c];
            if (corner.eldurBox && corner.eldurOrder !== undefined) {
                out.splice(Math.min(corner.eldurOrder, out.length), 0, corner.eldurBox);
            }
        }
    }
    return out;
}

// The next item up that a script's DOM has: an out-of-flow box with no parent
// in the DOM stops here, wherever it is painted.
function _domUp(raw) {
    if (raw.eldurSlot !== undefined) {
        return _logicalParent(raw);
    }
    return _logicalParent(raw) || raw.parent || null;
}

// An anonymous block's lines are not a node of the page.
function _isAnonLines(raw) {
    return typeof raw.firstBaseline === "number" && !raw.objectName && !_meta(raw).createdTag
        && typeof raw.eldurTag !== "string";
}

function _logicalParent(raw) {
    if (raw && raw.__eldurInline) {
        return raw.parent;
    }
    if (raw && raw.eldurInlineOwner !== undefined) {
        var cutFrom = _vCutOwner(raw);
        if (cutFrom) {
            return cutFrom;
        }
    }
    if (raw && raw.eldurSlot) {
        raw = raw.eldurSlot;
    }
    if (!raw || !raw.parent || raw.eldurSlot === null) {
        return null;
    }
    var p = raw.parent;
    var pm = _meta(p);
    if (_isAnonLines(p)) {
        return _logicalParent(p);
    }
    if (pm.hostBox) {
        return pm.hostBox;
    }
    if (p.parent && p.parent.contentItem === p && !p.objectName && !pm.createdTag && typeof p.eldurTag !== "string") {
        return p.parent;
    }
    if ((typeof p.spacing === "number" || typeof p.columnSpacing === "number")
        && !p.objectName && !pm.createdTag && typeof p.eldurTag !== "string"
        && p.parent) {
        if (p.parent.parent && p.parent.parent.contentItem === p.parent
            && !p.parent.objectName && !_meta(p.parent).createdTag && typeof p.parent.eldurTag !== "string") {
            return p.parent.parent;
        }
        if ((p.parent.color !== undefined || p.parent.clip === true || typeof p.parent.eldurTag === "string" || p.parent.objectName)
            && typeof p.parent.spacing !== "number" && typeof p.parent.columnSpacing !== "number") {
            return p.parent;
        }
    }
    return p;
}

function _inferTagName(raw) {
    if (!raw) {
        return "";
    }
    if (typeof raw.eldurTag === "string" && raw.eldurTag !== "") {
        return raw.eldurTag.toUpperCase();
    }
    var m = _meta(raw);
    if (m.createdTag) {
        return String(m.createdTag).toUpperCase();
    }
    if (typeof raw.eldurTag === "string" && raw.eldurTag !== "") {
        return raw.eldurTag.toUpperCase();
    }
    if (typeof raw.find === "function" && typeof raw.currentIndex === "number") {
        return "SELECT";
    }
    if (typeof raw.checked === "boolean" && typeof raw.toggled === "function" && raw.checkable) {
        return "INPUT";
    }
    if (typeof raw.value === "number" && (typeof raw.from === "number" || typeof raw.stepSize === "number")
        && typeof raw.indeterminate !== "boolean") {
        return "INPUT";
    }
    if (typeof raw.value === "number") {
        return "PROGRESS";
    }
    if (typeof raw.clicked === "function") {
        return "BUTTON";
    }
    if (typeof raw.readOnly === "boolean" && typeof raw.placeholderText === "string") {
        return raw.textFormat !== undefined ? "TEXTAREA" : "INPUT";
    }
    if (raw.source !== undefined) {
        return "IMG";
    }
    if (typeof raw.text === "string") {
        return "SPAN";
    }
    return "DIV";
}

function getText(item) {
    var raw = _unwrap(item);
    if (!raw) {
        return "";
    }
    if (raw.__eldurInline) {
        return _vText(raw);
    }
    var vOwn = _vModelOf(raw);
    if (vOwn) {
        return _vText(vOwn);
    }
    var vRun = _findTextChild(raw);
    if (vRun && _gIsBox(raw) && _vModelOf(vRun)) {
        return _vText(_vModelOf(vRun));
    }
    if (vRun && _gIsBox(raw) && _meta(vRun).vSplit) {
        return _rawKids(raw, true).filter(function(k) { return k !== vRun; }).map(getText).join("");
    }
    if (vRun && _gIsBox(raw) && vRun.text !== "" && _rawKids(raw, true).length > 0) {
        return vRun.text + _rawKids(raw, true).filter(function(k) { return k !== vRun; }).map(getText).join("");
    }
    if ((vRun && _vAnonText(vRun)) || ((typeof raw.text !== "string" || _meta(raw).vSplit) && _holdsLines(raw))) {
        return _rawKids(raw, true).map(function(kid) { return _wsText(_wsBefore(kid)) + getText(kid); }).join("")
            + _wsText(_wsEnd(raw));
    }
    if (_isPlainJsObject(raw) && ("textContent" in raw || "innerText" in raw)) {
        return raw.textContent !== undefined ? raw.textContent : raw.innerText;
    }
    var run = _meta(raw).runText;
    if (run !== undefined) {
        return run;
    }
    // What is typed in an input is its value; it has no content.
    if (raw.eldurTag === "input" || _meta(raw).createdTag === "input") {
        return "";
    }
    if (typeof raw.text === "string") {
        return _pseudoOwn(raw, raw.text);
    }
    var textChild = _findTextChild(raw);
    // A row of controls in a Flow: its text item is only the space between them.
    // A tagged one is an element of its own, read with the others below.
    if (textChild && !textChild.eldurTag
        && !(_rawElementChildren(raw).length > 0 && textChild.text.trim() === "")) {
        return _pseudoOwn(raw, String(textChild.text));
    }
    var m = _meta(raw);
    if (m.explicitText !== undefined) {
        return String(m.explicitText);
    }
    // On lines a run's own text nodes hold the space the record has too.
    var lined = _childContainer(raw);
    if (lined && typeof lined.firstBaseline === "number") {
        var words = "";
        _rawKids(raw, true).forEach(function(kid) {
            if (!kid.vtext && !/\s$/.test(words)) {
                words += _wsText(_wsBefore(kid));
            }
            words += getText(kid);
        });
        return words + _wsText(_wsEnd(raw));
    }
    // `textContent` is the text of everything inside, the whitespace between too.
    var inside = _rawElementChildren(raw).map(function(kid) { return _wsText(_wsBefore(kid)) + getText(kid); });
    return inside.join("") + _wsText(_wsEnd(raw));
}

// The converter's record of whitespace between elements (`declare_whitespace`),
// as text nodes until a script moves something; no item stands for it. Each
// stays a node of its own, as a removal between two leaves two.
function _wsRuns(raw, key, declared) {
    if (!raw || raw.__eldurInline) {
        return [];
    }
    var m = _meta(raw);
    if (!m[key]) {
        var given = raw[declared];
        m[key] = typeof given === "string" && given !== "" && !_isCreatedElement(raw)
            ? [_vLeaf(given, null, null)] : [];
        m[key].forEach(function(leaf) { leaf.wsOwner = raw; leaf.wsKey = key; });
    }
    return m[key];
}

// Whitespace in markup a script gives, kept like the converter's.
function _wsGiven(raw, key, text) {
    if (text && !raw.__eldurInline) {
        var leaf = _vLeaf(text, null, null);
        leaf.wsOwner = raw;
        leaf.wsKey = key;
        _wsRuns(raw, key, "").push(leaf);
    }
}

function _wsBefore(raw) {
    return _wsRuns(raw, "ws", "eldurWs");
}

function _wsEnd(raw) {
    return _wsRuns(raw, "wsEnd", "eldurWsEnd");
}

function _wsText(runs) {
    return runs.map(function(leaf) { return leaf.data; }).join("");
}

function _wsMove(runs, owner, key, front) {
    var into = key === "ws" ? _wsBefore(owner) : _wsEnd(owner);
    runs.forEach(function(leaf) {
        leaf.wsOwner = owner;
        leaf.wsKey = key;
        leaf.vparent = key === "ws" ? _logicalParent(owner) : owner;
    });
    if (front) {
        Array.prototype.unshift.apply(into, runs);
    } else {
        Array.prototype.push.apply(into, runs);
    }
    runs.length = 0;
}

// A removed element's whitespace stays where it was, before what comes next.
function _wsLeave(raw) {
    var runs = _wsBefore(raw);
    if (runs.length === 0) {
        return;
    }
    var parent = _logicalParent(raw);
    if (!parent) {
        return;
    }
    var kids = _rawElementChildren(parent);
    var next = kids[kids.indexOf(raw) + 1];
    if (next) {
        _wsMove(runs, next, "ws", true);
    } else {
        _wsMove(runs, parent, "wsEnd", true);
    }
}

// What is inserted goes after the whitespace that was at that place.
function _wsEnter(count, ref, container) {
    var owner = _meta(container).hostBox || container;
    // Looked up, as insertion can swap an element for its template's form.
    var kids = _rawElementChildren(owner);
    var first = kids[(ref ? kids.indexOf(ref) : kids.length) - count];
    if (!first || first.__eldurInline || (ref && ref.__eldurInline)) {
        return;
    }
    _wsMove(ref ? _wsBefore(ref) : _wsEnd(owner), first, "ws", false);
}

// `display` by element, as a page's closed form has no computed styles to ask.
var _IT_BLOCK = /^(address|article|aside|blockquote|center|dd|details|dialog|dir|div|dl|dt|fieldset|figcaption|figure|footer|form|h[1-6]|header|hgroup|hr|li|main|menu|nav|ol|p|pre|section|summary|table|tbody|tfoot|thead|ul)$/;

// The HTML standard's rendered text collection: what is shown, with a line
// between blocks, two around a paragraph, and a tab between cells.
var _IT_ATOM = "\uE000";

function _itCollect(raw, out) {
    if (raw.__eldurInline) {
        if (raw.vtext) {
            if (!raw.vparent || _vStyle(raw.vparent).visible !== false) {
                out.push(raw.data);
            }
            return;
        }
        if (raw.eldurTag === "br") {
            out.push("\n");
            return;
        }
        if (_vStyle(raw).display === "none") {
            return;
        }
    } else if (_itHidden(raw)) {
        return;
    }
    var tag = _inferTagName(raw).toLowerCase();
    if (tag === "br") {
        out.push("\n");
        return;
    }
    // A control has no text, but the spaces either side of it do not collapse.
    if (/^(input|select|textarea|img)$/.test(tag)) {
        out.push(_IT_ATOM);
        return;
    }
    var edge = tag === "p" ? 2 : _IT_BLOCK.test(tag) ? 1 : 0;
    // A run of text in its lines is no block of its own, and its model counts.
    var run = !raw.__eldurInline && raw.parent && typeof raw.parent.firstBaseline === "number";
    if (run) {
        edge = 0;
    }
    if (edge) {
        out.push(edge);
    }
    var model = raw.__eldurInline ? raw : run ? _vModelOf(raw) || _vOwnModel(raw) : _vOwnModel(raw);
    if (model && model.kids) {
        model.kids.filter(_vIsRendered).forEach(function(k) { _itCollect(k, out); });
    } else {
        var kids = _rawKids(raw, true);
        if (kids.length === 0) {
            out.push(getText(raw));
        } else if (kids[0].tableSlot) {
            _itGrid(kids, out);
        } else {
            _itCells(kids, tag, out);
        }
    }
    if (edge) {
        out.push(edge);
    }
}

// The cells of a static table are one list; each knows its row.
function _itGrid(kids, out) {
    var row = -1;
    kids.forEach(function(kid) {
        if (kid.tableSlot) {
            if (row >= 0) {
                out.push(kid.tableSlot[0] !== row ? 1 : "\t");
            }
            row = kid.tableSlot[0];
            _itCollect(kid, out);
        } else {
            _itCollect(kid, out);
        }
    });
}

function _itCells(kids, tag, out) {
    kids.forEach(function(kid, i) {
        _itCollect(kid, out);
        if (tag === "tr" && i < kids.length - 1) {
            out.push("\t");
        }
    });
    if (tag === "tr") {
        out.push(1);
    }
}

function getInnerText(item) {
    var raw = _unwrap(item);
    return raw ? _innerText(raw) : "";
}

function _itHidden(raw) {
    // Until the window shows, `visible` says no of everything.
    return !raw.__eldurInline && raw.visible === false
        && (_root.contentItem ? _root.contentItem.visible : _root.visible) !== false;
}

function _innerText(raw) {
    // An element not being rendered reads as its textContent.
    if (_itHidden(raw)) {
        return getText(raw);
    }
    var out = [];
    _itCollect(raw, out);
    var text = "";
    var pending = 0;
    for (var i = 0; i < out.length; ++i) {
        if (typeof out[i] === "number") {
            pending = Math.max(pending, out[i]);
            continue;
        }
        var piece = out[i];
        if (piece.trim() === "" && piece !== "\t" && piece !== "\n" && (pending > 0 || text === "" || /\s$/.test(text))) {
            continue;
        }
        if (pending > 0 && text !== "") {
            text = text.replace(/ +$/, "") + "\n\n".slice(0, pending);
        }
        pending = 0;
        text += piece;
    }
    return text.replace(/[ \t\r\f]*\n[ \t\r\f]*(?=\S)/g, function(m) { return m.replace(/[^\n]/g, ""); })
        .replace(/[ \r\f\n]+/g, function(m) { return m.indexOf("\n") >= 0 ? m.replace(/[^\n]/g, "") : " "; })
        .replace(/^ +| +$/g, "")
        .split(_IT_ATOM).join("");
}

function setText(item, value) {
    var raw = _unwrap(item);
    if (!raw) {
        return;
    }
    if (raw.__eldurInline) {
        _vSetText(raw, value);
        return;
    }
    _gTouch();
    var vKnown = _isPlainJsObject(raw) ? null : _vExisting(raw);
    if (vKnown) {
        _vSetText(vKnown, value);
        return;
    }
    if (_isPlainJsObject(raw)) {
        raw.textContent = value;
        return;
    }
    var str = value === null || value === undefined ? "" : String(value);
    var m = _meta(raw);
    if (_holdsLines(raw)) {
        // Every child goes, and the runs with them; the line stays for the text.
        _rawKids(raw, true).forEach(function(kid) {
            if (kid.__eldurInline || _meta(kid).createdTag || kid.objectName) {
                _detachNode(kid);
            } else {
                kid.destroy();
            }
        });
        _wsEnd(raw).length = 0;
        if (str !== "") {
            _insertChildren(raw, [_unwrap(document.createTextNode(str))], null);
        }
        return;
    }
    m.explicitText = str;
    if (m.runText !== undefined) {
        m.runText = str;
        raw.text = _runText(str);
        return;
    }
    if (typeof raw.text === "string") {
        raw.text = _pseudoWrapped(raw, str);
        return;
    }
    _clearAppended(raw);
    var textChild = _findTextChild(raw);
    if (textChild) {
        textChild.text = _pseudoWrapped(raw, str);
    }
}

// What a `<template>` holds is parsed when a script first asks for it.
function _templateContent(raw) {
    var m = _meta(raw);
    var written = _root && _root.eldurHtmlTemplates;
    var markup = typeof written === "string" && _nodeTag(raw) === "template"
        ? JSON.parse(written)[_nodeId(raw)] : undefined;
    if (markup === undefined) {
        return undefined;
    }
    if (!m.templateContent) {
        m.templateContent = document.createDocumentFragment();
        var nodes = _parseHtmlFragmentNodes(markup);
        for (var i = 0; i < nodes.length; ++i) {
            m.templateContent.appendChild(nodes[i]);
        }
    }
    return m.templateContent;
}

var _G_BLOCK_MARKUP = /<(address|article|aside|blockquote|button|details|div|dl|fieldset|figure|footer|form|h[1-6]|header|hr|input|li|main|nav|ol|p|section|select|table|textarea|ul)\b/i;

function setInnerHTML(item, html) {
    var raw = _unwrap(item);
    if (!raw) {
        return;
    }
    var str = html === null || html === undefined ? "" : String(html);
    if (/<\s*(script|style)\b/i.test(str)) {
        console.warn("eldur: <script> and <style> inside innerHTML are unsupported");
        return;
    }
    if (str === "" && !raw.__eldurInline) {
        _rawElementChildren(raw).forEach(_detachNode);
    }
    var vHtml = raw.__eldurInline ? raw : _vExisting(raw);
    if (!vHtml && _vHasRules() && !_markupMakesElements(raw) && typeof raw.text === "string") {
        vHtml = _vOwnModel(raw);
    }
    if (vHtml) {
        _vSetInnerHTML(vHtml, str);
        return;
    }
    // Elements a paragraph could not hold: the empty container the converter
    // wrote as a line of text was never one.
    if (typeof raw.text === "string" && _gHasCascade() && _G_BLOCK_MARKUP.test(str)
        && typeof raw.eldurTag === "string" && !_G_CONTROL.test(raw.eldurTag)) {
        var box = _gAdopt(raw);
        if (box) {
            _replaceWithMarkup(box, str);
            _rememberMarkup(box, str);
            return;
        }
    }
    // A box the converter gave an empty line of text holds elements as any other.
    var blockInBox = typeof raw.text !== "string" && _gHasCascade() && _G_BLOCK_MARKUP.test(str)
        && typeof raw.eldurTag === "string" && !_G_CONTROL.test(raw.eldurTag);
    if (blockInBox || _markupMakesElements(raw)) {
        _replaceWithMarkup(raw, str);
        _rememberMarkup(raw, str);
        return;
    }
    var target = typeof raw.text === "string" ? raw : _findTextChild(raw);
    if (target && typeof target.text === "string") {
        if (typeof target.textFormat === "number") {
            // Text.StyledText === 4 in QtQuick
            target.textFormat = 4;
        }
        target.text = str;
        _rememberMarkup(raw, str);
        return;
    }
    console.warn("eldur: innerHTML assignment on non-text QML item is unsupported");
}

// Whether markup set on `raw` becomes elements, as `createElement` makes
// them, rather than styled text. A `Text` or control shows it as text. A box
// does when it has a text child the converter predicted no elements in:
// inline markup in a paragraph stays one run of text.
function _markupMakesElements(raw) {
    if (typeof raw.text === "string") {
        return false;
    }
    if (!_findTextChild(raw)) {
        return true;
    }
    var templates = _root && _root.eldurTemplates;
    if (!templates || typeof templates !== "object") {
        return false;
    }
    var contexts = [];
    if (typeof raw.eldurKey === "string" && raw.eldurKey !== "") {
        contexts.push("@" + raw.eldurKey);
    }
    if (typeof raw.eldurTemplate === "string" && raw.eldurTemplate !== "") {
        contexts.push("@" + raw.eldurTemplate);
    }
    var keys = Object.keys(templates);
    for (var i = 0; i < keys.length; ++i) {
        for (var c = 0; c < contexts.length; ++c) {
            var k = keys[i];
            if (k.length > contexts[c].length && k.slice(-contexts[c].length) === contexts[c]) {
                return true;
            }
        }
    }
    return false;
}

// `sort` as the language has it, stable and undefined last, which a page's table
// sorted on one column and then another depends on.
function sort(list, compare) {
    var typed = ArrayBuffer.isView(list) && !(list instanceof DataView);
    if (!Array.isArray(list) && !typed) {
        return list.sort(compare);
    }
    if (compare !== undefined && typeof compare !== "function") {
        throw new TypeError("The comparison function must be either a function or undefined");
    }
    if (typed) {
        // V4's typed arrays have no `sort`; the default order is numeric here.
        var values = _mergeSort(Array.from(list), compare || _numericOrder);
        for (var t = 0; t < values.length; ++t) {
            list[t] = values[t];
        }
        return list;
    }
    var order = compare || function(a, b) {
        var x = String(a), y = String(b);
        return x < y ? -1 : x > y ? 1 : 0;
    };
    var kept = [];
    var missing = 0;
    for (var i = 0; i < list.length; ++i) {
        if (list[i] === undefined) {
            missing++;
        } else {
            kept.push(list[i]);
        }
    }
    var merged = _mergeSort(kept, order);
    for (var j = 0; j < merged.length; ++j) {
        list[j] = merged[j];
    }
    for (var u = merged.length; u < merged.length + missing; ++u) {
        list[u] = undefined;
    }
    return list;
}

// %TypedArray%.prototype.sort's default: NaN last, -0 before +0.
function _numericOrder(a, b) {
    if (a !== a) {
        return b !== b ? 0 : 1;
    }
    if (b !== b) {
        return -1;
    }
    if (a === 0 && b === 0) {
        return 1 / a === 1 / b ? 0 : 1 / a < 1 / b ? -1 : 1;
    }
    return a - b;
}

function _mergeSort(items, order) {
    if (items.length < 2) {
        return items;
    }
    var mid = items.length >> 1;
    var left = _mergeSort(items.slice(0, mid), order);
    var right = _mergeSort(items.slice(mid), order);
    var out = [];
    var l = 0, r = 0;
    while (l < left.length && r < right.length) {
        out.push(order(left[l], right[r]) <= 0 ? left[l++] : right[r++]);
    }
    return out.concat(left.slice(l), right.slice(r));
}

// `innerHTML = markup` on a box: its children are replaced by the markup's
// nodes, made by `createElement`, so connecting them looks their templates
// up (§6.7) as for any created element.
function _replaceWithMarkup(raw, str) {
    _layoutChanged(raw);
    var m = _meta(raw);
    if (_findTextChild(raw)) {
        setText(raw, "");
        delete m.explicitText;
    }
    var old = _rawElementChildren(raw);
    for (var i = 0; i < old.length; ++i) {
        _detachNode(old[i]);
    }
    _wsEnd(raw).length = 0;
    var parsed = _parseHtmlFragmentNodes(str);
    var nodes = _expandInsertRawNodes(parsed);
    _insertRawNodesBefore(_appendTarget(raw), nodes, null);
    _wsGiven(raw, "wsEnd", parsed.wsEnd);
}

function _rememberMarkup(raw, str) {
    var m = _meta(raw);
    m.innerHTML = str;
    m.innerHTMLKids = _rawElementChildren(raw);
}

function getInnerHTML(item) {
    var raw = _unwrap(item);
    var vInner = !raw ? null : raw.__eldurInline ? raw : _vExisting(raw);
    if (vInner) {
        return vInner.vtext ? "" : _vInner(vInner);
    }
    var m = _meta(raw);
    var kids = raw ? _rawElementChildren(raw) : [];
    // What was assigned reads back as written, until the children change.
    if (m.innerHTML !== undefined && m.innerHTMLKids.length === kids.length
        && kids.every(function(k, i) { return k === m.innerHTMLKids[i]; })) {
        return m.innerHTML;
    }
    return kids.length > 0 ? kids.map(_serializeElement).join("") : getText(raw);
}

// Elements written back as markup, for `innerHTML` to be read after the tree
// changed and for `+=`, which parses what it read.
function _serializeElement(raw) {
    if (raw.__eldurInline) {
        return _vSerialize(raw);
    }
    var m = _meta(raw);
    var tag = _nodeTag(raw);
    var attrs = {};
    var id = _nodeId(raw);
    if (id) {
        attrs.id = id;
    }
    if (m.classes.length > 0) {
        attrs["class"] = m.classes.join(" ");
    }
    Object.keys(m.attrs).forEach(function(k) { attrs[k] = m.attrs[k]; });
    Object.keys(m.dataset).forEach(function(k) {
        attrs["data-" + k.replace(/[A-Z]/g, function(c) { return "-" + c.toLowerCase(); })] = m.dataset[k];
    });
    var open = "<" + tag;
    Object.keys(attrs).forEach(function(k) {
        open += " " + k + "=\"" + String(attrs[k]).replace(/&/g, "&amp;").replace(/"/g, "&quot;") + "\"";
    });
    return open + ">" + getInnerHTML(raw) + "</" + tag + ">";
}

function setStyle(item, prop, value) {
    var raw = _unwrap(item);
    if (!raw) {
        return;
    }
    if (raw.__eldurInline) {
        _vSetStyle(raw, prop, value);
        return;
    }
    var rawName = String(prop);
    var str = value === null || value === undefined ? "" : String(value);
    var m = _meta(raw);
    if (rawName === "cssText") {
        m.styles = {};
        var decls = str.split(";");
        for (var i = 0; i < decls.length; ++i) {
            var d = decls[i].trim();
            if (!d) {
                continue;
            }
            var colon = d.indexOf(":");
            if (colon === -1) {
                continue;
            }
            var k = d.slice(0, colon).trim();
            var v = d.slice(colon + 1).trim();
            if (k) {
                setStyle(raw, k, v);
            }
        }
        return;
    }
    var name = rawName.indexOf("--") === 0 ? rawName : _kebabToCamel(rawName);
    m.styles[name] = str;
    if (rawName !== name) {
        m.styles[rawName] = str;
    }
    _gTouch();
    if (_gIsBox(raw)) {
        return;
    }

    if (_isPlainJsObject(raw)) {
        if (!raw.style) {
            raw.style = {};
        }
        raw.style[name] = str;
        return;
    }

    if (name === "display") {
        if (typeof raw.visible === "boolean") {
            if (_gHasCascade()) {
                ++_gGen;
                _gShowAsRules(raw);
            } else if (str === "") {
                _layoutChanged(raw.parent);
                _inlineRelease(raw, raw, "visible");
            } else {
                _setShown(raw, str !== "none");
                _gShownInline(raw, str === "inline");
            }
        }
    } else if (name === "visibility" || name === "opacity") {
        _inlineOpacity(raw, m);
    } else if (name === "position") {
        // A template holds the box's form, and layouts place what is in them.
        if (str === "absolute" || str === "fixed") {
            console.warn("eldur: style.position = \"" + str + "\" from a script is not applied; use a class");
        }
    } else if (name === "left" || name === "top" || name === "right" || name === "bottom") {
        _setOffset(raw, name, str);
    } else if (name === "zIndex") {
        var zi = parseInt(str, 10);
        if (!isNaN(zi) && typeof raw.z === "number") {
            raw.z = zi;
        }
    } else if (name === "color") {
        _inlineColor(raw, Boolean(m.styles.color));
    } else if (_gActive() && _gStaticBase !== null) {
        // The next pass adopts it: only the box form has all of a box's answers.
    } else if (name === "width") {
        _inlinePx(raw, raw, "width", str);
    } else if (name === "height") {
        _inlinePx(raw, raw, "height", str);
    } else if (name === "backgroundColor" || name === "background") {
        if (raw.background && raw.background.color !== undefined) {
            _inlinePaint(raw, raw.background, "color", str);
        } else if (raw.color !== undefined && typeof raw.text !== "string") {
            _inlinePaint(raw, raw, "color", str);
        }
    } else if (name === "fontSize") {
        var fontTarget = raw.font !== undefined ? raw : _findTextChild(raw);
        if (fontTarget && fontTarget.font !== undefined) {
            _inlinePx(raw, fontTarget, "font.pixelSize", str);
        }
    } else if (name === "borderRadius" && typeof raw.radius === "number") {
        _inlinePx(raw, raw, "radius", str);
    } else if (name === "borderColor" && raw.border !== undefined) {
        _inlinePaint(raw, raw, "border.color", str);
    } else if (name === "borderWidth" && raw.border !== undefined) {
        _inlinePx(raw, raw, "border.width", str);
    }
}

function _inlinePaint(raw, target, prop, str) {
    if (str === "") {
        _inlineRelease(raw, target, prop);
    } else {
        _inlineHold(raw, target, prop, str);
    }
}

function _inlinePx(raw, target, prop, str) {
    var px = parseFloat(str);
    if (isNaN(px)) {
        _inlineRelease(raw, target, prop);
    } else {
        _inlineHold(raw, target, prop, px);
    }
}

// `color` is inherited: each `Text` below that has no inline colour of its own
// takes what the rules now give it, or goes back to the converter's.
function _inlineColor(raw, under) {
    var m = _meta(raw);
    var own = Boolean(m.styles.color);
    var text = typeof raw.text === "string" && raw.color !== undefined ? raw : _findTextChild(raw);
    if (text && text.color !== undefined) {
        if (under || own) {
            var color = own && !_gHasCascade() ? m.styles.color : null;
            if (color === null) {
                ++_gGen;
                color = _gQtColor(_gStyle(raw).color);
            }
            _inlineHold(raw, text, "color", color);
        } else {
            _inlineRelease(raw, text, "color");
        }
    }
    var kids = _gHasCascade() ? _rawElementChildren(raw) : [];
    for (var i = 0; i < kids.length; ++i) {
        if (!_gIsBox(kids[i]) && !kids[i].__eldurInline) {
            _inlineColor(kids[i], under || own);
        }
    }
}

function _shownByAttribute(raw, hidden) {
    if (_gHasCascade()) {
        ++_gGen;
        _gShowAsRules(raw);
    } else if (hidden) {
        _setShown(raw, false);
    } else if (_meta(raw).styles.display === "none") {
        _setShown(raw, false);
    } else {
        // Nothing here but the page's item says it is hidden, and the attribute
        // may be why, so there is no binding to go back to.
        _setShown(raw, true);
    }
}

// `visibility` has no property of its own here: it paints as no opacity.
function _inlineOpacity(raw, m) {
    if (typeof raw.opacity !== "number") {
        return;
    }
    var hidden = m.styles.visibility;
    var written = m.styles.opacity;
    if (!hidden && !written) {
        _inlineRelease(raw, raw, "opacity");
        _inlineRelease(raw, raw, "enabled");
        return;
    }
    var shown = true;
    var opacity = parseFloat(written);
    if (_gHasCascade()) {
        ++_gGen;
        var s = _gStyle(raw);
        shown = s.visibility !== "hidden";
        opacity = parseFloat(s.opacity);
    } else if (hidden) {
        shown = hidden !== "hidden";
    }
    _inlineHold(raw, raw, "opacity", !shown ? 0 : isNaN(opacity) ? 1 : opacity);
    if (typeof raw.enabled === "boolean") {
        _inlineHold(raw, raw, "enabled", shown);
    }
}

function _camelToKebab(propName) {
    return String(propName).replace(/[A-Z]/g, function(ch) {
        return "-" + ch.toLowerCase();
    });
}

function getStyle(item, prop) {
    var vRaw = _unwrap(item);
    if (vRaw && vRaw.__eldurInline) {
        return _vGetStyle(vRaw, prop);
    }
    var m = _meta(item);
    var rawName = String(prop);
    if (rawName === "cssText") {
        var keys = Object.keys(m.styles);
        var parts = [];
        var seen = {};
        for (var i = 0; i < keys.length; ++i) {
            var k = keys[i];
            var kebab = k.indexOf("--") === 0 ? k : _camelToKebab(k);
            if (!seen[kebab] && m.styles[k] !== "") {
                seen[kebab] = true;
                parts.push(kebab + ": " + m.styles[k]);
            }
        }
        return parts.join("; ");
    }
    if (Object.prototype.hasOwnProperty.call(m.styles, rawName)) {
        return m.styles[rawName];
    }
    var camel = rawName.indexOf("--") === 0 ? rawName : _kebabToCamel(rawName);
    if (Object.prototype.hasOwnProperty.call(m.styles, camel)) {
        return m.styles[camel];
    }
    var raw = _unwrap(item);
    if (camel === "display" && raw && typeof raw.visible === "boolean") {
        return raw.visible ? "" : "none";
    }
    return "";
}

function getComputedStyle(item) {
    var raw = _unwrap(item);
    if (raw && raw.__eldurInline) {
        return _vComputedStyle(raw);
    }
    function computeProp(prop) {
        var p = String(prop);
        if (_gHasCascade() && raw && typeof raw === "object") {
            _layoutForRead();
            // Settling may have replaced the item.
            raw = _gCurrent(raw);
            var settled = _gComputed(raw, p);
            if (settled !== null) {
                return settled;
            }
        }
        var explicit = getStyle(raw, p);
        if (explicit !== "") {
            return explicit;
        }
        var camel = p.indexOf("--") === 0 ? p : _kebabToCamel(p);
        if (!raw || typeof raw !== "object") {
            return "";
        }
        if (camel === "display") {
            return raw.visible === false ? "none" : "block";
        }
        if (camel === "visibility") {
            return raw.opacity === 0 ? "hidden" : "visible";
        }
        if (camel === "opacity") {
            return typeof raw.opacity === "number" ? String(raw.opacity) : "1";
        }
        if (camel === "width") {
            return typeof raw.width === "number" ? raw.width + "px" : "0px";
        }
        if (camel === "height") {
            return typeof raw.height === "number" ? raw.height + "px" : "0px";
        }
        if (camel === "color") {
            var tc = typeof raw.text === "string" ? raw : _findTextChild(raw);
            return tc && tc.color !== undefined ? String(tc.color) : "";
        }
        if (camel === "backgroundColor") {
            if (raw.background && raw.background.color !== undefined) {
                return String(raw.background.color);
            }
            if (raw.color !== undefined && typeof raw.text !== "string") {
                return String(raw.color);
            }
            return "";
        }
        if (camel === "fontSize") {
            var ft = raw.font !== undefined ? raw : _findTextChild(raw);
            return ft && ft.font && typeof ft.font.pixelSize === "number" ? ft.font.pixelSize + "px" : "";
        }
        if (camel === "borderRadius") {
            return typeof raw.radius === "number" ? raw.radius + "px" : "0px";
        }
        return "";
    }
    var base = {
        getPropertyValue: function(prop) {
            return computeProp(prop);
        }
    };
    return new Proxy(base, {
        get: function(target, prop) {
            if (prop in target) {
                return target[prop];
            }
            if (typeof prop === "string") {
                return computeProp(prop);
            }
            return undefined;
        }
    });
}

function _syncNumericControlAttr(raw, name, val) {
    if (!raw || typeof raw !== "object") {
        return;
    }
    if (name === "min" && typeof raw.from === "number") {
        var nMin = Number(val);
        if (!isNaN(nMin)) {
            raw.from = nMin;
        }
    } else if (name === "max" && typeof raw.to === "number") {
        var nMax = Number(val);
        if (!isNaN(nMax)) {
            raw.to = nMax;
        }
    } else if (name === "step" && typeof raw.stepSize === "number") {
        var nStep = Number(val);
        if (!isNaN(nStep) && nStep > 0) {
            raw.stepSize = nStep;
        }
    }
}

function setAttribute(item, attrName, attrValue) {
    if (item && !_isDomTarget(item) && typeof item.setAttribute === "function") {
        return item.setAttribute(attrName, attrValue);
    }
    var raw = _unwrap(item);
    if (!raw) {
        return;
    }
    if (raw.__eldurInline) {
        _vSetAttr(raw, attrName, attrValue);
        return;
    }
    var name = String(attrName);
    var val = String(attrValue);
    var m = _meta(raw);
    m.attrs[name] = val;

    if (name.indexOf("data-") === 0 && name.length > 5) {
        m.dataset[_kebabToCamel(name.slice(5))] = val;
        return;
    }
    _gTouch();
    if (name === "id" && typeof raw.objectName === "string") {
        raw.objectName = val;
    } else if (name === "type" && m.createdTag === "input") {
        wrap(raw).type = val;
    } else if (name === "class") {
        setClassName(raw, val);
    } else if (name === "disabled" && typeof raw.enabled === "boolean") {
        raw.enabled = false;
    } else if (name === "hidden" && typeof raw.visible === "boolean") {
        _shownByAttribute(raw, true);
    } else if (name === "readonly" && typeof raw.readOnly === "boolean") {
        raw.readOnly = true;
    } else if (name === "value") {
        if (typeof raw.find === "function" && typeof raw.currentIndex === "number") {
            raw.currentIndex = raw.find(val);
        } else if (typeof raw.text === "string") {
            raw.text = val;
        } else if (typeof raw.value === "number") {
            raw.value = Number(val) || 0;
        }
    } else if (name === "min" || name === "max" || name === "step") {
        _syncNumericControlAttr(raw, name, val);
    } else if (name === "src" && raw.source !== undefined) {
        raw.source = val;
        m.lastSyncedSource = String(raw.source);
    } else if (name === "placeholder" && typeof raw.placeholderText === "string") {
        raw.placeholderText = val;
    }
}

function getAttribute(item, attrName) {
    if (item && !_isDomTarget(item) && typeof item.getAttribute === "function") {
        return item.getAttribute(attrName);
    }
    var raw = _unwrap(item);
    if (!raw) {
        return null;
    }
    if (raw.__eldurInline) {
        return raw.vtext ? null : _vGetAttr(raw, attrName);
    }
    var name = String(attrName);
    var m = _meta(raw);
    if (name.indexOf("data-") === 0 && name.length > 5) {
        var camel = _kebabToCamel(name.slice(5));
        if (Object.prototype.hasOwnProperty.call(m.dataset, camel)) {
            return String(m.dataset[camel]);
        }
    }
    if (name === "src" && raw.source !== undefined && m.lastSyncedSource !== undefined && String(raw.source) !== m.lastSyncedSource) {
        delete m.attrs.src;
        m.lastSyncedSource = undefined;
    }
    if (Object.prototype.hasOwnProperty.call(m.attrs, name)) {
        return m.attrs[name];
    }
    if (name === "id" && typeof raw.objectName === "string" && raw.objectName.length > 0) {
        return raw.objectName;
    }
    if (name === "class" && (m.classes.length > 0 || m.classAttr)) {
        return m.classes.join(" ");
    }
    if (name === "src" && raw.source !== undefined && String(raw.source) !== "") {
        var rawSrcAttr = String(raw.source);
        if (typeof Qt !== "undefined" && typeof Qt.resolvedUrl === "function") {
            var qmlBaseDir = String(Qt.resolvedUrl("."));
            if (qmlBaseDir && rawSrcAttr.indexOf(qmlBaseDir) === 0) {
                rawSrcAttr = rawSrcAttr.slice(qmlBaseDir.length);
            }
        }
        return rawSrcAttr;
    }
    if (name === "href") {
        if (typeof raw.href === "string") {
            return raw.href;
        }
        var txt = typeof raw.text === "string" ? raw.text : "";
        if (!txt) {
            var tc = _findTextChild(raw);
            if (tc && typeof tc.text === "string") {
                txt = tc.text;
            }
        }
        if (txt) {
            var hrefMatch = /<a\b[^>]*?\bhref\s*=\s*(?:"([^"]*)"|'([^']*)')/i.exec(txt);
            if (hrefMatch) {
                return hrefMatch[1] !== undefined ? hrefMatch[1] : hrefMatch[2];
            }
        }
    }
    return null;
}

function removeAttribute(item, attrName) {
    if (item && !_isDomTarget(item) && typeof item.removeAttribute === "function") {
        return item.removeAttribute(attrName);
    }
    var raw = _unwrap(item);
    if (!raw) {
        return;
    }
    if (raw.__eldurInline) {
        _vRemoveAttr(raw, attrName);
        return;
    }
    var name = String(attrName);
    var m = _meta(raw);
    delete m.attrs[name];
    if (name.indexOf("data-") === 0 && name.length > 5) {
        delete m.dataset[_kebabToCamel(name.slice(5))];
    } else if (name === "id" && typeof raw.objectName === "string") {
        raw.objectName = "";
    } else if (name === "disabled" && typeof raw.enabled === "boolean") {
        raw.enabled = true;
    } else if (name === "hidden" && typeof raw.visible === "boolean") {
        _shownByAttribute(raw, false);
    } else if (name === "readonly" && typeof raw.readOnly === "boolean") {
        raw.readOnly = false;
    } else if (name === "class") {
        setClassName(raw, "");
    }
}

function hasAttribute(item, attrName) {
    if (item && !_isDomTarget(item) && typeof item.hasAttribute === "function") {
        return item.hasAttribute(attrName);
    }
    return getAttribute(item, attrName) !== null;
}

function toggleAttribute(item, attrName, force) {
    if (item && !_isDomTarget(item) && typeof item.toggleAttribute === "function") {
        return item.toggleAttribute(attrName, force);
    }
    var name = String(attrName);
    var has = hasAttribute(item, name);
    var shouldHave = force !== undefined ? Boolean(force) : !has;
    if (shouldHave) {
        if (!has) {
            setAttribute(item, name, "");
        }
        return true;
    }
    if (has) {
        removeAttribute(item, name);
    }
    return false;
}

// A browser takes focus off what stops being rendered. Qt only moves activeFocus and
// leaves `focus` set, which would hand it back when the element shows again.
function _focusGone(raw) {
    if (raw === _focusHeld) {
        return false;
    }
    if (raw.visible !== false || !_root || _root.visible !== true) {
        return false;
    }
    if (raw.focus === true) {
        raw.focus = false;
    }
    return true;
}

// A browser notices at its next update, after the script's turn: until then
// `activeElement` still answers with the element, and only then do `blur` and
// `focusout` fire. Qt moves activeFocus as soon as an item hides, so what it says
// about a hidden item waits here.
var _focusHeld = null;
var _focusQueue = [];
var _focusLooked = false;

function _focusIsGhost(raw) {
    if (_gHasCascade()) {
        return _gStyle(raw).visibility === "hidden";
    }
    for (var p = raw; p; p = p.parent) {
        if (p.opacity === 0 && p.enabled === false) {
            return true;
        }
    }
    return false;
}

function _holdFocus() {
    if (_focusLooked || !_root) {
        return;
    }
    _focusLooked = true;
    Qt.callLater(_focusSettle);
    _focusHeld = _focusIn(_root);
}

function _focusIn(item) {
    var kids = item.children || [];
    for (var i = 0; i < kids.length; ++i) {
        var found = _focusIn(kids[i]);
        if (found) {
            return found;
        }
    }
    return item !== _root && item.activeFocus ? item : null;
}

// What Qt says when an item's focus goes: `run` hears it now, unless the item went
// because it stopped being rendered.
function whenBlurred(raw, type, run) {
    if (raw !== _focusHeld && (raw.visible !== false || !_root || _root.visible !== true)) {
        run();
        return;
    }
    if (!_focusLooked) {
        _focusLooked = true;
        Qt.callLater(_focusSettle);
    }
    _focusHeld = _focusHeld || raw;
    _focusQueue.push({ type: type, run: run });
}

function _focusSettle() {
    _gSettle();
    // Held from before the turn's click moved it, or found now: Qt can keep
    // activeFocus on a hidden item until the frame.
    var found = _root ? _focusIn(_root) : null;
    var held = found && found.visible === false ? found : _focusHeld;
    var queue = _focusQueue;
    _focusHeld = null;
    _focusQueue = [];
    _focusLooked = false;
    if (held) {
        if (held.visible === false || _focusIsGhost(held)) {
            // Left set, Qt would hand it back when the element shows again.
            _focusHeld = held;
            _focusLooked = true;
            held.focus = false;
            _focusHeld = null;
            _focusLooked = false;
            queue = queue.concat(_focusQueue);
            _focusQueue = [];
            if (_activeElement === held) {
                _activeElement = null;
            }
        } else if (held.activeFocus) {
            // Hidden and shown again within a turn: a browser saw nothing.
            queue = [];
        }
    }
    ["blur", "focusout"].forEach(function(type) {
        queue.forEach(function(entry) {
            if (entry.type === type) {
                entry.run();
            }
        });
    });
}

function focus(item) {
    if (item && !_isDomTarget(item) && typeof item.focus === "function") {
        return item.focus();
    }
    var raw = _unwrap(item);
    if (!raw) {
        return;
    }
    // What is not rendered cannot take focus, and whatever has it keeps it.
    if (!raw.__eldurInline && _itHidden(raw)) {
        return;
    }
    var prev = _activeElement;
    _activeElement = raw;
    var hadFocus = Boolean(raw.activeFocus || raw.focus);
    if (typeof raw.forceActiveFocus === "function") {
        raw.forceActiveFocus();
    } else if (typeof raw.focus === "boolean") {
        raw.focus = true;
    }
    var nowFocus = Boolean(raw.activeFocus || raw.focus);
    if (!hadFocus && !nowFocus) {
        if (prev && prev !== raw) {
            _dispatchDynamic(prev, "blur", null);
            _dispatchDynamic(prev, "focusout", null);
        }
        _dispatchDynamic(raw, "focus", null);
        _dispatchDynamic(raw, "focusin", null);
    }
}

function blur(item) {
    if (item && !_isDomTarget(item) && typeof item.blur === "function") {
        return item.blur();
    }
    var raw = _unwrap(item);
    if (!raw) {
        return;
    }
    var wasActive = _activeElement === raw || Boolean(raw.activeFocus || raw.focus);
    if (_activeElement === raw) {
        _activeElement = null;
    }
    var hadFocus = Boolean(raw.activeFocus || raw.focus);
    if (typeof raw.focus === "boolean") {
        raw.focus = false;
    }
    var nowFocus = Boolean(raw.activeFocus || raw.focus);
    if (wasActive && (!hadFocus || nowFocus)) {
        _dispatchDynamic(raw, "blur", null);
        _dispatchDynamic(raw, "focusout", null);
    }
}

// The attribute, not whether the element is shown: `display: none` is not `hidden`.
function hiddenAttribute(item) {
    var raw = _unwrap(item);
    if (getAttribute(raw, "hidden") !== null) {
        return true;
    }
    var m = _meta(raw);
    if (m.explicitHidden !== undefined || m.createdTag || m.styles.display !== undefined
        || typeof raw.eldurStyle === "string") {
        return m.explicitHidden === true;
    }
    return raw.visible === false;
}

// A closed `<details>` keeps its content's boxes (`content-visibility`), unpainted.
function _inClosedDetails(raw) {
    for (var p = raw.parent; p; p = p.parent) {
        if (_nodeTag(p) === "details") {
            return _meta(p).detailsOpen !== true;
        }
    }
    return false;
}

function _hiddenByAttribute(raw) {
    for (var p = raw; p; p = p.parent) {
        if (_meta(p).explicitHidden === true) {
            return true;
        }
    }
    return false;
}

function getBoundingClientRect(item) {
    _layoutForRead();
    var raw = _gCurrent(_unwrap(item));
    if (raw && raw.__eldurInline) {
        return _vClientRect(raw);
    }
    if (!raw || raw.visible === false && !_inClosedDetails(raw)) {
        return { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 };
    }
    var w = (typeof raw.width === "number" ? raw.width : 0) - (_meta(raw).tableBorderOver || 0);
    var h = typeof raw.height === "number" ? raw.height : 0;
    var x = typeof raw.x === "number" ? raw.x : 0;
    var y = typeof raw.y === "number" ? raw.y : 0;
    if (typeof raw.mapToItem === "function") {
        var pt = raw.mapToItem(null, 0, 0);
        if (pt) {
            x = pt.x;
            y = pt.y - _gGlyphShift(raw);
        }
        // An inline element is as tall as its font, whatever its line holds.
        if (_isLines(raw) && !_isAnonLines(raw) && _vInlineTag(_nodeTag(raw))
            && _gHasCascade() && _gStyle(raw).display === "inline") {
            y += raw.fontTop;
            h = raw.fontHeight;
        }
        // A created inline element is a box of its text; its rect is the font's.
        if (_gHasCascade() && _gIsBox(raw) && _isLines(raw.parent) && _gStyle(raw).display === "inline") {
            var inlineBox = _gMetrics(_gStyle(raw));
            var inlineH = Math.round(inlineBox.ascent) + Math.round(inlineBox.descent);
            y += (h - inlineH) / 2;
            h = inlineH;
        }
        if (_meta(raw).gShownInline && typeof raw.text === "string") {
            var fontBox = _vMetrics(raw).height;
            y += (h - fontBox) / 2;
            h = fontBox;
        }
        // A transform moves what a box's rect reports: the corners' bounds.
        if (_meta(raw).gMatrix) {
            var corners = [pt, raw.mapToItem(null, w, 0), raw.mapToItem(null, 0, h), raw.mapToItem(null, w, h)];
            var xs = corners.map(function(c) { return c.x; });
            var ys = corners.map(function(c) { return c.y; });
            x = Math.min.apply(null, xs);
            y = Math.min.apply(null, ys);
            w = Math.max.apply(null, xs) - x;
            h = Math.max.apply(null, ys) - y;
        }
    }
    return {
        x: x,
        y: y,
        left: x,
        top: y,
        width: w,
        height: h,
        right: x + w,
        bottom: y + h
    };
}

// CSSOM View 7: whole pixels; a box that isn't rendered has none.
function _offsetSize(item, axis) {
    _vSettle();
    var r = getBoundingClientRect(item);
    return Math.round(r[axis]);
}

function _borderOf(raw, side) {
    if (_gHasCascade()) {
        var width = parseFloat(_gComputed(raw, "border-" + side + "-width"));
        if (!isNaN(width)) {
            return width;
        }
    }
    // A Rectangle nobody gave a border reads as one black pixel, and paints none.
    var pen = raw.border;
    if (!pen || typeof pen.width !== "number" || (pen.width === 1 && String(pen.color) === "#000000")) {
        return 0;
    }
    return pen.width;
}

// An inline box has no client area.
function _clientSize(item, axis) {
    _layoutForRead();
    var raw = _gCurrent(_unwrap(item));
    if (!raw || raw.__eldurInline || raw.visible === false || typeof raw[axis] !== "number"
        || _meta(raw).gShownInline) {
        return 0;
    }
    var sides = axis === "width" ? ["left", "right"] : ["top", "bottom"];
    return Math.round(raw[axis] - _borderOf(raw, sides[0]) - _borderOf(raw, sides[1]));
}

function _isRendered(item) {
    var raw = _gCurrent(_unwrap(item));
    if (!raw) {
        return false;
    }
    if (!raw.__eldurInline) {
        return raw.visible !== false || _inClosedDetails(raw);
    }
    if (!raw.model || raw.model.select || raw.model.text.visible === false) {
        return false;
    }
    for (var at = raw; at; at = at.vparent) {
        if (_vStyle(at).display === "none") {
            return false;
        }
    }
    return true;
}

function _offsetParentRaw(item) {
    _layoutForRead();
    var tag = _nodeTag(_unwrap(item));
    if (!_isRendered(item) || tag === "body" || tag === "html") {
        return null;
    }
    for (var p = wrap(item).parentElement; p; p = p.parentElement) {
        var ptag = String(p.tagName).toLowerCase();
        if (ptag === "body" || ptag === "td" || ptag === "th" || ptag === "table") {
            return p;
        }
        var position = getComputedStyle(p).position;
        if (position && position !== "static") {
            return p;
        }
    }
    return null;
}

function _offsetParent(item) {
    return _offsetParentRaw(item);
}

// From the offset parent's padding edge, or the page's when that is the body.
function _offsetPlace(item, side) {
    var parent = _offsetParentRaw(item);
    if (!parent) {
        return 0;
    }
    var r = getBoundingClientRect(item);
    if (String(parent.tagName).toLowerCase() === "body") {
        return Math.round(r[side]);
    }
    var pr = getBoundingClientRect(_unwrap(parent));
    return Math.round(r[side] - pr[side] - _borderOf(_unwrap(parent), side));
}

// ---------------------------------------------------------------------------
// 8. Events and dynamic signal wiring
// ---------------------------------------------------------------------------

function _mapKey(qtKey, text, shiftKey) {
    if (qtKey >= 0x41 && qtKey <= 0x5a) {
        var upper = String.fromCharCode(qtKey);
        var keyChar = text && text.length > 0 ? text : (shiftKey ? upper : upper.toLowerCase());
        return { key: keyChar, code: "Key" + upper, keyCode: qtKey };
    }
    if (qtKey >= 0x30 && qtKey <= 0x39) {
        var digit = String.fromCharCode(qtKey);
        var dKey = text && text.length > 0 ? text : digit;
        return { key: dKey, code: "Digit" + digit, keyCode: qtKey };
    }
    if (qtKey === 0x20) {
        return { key: " ", code: "Space", keyCode: 32 };
    }
    if (qtKey >= 0x01000030 && qtKey <= 0x0100003b) {
        var fnNum = (qtKey - 0x01000030) + 1;
        var fnName = "F" + fnNum;
        return { key: fnName, code: fnName, keyCode: 111 + fnNum };
    }
    switch (qtKey) {
    case 0x01000000:
        return { key: "Escape", code: "Escape", keyCode: 27 };
    case 0x01000001:
    case 0x01000002:
        return { key: "Tab", code: "Tab", keyCode: 9 };
    case 0x01000003:
        return { key: "Backspace", code: "Backspace", keyCode: 8 };
    case 0x01000004:
        return { key: "Enter", code: "Enter", keyCode: 13 };
    case 0x01000005:
        return { key: "Enter", code: "NumpadEnter", keyCode: 13 };
    case 0x01000006:
        return { key: "Insert", code: "Insert", keyCode: 45 };
    case 0x01000007:
        return { key: "Delete", code: "Delete", keyCode: 46 };
    case 0x01000010:
        return { key: "Home", code: "Home", keyCode: 36 };
    case 0x01000011:
        return { key: "End", code: "End", keyCode: 35 };
    case 0x01000012:
        return { key: "ArrowLeft", code: "ArrowLeft", keyCode: 37 };
    case 0x01000013:
        return { key: "ArrowUp", code: "ArrowUp", keyCode: 38 };
    case 0x01000014:
        return { key: "ArrowRight", code: "ArrowRight", keyCode: 39 };
    case 0x01000015:
        return { key: "ArrowDown", code: "ArrowDown", keyCode: 40 };
    case 0x01000016:
        return { key: "PageUp", code: "PageUp", keyCode: 33 };
    case 0x01000017:
        return { key: "PageDown", code: "PageDown", keyCode: 34 };
    case 0x01000020:
        return { key: "Shift", code: "ShiftLeft", keyCode: 16 };
    case 0x01000021:
        return { key: "Control", code: "ControlLeft", keyCode: 17 };
    case 0x01000022:
        return { key: "Meta", code: "MetaLeft", keyCode: 91 };
    case 0x01000023:
        return { key: "Alt", code: "AltLeft", keyCode: 18 };
    case 0x01000024:
        return { key: "CapsLock", code: "CapsLock", keyCode: 20 };
    default:
        return {
            key: text && text.length > 0 ? text : "Unidentified",
            code: "Unidentified",
            keyCode: qtKey || 0
        };
    }
}

var _BUBBLES = {
    click: true, dblclick: true, input: true, change: true, submit: true, reset: true,
    keydown: true, keyup: true, keypress: true, focusin: true, focusout: true,
    mousedown: true, mouseup: true, mousemove: true, mouseover: true, mouseout: true,
    pointerdown: true, pointerup: true, touchstart: true, touchend: true,
    contextmenu: true, wheel: true
};

function event(type, targetItem, qtEvent) {
    var rawTarget = _unwrap(targetItem);
    var ev = qtEvent || {};
    var mods = typeof ev.modifiers === "number" ? ev.modifiers : 0;
    var shiftKey = Boolean(mods & Qt.ShiftModifier);
    var ctrlKey = Boolean(mods & Qt.ControlModifier);
    var altKey = Boolean(mods & Qt.AltModifier);
    var metaKey = Boolean(mods & Qt.MetaModifier);

    var keyInfo = { key: "", code: "", keyCode: 0 };
    if (typeof ev.key === "number") {
        keyInfo = _mapKey(ev.key, typeof ev.text === "string" ? ev.text : "", shiftKey);
    }

    var offsetX = 0;
    var offsetY = 0;
    if (typeof ev.x === "number" && typeof ev.y === "number") {
        offsetX = ev.x;
        offsetY = ev.y;
    } else if (ev.position && typeof ev.position.x === "number") {
        offsetX = ev.position.x;
        offsetY = ev.position.y;
    }

    var clientX = offsetX;
    var clientY = offsetY;
    if (ev.scenePosition && typeof ev.scenePosition.x === "number") {
        clientX = ev.scenePosition.x;
        clientY = ev.scenePosition.y;
    } else if (rawTarget && typeof rawTarget.mapToItem === "function" && (typeof ev.x === "number" || ev.position)) {
        var mapped = rawTarget.mapToItem(null, offsetX, offsetY);
        if (mapped) {
            clientX = mapped.x;
            clientY = mapped.y;
        }
    }

    var button = 0;
    if (typeof ev.button === "number") {
        if (ev.button === Qt.RightButton) {
            button = 2;
        } else if (ev.button === Qt.MiddleButton) {
            button = 1;
        } else {
            button = 0;
        }
    }
    var buttons = typeof ev.buttons === "number" ? ev.buttons : 0;

    var deltaX = 0;
    var deltaY = 0;
    if (ev.pixelDelta && (ev.pixelDelta.x !== 0 || ev.pixelDelta.y !== 0)) {
        deltaX = ev.pixelDelta.x === 0 ? 0 : -ev.pixelDelta.x;
        deltaY = ev.pixelDelta.y === 0 ? 0 : -ev.pixelDelta.y;
    } else if (ev.angleDelta) {
        var dx = -ev.angleDelta.x * (100 / 120);
        var dy = -ev.angleDelta.y * (100 / 120);
        deltaX = dx === 0 ? 0 : dx;
        deltaY = dy === 0 ? 0 : dy;
    }

    // Listeners read `event.target.value`, which only the wrapper has.
    var domTarget = rawTarget && typeof rawTarget.mapToItem === "function" ? wrap(rawTarget) : targetItem;
    var domEvent = {
        type: String(type),
        target: domTarget || null,
        currentTarget: domTarget || null,
        key: keyInfo.key,
        code: keyInfo.code,
        keyCode: keyInfo.keyCode,
        repeat: Boolean(ev.isAutoRepeat),
        shiftKey: shiftKey,
        ctrlKey: ctrlKey,
        altKey: altKey,
        metaKey: metaKey,
        clientX: clientX,
        clientY: clientY,
        pageX: clientX,
        pageY: clientY,
        offsetX: offsetX,
        offsetY: offsetY,
        button: button,
        buttons: buttons,
        deltaX: deltaX,
        deltaY: deltaY,
        deltaMode: 0,
        bubbles: _BUBBLES[String(type)] === true,
        cancelable: true,
        defaultPrevented: false,
        eventPhase: 0,
        timeStamp: Date.now() - _startTime,
        detail: ev.detail !== undefined ? ev.detail : null,
        _propagationStopped: false,
        _immediateStopped: false,
        preventDefault: function() {
            domEvent.defaultPrevented = true;
            if (qtEvent && typeof qtEvent.accepted === "boolean") {
                qtEvent.accepted = true;
            }
        },
        stopPropagation: function() {
            domEvent._propagationStopped = true;
            if (qtEvent && typeof qtEvent.accepted === "boolean" && typeof qtEvent.key === "number") {
                qtEvent.accepted = true;
            }
        },
        stopImmediatePropagation: function() {
            domEvent._immediateStopped = true;
            domEvent._propagationStopped = true;
        }
    };
    return domEvent;
}

function Event(type, opts) {
    var o = opts || {};
    var e = event(type, null, { detail: o.detail });
    e.bubbles = o.bubbles !== undefined ? Boolean(o.bubbles) : false;
    e.cancelable = o.cancelable !== undefined ? Boolean(o.cancelable) : false;
    if (typeof o.key === "string") { e.key = o.key; }
    if (typeof o.code === "string") { e.code = o.code; }
    if (typeof o.keyCode === "number") { e.keyCode = o.keyCode; }
    if (o.ctrlKey !== undefined) { e.ctrlKey = Boolean(o.ctrlKey); }
    if (o.shiftKey !== undefined) { e.shiftKey = Boolean(o.shiftKey); }
    if (o.altKey !== undefined) { e.altKey = Boolean(o.altKey); }
    if (o.metaKey !== undefined) { e.metaKey = Boolean(o.metaKey); }
    if (o.repeat !== undefined) { e.repeat = Boolean(o.repeat); }
    if (typeof o.clientX === "number") { e.clientX = o.clientX; e.pageX = o.clientX; }
    if (typeof o.clientY === "number") { e.clientY = o.clientY; e.pageY = o.clientY; }
    if (typeof o.offsetX === "number") { e.offsetX = o.offsetX; }
    if (typeof o.offsetY === "number") { e.offsetY = o.offsetY; }
    if (typeof o.button === "number") { e.button = o.button; }
    if (typeof o.buttons === "number") { e.buttons = o.buttons; }
    if (typeof o.deltaX === "number") { e.deltaX = o.deltaX; }
    if (typeof o.deltaY === "number") { e.deltaY = o.deltaY; }
    if (typeof o.deltaMode === "number") { e.deltaMode = o.deltaMode; }
    if (o.data !== undefined) { e.data = o.data; }
    return e;
}

function CustomEvent(type, opts) {
    return Event(type, opts);
}

function KeyboardEvent(type, opts) {
    return Event(type, opts);
}

function MouseEvent(type, opts) {
    return Event(type, opts);
}

function PointerEvent(type, opts) {
    return Event(type, opts);
}

function FocusEvent(type, opts) {
    return Event(type, opts);
}

function InputEvent(type, opts) {
    return Event(type, opts);
}

function WheelEvent(type, opts) {
    return Event(type, opts);
}

function EventTarget() {
    var selfTarget = {
        __eldurEventTarget: true,
        addEventListener: function(type, fn, opts) {
            _addListener(selfTarget, type, fn, opts);
        },
        removeEventListener: function(type, fn) {
            unlisten(selfTarget, type, fn);
        },
        dispatchEvent: function(ev) {
            return dispatchEvent(selfTarget, ev);
        }
    };
    return selfTarget;
}

function MutationObserver(_callback) {
    return {
        observe: function() {},
        disconnect: function() {},
        takeRecords: function() { return []; }
    };
}

function ResizeObserver(_callback) {
    return {
        observe: function() {},
        unobserve: function() {},
        disconnect: function() {}
    };
}

function IntersectionObserver(_callback) {
    return {
        observe: function() {},
        unobserve: function() {},
        disconnect: function() {},
        takeRecords: function() { return []; }
    };
}

// A browser's one event: target and ancestors, fixed when it starts, capture
// listeners from the root down, then the rest from the target up.
function _dispatchDynamic(raw, type, qtEv) {
    // A tap, heard by some handler under the point: see `_beginTap`.
    if (type === "click" && !raw.__eldurInline && qtEv && qtEv.scenePosition && !qtEv.preventDefault) {
        return !_beginTap(qtEv, raw).ev.defaultPrevented;
    }
    var target = _eventTarget(raw);
    var domEv = qtEv && qtEv.preventDefault ? qtEv : event(type, target, qtEv);
    domEv.target = target;
    var path = _eventPath(raw);
    for (var down = path.length - 1; down > 0 && !domEv._propagationStopped; --down) {
        _runListeners(path[down], "capture:" + type, type, domEv, 1);
    }
    if (!domEv._propagationStopped) {
        _runListeners(raw, "capture:" + type, type, domEv, 2);
    }
    if (!domEv._propagationStopped) {
        _runListeners(raw, type, type, domEv, 2);
    }
    for (var up = 1; up < path.length && domEv.bubbles && !domEv._propagationStopped; ++up) {
        _runListeners(path[up], type, type, domEv, 3);
    }
    domEv.currentTarget = null;
    domEv.eventPhase = 0;
    return !domEv.defaultPrevented;
}

function _eventTarget(raw) {
    return raw === document || raw === window ? raw : wrap(raw);
}

function _eventPath(raw) {
    var path = [raw];
    if (raw === document || raw === window) {
        return raw === document ? [document, window] : path;
    }
    if (raw === _root) {
        return [_root, document, window];
    }
    // The root may be inside an item that is not the page's.
    for (var at = _logicalParent(raw); at; at = _logicalParent(at)) {
        if (at === _root) {
            path.push(_root, document, window);
            break;
        }
        if (_isDomTarget(at) && !_isInternalHelperNode(at)) {
            path.push(at);
        }
    }
    return path;
}

function _runListeners(raw, key, type, domEv, phase) {
    var m = _listenerMeta(raw);
    var list = m.listeners[key];
    if (!list || list.length === 0) {
        return;
    }
    var target = _eventTarget(raw);
    domEv.currentTarget = target;
    domEv.eventPhase = phase;
    var onceList = m.onceListeners[key];
    var copy = list.slice();
    for (var i = 0; i < copy.length; ++i) {
        var fn = copy[i];
        // One removed by a listener before it runs does not run.
        var at = list.indexOf(fn);
        if (at === -1) {
            continue;
        }
        if (onceList) {
            var onceIdx = onceList.indexOf(fn);
            if (onceIdx !== -1) {
                onceList.splice(onceIdx, 1);
                list.splice(at, 1);
            }
        }
        fn.call(target, domEv);
        if (domEv._immediateStopped) {
            break;
        }
    }
}

// The converter's handler for what the runtime cannot connect itself; `false`
// when a listener prevented the default.
function fire(item, type, qtEv) {
    var raw = _unwrap(item);
    if (Boolean(_meta(raw).connected[type])) {
        return true;
    }
    // A key no handler accepted goes on to the parent's handler, but is one event.
    if (type === "keydown" || type === "keyup") {
        if (_keyHeard[type]) {
            return true;
        }
        _keyHeard[type] = true;
        Qt.callLater(_flushKeys);
    }
    return _dispatchDynamic(raw, type, qtEv);
}

var _keyHeard = {};

function _flushKeys() {
    _keyHeard = {};
}

function _flushChange(raw) {
    var m = _meta(raw);
    if (m.changeBase !== String(raw.text)) {
        m.changeBase = String(raw.text);
        _dispatchDynamic(raw, "change", null);
    }
}

function _pageSurface() {
    var home = _root.contentItem || _root;
    for (var i = 0; i < home.children.length; ++i) {
        if (typeof home.children[i].contentY === "number") {
            return home.children[i];
        }
    }
    return home;
}

function _connectSignalOnce(raw, type) {
    var m = _meta(raw);
    if (m.connected[type]) {
        return true;
    }
    if (!raw || typeof raw !== "object") {
        return false;
    }
    if (raw.__eldurInline) {
        _vConnect(raw, type);
        return true;
    }
    if (_isPlainJsObject(raw)) {
        m.connected[type] = true;
        return true;
    }

    if (type === "click") {
        if (typeof raw.clicked === "function") {
            // `toggled` comes before `clicked`, and a browser's `change` after
            // its `click`, which can undo the toggle. A toggle with no press
            // has no click to wait for.
            if (typeof raw.toggled === "function") {
                raw.toggled.connect(function() {
                    if (m.pressing) {
                        m.toggledNow = true;
                    } else {
                        _toggleEvents(raw);
                    }
                });
                raw.pressedChanged.connect(function() {
                    if (raw.pressed) {
                        m.pressing = true;
                    } else {
                        Qt.callLater(function() { m.pressing = false; });
                    }
                });
            }
            raw.clicked.connect(function() { _clickedControl(raw); });
            m.connected[type] = true;
            return true;
        }
        // The page scrolls in a Flickable, which keeps a press for itself from
        // whatever is behind it; and a window is no item.
        var heard = raw === _root ? _pageSurface() : raw;
        if (typeof heard.mapToItem === "function") {
            var tap = Qt.createQmlObject("import QtQuick; TapHandler {}", heard, "eldur_tap");
            tap.tapped.connect(function(eventPoint, btn) {
                _dispatchDynamic(raw, "click", {
                    position: eventPoint.position,
                    scenePosition: eventPoint.scenePosition,
                    button: btn
                });
            });
            m.connected[type] = true;
            return true;
        }
    } else if (type === "dblclick") {
        if (typeof raw.doubleClicked === "function") {
            raw.doubleClicked.connect(function() { _dispatchDynamic(raw, "dblclick", null); });
            m.connected[type] = true;
            return true;
        }
        if (typeof raw.mapToItem === "function") {
            var dtap = Qt.createQmlObject("import QtQuick; TapHandler {}", raw, "eldur_dbltap");
            dtap.doubleTapped.connect(function(eventPoint, btn) {
                _dispatchDynamic(raw, "dblclick", {
                    position: eventPoint.position,
                    scenePosition: eventPoint.scenePosition,
                    button: btn
                });
            });
            m.connected[type] = true;
            return true;
        }
    } else if ((type === "input" || type === "change") && typeof raw.toggled === "function"
               && typeof raw.clicked === "function") {
        _connectSignalOnce(raw, "click");
        m.connected[type] = true;
        return true;
    } else if (type === "input") {
        if (typeof raw.textEdited === "function") {
            raw.textEdited.connect(function() { _dispatchDynamic(raw, "input", null); });
            m.connected[type] = true;
            return true;
        }
        if (typeof raw.moved === "function") {
            raw.moved.connect(function() { _dispatchDynamic(raw, "input", null); });
            m.connected[type] = true;
            return true;
        }
        var inputSignal = raw.toggled || raw.activated || raw.valueModified;
        if (typeof inputSignal === "function") {
            inputSignal.connect(function() { _dispatchDynamic(raw, "input", null); });
            m.connected[type] = true;
            return true;
        }
        if (typeof raw.textChanged === "function") {
            // TextArea has no "edited by the user" signal.
            raw.textChanged.connect(function() {
                if (raw.activeFocus) {
                    _dispatchDynamic(raw, "input", null);
                }
            });
            m.connected[type] = true;
            return true;
        }
    } else if (type === "keydown" || type === "keyup") {
        // `Keys` cannot be attached from here; only the runtime's own
        // controls declare these. Others get their handler from the converter.
        var keySignal = type === "keydown" ? raw.eldurKeyDown : raw.eldurKeyUp;
        if (typeof keySignal === "function") {
            keySignal.connect(function(qtEv) { _dispatchDynamic(raw, type, qtEv); });
            m.connected[type] = true;
            return true;
        }
    } else if (type === "change") {
        if (typeof raw.moved === "function" && typeof raw.pressedChanged === "function") {
            raw.pressedChanged.connect(function() {
                if (!raw.pressed) {
                    _dispatchDynamic(raw, "change", null);
                }
            });
            m.connected[type] = true;
            return true;
        }
        if (typeof raw.toggled === "function") {
            raw.toggled.connect(function() { _dispatchDynamic(raw, "change", null); });
            m.connected[type] = true;
            return true;
        }
        if (typeof raw.activated === "function") {
            raw.activated.connect(function() { _dispatchDynamic(raw, "change", null); });
            m.connected[type] = true;
            return true;
        }
        if (typeof raw.editingFinished === "function") {
            // Enter, then leaving the field, is one `change`, and none if nothing changed.
            m.changeBase = String(raw.text);
            if (typeof raw.activeFocusChanged === "function") {
                raw.activeFocusChanged.connect(function() {
                    if (raw.activeFocus) {
                        m.changeBase = String(raw.text);
                    }
                });
            }
            raw.editingFinished.connect(function() { _flushChange(raw); });
            m.connected[type] = true;
            return true;
        }
        if (typeof raw.valueModified === "function") {
            raw.valueModified.connect(function() { _dispatchDynamic(raw, "change", null); });
            m.connected[type] = true;
            return true;
        }
    } else if (type === "submit" && typeof raw.accepted === "function") {
        raw.accepted.connect(function() { _dispatchDynamic(raw, "submit", null); });
        m.connected[type] = true;
        return true;
    } else if ((type === "focus" || type === "focusin" || type === "blur" || type === "focusout") &&
               (typeof raw.activeFocusChanged === "function" || typeof raw.focusChanged === "function")) {
        var lastFocused = Boolean(raw.activeFocus || raw.focus);
        var onFocusChange = function() {
            var nowFocused = Boolean(raw.activeFocus || raw.focus);
            if (nowFocused === lastFocused) {
                return;
            }
            lastFocused = nowFocused;
            if (nowFocused) {
                _activeElement = raw;
                if (type === "focus" || type === "focusin") {
                    _dispatchDynamic(raw, type, null);
                }
            } else {
                if (_activeElement === raw && raw !== _focusHeld) {
                    _activeElement = null;
                }
                if (type === "blur" || type === "focusout") {
                    whenBlurred(raw, type, function() { _dispatchDynamic(raw, type, null); });
                }
            }
        };
        if (typeof raw.activeFocusChanged === "function") {
            raw.activeFocusChanged.connect(onFocusChange);
        }
        if (typeof raw.focusChanged === "function") {
            raw.focusChanged.connect(onFocusChange);
        }
        m.connected[type] = true;
        return true;
    } else if ((type === "mouseenter" || type === "mouseover" || type === "mouseleave" || type === "mouseout") &&
               typeof raw.mapToItem === "function") {
        var hover = Qt.createQmlObject("import QtQuick; HoverHandler {}", raw, "eldur_hover");
        hover.hoveredChanged.connect(function() {
            if (hover.hovered && (type === "mouseenter" || type === "mouseover")) {
                _dispatchDynamic(raw, type, null);
            } else if (!hover.hovered && (type === "mouseleave" || type === "mouseout")) {
                _dispatchDynamic(raw, type, null);
            }
        });
        m.connected[type] = true;
        return true;
    } else if (type === "mousemove" && typeof raw.mapToItem === "function") {
        var moveHover = Qt.createQmlObject("import QtQuick; HoverHandler {}", raw, "eldur_move");
        moveHover.pointChanged.connect(function() {
            _dispatchDynamic(raw, "mousemove", {
                position: moveHover.point.position,
                scenePosition: moveHover.point.scenePosition
            });
        });
        m.connected[type] = true;
        return true;
    } else if (type === "wheel" && typeof raw.mapToItem === "function") {
        var wheelH = Qt.createQmlObject("import QtQuick; WheelHandler {}", raw, "eldur_wheel");
        wheelH.wheel.connect(function(wheelEv) { _dispatchDynamic(raw, "wheel", wheelEv); });
        m.connected[type] = true;
        return true;
    } else if (type === "scroll") {
        if (typeof raw.contentYChanged === "function") {
            raw.contentYChanged.connect(function() { _dispatchDynamic(raw, "scroll", null); });
        }
        if (typeof raw.contentXChanged === "function") {
            raw.contentXChanged.connect(function() { _dispatchDynamic(raw, "scroll", null); });
        }
        m.connected[type] = true;
        return true;
    } else if (type === "resize" && typeof raw.widthChanged === "function") {
        raw.widthChanged.connect(function() { _dispatchDynamic(raw, "resize", null); });
        if (typeof raw.heightChanged === "function") {
            raw.heightChanged.connect(function() { _dispatchDynamic(raw, "resize", null); });
        }
        m.connected[type] = true;
        return true;
    } else if (type === "keypress" && typeof raw.eldurKeyDown === "function") {
        raw.eldurKeyDown.connect(function(qtEv) { _dispatchDynamic(raw, "keypress", qtEv); });
        m.connected[type] = true;
        return true;
    } else if ((type === "mousedown" || type === "pointerdown" || type === "touchstart") && typeof raw.pressed === "function") {
        raw.pressed.connect(function() { _dispatchDynamic(raw, type, null); });
        m.connected[type] = true;
        return true;
    } else if ((type === "mouseup" || type === "pointerup" || type === "touchend") && typeof raw.released === "function") {
        raw.released.connect(function() { _dispatchDynamic(raw, type, null); });
        m.connected[type] = true;
        return true;
    } else if (type === "contextmenu" && typeof raw.pressAndHold === "function") {
        raw.pressAndHold.connect(function() { _dispatchDynamic(raw, "contextmenu", null); });
        m.connected[type] = true;
        return true;
    } else if (type === "hashchange" || type === "popstate" || type === "DOMContentLoaded" || type === "load" || type === "beforeunload" || type === "unload") {
        m.connected[type] = true;
        return true;
    }

    return false;
}

function listen(item, eventType, handler, options) {
    if (item && !_isDomTarget(item) && typeof item.addEventListener === "function") {
        return item.addEventListener(eventType, handler, options);
    }
    var raw = _listenerHome(item, eventType);
    // An event no signal delivers (a custom one, say) can still be dispatched.
    if (_addListener(raw, eventType, handler, options)) {
        _connectSignalOnce(raw === document || raw === window ? _root : raw, String(eventType));
    }
}

// For events QML handlers already deliver (keys, a form's submit): the
// generated handler calls `dispatchEvent`, there is nothing to connect.
function listenBound(item, eventType, handler, options) {
    _addListener(_listenerHome(item, eventType), eventType, handler, options);
}

// `document` and `window` are on the path of a bubbling event, each with its
// own listeners. What is fired only at the root (`resize`, `hashchange`, ...)
// is listened for there.
function _listenerHome(item, eventType) {
    if ((item === document || item === window) && _BUBBLES[String(eventType)] === true) {
        return item;
    }
    return _unwrap(item);
}

var _documentListeners = { listeners: {}, onceListeners: {}, onHandlers: {} };
var _windowListeners = { listeners: {}, onceListeners: {}, onHandlers: {} };

// `_meta` of either is the root's.
function _listenerMeta(raw) {
    if (raw === document) {
        return _documentListeners;
    }
    return raw === window ? _windowListeners : _meta(raw);
}

function _isCapture(options) {
    return options === true || Boolean(options && typeof options === "object" && options.capture);
}

function _addListener(item, eventType, handler, options) {
    if (!item || typeof handler !== "function") {
        return false;
    }
    var raw = item === document || item === window ? item : _unwrap(item);
    var type = String(eventType);
    var key = _isCapture(options) ? "capture:" + type : type;
    var m = _listenerMeta(raw);
    if (!m.listeners[key]) {
        m.listeners[key] = [];
    }
    if (m.listeners[key].indexOf(handler) === -1) {
        m.listeners[key].push(handler);
        if (options && typeof options === "object" && Boolean(options.once)) {
            if (!m.onceListeners[key]) {
                m.onceListeners[key] = [];
            }
            if (m.onceListeners[key].indexOf(handler) === -1) {
                m.onceListeners[key].push(handler);
            }
        }
    }
    _wantForControls(type);
    return true;
}

// A control takes the press or the key for itself and tells only its own
// signal, so once anything listens for an event a control can start, every
// control's signal is connected: the listener may be on an ancestor.
var _CONTROL_EVENTS = { click: true, dblclick: true, input: true, change: true, focusin: true, focusout: true };
var _controlEvents = {};

function _wantForControls(type) {
    if (_CONTROL_EVENTS[type] !== true || _controlEvents[type]) {
        return;
    }
    _controlEvents[type] = true;
    if (_root) {
        _connectControls(_root);
    }
}

function _connectControls(from) {
    var kinds = Object.keys(_controlEvents);
    if (kinds.length === 0) {
        return;
    }
    _walkItems(from, function(item) {
        if (!item.__eldurInline && _startsEvents(item)) {
            for (var i = 0; i < kinds.length; ++i) {
                _connectSignalOnce(item, kinds[i]);
            }
        } else if (_controlEvents.click && _isCreatedBox(item)) {
            // What a script adds can lie outside the box of what it went into,
            // where that one's handler does not hear.
            _connectSignalOnce(item, "click");
        }
        return false;
    });
}

function _isCreatedBox(item) {
    var tag = _meta(item).createdTag;
    return !item.__eldurInline && Boolean(tag) && tag !== "#text";
}

// Tagged, because a control is built of untagged ones.
function _startsEvents(item) {
    return (_vIsTagged(item) || Boolean(_meta(item).createdTag))
        && (typeof item.clicked === "function" || typeof item.editingFinished === "function"
            || typeof item.activated === "function" || typeof item.moved === "function"
            || typeof item.valueModified === "function");
}

// A listener on a known element can get both a connection and the generated
// QML handler; the handler asks this first, so one action fires once.
function connected(item, eventType) {
    return Boolean(_meta(item).connected[String(eventType)]);
}

function unlisten(item, eventType, handler, options) {
    if (item && !_isDomTarget(item) && typeof item.removeEventListener === "function") {
        return item.removeEventListener(eventType, handler);
    }
    if (!item || typeof handler !== "function") {
        return;
    }
    var m = _listenerMeta(_listenerHome(item, eventType));
    var type = _isCapture(options) ? "capture:" + eventType : String(eventType);
    var list = m.listeners[type];
    if (!list) {
        return;
    }
    var idx = list.indexOf(handler);
    if (idx !== -1) {
        list.splice(idx, 1);
    }
    var onceList = m.onceListeners[type];
    if (onceList) {
        var onceIdx = onceList.indexOf(handler);
        if (onceIdx !== -1) {
            onceList.splice(onceIdx, 1);
        }
    }
}

function setOn(item, eventType, handler) {
    if (!item) {
        return handler;
    }
    var raw = _listenerHome(item, eventType);
    var type = String(eventType);
    _setOnSlot(raw, type, handler);
    if (typeof handler === "function") {
        if (!_connectSignalOnce(raw === document || raw === window ? _root : raw, type)) {
            console.warn("eldur: dynamic on" + type + " handler cannot bind to a QML signal");
        }
    }
    return handler;
}

function fireSubmit(item) {
    for (var at = item; at; at = at.parent) {
        if (at.eldurTag === "form") {
            fire(at, "submit", null);
            return;
        }
    }
}

// An inline element with a handler attribute, by the converter's key.
function inlineNode(key) {
    var found = null;
    _walkItems(_root, function(item) {
        if (item.__eldurInline && item.eldurKey === key) {
            found = item;
        }
        return found !== null;
    });
    return found;
}

// An attribute's handler, for an event QML handlers deliver.
function setOnBound(item, eventType, handler) {
    _setOnSlot(_listenerHome(item, eventType), String(eventType), handler);
}

// Among the listeners where it was first set; a new function keeps the place.
// Setting null gives the place up (measured in Chromium).
function _setOnSlot(raw, type, handler) {
    var m = _listenerMeta(raw);
    var isFn = typeof handler === "function";
    m.onHandlers[type] = isFn ? handler : null;
    if (!m.onSlots) {
        m.onSlots = {};
    }
    if (isFn && !m.onSlots[type]) {
        m.onSlots[type] = function(domEv) {
            if (m.onHandlers[type].call(this, domEv) === false) {
                domEv.preventDefault();
            }
        };
        _addListener(raw, type, m.onSlots[type], undefined);
    } else if (!isFn && m.onSlots[type]) {
        unlisten(raw, type, m.onSlots[type], undefined);
        m.onSlots[type] = null;
    }
}

function dispatchEvent(item, domEv) {
    if (item && !_isDomTarget(item) && typeof item.dispatchEvent === "function") {
        return item.dispatchEvent(domEv);
    }
    if (!item || !domEv || !domEv.type) {
        return true;
    }
    return _dispatchDynamic(_listenerHome(item, domEv.type), String(domEv.type), domEv);
}

// ---------------------------------------------------------------------------
// 9. Minimal Element wrapper and Eldur.document tree lookup
// ---------------------------------------------------------------------------

var _NATIVE_IMPORTS = "import QtQuick; import QtQuick.Controls.Basic; ";

// Keys handlers accept the key unless told otherwise, and an accepted key
// never reaches the field; `preventDefault` accepts it.
var _KEY_SIGNALS = "signal eldurKeyDown(var event); signal eldurKeyUp(var event); "
    + "Keys.onPressed: (event) => { event.accepted = false; eldurKeyDown(event); } "
    + "Keys.onReleased: (event) => { event.accepted = false; eldurKeyUp(event); } ";

// On a GPU Qt draws text with distance fields unless told otherwise; the
// converter asks for native glyphs on what it emits (layout.md §1.2), and these
// sit beside its text on a page.
var _NATIVE_TEXT = "renderType: Text.NativeRendering; ";

// The Basic style's IconLabel has no renderType to ask for.
function _buttonSource(signals) {
    return _NATIVE_IMPORTS + "Button { id: control; " + signals
        + "palette.buttonText: \"black\"; "
        + "contentItem: Text { " + _NATIVE_TEXT + "text: control.text; font: control.font; "
        + "color: control.palette.buttonText; "
        + "horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter } "
        + "background: Rectangle { color: \"#efefef\"; border.color: \"#767676\"; radius: 3 } }";
}

// Created elements have no computed style, since CSS is applied at
// conversion; these are a browser's unstyled controls, whatever the system
// palette is.
// What these say with no `value`, which is still "" to a script.
var _G_INPUT_LABEL = { submit: "Submit", reset: "Reset" };

function _inputSource(type) {
    if (type === "checkbox" || type === "radio") {
        var isRadio = type === "radio";
        return _NATIVE_IMPORTS + (isRadio ? "RadioButton" : "CheckBox") + " { id: control; " + _KEY_SIGNALS
            + "property string value: \"on\"; padding: 0; spacing: 0; implicitWidth: 13; implicitHeight: 13; "
            + "contentItem: Item {} "
            + "indicator: Rectangle { width: control.width; height: control.height; "
            + (isRadio ? "radius: width / 2; color: \"white\"; " : "radius: 2; color: control.checked ? \"#0075ff\" : \"white\"; ")
            + "border.color: control.checked ? \"#0075ff\" : \"#767676\"; "
            + (isRadio
                ? "Rectangle { anchors.centerIn: parent; width: parent.width * 0.6; height: width; radius: width / 2; color: \"#0075ff\"; visible: control.checked } "
                : "Text { " + _NATIVE_TEXT + "anchors.centerIn: parent; text: \"\\u2713\"; color: \"white\"; font.pixelSize: Math.max(1, control.height - 2); visible: control.checked } ")
            + "} }";
    }
    if (type === "range") {
        return _NATIVE_IMPORTS + "Slider { from: 0; to: 100; value: 50; stepSize: 1; snapMode: Slider.SnapAlways; " + _KEY_SIGNALS + "}";
    }
    if (type === "number") {
        return _NATIVE_IMPORTS + "SpinBox { editable: true; from: -2147483648; to: 2147483647; " + _KEY_SIGNALS
            // The style's TextInput is typed Item here, so no grouped property.
            + "Component.onCompleted: contentItem.renderType = Text.NativeRendering }";
    }
    if (type === "submit" || type === "button" || type === "reset") {
        var label = _G_INPUT_LABEL[type];
        return _buttonSource(_KEY_SIGNALS + (label ? "text: \"" + label + "\"; " : ""));
    }
    return _NATIVE_IMPORTS + "TextField { " + _NATIVE_TEXT + _KEY_SIGNALS + "color: \"black\"; "
        + "background: Rectangle { color: \"white\"; border.color: \"#767676\"; radius: 2 } }";
}

// Inside a layout, as an in-flow or `relative` box is, an item's place is the
// layout's to set, and this has no effect.
function _setOffset(raw, name, str) {
    var vertical = name === "top" || name === "bottom";
    var px = parseFloat(str);
    if (isNaN(px) || !raw.parent) {
        return;
    }
    if (/%\s*$/.test(str)) {
        px *= (vertical ? raw.parent.height : raw.parent.width) / 100;
    }
    var start = vertical ? "top" : "left";
    var end = vertical ? "bottom" : "right";
    if (name === start) {
        raw.anchors[end] = undefined;
        raw[vertical ? "y" : "x"] = px;
    } else {
        raw.anchors[end] = raw.parent[end];
        raw.anchors[end + "Margin"] = px;
    }
}

function _coerceNode(nodeOrString) {
    if (nodeOrString === null || nodeOrString === undefined) {
        return null;
    }
    if (typeof nodeOrString === "object" || typeof nodeOrString === "function") {
        return _unwrap(nodeOrString);
    }
    return _unwrap(document.createTextNode(String(nodeOrString)));
}

function _forRealItems(nodes) {
    return nodes.map(function(n) { return n.__eldurInline ? _vToReal(n) : n; });
}

function _expandInsertRawNodes(nodes) {
    var out = [];
    for (var i = 0; i < nodes.length; ++i) {
        var raw = _coerceNode(nodes[i]);
        if (!raw) {
            continue;
        }
        if (_meta(raw).isFragment) {
            var fragKids = _rawElementChildren(raw);
            for (var j = 0; j < fragKids.length; ++j) {
                out.push(fragKids[j]);
            }
        } else {
            out.push(raw);
        }
    }
    return out;
}

// What stands for an out-of-flow box among its DOM siblings. Hidden: a layout
// does not count it, and no script-made box reads a static position from it.
function _slotFor(box) {
    if (!box.eldurSlot) {
        box.eldurSlot = Qt.createQmlObject(
            "import QtQuick; Item { visible: false; property Item eldurBox }",
            _qobjectParent(), "eldur_slot");
        box.eldurSlot.eldurBox = box;
    }
    return box.eldurSlot;
}

// A created box is painted in an item the size of its containing block, above
// the page since a layout would place it there. The item goes with the box's
// DOM parent, which is not its parent in QML.
function _leaveFlow(item) {
    var block = item.eldurBlock;
    if (block === undefined) {
        return;
    }
    var m = _meta(item);
    if (!m.outOfFlowGate) {
        m.outOfFlowGate = Qt.createQmlObject(
            "import QtQuick; Item { property Item block; property Item dom; "
            + "function at(axis) { let n = 0; for (let i = block; i && i !== parent.parent; i = i.parent) n += i[axis]; return n } "
            + "x: parent ? at(\"x\") : 0; y: parent ? at(\"y\") : 0; "
            + "width: block ? block.width : parent ? parent.width : 0; height: block ? block.height : parent ? parent.height : 0; "
            + "visible: dom ? dom.visible : false }",
            _qobjectParent(), "eldur_out_of_flow");
        _meta(m.outOfFlowGate).hostBox = item;
    }
    var gate = m.outOfFlowGate;
    var dom = _logicalParent(item);
    var initial = _root.eldurInitialBlock;
    gate.dom = dom;
    gate.block = block === "parent" ? dom : (typeof block === "string" ? null : block);
    // Outside the Flickable, so it does not scroll.
    gate.parent = block === "window" ? initial.parent.parent.parent : initial;
    item.parent = gate;
}

// --- Lines a script makes --------------------------------------------------

function _isLines(raw) {
    return Boolean(raw) && typeof raw.firstBaseline === "number";
}

// A text node or an inline-level element: what CSS puts on a line.
function _joinsLine(node) {
    if (node && node.__eldurInline) {
        return node.eldurTag === "#text";
    }
    if (!node || node.eldurSlot !== undefined || node.eldurBlock !== undefined) {
        return false;
    }
    var tag = String(_meta(node).createdTag || "");
    if (tag === "#text") {
        return true;
    }
    if (!tag) {
        return false;
    }
    var atomic = /^(button|input|textarea|select|img|canvas)$/.test(tag) || _vInlineTag(tag);
    if (_gHasCascade()) {
        ++_gGen;
        var s = _gStyle(node);
        // Hidden, it holds the place in the line its element would take when shown.
        return _gInlineLevel(s) || s.display === "none" && atomic;
    }
    return atomic;
}

// A block whose inline content is on lines: new text goes on them too.
function _holdsLines(raw) {
    if (_isLines(raw)) {
        return true;
    }
    var container = _childContainer(raw);
    var kids = container ? container.children : null;
    for (var i = 0; kids && i < kids.length; ++i) {
        if (_isLines(kids[i]) && _isAnonLines(kids[i])) {
            return true;
        }
    }
    return false;
}

function _isTextNode(node) {
    return node.__eldurInline ? node.eldurTag === "#text" : _meta(node).createdTag === "#text";
}

function _isBlockColumn(raw, column) {
    if (String(column).indexOf("QQuickColumnLayout") !== 0) {
        return false;
    }
    if (!_gHasCascade()) {
        return true;
    }
    ++_gGen;
    return !/flex|grid|table/.test(_gStyle(raw).display);
}

function _newLines(owner) {
    var lines = Qt.createQmlObject(
        "import QtQuick; import QtQuick.Layouts; import \".\"; "
        + "EldurLines { Layout.fillWidth: true; Layout.preferredHeight: implicitHeight }",
        _qobjectParent(), Qt.resolvedUrl("eldur_lines"));
    if (_gHasCascade()) {
        ++_gGen;
        var s = _gStyle(owner);
        lines.font = _gFontOf(s);
        lines.color = s.color;
        var line = _gLineHeight(s);
        lines.lineHeight = line === null ? 0 : line;
    }
    return lines;
}

function _dressLineText(lines, node) {
    if (!_isTextNode(node) || typeof node.text !== "string") {
        return;
    }
    node.font = lines.font;
    node.color = lines.color;
    node.textFormat = 0; // Text.PlainText; a library script has no `Text`
    node.wrapMode = 1; // Text.WordWrap
    node.renderType = 1; // Text.NativeRendering
}

// The text a block already shows starts its first line: from here on the line
// holds it and its height is the column's.
function _textStartsLine(raw, column, lines) {
    var text = _findTextChild(raw);
    if (!text || text.text === "") {
        return;
    }
    var model = _vModelOf(text);
    var top = text.y;
    var extra = raw.implicitHeight - text.implicitHeight - column.implicitHeight;
    text.anchors.left = undefined;
    text.anchors.right = undefined;
    text.anchors.top = undefined;
    text.parent = lines;
    lines.font = text.font;
    lines.color = text.color;
    if (text.lineHeightMode === 1) {
        lines.lineHeight = text.lineHeight;
    }
    // The line places it now, on its natural height.
    text.lineHeightMode = 0; // Text.ProportionalHeight
    text.lineHeight = 1;
    text.transform = [];
    column.y = top;
    raw.implicitHeight = Qt.binding(function() { return extra + column.implicitHeight; });
    if (model) {
        _vRender(model);
    }
}

// The line `nodes` go on when `raw` takes them before `ref`, made if a box needs
// it; null when they go where they always did.
function _lineToJoin(raw, nodes, ref) {
    if (_gIsBox(raw) || raw.__eldurInline || !nodes.every(_joinsLine)) {
        return null;
    }
    if (ref && ref.__eldurInline) {
        return _linesAvailable() ? _lineSplitsModel(raw, ref) : null;
    }
    if (_isLines(raw)) {
        return { lines: raw, ref: ref };
    }
    if (ref && _isLines(ref.parent)) {
        return { lines: ref.parent, ref: ref };
    }
    var boxes = nodes.some(function(n) { return !_isTextNode(n) && !_vStaysInline(n); });
    // A box inside a created inline element: the element becomes the line.
    if (_meta(raw).createdTag && _isPlainTextItem(raw)) {
        return boxes ? { lines: _inlineBecomesLines(raw), ref: ref } : null;
    }
    if (raw.contentItem || _meta(raw).tableGrid || _TABLE_PARTS[_nodeTag(raw)]) {
        return null;
    }
    if (boxes && _isPlainTextItem(raw) && !_isTextNode(raw) && !_meta(raw).vSplit && _linesAvailable()) {
        return _lineSplitsModel(raw, null);
    }
    var column = _appendTarget(raw);
    if (!column || !_isBlockColumn(raw, column)) {
        return null;
    }
    var kids = [];
    for (var i = 0; i < column.children.length; ++i) {
        if (!_isLayoutSpacer(column.children[i])) {
            kids.push(column.children[i]);
        }
    }
    var at = ref ? kids.indexOf(ref) : kids.length;
    if (at === -1) {
        return null;
    }
    var before = at > 0 ? kids[at - 1] : null;
    if (before && _isAnonLines(before) && _isLines(before)) {
        return { lines: before, ref: null };
    }
    var after = at < kids.length ? kids[at] : null;
    if (after && _isLines(after) && _isAnonLines(after)) {
        return { lines: after, ref: after.children.length > 0 ? after.children[0] : null };
    }
    if (!boxes || !_linesAvailable()) {
        return null;
    }
    if (before && _vAnonText(before)) {
        return { lines: _anonTextBecomesLines(raw, column, before, after), ref: null };
    }
    if (after && _vAnonText(after)) {
        var next = at + 1 < kids.length ? kids[at + 1] : null;
        var made = _anonTextBecomesLines(raw, column, after, next);
        return { lines: made, ref: made.children.length > 0 ? made.children[0] : null };
    }
    var lines = _newLines(raw);
    _insertInFlow(column, [lines], after);
    if (at === 0 && !_meta(raw).createdTag) {
        _textStartsLine(raw, column, lines);
    }
    // Inline elements already made are on the same line.
    var pull = [];
    for (var p = at - 1; p >= 0 && _meta(kids[p]).createdTag && _joinsLine(kids[p]); --p) {
        pull.unshift(kids[p]);
    }
    pull.forEach(function(k) { k.parent = lines; _dressLineText(lines, k); });
    return { lines: lines, ref: null };
}

// The converter writes the component beside this file; a host of its own may not.
var _linesKnown = null;

function _linesAvailable() {
    if (_linesKnown === null) {
        _linesKnown = Qt.createComponent(Qt.resolvedUrl("EldurLines.qml")).status === 1; // Component.Ready
    }
    return _linesKnown;
}

// A block's anonymous text a box comes next to: they share a line, as the text
// had no element of its own the line would have to stand in for.
function _anonTextBecomesLines(owner, column, text, next) {
    var lines = _newLines(owner);
    lines.font = text.font;
    lines.color = text.color;
    if (text.lineHeightMode === 1) {
        lines.lineHeight = text.lineHeight;
    }
    var runs = _takeRuns(text, _vModelOf(text));
    text.parent = null;
    text.destroy();
    _insertInFlow(column, [lines], next);
    if (runs.length > 0) {
        _insertInFlow(lines, runs, null);
        runs.forEach(function(n) { _dressLineText(lines, n); });
    }
    return lines;
}

// A box among the runs of one Text: a Text cannot hold it, so each run becomes
// an item of a line, which takes the Text's place in the element.
function _lineSplitsModel(raw, ref) {
    var plain = _isPlainTextItem(raw);
    var text = plain ? raw : _findTextChild(raw);
    var model = text ? _vModelOf(text) : null;
    if (!plain && !text) {
        return null;
    }
    var column = plain ? null : _appendTarget(raw);
    var lines = _newLines(raw);
    lines.font = text.font;
    lines.color = text.color;
    if (text.lineHeightMode === 1) {
        lines.lineHeight = text.lineHeight;
    }
    var runs = _takeRuns(text, model);
    if (plain) {
        column = _appendTarget(raw);
    }
    _insertInFlow(column, [lines], null);
    if (runs.length > 0) {
        _insertInFlow(lines, runs, null);
        runs.forEach(function(n) { _dressLineText(lines, n); });
    }
    return { lines: lines, ref: ref ? _gCurrent(ref) : null };
}

// What a Text shows as items of their own, leaving the Text empty.
function _takeRuns(text, model) {
    var runs = [];
    if (model) {
        runs = model.kids.slice();
        _vModels.splice(_vModels.indexOf(model), 1);
    } else if (text.text !== "") {
        runs = [_unwrap(document.createTextNode(text.text))];
    }
    runs = runs.map(_vRunToItem);
    var m = _meta(text);
    m.vModel = null;
    m.vSplit = true;
    delete m.explicitText;
    _vRendering = true;
    text.textFormat = 0; // Text.PlainText; a library script has no `Text`
    text.text = "";
    _vRendering = false;
    return runs;
}

// A box or block coming into a box with text: its Text cannot stand beside them
// on a line, so the runs become items and flow with them.
function _gBoxTextToKids(raw) {
    var text = _findTextChild(raw);
    if (!text || !_linesAvailable()) {
        return;
    }
    var model = _vModelOf(text);
    if (!model && text.text === "") {
        return;
    }
    var runs = _takeRuns(text, model);
    if (runs.length > 0) {
        _insertInFlow(raw, runs, null);
    }
}

function _vRunToItem(run) {
    return run.__eldurInline ? _vToReal(run) : run;
}

function _inlineBecomesLines(raw) {
    var source = "import QtQuick; import QtQuick.Layouts; import \".\"; EldurLines {}";
    var m = _meta(raw);
    var text = m.explicitText !== undefined ? String(m.explicitText) : String(raw.text);
    var font = raw.font;
    var color = raw.color;
    var name = raw.objectName;
    var next = _swapRawItem(raw, null, source);
    next.objectName = name;
    next.font = font;
    next.color = color;
    if (text !== "") {
        var node = _unwrap(document.createTextNode(text));
        _insertInFlow(next, [node], null);
        _dressLineText(next, node);
    }
    return next;
}

function _insertChildren(raw, nodes, ref) {
    var line = nodes.length > 0 ? _lineToJoin(raw, nodes, ref || null) : null;
    if (line) {
        var usable = _forRealItems(nodes);
        // The space the script's ref or the end held now comes before these.
        var spaces = ref && !_isLines(ref.parent) ? _wsBefore(ref) : ref ? [] : _wsEnd(raw);
        if (spaces.length > 0) {
            usable.unshift(_unwrap(document.createTextNode(_wsText(spaces))));
            usable[0] = _forRealItems([usable[0]])[0];
            spaces.length = 0;
        }
        _insertRawNodesBefore(line.lines, usable, line.ref);
        usable.forEach(function(n) {
            _dressLineText(line.lines, n);
            // A created element's Text breaks where the line says.
            if (_isPlainTextItem(n)) {
                n.wrapMode = 1; // Text.WordWrap
            }
        });
        return;
    }
    if (_gIsBox(raw) && !nodes.every(_vStaysInline)) {
        _gBoxTextToKids(raw);
        ref = ref ? _gCurrent(ref) : ref;
    }
    // What is not inline-level goes beside the anonymous line, not in it.
    var at = ref && ref.parent && _isLines(ref.parent) && _isAnonLines(ref.parent) ? ref.parent : ref;
    // A box's lines are remade from its children at its next layout.
    if (at && at !== ref && _gIsBox(raw)) {
        var inLine = [];
        for (var k = 0; k < at.children.length; ++k) {
            inLine.push(at.children[k]);
        }
        _insertInFlow(raw, inLine, at);
        at.parent = null;
        at.destroy();
        at = ref;
    }
    _insertRawNodesBefore(_appendTarget(raw), nodes, at);
}

function _insertRawNodesBefore(container, rawNodes, rawRef) {
    rawNodes = _forRealItems(rawNodes);
    if (!container || rawNodes.length === 0) {
        return;
    }
    rawNodes.forEach(_wsLeave);
    var places = [];
    for (var n = 0; n < rawNodes.length; ++n) {
        var node = rawNodes[n];
        places.push(node && node.eldurBlock !== undefined ? _slotFor(node) : (node && node.eldurSlot) || node);
    }
    _insertInFlow(container, places, (rawRef && rawRef.eldurSlot) || rawRef);
    _wsEnter(rawNodes.length, rawRef, container);
    for (var o = 0; o < rawNodes.length; ++o) {
        if (rawNodes[o] && rawNodes[o].eldurBlock !== undefined) {
            _leaveFlow(rawNodes[o]);
        }
    }
}

function _insertInFlow(container, rawNodes, rawRef) {
    _layoutChanged(container);
    _gTouch();
    if (!rawRef && container.children
        && (typeof container.spacing === "number" || typeof container.columnSpacing === "number")) {
        for (var s = container.children.length - 1; s >= 0 && _isLayoutSpacer(container.children[s]); --s) {
            rawRef = container.children[s];
        }
    }
    if (!rawRef || !container.children) {
        for (var a = 0; a < rawNodes.length; ++a) {
            if (rawNodes[a] && rawNodes[a].parent !== undefined) {
                rawNodes[a].parent = null;
                rawNodes[a].parent = container;
            }
        }
    } else {
        var kids = [];
        for (var i = 0; i < container.children.length; ++i) {
            kids.push(container.children[i]);
        }
        var idx = kids.indexOf(rawRef);
        if (idx === -1) {
            for (var b = 0; b < rawNodes.length; ++b) {
                if (rawNodes[b] && rawNodes[b].parent !== undefined) {
                    rawNodes[b].parent = null;
                    rawNodes[b].parent = container;
                }
            }
        } else {
            var tail = [];
            for (var t = idx; t < kids.length; ++t) {
                if (rawNodes.indexOf(kids[t]) === -1) {
                    tail.push(kids[t]);
                }
            }
            for (var j = 0; j < tail.length; ++j) {
                tail[j].parent = null;
            }
            for (var m = 0; m < rawNodes.length; ++m) {
                if (rawNodes[m] && rawNodes[m].parent !== undefined) {
                    rawNodes[m].parent = null;
                    rawNodes[m].parent = container;
                }
            }
            for (var k = 0; k < tail.length; ++k) {
                tail[k].parent = container;
            }
        }
    }
    if (_isConnected(container)) {
        for (var mIdx = 0; mIdx < rawNodes.length; ++mIdx) {
            _connectSubtree(rawNodes[mIdx]);
        }
    }
    _reflowFilled(container, true);
    // Before the script reads styles back; removals settle with the layouts.
    _renumber(container);
}

function _genericElementSource(tag, inputType) {
    if (_gActive() && _gWantsBox(tag) && !_vInlineTag(tag)) {
        return _gBoxSource(tag, false);
    }
    if (tag === "input") {
        return _inputSource(inputType || "text");
    }
    if (tag === "textarea") {
        return _NATIVE_IMPORTS + "TextArea { " + _NATIVE_TEXT + "color: \"black\"; background: Rectangle { color: \"white\"; border.color: \"#767676\" } }";
    }
    if (tag === "select") {
        return _NATIVE_IMPORTS + "ComboBox { id: control; " + _KEY_SIGNALS
            // A static select's model starts from its markup; this one from none.
            + "property string eldurInline: '{\"select\":true,\"kids\":[]}'; "
            + "textRole: \"text\"; valueRole: \"value\"; model: []; "
            + "contentItem: Text { " + _NATIVE_TEXT + "text: control.displayText; font: control.font; color: \"black\"; "
            + "verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight } "
            + "indicator: Text { " + _NATIVE_TEXT + "x: control.width - width - 4; y: (control.height - height) / 2; text: \"\\u25be\"; font.pixelSize: 12 } "
            + "background: Rectangle { color: \"white\"; border.color: \"#767676\"; radius: 2 } }";
    }
    if (tag === "li") {
        return "import QtQuick; import QtQuick.Layouts; RowLayout { Layout.fillWidth: true; spacing: 8 }";
    }
    // `_tableSettle` places a row's cells: they may span rows, which no row-wide
    // layout could hold.
    if (tag === "tr") {
        return "import QtQuick; import QtQuick.Layouts; Item { Layout.fillWidth: true; Layout.preferredHeight: implicitHeight; property real eldurSide: 0; property real eldurTop: 0; property real eldurBottom: 0; Layout.leftMargin: eldurSide; Layout.rightMargin: eldurSide; Layout.topMargin: eldurTop; Layout.bottomMargin: eldurBottom; }";
    }
    if (tag === "table") {
        return "import QtQuick; import QtQuick.Layouts; ColumnLayout { Layout.fillWidth: false; spacing: 0 }";
    }
    if (tag === "thead" || tag === "tbody" || tag === "tfoot") {
        return "import QtQuick; import QtQuick.Layouts; ColumnLayout { Layout.fillWidth: true; spacing: 0; property real eldurSide: 0; property real eldurTop: 0; property real eldurBottom: 0; Layout.leftMargin: eldurSide; Layout.rightMargin: eldurSide; Layout.topMargin: eldurTop; Layout.bottomMargin: eldurBottom; }";
    }
    if (tag === "ul" || tag === "ol") {
        return "import QtQuick; import QtQuick.Layouts; ColumnLayout { Layout.fillWidth: true; spacing: 0 }";
    }
    if (tag === "span" || tag === "p" || tag === "label" || tag === "h1" || tag === "h2" || tag === "h3" || tag === "a" || _vInlineTag(tag)) {
        return "import QtQuick; Text { " + _NATIVE_TEXT + "color: \"black\"; font.pixelSize: 16 }";
    }
    if (tag === "div" || tag === "section" || tag === "article" || tag === "header" || tag === "footer" || tag === "main" || tag === "nav") {
        return "import QtQuick; Rectangle { color: \"transparent\" }";
    }
    if (tag === "img") {
        return "import QtQuick; Image {}";
    }
    if (tag === "button") {
        return _buttonSource("");
    }
    return "import QtQuick; Item {}";
}

function _isCreatedElement(raw) {
    if (!raw || typeof raw !== "object") {
        return false;
    }
    var m = _meta(raw);
    return Boolean(m.createdTag && m.createdTag !== "#text" && !m.isFragment);
}

function _isConnected(raw) {
    if (!raw || !_root) {
        return false;
    }
    var cur = raw;
    while (cur) {
        if (cur === _root || (_root.contentItem && cur === _root.contentItem)) {
            return true;
        }
        cur = _domUp(cur);
    }
    return false;
}

function _logicalAncestorAt(raw, distance) {
    var cur = raw;
    for (var d = 0; d < distance && cur; ++d) {
        cur = _logicalParent(cur);
        if (cur === _root) {
            return null;
        }
    }
    return cur || null;
}

function _getComponentSchema(component) {
    if (!component) {
        return null;
    }
    var schema = _componentProbeCache.get(component);
    if (schema) {
        return schema;
    }
    var probe = component.createObject(null);
    schema = {
        boolProps: {},
        hasClasses: Boolean(probe && typeof probe.eldurClasses === "string"),
        upRanks: [],
        templateName: (probe && typeof probe.eldurTemplate === "string") ? probe.eldurTemplate : ""
    };
    if (probe) {
        for (var k in probe) {
            if (k.indexOf("cls_") === 0 && typeof probe[k] === "boolean") {
                schema.boolProps[k] = true;
            } else {
                var upMatch = /^eldurUp([1-9][0-9]*)$/.exec(k);
                if (upMatch && typeof probe[k] !== "function") {
                    schema.upRanks.push(Number(upMatch[1]));
                }
            }
        }
        probe.destroy();
    }
    _componentProbeCache.set(component, schema);
    return schema;
}

function _buildTemplateKeys(raw) {
    var m = _meta(raw);
    var tag = String(m.createdTag || (typeof raw.eldurTag === "string" ? raw.eldurTag : "")).toLowerCase();
    var inputType = tag === "input" ? String(m.inputType || m.attrs.type || "").toLowerCase() : "";
    // The converter keys an input with no type as the text one it is.
    if (tag === "input" && inputType === "" && _root && _root.eldurTemplates
        && Object.keys(_root.eldurTemplates).some(function(k) { return k.indexOf("input[type=text]") === 0; })) {
        inputType = "text";
    }
    var tagPart = (tag === "input" && inputType !== "") ? ("input[type=" + inputType + "]") : tag;

    var styled = (_root && _root.eldurStyledClasses !== undefined && _root.eldurStyledClasses !== null && typeof _root.eldurStyledClasses.length === "number")
        ? _root.eldurStyledClasses
        : null;
    var filtered = [];
    for (var i = 0; i < m.classes.length; ++i) {
        var c = m.classes[i];
        if (!c || filtered.indexOf(c) !== -1) {
            continue;
        }
        if (styled === null || Array.prototype.indexOf.call(styled, c) !== -1) {
            filtered.push(c);
        }
    }
    filtered.sort();
    var classPart = filtered.length > 0 ? ("." + filtered.join(".")) : "";

    var p = _logicalParent(raw);
    var context = "*";
    if (p && typeof p === "object") {
        if (typeof p.eldurTemplate === "string" && p.eldurTemplate !== "") {
            context = p.eldurTemplate;
        } else if (typeof p.eldurKey === "string" && p.eldurKey !== "") {
            context = p.eldurKey;
        }
    }

    return [
        tagPart + classPart + "@" + context,
        tagPart + classPart + "@*",
        tagPart + "@" + context,
        tagPart + "@*"
    ];
}

function _lookupTemplate(raw) {
    if (!_root || !_root.eldurTemplates || typeof _root.eldurTemplates !== "object") {
        return null;
    }
    // A row has no form of its own for a template to describe, nor has a table
    // whose rows are not there yet.
    if (_TABLE_PARTS[_meta(raw).createdTag]) {
        return null;
    }
    var templates = _root.eldurTemplates;
    var keys = _buildTemplateKeys(raw);
    var exact = keys[0];
    if (_gActive()) {
        // A template is right when the converter wrote it for exactly this: the
        // same classes, and a place that either is the one it wrote it for or
        // that no matching selector asks about.
        if (templates[exact] && _gTemplateFits(raw, exact)) {
            return { component: templates[exact], exactKey: exact, step: exact };
        }
        return { component: null, exactKey: exact, step: "generic" };
    }
    if (templates[exact]) {
        return { component: templates[exact], exactKey: exact, step: exact };
    }
    for (var i = 1; i < keys.length; ++i) {
        var cand = keys[i];
        if (templates[cand]) {
            if (!_warnedMissingTemplates[exact]) {
                _warnedMissingTemplates[exact] = true;
                console.warn("eldur: no style template for " + exact + "; using " + cand);
            }
            return { component: templates[cand], exactKey: exact, step: cand };
        }
    }
    // Connected before the script says its class: templates for this tag are
    // keyed by the classes it comes to have.
    var classed = exact.split("@")[0] + ".";
    var classless = exact.indexOf(".") < 0
        && Object.keys(templates).some(function(k) { return k.indexOf(classed) === 0; });
    if (!classless && !_warnedMissingTemplates[exact]) {
        _warnedMissingTemplates[exact] = true;
        console.warn("eldur: no style template for " + exact + "; using generic");
    }
    return { component: null, exactKey: exact, step: "generic" };
}

// What one form cannot paint and a template can: a created box has no gradient,
// shadow, transform or animation, and no state of its own to take a `:hover` in.
var _G_TEMPLATE_ONLY = /^(background-image|box-shadow|transform|filter|transition-property|animation-name)$/;
var _G_STATE = /::?(hover|focus-within|focus-visible|focus|active|before|after)\b/g;

function _gTemplateFits(raw, key) {
    var tag = _nodeTag(raw);
    if (!_gWantsBox(tag)) {
        return true;
    }
    ++_gGen;
    var rules = _vLoadRules();
    var matched = _gMatched(raw);
    for (var i = 0; i < matched.length; ++i) {
        var decls = rules[matched[i]].decls;
        for (var d = 0; d < decls.length; ++d) {
            if (_G_TEMPLATE_ONLY.test(decls[d][0]) && decls[d][1] !== "none" && decls[d][1] !== "initial") {
                return true;
            }
        }
    }
    // A template can't be relied on for what a Text doesn't paint, and an open
    // page's box does follow its states.
    if (_gActive()) {
        for (var p = 0; p < matched.length; ++p) {
            var own = rules[matched[p]].decls;
            for (var q = 0; q < own.length; ++q) {
                if (/^(background|box-shadow|border|outline|opacity|transform)/.test(own[q][0])) {
                    return false;
                }
            }
        }
    }
    for (var r = 0; r < rules.length; ++r) {
        if (rules[r].sel.search(_G_STATE) === -1) {
            continue;
        }
        _G_STATE.lastIndex = 0;
        if (rules[r].stateless === undefined) {
            try {
                rules[r].stateless = _parseSelector(rules[r].sel.replace(_G_STATE, ""));
            } catch (e) {
                rules[r].stateless = null;
            }
        }
        if (rules[r].stateless && _matchesList(raw, rules[r].stateless)) {
            return true;
        }
    }
    return false;
}

function _syncUpPointersOnItem(raw) {
    if (!raw || typeof raw !== "object") {
        return;
    }
    var m = _meta(raw);
    var schema = m.templateComponent ? _getComponentSchema(m.templateComponent) : null;
    if (schema && schema.upRanks.length > 0) {
        for (var i = 0; i < schema.upRanks.length; ++i) {
            var rank = schema.upRanks[i];
            var anc = _logicalAncestorAt(raw, rank);
            raw["eldurUp" + rank] = anc;
        }
    } else {
        for (var n = 1; n <= 8; ++n) {
            var prop = "eldurUp" + n;
            if (raw[prop] !== undefined && typeof raw[prop] !== "function") {
                raw[prop] = _logicalAncestorAt(raw, n);
            }
        }
    }
}

function _swapRawItem(oldRaw, component, fallbackQmlSrc) {
    var m = _meta(oldRaw);
    var parentContainer = oldRaw.parent || null;

    var tail = [];
    if (parentContainer && parentContainer.children) {
        var pKids = parentContainer.children;
        var foundOld = false;
        for (var pk = 0; pk < pKids.length; ++pk) {
            if (pKids[pk] === oldRaw) {
                foundOld = true;
            } else if (foundOld) {
                tail.push(pKids[pk]);
            }
        }
    }

    if (typeof oldRaw.checked === "boolean") {
        m.explicitChecked = oldRaw.checked;
    }
    var childElements = _rawElementChildren(oldRaw);
    var savedText = (childElements.length === 0 && m.createdTag !== "input" && m.createdTag !== "textarea")
        ? (m.explicitText !== undefined ? m.explicitText : getText(oldRaw))
        : m.explicitText;
    if (m.flow) {
        delete m.flow;
    }

    var next = null;
    if (component) {
        var schema = _getComponentSchema(component);
        var initProps = {};
        var bKeys = Object.keys(schema.boolProps);
        for (var b = 0; b < bKeys.length; ++b) {
            initProps[bKeys[b]] = false;
        }
        for (var ci = 0; ci < m.classes.length; ++ci) {
            var cProp = _sanitizeClassProp(m.classes[ci]);
            if (schema.boolProps[cProp]) {
                initProps[cProp] = true;
            }
        }
        if (schema.hasClasses) {
            initProps.eldurClasses = m.classes.join(" ");
        }
        for (var u = 0; u < schema.upRanks.length; ++u) {
            var rank = schema.upRanks[u];
            var anc = _logicalAncestorAt(oldRaw, rank);
            if (anc) {
                initProps["eldurUp" + rank] = anc;
            }
        }
        // Owned by the root, not the container: the container may be a part
        // of an ancestor that is itself swapped and destroyed later, while
        // this item is kept and moved into the ancestor's replacement.
        next = component.createObject(_qobjectParent(), initProps);
        if (next && next.parent !== undefined) {
            next.parent = parentContainer;
            if (oldRaw.eldurSlot && next.eldurSlot !== undefined) {
                next.eldurSlot = oldRaw.eldurSlot;
                next.eldurSlot.eldurBox = next;
            }
            if (next.eldurBlock !== undefined && parentContainer) {
                if (!next.eldurSlot) {
                    _slotFor(next).parent = parentContainer;
                }
                _leaveFlow(next);
            }
        }
        m.templateComponent = component;
    } else {
        var tag = String(m.createdTag || "div").toLowerCase();
        var src = fallbackQmlSrc || _genericElementSource(tag, m.inputType);
        next = Qt.createQmlObject(src, _qobjectParent(), "eldur_el_" + tag);
        if (next && next.parent !== undefined) {
            next.parent = parentContainer;
        }
        if (_gActive() && m.createdTag) {
            _gLive.push(next);
        }
        m.templateComponent = null;
    }

    if (oldRaw.parent !== undefined) {
        oldRaw.parent = null;
    }
    for (var t = 0; t < tail.length; ++t) {
        if (tail[t] && tail[t] !== next && tail[t].parent === parentContainer) {
            tail[t].parent = null;
            tail[t].parent = parentContainer;
        }
    }

    // The element's id is not the template's business.
    if (oldRaw.objectName && !next.objectName) {
        next.objectName = oldRaw.objectName;
    }
    _itemMeta.delete(oldRaw);
    _itemMeta.set(next, m);
    m.raw = next;
    if (_gActive()) {
        _gReplaced.set(oldRaw, next);
        var liveAt = _gLive.indexOf(oldRaw);
        if (liveAt !== -1) {
            _gLive.splice(liveAt, 1);
        }
    }
    if (typeof m._setRaw === "function") {
        m._setRaw(next);
    }
    // The page's own items are read by id from bindings written beside them.
    if (m.gKeepReplaced) {
        oldRaw.visible = false;
        // Not `null`: its bindings go on reading `parent`.
        if (!_gGrave) {
            _gGrave = Qt.createQmlObject("import QtQuick; Item { visible: false }", _qobjectParent(), "eldur_grave");
            _gGrave.parent = null;
        }
        oldRaw.parent = _gGrave;
    } else {
        oldRaw.destroy();
    }
    // A new item starts with its template's place.
    _renumber(parentContainer);

    if (typeof next.eldurClasses === "string" && next.eldurClasses !== m.classes.join(" ")) {
        next.eldurClasses = m.classes.join(" ");
    }
    for (var k = 0; k < m.classes.length; ++k) {
        var cp = _sanitizeClassProp(m.classes[k]);
        if (typeof next[cp] === "boolean" && next[cp] !== true) {
            next[cp] = true;
        }
    }

    if (childElements.length === 0 && savedText !== undefined && m.createdTag !== "input" && m.createdTag !== "textarea") {
        if (savedText !== "" || m.explicitText !== undefined) {
            setText(next, savedText);
        }
    }

    if (childElements.length > 0) {
        // Before the template's trailing spacers, renumbered.
        _insertChildren(next, childElements, null);
    }

    var attrNames = Object.keys(m.attrs);
    for (var a = 0; a < attrNames.length; ++a) {
        var ak = attrNames[a];
        if (ak !== "type" && ak !== "class") {
            setAttribute(next, ak, m.attrs[ak]);
        }
    }

    if (m.explicitValue !== undefined) {
        if (typeof next.find === "function" && typeof next.currentIndex === "number") {
            var byValue = next.indexOfValue(String(m.explicitValue));
            next.currentIndex = byValue !== -1 ? byValue : next.find(String(m.explicitValue));
        } else if (typeof next.checked === "boolean" && typeof next.value === "string") {
            next.value = String(m.explicitValue);
        } else if (typeof next.text === "string" && (m.createdTag === "input" || m.createdTag === "textarea")) {
            next.text = String(m.explicitValue);
        } else if (typeof next.value === "number") {
            next.value = Number(m.explicitValue) || 0;
        }
    }
    if (m.explicitChecked !== undefined && typeof next.checked === "boolean") {
        next.checked = Boolean(m.explicitChecked);
    }
    if (m.explicitDisabled !== undefined && typeof next.enabled === "boolean") {
        next.enabled = !m.explicitDisabled;
    }
    if (m.explicitReadOnly !== undefined && typeof next.readOnly === "boolean") {
        next.readOnly = Boolean(m.explicitReadOnly);
    }
    if (m.explicitHidden !== undefined && typeof next.visible === "boolean") {
        next.visible = !m.explicitHidden;
    }

    var styleNames = Object.keys(m.styles);
    for (var s = 0; s < styleNames.length; ++s) {
        var sk = styleNames[s];
        if (sk.indexOf("--") === 0 || sk.indexOf("-") === -1) {
            setStyle(next, sk, m.styles[sk]);
        }
    }

    _syncUpPointersOnItem(next);

    var eventTypes = {};
    var cKeys = Object.keys(m.connected);
    var ei;
    for (ei = 0; ei < cKeys.length; ++ei) {
        eventTypes[cKeys[ei]] = true;
    }
    var lKeys = Object.keys(m.listeners);
    for (ei = 0; ei < lKeys.length; ++ei) {
        if (m.listeners[lKeys[ei]] && m.listeners[lKeys[ei]].length > 0) {
            eventTypes[lKeys[ei].replace("capture:", "")] = true;
        }
    }
    var oKeys = Object.keys(m.onHandlers);
    for (ei = 0; ei < oKeys.length; ++ei) {
        if (typeof m.onHandlers[oKeys[ei]] === "function") {
            eventTypes[oKeys[ei]] = true;
        }
    }
    m.connected = {};
    var evNames = Object.keys(eventTypes);
    for (ei = 0; ei < evNames.length; ++ei) {
        _connectSignalOnce(next, evNames[ei]);
    }

    return next;
}

// A text node in a line of created inline elements (a `Flow` with
// `eldurTextRun`, §6.8) is a `Text` of its own between them, in the line's
// font and colour. Qt does not measure a trailing space, and the spaces
// between items are the text's own, so they are no-break spaces.
function _runText(str) {
    return String(str).replace(/[ \t\n\r\f]+/g, "\u00a0");
}

function _materializeTextRun(raw) {
    var container = raw.parent;
    var m = _meta(raw);
    var component = container && container.eldurTextRun;
    if (!component || typeof component.createObject !== "function"
        || m.textRunComponent === component) {
        return raw;
    }
    var source = m.runText !== undefined ? m.runText : String(raw.text);
    var tail = [];
    var foundOld = false;
    for (var i = 0; i < container.children.length; ++i) {
        if (container.children[i] === raw) {
            foundOld = true;
        } else if (foundOld) {
            tail.push(container.children[i]);
        }
    }
    var next = component.createObject(_qobjectParent(), { text: _runText(source) });
    if (!next) {
        return raw;
    }
    next.parent = container;
    raw.parent = null;
    for (var t = 0; t < tail.length; ++t) {
        tail[t].parent = null;
        tail[t].parent = container;
    }
    _itemMeta.delete(raw);
    _itemMeta.set(next, m);
    m.raw = next;
    m.runText = source;
    m.textRunComponent = component;
    if (typeof m._setRaw === "function") {
        m._setRaw(next);
    }
    raw.destroy();
    _layoutChanged(next);
    return next;
}

function _materializeSubtree(raw) {
    if (!raw || typeof raw !== "object") {
        return raw;
    }
    if (_meta(raw).createdTag === "#text") {
        return _materializeTextRun(raw);
    }
    var cur = raw;
    if (_isCreatedElement(cur)) {
        var m = _meta(cur);
        var res = _lookupTemplate(cur);
        // A span the rules make a box is one, with or without a template for its text.
        if ((!res || !res.component) && _gActive() && !_gIsBox(cur) && _gWantsBox(m.createdTag)
            && _gStyle(cur).display !== "inline") {
            cur = _swapRawItem(cur, null, _gBoxSource(m.createdTag, false));
            _gLive.push(cur);
        } else if (res) {
            if (res.component) {
                var schema = _getComponentSchema(res.component);
                var sameComp = (m.templateComponent === res.component)
                    || (typeof cur.eldurTemplate === "string" && cur.eldurTemplate !== "" && schema.templateName !== "" && cur.eldurTemplate === schema.templateName);
                if (!sameComp) {
                    cur = _swapRawItem(cur, res.component, null);
                } else {
                    m.templateComponent = res.component;
                    _syncUpPointersOnItem(cur);
                }
            } else {
                var wasTemplate = Boolean(m.templateComponent || (typeof cur.eldurTemplate === "string" && cur.eldurTemplate !== ""));
                var inputChanged = (m.createdTag === "input" && m.lastGenericInputSource && m.lastGenericInputSource !== _inputSource(m.inputType || "text"));
                if (wasTemplate || inputChanged) {
                    cur = _swapRawItem(cur, null, _genericElementSource(m.createdTag, m.inputType));
                } else {
                    _syncUpPointersOnItem(cur);
                }
            }
        } else {
            _syncUpPointersOnItem(cur);
        }
    }
    var kids = _rawElementChildren(cur);
    for (var i = 0; i < kids.length; ++i) {
        _materializeSubtree(kids[i]);
    }
    return cur;
}

// Whether an item's glyphs reach the screen other than pixel for pixel, as
// `resamples` in the converter's paint.rs decides. It reads the item's values
// now; the converter also counts a property that is only bound.
function _resamples(item) {
    if (item.rotation !== 0 || item.scale !== 1) {
        return true;
    }
    for (var i = 0; item.transform && i < item.transform.length; ++i) {
        if (item.transform[i].x === undefined) {
            return true;
        }
    }
    return false;
}

// Text.QtRendering and Text.NativeRendering; a library script has no `Text`.
function _fitGlyphs(item, turned) {
    turned = turned || _resamples(item);
    var wanted = turned ? 0 : 1;
    if (typeof item.renderType === "number" && item.renderType !== wanted) {
        item.renderType = wanted;
    }
    for (var i = 0; i < item.children.length; ++i) {
        _fitGlyphs(item.children[i], turned);
    }
}

// Created elements and templates are made native without knowing what they
// will sit under; only here is it known (layout.md §1.2).
function _connectSubtree(raw) {
    var cur = _materializeSubtree(raw);
    var turned = false;
    for (var up = cur.parent; up && !turned; up = up.parent) {
        turned = _resamples(up);
    }
    _fitGlyphs(cur, turned);
}

function _unescapeHtmlText(str) {
    return String(str)
        .replace(/&lt;/g, "<")
        .replace(/&gt;/g, ">")
        .replace(/&quot;/g, "\"")
        .replace(/&#39;/g, "'")
        .replace(/&amp;/g, "&");
}

function _applyAttrsFromHtml(el, attrStr) {
    if (!attrStr) {
        return;
    }
    var re = /([a-zA-Z_:][-a-zA-Z0-9_:.]*)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/g;
    var match;
    while ((match = re.exec(attrStr)) !== null) {
        var name = match[1];
        var val = match[2] !== undefined ? match[2] : (match[3] !== undefined ? match[3] : (match[4] !== undefined ? match[4] : ""));
        val = _unescapeHtmlText(val);
        var lower = name.toLowerCase();
        if (lower === "id") {
            el.id = val;
        } else if (lower === "class") {
            el.className = val;
        } else if (lower === "style") {
            el.style.cssText = val;
        } else if (lower === "type") {
            el.type = val;
            el.setAttribute("type", val);
        } else {
            el.setAttribute(name, val);
        }
    }
}

var _HTML_INLINE = /^(a|abbr|b|bdi|bdo|cite|code|data|dfn|em|i|kbd|mark|q|s|samp|small|span|strong|sub|sup|time|u|var)$/i;

function _parseHtmlFragmentNodes(html) {
    var src = String(html === null || html === undefined ? "" : html);
    var nodes = [];
    var gap = "";
    var lastTag = "";
    var pos = 0;
    var tagOpenRe = /<([a-zA-Z][a-zA-Z0-9-]*)(\s[^>]*)?\s*(\/?)>/g;
    var voidTags = { area: true, base: true, br: true, col: true, embed: true, hr: true, img: true, input: true, link: true, meta: true, source: true, track: true, wbr: true };

    while (pos < src.length) {
        tagOpenRe.lastIndex = pos;
        var m = tagOpenRe.exec(src);
        if (!m) {
            var trailing = src.slice(pos);
            if (trailing.trim()) {
                nodes.push(document.createTextNode(_unescapeHtmlText(trailing)));
            } else {
                nodes.wsEnd = trailing;
            }
            break;
        }
        if (m.index > pos) {
            var lead = src.slice(pos, m.index);
            if (lead.trim()) {
                nodes.push(document.createTextNode(_unescapeHtmlText(lead)));
            } else if (nodes.length > 0 && _HTML_INLINE.test(lastTag) && _HTML_INLINE.test(m[1])) {
                // Between inline elements a space is drawn: it is text.
                nodes.push(document.createTextNode(" "));
            } else {
                gap = lead;
            }
        }
        lastTag = m[1];
        var tag = m[1].toLowerCase();
        var attrStr = m[2] || "";
        var selfClosing = m[3] === "/" || Boolean(voidTags[tag]);
        var el = document.createElement(tag);
        _applyAttrsFromHtml(el, attrStr);
        _wsGiven(_unwrap(el), "ws", gap);
        gap = "";

        var afterOpen = tagOpenRe.lastIndex;
        if (selfClosing) {
            nodes.push(el);
            pos = afterOpen;
            continue;
        }
        // Find matching closing tag for `tag`
        var depth = 1;
        var scanPos = afterOpen;
        var tokenRe = new RegExp("<(/?)" + tag + "\\b[^>]*>", "gi");
        tokenRe.lastIndex = scanPos;
        var tok;
        var closeStart = -1;
        var closeEnd = -1;
        while ((tok = tokenRe.exec(src)) !== null) {
            var fullTok = tok[0];
            if (tok[1] === "/") {
                depth -= 1;
                if (depth === 0) {
                    closeStart = tok.index;
                    closeEnd = tokenRe.lastIndex;
                    break;
                }
            } else if (fullTok.slice(-2) !== "/>" && !voidTags[tag]) {
                depth += 1;
            }
        }
        if (closeStart === -1) {
            var rest = src.slice(afterOpen);
            if (rest) {
                el.textContent = _unescapeHtmlText(rest);
            }
            nodes.push(el);
            break;
        }
        var inner = src.slice(afterOpen, closeStart);
        if (/<[a-zA-Z]/.test(inner)) {
            var childWrappers = _parseHtmlFragmentNodes(inner);
            _wsGiven(_unwrap(el), "wsEnd", childWrappers.wsEnd);
            for (var c = 0; c < childWrappers.length; ++c) {
                el.appendChild(childWrappers[c]);
            }
        } else if (inner) {
            el.textContent = _unescapeHtmlText(inner);
        }
        nodes.push(el);
        pos = closeEnd;
    }
    return nodes;
}

function wrap(rawItem) {
    if (!rawItem) {
        return null;
    }
    if (rawItem.__eldurWrapper) {
        return rawItem;
    }
    if ((rawItem === window || rawItem === document) && _root) {
        rawItem = _root;
    }
    if (!_isQObject(rawItem)) {
        return rawItem;
    }
    var m = _meta(rawItem);

    function _computeScrollDimension(axis) {
        var isY = axis === "y";
        var contentProp = isY ? "contentHeight" : "contentWidth";
        var sizeProp = isY ? "height" : "width";
        var implicitProp = isY ? "implicitHeight" : "implicitWidth";
        if (typeof rawItem[contentProp] === "number" && rawItem[contentProp] > 0) {
            return rawItem[contentProp];
        }
        var maxDim = Math.max(
            typeof rawItem[sizeProp] === "number" ? rawItem[sizeProp] : 0,
            typeof rawItem[implicitProp] === "number" ? rawItem[implicitProp] : 0
        );
        var container = _childContainer(rawItem);
        if (container && container !== rawItem) {
            if (typeof container[implicitProp] === "number" && container[implicitProp] > maxDim) {
                maxDim = container[implicitProp];
            }
            if (typeof container[sizeProp] === "number" && container[sizeProp] > maxDim) {
                maxDim = container[sizeProp];
            }
        }
        var kids = _rawElementChildren(rawItem);
        for (var i = 0; i < kids.length; ++i) {
            var ch = kids[i];
            var cm = _meta(ch);
            var basePos = isY
                ? (cm.baseY !== undefined ? cm.baseY : (typeof ch.y === "number" ? ch.y : 0))
                : (cm.baseX !== undefined ? cm.baseX : (typeof ch.x === "number" ? ch.x : 0));
            var chDim = typeof ch[sizeProp] === "number" && ch[sizeProp] > 0
                ? ch[sizeProp]
                : (typeof ch[implicitProp] === "number" ? ch[implicitProp] : 0);
            if (basePos + chDim > maxDim) {
                maxDim = basePos + chDim;
            }
        }
        return maxDim;
    }

    function _applyScrollOffset(axis, rawVal) {
        var isY = axis === "y";
        var n = Number(rawVal) || 0;
        var scrollDim = _computeScrollDimension(axis);
        var clientDim = isY
            ? (typeof rawItem.height === "number" ? rawItem.height : 0)
            : (typeof rawItem.width === "number" ? rawItem.width : 0);
        var maxScroll = (clientDim > 0 && scrollDim > clientDim)
            ? Math.max(0, scrollDim - clientDim)
            : 0;
        n = Math.max(0, Math.min(n, maxScroll));
        var propName = isY ? "scrollTop" : "scrollLeft";
        var prev = m[propName] || 0;
        m[propName] = n;
        var contentPosProp = isY ? "contentY" : "contentX";
        if (typeof rawItem[contentPosProp] === "number") {
            rawItem[contentPosProp] = n;
        } else {
            var container = _childContainer(rawItem);
            if (container && container !== rawItem && container.anchors) {
                var cm = _meta(container);
                if (isY) {
                    if (cm.baseTopMargin === undefined) {
                        cm.baseTopMargin = typeof container.anchors.topMargin === "number"
                            ? container.anchors.topMargin
                            : 0;
                    }
                    container.anchors.topMargin = cm.baseTopMargin - n;
                } else {
                    if (cm.baseLeftMargin === undefined) {
                        cm.baseLeftMargin = typeof container.anchors.leftMargin === "number"
                            ? container.anchors.leftMargin
                            : 0;
                    }
                    if (cm.baseRightMargin === undefined) {
                        cm.baseRightMargin = typeof container.anchors.rightMargin === "number"
                            ? container.anchors.rightMargin
                            : 0;
                    }
                    container.anchors.leftMargin = cm.baseLeftMargin - n;
                    if (container.anchors.right) {
                        container.anchors.rightMargin = cm.baseRightMargin + n;
                    }
                }
            } else {
                var kids = _rawElementChildren(rawItem);
                for (var i = 0; i < kids.length; ++i) {
                    var ch = kids[i];
                    var km = _meta(ch);
                    if (isY) {
                        if (km.baseY === undefined) {
                            km.baseY = typeof ch.y === "number" ? ch.y : 0;
                        }
                        if (typeof ch.y === "number") {
                            ch.y = km.baseY - n;
                        }
                    } else {
                        if (km.baseX === undefined) {
                            km.baseX = typeof ch.x === "number" ? ch.x : 0;
                        }
                        if (typeof ch.x === "number") {
                            ch.x = km.baseX - n;
                        }
                    }
                }
            }
        }
        if (n !== prev) {
            _dispatchDynamic(rawItem, "scroll", null);
        }
    }
    m.raw = rawItem;
    if (m.wrapper) {
        return m.wrapper;
    }
    m._setRaw = function(nextRaw) {
        rawItem = nextRaw;
        m.raw = nextRaw;
    };

    var styleMethods = {
        setProperty: function(prop, val) {
            setStyle(rawItem, prop, val);
        },
        getPropertyValue: function(prop) {
            return getStyle(rawItem, prop);
        },
        removeProperty: function(prop) {
            var old = getStyle(rawItem, prop);
            setStyle(rawItem, prop, "");
            var rawName = String(prop);
            var camel = rawName.indexOf("--") === 0 ? rawName : _kebabToCamel(rawName);
            delete _meta(rawItem).styles[rawName];
            delete _meta(rawItem).styles[camel];
            return old;
        }
    };

    var styleProxy = new Proxy(styleMethods, {
        get: function(target, prop) {
            if (prop in target) {
                return target[prop];
            }
            return typeof prop === "string" ? getStyle(rawItem, prop) : undefined;
        },
        set: function(_t, prop, val) {
            if (typeof prop === "string") {
                setStyle(rawItem, prop, val);
            }
            return true;
        }
    });

    var wrapper = {
        __eldurWrapper: true,
        get _item() { return rawItem; },
        style: styleProxy,
        get elements() { return _formElements(rawItem); },
        get type() {
            // A static control's QML type is what it was written as.
            var written = m.inputType || getAttribute(rawItem, "type") || "";
            if (written === "" && typeof rawItem.checked === "boolean" && typeof rawItem.checkState === "number") {
                return "checkbox";
            }
            return written !== "" || typeof rawItem.checked !== "boolean" ? written : "radio";
        },
        // `createElement('input')` must return before the script says what
        // type it is, and a QML object cannot change type.
        set type(v) {
            var ty = String(v).toLowerCase();
            var from = m.inputType || "text";
            m.inputType = ty;
            m.attrs.type = ty;
            if (m.createdTag !== "input") {
                return;
            }
            if (_isConnected(rawItem) && _root && _root.eldurTemplates && typeof _root.eldurTemplates === "object") {
                _connectSubtree(rawItem);
                return;
            }
            if (_inputSource(ty) === _inputSource(from) && !m.templateComponent) {
                return;
            }
            m.lastGenericInputSource = _inputSource(ty);
            _swapRawItem(rawItem, null, _inputSource(ty));
            if (_isConnected(m.raw)) {
                _connectSubtree(m.raw);
            }
        },
        get id() {
            if (typeof rawItem.objectName === "string" && rawItem.objectName !== "") {
                return rawItem.objectName;
            }
            return m.attrs.id || "";
        },
        set id(v) {
            var s = String(v);
            if (rawItem.__eldurInline) {
                _vSetAttr(rawItem, "id", s);
                return;
            }
            m.attrs.id = s;
            if (typeof rawItem.objectName === "string") {
                rawItem.objectName = s;
            }
        },
        get tagName() { return _inferTagName(rawItem); },
        get nodeName() { return _inferTagName(rawItem); },
        get baseURI() { return _getDocumentBaseUrl(); },
        get textContent() { return getText(rawItem); },
        set textContent(v) { setText(rawItem, v); },
        get innerText() { return _innerText(rawItem); },
        set innerText(v) { setText(rawItem, v); },
        get outerHTML() { return _serializeElement(rawItem); },
        set outerHTML(v) {
            this.replaceWith.apply(this, _parseHtmlFragmentNodes(v));
        },
        get content() { return _templateContent(rawItem); },
        get open() {
            return _nodeTag(rawItem) === "details" ? m.detailsOpen === true : getAttribute(rawItem, "open") !== null;
        },
        set open(v) {
            if (_nodeTag(rawItem) !== "details") {
                return;
            }
            var next = Boolean(v);
            if (next === (m.detailsOpen === true)) {
                return;
            }
            _detailsShow(rawItem, next);
            Qt.callLater(function() {
                dispatchEvent(wrap(rawItem), new Event("toggle"));
            });
        },
        get innerHTML() { return getInnerHTML(rawItem); },
        set innerHTML(v) { setInnerHTML(rawItem, v); },
        get nodeType() {
            return rawItem.vtext || (_isPlainTextItem(rawItem) && !_vIsTagged(rawItem) && !m.createdTag)
                || m.createdTag === "#text" ? 3 : 1;
        },
        get data() { return rawItem.vtext ? rawItem.data : getText(rawItem); },
        set data(v) { setText(rawItem, v); },
        get nodeValue() { return this.nodeType === 3 ? this.data : null; },
        set nodeValue(v) {
            if (this.nodeType === 3) {
                setText(rawItem, v);
            }
        },
        get hash() {
            if (_inferTagName(rawItem) !== "A") {
                return undefined;
            }
            var hashHref = getAttribute(rawItem, "href");
            var hashAt = hashHref === null ? -1 : hashHref.indexOf("#");
            return hashAt === -1 || hashAt === hashHref.length - 1 ? "" : hashHref.slice(hashAt);
        },
        getClientRects: function() {
            var r = getBoundingClientRect(rawItem);
            if (rawItem.__eldurInline && (r.width > 0 || r.height > 0)) {
                return _asNodeList(_vClientRects(rawItem, r));
            }
            var holder = rawItem.parent;
            var placed = holder && typeof holder.runLines === "function" ? holder.runLines(rawItem) : [];
            if (placed.length > 1) {
                return _asNodeList(placed.map(function(p) {
                    var o = rawItem.mapToItem(null, p.x, p.y);
                    return { x: o.x, y: o.y, left: o.x, top: o.y, right: o.x + p.width,
                        bottom: o.y + p.height, width: p.width, height: p.height };
                }));
            }
            return _asNodeList(r.width > 0 || r.height > 0 ? [r] : []);
        },
        get selected() { return rawItem.sel === true; },
        set selected(v) {
            if (rawItem.eldurTag === "option") {
                var owner = rawItem.model;
                rawItem.sel = Boolean(v);
                if (owner) {
                    if (v) {
                        _vPick(owner, rawItem);
                    }
                    _vEnsureSelected(owner);
                    _vChanged(owner);
                }
            }
        },
        get defaultSelected() { return rawItem.__eldurInline && _vGetAttr(rawItem, "selected") !== null; },
        get text() { return rawItem.eldurTag === "option" ? _vOptionText(rawItem) : undefined; },
        set text(v) { setText(rawItem, v); },
        get label() {
            var labelAttr = rawItem.__eldurInline ? _vGetAttr(rawItem, "label") : null;
            return labelAttr !== null ? labelAttr : _vOptionText(rawItem);
        },
        get index() {
            return rawItem.model && rawItem.model.select ? _vOptions(rawItem.model).indexOf(rawItem) : undefined;
        },
        get options() {
            var sel = _vSelectModel(rawItem);
            if (!sel) {
                return undefined;
            }
            if (!m.optionCollection) {
                m.optionCollection = _liveCollection(function() { return _vOptions(sel); }, function(n) {
                    var opts = _vOptions(sel);
                    for (var o = opts.length - 1; o >= n; --o) {
                        _vRemove(opts[o]);
                    }
                });
            }
            return m.optionCollection;
        },
        get selectedOptions() {
            var sel = _vSelectModel(rawItem);
            return sel ? _asNodeList(_vOptions(sel).filter(function(o) { return o.sel; }).map(wrap)) : undefined;
        },
        get length() {
            var sel = _vSelectModel(rawItem);
            return sel ? _vOptions(sel).length : undefined;
        },
        set length(n) {
            if (this.options) {
                this.options.length = n;
            }
        },
        item: function(n) {
            var sel = _vSelectModel(rawItem);
            var opt = sel ? _vOptions(sel)[n] : null;
            return opt ? wrap(opt) : null;
        },
        add: function(option, before) {
            var sel = _vSelectModel(rawItem);
            if (sel) {
                var ref = typeof before === "number" ? _vOptions(sel)[before] : _unwrap(before);
                _vInsert(sel, [_unwrap(option)], ref || null);
            }
        },
        get value() {
            if (rawItem.eldurTag === "option") {
                return _vOptionValue(rawItem);
            }
            var valueOf = _vSelectModel(rawItem);
            if (valueOf) {
                var selAt = _vSelected(valueOf);
                return selAt === -1 ? "" : _vOptionValue(_vOptions(valueOf)[selAt]);
            }
            if (typeof rawItem.currentText === "string") {
                var current = rawItem.currentValue;
                return current !== undefined && current !== null ? String(current) : rawItem.currentText;
            }
            // Before `text`: a checkbox's `text` is not its form value.
            if (typeof rawItem.checked === "boolean" && typeof rawItem.value === "string") {
                return rawItem.value;
            }
            var unset = m.explicitValue === undefined && getAttribute(rawItem, "value") === null;
            var label = _G_INPUT_LABEL[getAttribute(rawItem, "type") || m.inputType];
            if (unset && label && rawItem.text === label) {
                return "";
            }
            // A SpinBox has no empty state. Zero nobody wrote stands for one.
            if (unset && rawItem.value === 0 && typeof rawItem.editable === "boolean") {
                return "";
            }
            if (typeof rawItem.text === "string") {
                return rawItem.text;
            }
            if (typeof rawItem.value === "number") {
                return typeof rawItem.indeterminate === "boolean"
                    ? rawItem.value
                    : String(rawItem.value);
            }
            return "";
        },
        set value(v) {
            if (rawItem.eldurTag === "option") {
                _vSetAttr(rawItem, "value", String(v));
                return;
            }
            var valueTo = _vSelectModel(rawItem);
            if (valueTo) {
                var match = _vOptions(valueTo).filter(function(o) { return _vOptionValue(o) === String(v); });
                _vPick(valueTo, match.length > 0 ? match[0] : null);
                _vChanged(valueTo);
                return;
            }
            m.explicitValue = v;
            if (typeof rawItem.find === "function" && typeof rawItem.currentIndex === "number") {
                var byValue = rawItem.indexOfValue(String(v));
                rawItem.currentIndex = byValue !== -1 ? byValue : rawItem.find(String(v));
            } else if (typeof rawItem.checked === "boolean" && typeof rawItem.value === "string") {
                rawItem.value = String(v);
            } else if (typeof rawItem.text === "string") {
                rawItem.text = String(v);
            } else if (typeof rawItem.value === "number") {
                rawItem.value = Number(v) || 0;
            }
        },
        get selectedIndex() {
            if (_vSelectModel(rawItem)) {
                return _vSelected(_vSelectModel(rawItem));
            }
            return typeof rawItem.currentIndex === "number" ? rawItem.currentIndex : -1;
        },
        set selectedIndex(v) {
            var indexOn = _vSelectModel(rawItem);
            if (indexOn) {
                _vPick(indexOn, _vOptions(indexOn)[Number(v)] || null);
                _vChanged(indexOn);
                return;
            }
            if (typeof rawItem.currentIndex === "number") {
                rawItem.currentIndex = Number(v) || 0;
            }
        },
        get checked() {
            if (typeof rawItem.checked === "boolean") {
                return Boolean(rawItem.checked);
            }
            return Boolean(m.explicitChecked);
        },
        set checked(v) {
            m.explicitChecked = Boolean(v);
            if (typeof rawItem.checked === "boolean") {
                rawItem.checked = Boolean(v);
            }
        },
        get disabled() { return rawItem.__eldurInline ? _vGetAttr(rawItem, "disabled") !== null : rawItem.enabled !== undefined ? !rawItem.enabled : Boolean(m.explicitDisabled); },
        set disabled(v) {
            m.explicitDisabled = Boolean(v);
            _reflectFlag(m, "disabled", v);
            if (typeof rawItem.enabled === "boolean") {
                rawItem.enabled = !v;
            }
            // `focus` stays set in Qt, and `activeElement` would go on reading it.
            if (v && rawItem.focus === true) {
                rawItem.focus = false;
            }
        },
        get readOnly() {
            if (typeof rawItem.readOnly === "boolean") {
                return Boolean(rawItem.readOnly);
            }
            return Boolean(m.explicitReadOnly);
        },
        set readOnly(v) {
            m.explicitReadOnly = Boolean(v);
            if (typeof rawItem.readOnly === "boolean") {
                rawItem.readOnly = Boolean(v);
            }
        },
        // The attribute, not whether the element is shown: `display: none` is not `hidden`.
        get hidden() { return hiddenAttribute(rawItem); },
        set hidden(v) {
            m.explicitHidden = Boolean(v);
            _reflectFlag(m, "hidden", v);
            _gTouch();
            if (rawItem.__eldurInline) {
                _vChanged(rawItem.model);
            } else if (typeof rawItem.visible === "boolean") {
                _shownByAttribute(rawItem, Boolean(v));
            }
        },
        get src() {
            var rawSrc = getAttribute(rawItem, "src");
            if (!rawSrc) {
                return "";
            }
            return _resolveUrl(rawSrc);
        },
        set src(v) {
            setAttribute(rawItem, "src", String(v));
        },
        get placeholder() { return typeof rawItem.placeholderText === "string" ? rawItem.placeholderText : ""; },
        set placeholder(v) {
            if (typeof rawItem.placeholderText === "string") {
                rawItem.placeholderText = String(v);
            }
        },
        get name() {
            if (typeof rawItem.name === "string") {
                return rawItem.name;
            }
            return getAttribute(rawItem, "name") || "";
        },
        set name(v) {
            var s = String(v);
            if (typeof rawItem.name === "string") {
                rawItem.name = s;
            }
            setAttribute(rawItem, "name", s);
        },
        get title() { return getAttribute(rawItem, "title") || ""; },
        set title(v) { setAttribute(rawItem, "title", String(v)); },
        get href() {
            var rawHref = getAttribute(rawItem, "href");
            if (rawHref === null) {
                return _inferTagName(rawItem) === "BASE" ? _getDocumentBaseUrl() : "";
            }
            if (_inferTagName(rawItem) === "BASE") {
                var trimmed = String(rawHref).trim();
                var docUrl = _getDocumentUrl();
                return trimmed === "" ? docUrl : _resolveUrlWithBase(trimmed, docUrl);
            }
            return _resolveUrl(rawHref);
        },
        set href(v) { setAttribute(rawItem, "href", String(v)); },
        get alt() { return getAttribute(rawItem, "alt") || ""; },
        set alt(v) { setAttribute(rawItem, "alt", String(v)); },
        get min() {
            var attr = getAttribute(rawItem, "min");
            if (attr !== null) {
                return attr;
            }
            if (typeof rawItem.from === "number" && rawItem.from !== -2147483648) {
                return String(rawItem.from);
            }
            return "";
        },
        set min(v) { setAttribute(rawItem, "min", String(v)); },
        get max() {
            var attr = getAttribute(rawItem, "max");
            if (attr !== null) {
                return attr;
            }
            if (typeof rawItem.to === "number" && rawItem.to !== 2147483647) {
                return String(rawItem.to);
            }
            return "";
        },
        set max(v) { setAttribute(rawItem, "max", String(v)); },
        get step() {
            var attr = getAttribute(rawItem, "step");
            if (attr !== null) {
                return attr;
            }
            if (typeof rawItem.stepSize === "number" && rawItem.stepSize > 0) {
                return String(rawItem.stepSize);
            }
            return "";
        },
        set step(v) { setAttribute(rawItem, "step", String(v)); },
        get htmlFor() { return getAttribute(rawItem, "for") || ""; },
        set htmlFor(v) { setAttribute(rawItem, "for", String(v)); },
        get lang() { return getAttribute(rawItem, "lang") || ""; },
        set lang(v) { setAttribute(rawItem, "lang", String(v)); },
        get width() {
            if (typeof rawItem.width === "number") {
                return rawItem.width;
            }
            return Number(getAttribute(rawItem, "width")) || 0;
        },
        set width(v) {
            var n = Number(v) || 0;
            if (typeof rawItem.width === "number") {
                rawItem.width = n;
            }
            setAttribute(rawItem, "width", String(n));
        },
        get height() {
            if (typeof rawItem.height === "number") {
                return rawItem.height;
            }
            return Number(getAttribute(rawItem, "height")) || 0;
        },
        set height(v) {
            var n = Number(v) || 0;
            if (typeof rawItem.height === "number") {
                rawItem.height = n;
            }
            setAttribute(rawItem, "height", String(n));
        },
        get rows() {
            var tag = _nodeTag(rawItem);
            if (tag === "table") {
                return _liveCollection(function() { return _tableAllRows(rawItem); });
            }
            return _TABLE_SECTION[tag] !== undefined
                ? _liveCollection(function() { return _tablePartsOf(rawItem, "tr"); })
                : undefined;
        },
        get tBodies() {
            return _nodeTag(rawItem) === "table"
                ? _liveCollection(function() { return _tablePartsOf(rawItem, "tbody"); })
                : undefined;
        },
        get tHead() { return _tableFirst(rawItem, "table", "thead"); },
        get tFoot() { return _tableFirst(rawItem, "table", "tfoot"); },
        get caption() { return _tableFirst(rawItem, "table", "caption"); },
        insertRow: function(index) { return _tableInsertRow(rawItem, index === undefined ? -1 : Number(index)); },
        deleteRow: function(index) {
            var tag = _nodeTag(rawItem);
            var list = tag === "table" ? _tableAllRows(rawItem) : _tablePartsOf(rawItem, "tr");
            var at = Number(index);
            if (at === -1 && list.length === 0) {
                return;
            }
            at = at === -1 ? list.length - 1 : at;
            if (!(at >= 0 && at < list.length)) {
                throw new Error("IndexSizeError: no row " + index);
            }
            wrap(list[at]).remove();
        },
        createTHead: function() { return _tableMakePart(rawItem, "thead"); },
        createTFoot: function() { return _tableMakePart(rawItem, "tfoot"); },
        createTBody: function() { return _tableMakePart(rawItem, "tbody"); },
        createCaption: function() { return _tableMakePart(rawItem, "caption"); },
        deleteTHead: function() { _tableDropPart(rawItem, "thead"); },
        deleteTFoot: function() { _tableDropPart(rawItem, "tfoot"); },
        deleteCaption: function() { _tableDropPart(rawItem, "caption"); },
        get cells() {
            return _nodeTag(rawItem) === "tr"
                ? _liveCollection(function() { return _tableCellsOf(rawItem); })
                : undefined;
        },
        get rowIndex() {
            if (_nodeTag(rawItem) !== "tr") {
                return undefined;
            }
            var table = _tableOwner(rawItem);
            return table ? _tableAllRows(table).indexOf(rawItem) : -1;
        },
        get sectionRowIndex() {
            if (_nodeTag(rawItem) !== "tr") {
                return undefined;
            }
            var up = _logicalParent(rawItem);
            return up ? _tablePartsOf(up, "tr").indexOf(rawItem) : -1;
        },
        insertCell: function(index) {
            var list = _tableCellsOf(rawItem);
            var at = index === undefined ? -1 : Number(index);
            if (at < -1 || at > list.length) {
                throw new Error("IndexSizeError: no cell " + index);
            }
            var cell = document.createElement("td");
            if (at === -1 || at === list.length) {
                wrap(rawItem).appendChild(cell);
            } else {
                wrap(rawItem).insertBefore(cell, wrap(list[at]));
            }
            return cell;
        },
        deleteCell: function(index) {
            var list = _tableCellsOf(rawItem);
            var at = Number(index);
            if (at === -1 && list.length === 0) {
                return;
            }
            at = at === -1 ? list.length - 1 : at;
            if (!(at >= 0 && at < list.length)) {
                throw new Error("IndexSizeError: no cell " + index);
            }
            wrap(list[at]).remove();
        },
        get cellIndex() {
            if (!/^t[dh]$/.test(_nodeTag(rawItem))) {
                return undefined;
            }
            var up = _logicalParent(rawItem);
            return up && _nodeTag(up) === "tr" ? _tableCellsOf(up).indexOf(rawItem) : -1;
        },
        get colSpan() { return _tableSpan(rawItem, "colspan"); },
        set colSpan(v) { setAttribute(rawItem, "colspan", String(v)); },
        get rowSpan() { return _tableSpan(rawItem, "rowspan"); },
        set rowSpan(v) { setAttribute(rawItem, "rowspan", String(v)); },
        get className() { return getClassName(rawItem); },
        set className(v) { setClassName(rawItem, v); },
        get classList() { return classList(rawItem); },
        get dataset() { return dataset(rawItem); },
        get offsetWidth() { return _offsetSize(rawItem, "width"); },
        get offsetHeight() { return _offsetSize(rawItem, "height"); },
        get offsetParent() { return _offsetParent(rawItem); },
        get offsetLeft() { return _offsetPlace(rawItem, "left"); },
        get offsetTop() { return _offsetPlace(rawItem, "top"); },
        get clientWidth() { return _clientSize(rawItem, "width"); },
        get clientHeight() { return _clientSize(rawItem, "height"); },
        get scrollTop() {
            if (typeof rawItem.contentY === "number") {
                return rawItem.contentY;
            }
            return m.scrollTop || 0;
        },
        set scrollTop(v) {
            _applyScrollOffset("y", v);
        },
        get scrollLeft() {
            if (typeof rawItem.contentX === "number") {
                return rawItem.contentX;
            }
            return m.scrollLeft || 0;
        },
        set scrollLeft(v) {
            _applyScrollOffset("x", v);
        },
        get scrollWidth() {
            _layoutForRead();
            // The document's scrolling area is at least the viewport.
            return Math.max(_computeScrollDimension("x"), rawItem.eldurTag === "html" || rawItem === _root ? _root.width : 0);
        },
        get scrollHeight() {
            _layoutForRead();
            return Math.max(_computeScrollDimension("y"), rawItem.eldurTag === "html" || rawItem === _root ? _root.height : 0);
        },
        get isConnected() {
            var at = rawItem;
            while (at) {
                if (at === _root) {
                    return true;
                }
                at = _domUp(at);
            }
            return false;
        },
        get parentNode() {
            var p = _logicalParent(rawItem);
            return p ? wrap(p) : null;
        },
        get parentElement() {
            var p = _logicalParent(rawItem);
            return p ? wrap(p) : null;
        },
        get children() {
            if (!m.childCollection) {
                m.childCollection = _liveCollection(function() {
                    return _rawElementChildren(rawItem);
                });
            }
            return m.childCollection;
        },
        get childNodes() {
            return _asNodeList(_rawChildNodes(rawItem).map(wrap));
        },
        get childElementCount() {
            return _rawElementChildren(rawItem).length;
        },
        get firstElementChild() {
            var kids = _rawElementChildren(rawItem);
            return kids.length > 0 ? wrap(kids[0]) : null;
        },
        get lastElementChild() {
            var kids = _rawElementChildren(rawItem);
            return kids.length > 0 ? wrap(kids[kids.length - 1]) : null;
        },
        get firstChild() {
            var kids = _rawChildNodes(rawItem);
            return kids.length > 0 ? wrap(kids[0]) : null;
        },
        get lastChild() {
            var kids = _rawChildNodes(rawItem);
            return kids.length > 0 ? wrap(kids[kids.length - 1]) : null;
        },
        get nextElementSibling() {
            var p = _logicalParent(rawItem);
            if (!p) {
                return null;
            }
            var kids = _rawElementChildren(p);
            var idx = kids.indexOf(rawItem);
            return idx !== -1 && idx + 1 < kids.length ? wrap(kids[idx + 1]) : null;
        },
        get previousElementSibling() {
            var p = _logicalParent(rawItem);
            if (!p) {
                return null;
            }
            var kids = _rawElementChildren(p);
            var idx = kids.indexOf(rawItem);
            return idx > 0 ? wrap(kids[idx - 1]) : null;
        },
        get nextSibling() {
            var nextIn = _logicalParent(rawItem);
            var nextKids = nextIn ? _rawChildNodes(nextIn) : [];
            var nextAt = nextKids.indexOf(rawItem);
            return nextAt !== -1 && nextAt + 1 < nextKids.length ? wrap(nextKids[nextAt + 1]) : null;
        },
        get previousSibling() {
            var prevIn = _logicalParent(rawItem);
            var prevKids = prevIn ? _rawChildNodes(prevIn) : [];
            var prevAt = prevKids.indexOf(rawItem);
            return prevAt > 0 ? wrap(prevKids[prevAt - 1]) : null;
        },
        setAttribute: function(k, v) { setAttribute(rawItem, k, v); },
        getAttribute: function(k) { return getAttribute(rawItem, k); },
        removeAttribute: function(k) { removeAttribute(rawItem, k); },
        hasAttribute: function(k) { return hasAttribute(rawItem, k); },
        toggleAttribute: function(k, force) { return toggleAttribute(rawItem, k, force); },
        addEventListener: function(t, fn, opts) { listen(rawItem, t, fn, opts); },
        removeEventListener: function(t, fn, opts) { unlisten(rawItem, t, fn, opts); },
        dispatchEvent: function(ev) { return dispatchEvent(rawItem, ev); },
        focus: function() { focus(rawItem); },
        blur: function() { blur(rawItem); },
        click: function() {
            if (rawItem.__eldurInline) {
                _vClick(rawItem);
                return;
            }
            _dispatchDynamic(rawItem, "click", null);
        },
        scrollTo: function(xOrOpts, y) {
            if (xOrOpts && typeof xOrOpts === "object") {
                if (typeof xOrOpts.left === "number") { this.scrollLeft = xOrOpts.left; }
                if (typeof xOrOpts.top === "number") { this.scrollTop = xOrOpts.top; }
            } else {
                if (typeof xOrOpts === "number") { this.scrollLeft = xOrOpts; }
                if (typeof y === "number") { this.scrollTop = y; }
            }
        },
        scrollBy: function(xOrOpts, y) {
            if (xOrOpts && typeof xOrOpts === "object") {
                if (typeof xOrOpts.left === "number") { this.scrollLeft = this.scrollLeft + xOrOpts.left; }
                if (typeof xOrOpts.top === "number") { this.scrollTop = this.scrollTop + xOrOpts.top; }
            } else {
                if (typeof xOrOpts === "number") { this.scrollLeft = this.scrollLeft + xOrOpts; }
                if (typeof y === "number") { this.scrollTop = this.scrollTop + y; }
            }
        },
        scrollIntoView: function(arg) {
            var scroller = _findRootScroller();
            if (!scroller) {
                return;
            }
            var rect = getBoundingClientRect(rawItem);
            var curX = window.scrollX;
            var curY = window.scrollY;
            var vw = (typeof scroller.width === "number" && scroller.width > 0) ? scroller.width : window.innerWidth;
            var vh = (typeof scroller.height === "number" && scroller.height > 0) ? scroller.height : window.innerHeight;
            var block = "start";
            var inline = "nearest";
            if (arg === false) {
                block = "end";
            } else if (arg && typeof arg === "object") {
                if (typeof arg.block === "string") { block = arg.block; }
                if (typeof arg.inline === "string") { inline = arg.inline; }
            }
            var nextX = curX;
            if (inline === "start") {
                nextX = curX + rect.left;
            } else if (inline === "end") {
                nextX = curX + rect.right - vw;
            } else if (inline === "center") {
                nextX = curX + rect.left - (vw - rect.width) / 2;
            } else {
                if (rect.left < 0) {
                    nextX = curX + rect.left;
                } else if (vw > 0 && rect.right > vw) {
                    nextX = curX + rect.right - vw;
                }
            }
            var nextY = curY;
            if (block === "end") {
                nextY = curY + rect.bottom - vh;
            } else if (block === "center") {
                nextY = curY + rect.top - (vh - rect.height) / 2;
            } else if (block === "nearest") {
                if (rect.top < 0) {
                    nextY = curY + rect.top;
                } else if (vh > 0 && rect.bottom > vh) {
                    nextY = curY + rect.bottom - vh;
                }
            } else {
                nextY = curY + rect.top;
            }
            scrollTo(nextX, nextY);
        },
        getBoundingClientRect: function() { return getBoundingClientRect(rawItem); },
        matches: function(sel) {
            return _matchesSelectorComplex(rawItem, String(sel));
        },
        closest: function(sel) {
            var s = String(sel);
            var cur = rawItem;
            while (cur) {
                if (_matchesSelectorComplex(cur, s)) {
                    return wrap(cur);
                }
                cur = _logicalParent(cur);
            }
            return null;
        },
        contains: function(other) {
            var cur = _unwrap(other);
            while (cur) {
                if (cur === rawItem) {
                    return true;
                }
                cur = _domUp(cur);
            }
            return false;
        },
        appendChild: function(child) {
            var rawNodes = _expandInsertRawNodes([child]);
            if (_vTryInsert(rawItem, rawNodes, null)) {
                return child;
            }
            _insertChildren(rawItem, rawNodes, null);
            return child;
        },
        removeChild: function(child) {
            var rawChild = _unwrap(child);
            if (rawChild && rawChild.__eldurInline) {
                _vRemove(rawChild);
            } else if (rawChild && rawChild.parent !== undefined) {
                _detachNode(rawChild);
            }
            return child;
        },
        remove: function(n) {
            var removeFrom = _vSelectModel(rawItem);
            if (removeFrom && typeof n === "number") {
                if (_vOptions(removeFrom)[n]) {
                    _vRemove(_vOptions(removeFrom)[n]);
                }
                return;
            }
            if (rawItem.__eldurInline) {
                _vRemove(rawItem);
            } else if (rawItem.parent !== undefined) {
                _detachNode(rawItem);
            }
        },
        insertBefore: function(newChild, refChild) {
            var rawNodes = _expandInsertRawNodes([newChild]);
            var rawRef = _unwrap(refChild);
            if (_vTryInsert(rawItem, rawNodes, rawRef || null)) {
                return newChild;
            }
            _insertChildren(rawItem, rawNodes, rawRef);
            return newChild;
        },
        replaceChild: function(newChild, oldChild) {
            var rawOld = _unwrap(oldChild);
            if (!rawOld) {
                return oldChild;
            }
            var rawNodes = _expandInsertRawNodes([newChild]);
            if (_vTryInsert(rawItem, rawNodes, rawOld)) {
                _vRemove(rawOld);
                return oldChild;
            }
            _insertChildren(rawItem, rawNodes, rawOld);
            if (rawOld.parent !== undefined) {
                _detachNode(rawOld);
            }
            return oldChild;
        },
        append: function() {
            var rawNodes = _expandInsertRawNodes(arguments);
            if (_vTryInsert(rawItem, rawNodes, null)) {
                return;
            }
            _insertChildren(rawItem, rawNodes, null);
        },
        prepend: function() {
            var rawNodes = _expandInsertRawNodes(arguments);
            var vFirst = _vHostOf(rawItem, rawNodes, null);
            if (vFirst) {
                _vInsert(vFirst, rawNodes, vFirst.kids.filter(_vIsRendered)[0] || null);
                return;
            }
            var kids = _rawElementChildren(rawItem);
            var first = kids.length > 0 ? kids[0] : null;
            _insertChildren(rawItem, rawNodes, first);
        },
        before: function() {
            if (rawItem.__eldurInline) {
                _vInsert(rawItem.vparent || rawItem.model, _expandInsertRawNodes(arguments), rawItem);
                return;
            }
            var p = _logicalParent(rawItem);
            if (!p) {
                return;
            }
            var rawNodes = _expandInsertRawNodes(arguments);
            _insertChildren(p, rawNodes, rawItem);
        },
        after: function() {
            if (rawItem.__eldurInline) {
                var afterIn = rawItem.vparent || rawItem.model;
                _vInsert(afterIn, _expandInsertRawNodes(arguments), afterIn.kids[afterIn.kids.indexOf(rawItem) + 1] || null);
                return;
            }
            var p = _logicalParent(rawItem);
            if (!p) {
                return;
            }
            var kids = _rawElementChildren(p);
            var idx = kids.indexOf(rawItem);
            var nextRef = (idx !== -1 && idx + 1 < kids.length) ? kids[idx + 1] : null;
            var rawNodes = _expandInsertRawNodes(arguments);
            _insertChildren(p, rawNodes, nextRef);
        },
        replaceWith: function() {
            if (rawItem.__eldurInline) {
                _vInsert(rawItem.vparent || rawItem.model, _expandInsertRawNodes(arguments), rawItem);
                _vRemove(rawItem);
                return;
            }
            var p = _logicalParent(rawItem);
            if (!p) {
                return;
            }
            var rawNodes = _expandInsertRawNodes(arguments);
            _insertChildren(p, rawNodes, rawItem);
            if (rawItem.parent !== undefined) {
                _detachNode(rawItem);
            }
        },
        replaceChildren: function() {
            var vAll = rawItem.__eldurInline ? rawItem : _vExisting(rawItem);
            if (vAll) {
                _vDetachKids(vAll);
                _vInsert(vAll, _expandInsertRawNodes(arguments), null);
                return;
            }
            var existing = _rawElementChildren(rawItem);
            for (var i = 0; i < existing.length; ++i) {
                if (existing[i].parent !== undefined) {
                    _detachNode(existing[i]);
                }
            }
            _wsEnd(rawItem).length = 0;
            var rawNodes = _expandInsertRawNodes(arguments);
            _insertChildren(rawItem, rawNodes, null);
        },
        cloneNode: function(deep) {
            if (rawItem.__eldurInline) {
                return wrap(_vClone(rawItem, Boolean(deep)));
            }
            if (m.isFragment) {
                var whole = document.createDocumentFragment();
                _rawElementChildren(rawItem).forEach(function(kid) {
                    whole.appendChild(wrap(kid).cloneNode(true));
                });
                return whole;
            }
            var tag = _inferTagName(rawItem).toLowerCase() || "div";
            var clone = document.createElement(tag);
            if (m.inputType) {
                clone.type = m.inputType;
            }
            if (this.id) {
                clone.id = this.id;
            }
            if (m.classes.length > 0) {
                clone.className = m.classes.join(" ");
            }
            var attrKeys = Object.keys(m.attrs);
            for (var a = 0; a < attrKeys.length; ++a) {
                clone.setAttribute(attrKeys[a], m.attrs[attrKeys[a]]);
            }
            var dsKeys = Object.keys(m.dataset);
            for (var d = 0; d < dsKeys.length; ++d) {
                clone.dataset[dsKeys[d]] = m.dataset[dsKeys[d]];
            }
            var stKeys = Object.keys(m.styles);
            for (var s = 0; s < stKeys.length; ++s) {
                setStyle(clone, stKeys[s], m.styles[stKeys[s]]);
            }
            if (Boolean(deep)) {
                var kids = _rawElementChildren(rawItem);
                if (kids.length > 0) {
                    for (var c = 0; c < kids.length; ++c) {
                        var copy = wrap(kids[c]).cloneNode(true);
                        _wsGiven(_unwrap(copy), "ws", _wsText(_wsBefore(kids[c])));
                        clone.appendChild(copy);
                    }
                    _wsGiven(_unwrap(clone), "wsEnd", _wsText(_wsEnd(rawItem)));
                } else {
                    var txt = getText(rawItem);
                    if (txt) {
                        clone.textContent = txt;
                    }
                }
            }
            return clone;
        },
        insertAdjacentElement: function(position, element) {
            var pos = String(position).toLowerCase();
            var w = wrap(_unwrap(element));
            if (!w) {
                return null;
            }
            if (pos === "beforebegin") {
                this.before(w);
            } else if (pos === "afterbegin") {
                this.prepend(w);
            } else if (pos === "beforeend") {
                this.append(w);
            } else if (pos === "afterend") {
                this.after(w);
            } else {
                return null;
            }
            return w;
        },
        insertAdjacentText: function(position, text) {
            var tn = document.createTextNode(String(text === undefined ? "" : text));
            this.insertAdjacentElement(position, tn);
        },
        insertAdjacentHTML: function(position, html) {
            var pos = String(position).toLowerCase();
            var parsed = _parseHtmlFragmentNodes(html);
            if (parsed.length === 0) {
                return;
            }
            if (pos === "beforebegin") {
                this.before.apply(this, parsed);
            } else if (pos === "afterbegin") {
                this.prepend.apply(this, parsed);
            } else if (pos === "beforeend") {
                this.append.apply(this, parsed);
            } else if (pos === "afterend") {
                this.after.apply(this, parsed);
            }
        },
        querySelector: function(sel) {
            var matches = _queryTree(rawItem, String(sel), true);
            return matches.length > 0 ? wrap(matches[0]) : null;
        },
        querySelectorAll: function(sel) {
            return _asNodeList(_queryTree(rawItem, String(sel), false).map(wrap));
        },
        getElementsByClassName: function(classNames) {
            return _byClassNames(rawItem, classNames);
        },
        getElementsByTagName: function(tagName) {
            return _byTagName(rawItem, tagName);
        }
    };

    m.wrapper = wrapper;
    return wrapper;
}

// Inline nodes (docs/spec/dom-api.md). A run of inline content is one `Text`,
// so its elements have no QML object. A script gets these instead: objects
// that quack as QObjects for `wrap` and `_meta`, built from the model the
// converter declares in `eldurInline`, which then own the block's text.
var _vRules = null;
var _vModels = [];
var _vGen = 0;
var _vSyncing = false;
var _vVoid = { area: 1, base: 1, br: 1, col: 1, embed: 1, hr: 1, img: 1, input: 1, link: 1, meta: 1, source: 1, track: 1, wbr: 1 };
var _vProps = ["color", "font-weight", "font-style", "font-size", "font-family", "display", "visibility", "background-color", "text-decoration-line"];
var _vDefault = { color: "rgb(0, 0, 0)", weight: 400, italic: false, underline: false, strike: false, size: 16, family: "", bg: "rgba(0, 0, 0, 0)", display: "inline", visible: true };

function _vHasRules() {
    return Boolean(_root) && _root.eldurInlineRules !== undefined;
}

var _VProto = {
    __eldurInline: true,
    objectNameChanged: function() {},
    get parent() {
        return this.vparent || (this.model ? _vHostElement(this.model) : null);
    },
    get children() {
        return this.kids ? this.kids.filter(_vIsElement) : [];
    },
    get width() { return _vRect(this).width; },
    get height() { return _vRect(this).height; },
    get checked() { return this.eldurTag === "option" ? this.sel === true : undefined; }
};

function _vIsElement(k) {
    return Boolean(k.kids);
}

function _vIsRendered(k) {
    return !k.gen;
}

function _vLeaf(text, parent, model) {
    var leaf = Object.create(_VProto);
    leaf.eldurTag = "#text";
    leaf.objectName = "";
    leaf.vtext = true;
    leaf.data = String(text);
    leaf.vparent = parent;
    leaf.model = model;
    return leaf;
}

function _vElement(tag, attrs, parent, model) {
    var v = Object.create(_VProto);
    v.eldurTag = tag;
    v.objectName = "";
    v.eldurClasses = "";
    v.eldurAttrs = {};
    v.attrOrder = [];
    v.vparent = parent;
    v.model = model;
    v.kids = [];
    var styleAttr = null;
    for (var i = 0; i < attrs.length; ++i) {
        var name = attrs[i][0], value = attrs[i][1];
        v.attrOrder.push(name);
        if (name === "id") {
            v.objectName = value;
        } else if (name === "class") {
            v.eldurClasses = value;
        } else if (name === "style") {
            styleAttr = value;
        } else {
            v.eldurAttrs[name] = value;
        }
    }
    var m = _meta(v);
    m.raw = v;
    if (styleAttr !== null) {
        _vParseCss(m, styleAttr);
    }
    if (m.attrs.hidden !== undefined) {
        m.explicitHidden = true;
    }
    if (tag === "option") {
        v.sel = m.attrs.selected !== undefined;
    }
    return v;
}

function _vBuild(json, parent, model) {
    if (json.gen) {
        return { gen: json.gen };
    }
    if (json.text !== undefined) {
        return _vLeaf(json.text, parent, model);
    }
    var v = _vElement(json.tag, json.attrs, parent, model);
    v.eldurKey = json.key;
    v.c = json.c || null;
    v.before = json.before || null;
    v.after = json.after || null;
    v.kids = json.kids.map(function(k) { return _vBuild(k, v, model); });
    return v;
}

function _vEachElement(kids, fn) {
    for (var i = 0; i < kids.length; ++i) {
        if (kids[i].kids) {
            fn(kids[i]);
            _vEachElement(kids[i].kids, fn);
        }
    }
}

// The model of `item`, once a `Text` or `ComboBox` has one.
function _vModelOf(item) {
    if (!item || item.__eldurInline || item.__eldurWrapper) {
        return null;
    }
    var known = _itemMeta.get(item);
    if (known && known.vModel !== undefined) {
        return known.vModel;
    }
    if (typeof item.eldurInline !== "string") {
        return null;
    }
    var m = _meta(item);
    var model = JSON.parse(item.eldurInline);
    model.text = item;
    m.vModel = model;
    model.kids = model.kids.map(function(k) { return _vBuild(k, null, model); });
    _vModels.push(model);
    if (model.select) {
        _vEnsureSelected(model);
        item.currentIndexChanged.connect(function() {
            if (!_vSyncing) {
                _vPick(model, _vOptions(model)[item.currentIndex] || null);
            }
        });
    } else {
        // What the converter worked out holds until the rules matching a node
        // change.
        _vEachElement(model.kids, function(v) { v.sig0 = _vSig(v); });
        // A link takes the press, so nothing above hears of it otherwise.
        if (item.eldurInline.indexOf("\"tag\":\"a\"") !== -1 && _vHasRules()) {
            _vTapHandler(item);
        }
    }
    return model;
}

// A `Text` the converter saw no elements in, the first time a script gives
// it one or asks for its text node.
function _vEnsureModel(text) {
    var known = _vModelOf(text);
    if (known) {
        return known;
    }
    var model = {
        text: text,
        kids: [],
        k: _vLiveBase(text),
        hoisted: false,
        bare: false,
        html: false,
        pre: false,
        underline: Boolean(text.font && text.font.underline),
        strike: Boolean(text.font && text.font.strikeout)
    };
    var initial = _meta(text).explicitText !== undefined
        ? _meta(text).explicitText
        : (text.textFormat === 0 ? String(text.text) : _unescapeHtmlText(String(text.text)));
    if (initial !== "") {
        model.kids.push(_vLeaf(initial, null, model));
    }
    _meta(text).vModel = model;
    _vModels.push(model);
    return model;
}

function _vIsTagged(item) {
    return (typeof item.eldurTag === "string" && item.eldurTag !== "")
        || (typeof item.objectName === "string" && item.objectName !== "");
}

function _vHostElement(model) {
    var text = model.text;
    return model.select || _vIsTagged(text) ? text : _logicalParent(text);
}

// The model whose top-level nodes are `raw`'s children, when all of them are.
function _vOwnModel(raw) {
    var model = _vModelOf(raw);
    if (model) {
        return model.select || _vIsTagged(raw) ? model : null;
    }
    if (_vHasRules() && _isPlainTextItem(raw) && _vIsTagged(raw) && !_meta(raw).createdTag
        && !_meta(raw).vSplit) {
        return _vEnsureModel(raw);
    }
    return null;
}

function _vAnonText(item) {
    return item && !_vIsTagged(item) && !_meta(item).createdTag && !_meta(item).isFragment
        && typeof item.eldurInline === "string" && _isPlainTextItem(item);
}

// A box's only text, unless it also lays out other children.
function _vBoxModel(raw) {
    if (!_vHasRules() || raw.__eldurInline || typeof raw.text === "string"
        || typeof raw.spacing === "number" || typeof raw.columnSpacing === "number") {
        return null;
    }
    var text = _findTextChild(raw);
    if (!text || _meta(raw).flow || _meta(text).vSplit) {
        return null;
    }
    return _vEnsureModel(text);
}

// What holds `raw`'s inline children, for writes: a node, a model, or nothing
// when `raw` lays out real items.
function _vContainerOf(raw) {
    if (raw.__eldurInline) {
        return raw;
    }
    return _vOwnModel(raw) || _vBoxModel(raw);
}

function _vLeaves(kids, out) {
    for (var i = 0; i < kids.length; ++i) {
        if (kids[i].vtext) {
            out.push(kids[i].data);
        } else if (kids[i].kids) {
            _vLeaves(kids[i].kids, out);
        }
    }
    return out;
}

function _vContainerKids(c) {
    return c.kids;
}

function _vText(c) {
    return c.vtext ? c.data : _vLeaves(c.kids, []).join("");
}

function _vDetachKids(c) {
    var old = c.kids.slice();
    c.kids = c.kids.filter(function(k) { return k.gen; });
    for (var i = 0; i < old.length; ++i) {
        if (!old[i].gen) {
            _vSetModel(old[i], null);
            old[i].vparent = null;
        }
    }
}

function _vSetModel(node, model) {
    node.model = model;
    if (node.kids) {
        for (var i = 0; i < node.kids.length; ++i) {
            if (!node.kids[i].gen) {
                _vSetModel(node.kids[i], model);
            }
        }
    }
}

function _vModelFor(c) {
    return c.__eldurInline ? c.model : c;
}

function _vSetText(c, value) {
    var str = value === null || value === undefined ? "" : String(value);
    if (c.vtext) {
        c.data = str;
    } else {
        _vDetachKids(c);
        if (str !== "") {
            c.kids.push(_vLeaf(str, c.__eldurInline ? c : null, _vModelFor(c)));
        }
    }
    _vChanged(_vModelFor(c));
}

// Enough of HTML for what a script writes inside a line of text.
function _vParseInto(c, html) {
    var parent = c.__eldurInline ? c : null;
    var model = _vModelFor(c);
    var stack = [{ node: parent, list: c.kids }];
    var re = /<(\/?)([a-zA-Z][a-zA-Z0-9-]*)((?:\s+[^\s=>\/]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?)*)\s*(\/?)>/g;
    var pos = 0, hit;
    function addText(s) {
        if (s !== "") {
            var top = stack[stack.length - 1];
            top.list.push(_vLeaf(_unescapeHtmlText(s), top.node, model));
        }
    }
    while ((hit = re.exec(html)) !== null) {
        addText(html.slice(pos, hit.index));
        pos = hit.index + hit[0].length;
        var tag = hit[2].toLowerCase();
        if (hit[1]) {
            for (var d = stack.length - 1; d > 0; --d) {
                if (stack[d].node.eldurTag === tag) {
                    stack.length = d;
                    break;
                }
            }
            continue;
        }
        var attrs = [];
        var attrRe = /([^\s=>\/]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?/g, a;
        while ((a = attrRe.exec(hit[3])) !== null) {
            var val = a[2] !== undefined ? a[2] : a[3] !== undefined ? a[3] : a[4] !== undefined ? a[4] : "";
            attrs.push([a[1].toLowerCase(), _unescapeHtmlText(val)]);
        }
        var top = stack[stack.length - 1];
        var v = _vElement(tag, attrs, top.node, model);
        top.list.push(v);
        if (!_vVoid[tag] && !hit[4]) {
            stack.push({ node: v, list: v.kids });
        }
    }
    addText(html.slice(pos));
}

function _vSetInnerHTML(c, html) {
    _vDetachKids(c);
    _vParseInto(c, String(html === null || html === undefined ? "" : html));
    var model = _vModelFor(c);
    if (model && model.select) {
        _vEnsureSelected(model);
    }
    _vChanged(model);
}

function _vEscapeText(s) {
    return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function _vAttrNames(v) {
    var m = _meta(v);
    var names = v.attrOrder.slice();
    function need(name) {
        if (names.indexOf(name) === -1) {
            names.push(name);
        }
    }
    if (v.objectName !== "") {
        need("id");
    }
    if (m.classes.length > 0) {
        need("class");
    }
    if (Object.keys(m.styles).length > 0) {
        need("style");
    }
    Object.keys(m.attrs).forEach(need);
    return names;
}

function _vGetAttr(v, name) {
    var m = _meta(v);
    var key = String(name).toLowerCase();
    if (_vAttrNames(v).indexOf(key) === -1) {
        return null;
    }
    if (key === "id") {
        return v.objectName;
    }
    if (key === "class") {
        return m.classes.join(" ");
    }
    if (key === "style") {
        return _vCssText(m);
    }
    return m.attrs[key] !== undefined ? String(m.attrs[key]) : "";
}

function _vSetAttr(v, name, value) {
    var m = _meta(v);
    var key = String(name).toLowerCase();
    var str = String(value);
    if (v.attrOrder.indexOf(key) === -1) {
        v.attrOrder.push(key);
    }
    if (key === "id") {
        v.objectName = str;
    } else if (key === "class") {
        setClassName(v, str);
        return;
    } else if (key === "style") {
        m.styles = {};
        _vParseCss(m, str);
    } else {
        m.attrs[key] = str;
        if (key.indexOf("data-") === 0) {
            m.dataset[_kebabToCamel(key.slice(5))] = str;
        }
        if (key === "hidden") {
            m.explicitHidden = true;
        }
    }
    if (v.eldurTag === "option" && key === "selected") {
        v.sel = true;
    }
    _vAttrChanged(v);
}

function _vRemoveAttr(v, name) {
    var m = _meta(v);
    var key = String(name).toLowerCase();
    var at = v.attrOrder.indexOf(key);
    if (at !== -1) {
        v.attrOrder.splice(at, 1);
    }
    if (key === "id") {
        v.objectName = "";
    } else if (key === "class") {
        m.classes.length = 0;
    } else if (key === "style") {
        m.styles = {};
    } else {
        delete m.attrs[key];
        if (key.indexOf("data-") === 0) {
            delete m.dataset[_kebabToCamel(key.slice(5))];
        }
        if (key === "hidden") {
            m.explicitHidden = false;
        }
    }
    _vAttrChanged(v);
}

function _vAttrChanged(v) {
    _vChanged(v.model);
}

function _vSerialize(v) {
    if (v.vtext) {
        return _vEscapeText(v.data);
    }
    var out = "<" + v.eldurTag;
    var names = _vAttrNames(v);
    for (var i = 0; i < names.length; ++i) {
        var value = _vGetAttr(v, names[i]);
        out += " " + names[i] + "=\"" + String(value).replace(/&/g, "&amp;").replace(/"/g, "&quot;") + "\"";
    }
    out += ">";
    if (_vVoid[v.eldurTag]) {
        return out;
    }
    return out + _vInner(v) + "</" + v.eldurTag + ">";
}

function _vInner(c) {
    return c.kids.filter(_vIsRendered).map(_vSerialize).join("");
}

function _vKebab(name) {
    return String(name).replace(/[A-Z]/g, function(ch) { return "-" + ch.toLowerCase(); });
}

function _vParseCss(m, css) {
    var parts = String(css).split(";");
    for (var i = 0; i < parts.length; ++i) {
        var at = parts[i].indexOf(":");
        if (at > 0) {
            var name = parts[i].slice(0, at).trim();
            var value = parts[i].slice(at + 1).trim();
            if (value !== "") {
                m.styles[name.indexOf("--") === 0 ? name : _kebabToCamel(name)] = value;
            }
        }
    }
}

function _vCssText(m) {
    return Object.keys(m.styles).map(function(k) {
        return _vKebab(k) + ": " + m.styles[k] + ";";
    }).join(" ");
}

function _vSetStyle(v, prop, value) {
    var m = _meta(v);
    var name = String(prop);
    var str = value === null || value === undefined ? "" : String(value);
    if (name === "cssText") {
        m.styles = {};
        _vParseCss(m, str);
    } else {
        var camel = name.indexOf("--") === 0 ? name : _kebabToCamel(name);
        if (str === "") {
            delete m.styles[camel];
        } else {
            m.styles[camel] = str;
        }
    }
    _vChanged(v.model);
}

function _vGetStyle(v, prop) {
    var m = _meta(v);
    var name = String(prop);
    if (name === "cssText") {
        return _vCssText(m);
    }
    var value = m.styles[name.indexOf("--") === 0 ? name : _kebabToCamel(name)];
    return value === undefined ? "" : String(value);
}

// The rule table, without what nothing here reads and with what spares most
// selector matches.
function _vLoadRules() {
    if (_vRules) {
        return _vRules;
    }
    _vRules = _vReadRules(_vHasRules() && _root.eldurInlineRules !== "" ? _root.eldurInlineRules : "[]");
    return _vRules;
}

function _vReadRules(text) {
    var out = [];
    var table = JSON.parse(text);
    for (var i = 0; i < table.length; ++i) {
        var decls = table[i].decls;
        if (decls.length > 0) {
            var last = /([.#]?)([\w-]+)$/.exec(table[i].sel);
            // `a:hover` ends in no element's name.
            if (last && table[i].sel[last.index - 1] === ":") {
                last = null;
            }
            out.push({
                sel: table[i].sel,
                rank: table[i].spec[0] * 1000000 + table[i].spec[1] * 1000 + table[i].spec[2],
                ua: table[i].ua,
                layer: table[i].layer,
                paintsInline: table[i].sel.indexOf("::") === -1
                    && decls.some(function(d) { return _vProps.indexOf(d[0]) !== -1; }),
                decls: decls,
                lastKind: last ? last[1] : null,
                lastName: last ? last[2] : null,
                list: undefined
            });
        }
    }
    return out;
}

function _vRuleMatches(rule, v, m) {
    return _vRuleMatchesFor(rule, v, m, v.eldurTag, v.objectName);
}

function _vRuleMatchesFor(rule, v, m, tag, id) {
    if (rule.lastKind === "") {
        if (rule.lastName !== tag) {
            return false;
        }
    } else if (rule.lastKind === ".") {
        if (m.classes.indexOf(rule.lastName) === -1) {
            return false;
        }
    } else if (rule.lastKind === "#") {
        if (rule.lastName !== id) {
            return false;
        }
    }
    if (rule.list === undefined) {
        try {
            rule.list = _parseSelector(rule.sel);
        } catch (e) {
            rule.list = null;
        }
    }
    if (rule.list === null) {
        return false;
    }
    try {
        return _matchesList(v, rule.list);
    } catch (e2) {
        return false;
    }
}

function _vMatched(v) {
    if (v.matchGen === _vGen) {
        return v.matched;
    }
    var rules = _vLoadRules();
    var m = _meta(v);
    var out = [];
    for (var i = 0; i < rules.length; ++i) {
        if (rules[i].paintsInline && _vRuleMatches(rules[i], v, m)) {
            out.push(i);
        }
    }
    v.matched = out;
    v.matchGen = _vGen;
    return out;
}

function _vSig(v) {
    var m = _meta(v);
    return _vMatched(v).join(",") + "|" + _vCssText(m) + "|" + Boolean(m.explicitHidden);
}

function _vColorParts(c) {
    return "rgb(" + Math.round(c.r * 255) + ", " + Math.round(c.g * 255) + ", " + Math.round(c.b * 255) + ")";
}

function _vLiveColor(c) {
    if (c === undefined || c === null) {
        return "rgb(0, 0, 0)";
    }
    if (c.a !== undefined && c.a < 1) {
        var alpha = Math.round(c.a * 1000) / 1000;
        return "rgba(" + Math.round(c.r * 255) + ", " + Math.round(c.g * 255) + ", " + Math.round(c.b * 255) + ", " + alpha + ")";
    }
    return _vColorParts(c);
}

function _vLiveBase(text) {
    var f = text.font || {};
    return {
        color: _vLiveColor(text.color),
        weight: typeof f.weight === "number" ? f.weight : 400,
        italic: Boolean(f.italic),
        underline: Boolean(f.underline),
        strike: Boolean(f.strikeout),
        size: typeof f.pixelSize === "number" && f.pixelSize > 0 ? f.pixelSize : 16,
        family: typeof f.family === "string" ? f.family : "",
        bg: "rgba(0, 0, 0, 0)",
        display: "block",
        visible: true
    };
}

// What the top of a line inherits: the block as it is now, since a class on
// the block restyles the `Text`; a `Text` showing one element's style has lost
// the block's.
function _vBase(model) {
    if (!model || model.select) {
        return _vDefault;
    }
    if (model.hoisted) {
        return model.k;
    }
    var live = _vLiveBase(model.text);
    live.family = model.k.family;
    live.underline = model.k.underline;
    live.strike = model.k.strike;
    return live;
}

function _vColor(value, current) {
    var v = String(value).trim().toLowerCase();
    if (v === "currentcolor") {
        return current;
    }
    if (v === "transparent") {
        return "rgba(0, 0, 0, 0)";
    }
    var fn = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,\/]+([\d.]+%?))?\s*\)$/.exec(v);
    if (fn) {
        var alpha = fn[4] === undefined ? 1 : fn[4].slice(-1) === "%" ? parseFloat(fn[4]) / 100 : parseFloat(fn[4]);
        return alpha >= 1
            ? "rgb(" + Math.round(fn[1]) + ", " + Math.round(fn[2]) + ", " + Math.round(fn[3]) + ")"
            : "rgba(" + Math.round(fn[1]) + ", " + Math.round(fn[2]) + ", " + Math.round(fn[3]) + ", " + alpha + ")";
    }
    if (!/^(#[0-9a-f]{3,8}|[a-z]+)$/.test(v) || !Qt.colorEqual) {
        return current;
    }
    var hex = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/.exec(v);
    if (hex) {
        var h = hex[1];
        if (h.length <= 4) {
            h = h.replace(/./g, "$&$&");
        }
        var rgb = [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
        return h.length === 8 && h.slice(6) !== "ff"
            ? "rgba(" + rgb.join(", ") + ", " + Math.round(parseInt(h.slice(6), 16) / 255 * 1000) / 1000 + ")"
            : "rgb(" + rgb.join(", ") + ")";
    }
    var named = Qt.color(v);
    return named ? _vLiveColor(named) : current;
}

function _vApply(out, prop, value, pc) {
    var v = String(value).trim();
    var low = v.toLowerCase();
    var inherits = prop !== "display" && prop !== "background-color" && prop !== "text-decoration-line";
    if (low === "inherit" || ((low === "unset" || low === "revert") && inherits)) {
        low = v = "\u0000inherit";
    } else if (low === "initial" || low === "unset" || low === "revert") {
        low = v = "\u0000initial";
    }
    var key = { color: "color", "font-weight": "weight", "font-style": "italic", "font-size": "size",
        "font-family": "family", display: "display", visibility: "visible", "background-color": "bg" }[prop];
    if (prop === "text-decoration-line") {
        if (low === "\u0000inherit") {
            out.underline = pc.underline;
            out.strike = pc.strike;
        } else {
            out.underline = low.indexOf("underline") !== -1;
            out.strike = low.indexOf("line-through") !== -1;
        }
        return;
    }
    if (low === "\u0000inherit") {
        out[key] = pc[key];
    } else if (low === "\u0000initial") {
        out[key] = _vDefault[key];
    } else if (prop === "color") {
        out.color = _vColor(v, out.color);
    } else if (prop === "background-color") {
        out.bg = _vColor(v, out.bg);
    } else if (prop === "font-weight") {
        out.weight = low === "bold" ? 700 : low === "normal" ? 400
            : low === "bolder" ? (pc.weight >= 600 ? 900 : pc.weight >= 350 ? 700 : 400)
            : low === "lighter" ? (pc.weight >= 800 ? 700 : pc.weight >= 600 ? 400 : 100)
            : (Number(low) || out.weight);
    } else if (prop === "font-style") {
        out.italic = low === "italic" || low.indexOf("oblique") === 0;
    } else if (prop === "font-size") {
        var size = /^([\d.]+)(px|em|rem|%|pt)$/.exec(low);
        if (size) {
            var n = parseFloat(size[1]);
            out.size = size[2] === "px" ? n : size[2] === "em" ? n * pc.size : size[2] === "rem" ? n * 16
                : size[2] === "%" ? n * pc.size / 100 : n * 4 / 3;
        } else {
            out.size = low === "smaller" ? pc.size / 1.2 : low === "larger" ? pc.size * 1.2 : out.size;
        }
    } else if (prop === "font-family") {
        out.family = v.split(",")[0].trim().replace(/^["']|["']$/g, "");
    } else if (prop === "display") {
        out.display = low;
    } else if (prop === "visibility") {
        out.visible = low === "visible";
    }
}

// origin, then cascade layer, then specificity.
var _V_ORIGIN_BAND = 1000000000000;
var _V_LAYER_BAND = 10000000000;

// An inline node's variables are its nearest box's, plus what its own rules say.
function _vVars(v) {
    var box = v.model && v.model.text ? _logicalParent(v.model.text) : null;
    return box && !box.__eldurInline ? _gStyle(box).vars : {};
}

function _vCascade(v, pc) {
    var rules = _vLoadRules();
    var matched = _vMatched(v);
    var won = {};
    function offer(prop, value, weight) {
        if (!won[prop] || weight >= won[prop].weight) {
            won[prop] = { value: value, weight: weight };
        }
    }
    var band = _V_ORIGIN_BAND;
    var vars = null;
    for (var i = 0; i < matched.length; ++i) {
        var rule = rules[matched[i]];
        for (var d = 0; d < rule.decls.length; ++d) {
            var decl = rule.decls[d];
            var written = decl[1];
            if (written.indexOf("var(") !== -1) {
                vars = vars || _vVars(v);
                written = _gSubstitute(written, vars, []);
                if (written === null) {
                    continue;
                }
            }
            offer(decl[0], written, _gWeight(rule, decl[2]));
        }
    }
    var m = _meta(v);
    var keys = Object.keys(m.styles);
    for (var s = 0; s < keys.length; ++s) {
        var prop = _vKebab(keys[s]);
        var raw = String(m.styles[keys[s]]);
        var important = /!\s*important\s*$/i.test(raw);
        var value = raw.replace(/!\s*important\s*$/i, "");
        if (prop === "text-decoration") {
            prop = "text-decoration-line";
        }
        if (_vProps.indexOf(prop) !== -1) {
            offer(prop, value, (important ? 3.5 : 2) * band);
        }
    }
    var out = {
        color: pc.color, weight: pc.weight, italic: pc.italic, size: pc.size, family: pc.family,
        visible: pc.visible, underline: false, strike: false, bg: "rgba(0, 0, 0, 0)", display: "inline"
    };
    if (m.explicitHidden && !won.display) {
        out.display = "none";
    }
    for (var w = 0; w < _vProps.length; ++w) {
        if (won[_vProps[w]]) {
            _vApply(out, _vProps[w], won[_vProps[w]].value, pc);
        }
    }
    return out;
}

function _vStyle(v) {
    if (v.styleGen === _vGen && v.style) {
        return v.style;
    }
    var parent = v.vparent;
    var pc = parent ? _vStyle(parent) : _vBase(v.model);
    var clean = Boolean(v.c) && v.sig0 !== undefined && _vSig(v) === v.sig0;
    if (clean && parent) {
        clean = parent.cleanNow;
    } else if (clean) {
        var k = v.model && v.model.k;
        clean = !k || (pc.color === k.color && pc.weight === k.weight && pc.italic === k.italic);
    }
    v.cleanNow = clean;
    v.style = clean ? v.c : _vCascade(v, pc);
    v.styleGen = _vGen;
    return v.style;
}

function _vComputedStyle(v) {
    function read(prop) {
        var c = _vStyle(v);
        switch (_kebabToCamel(String(prop))) {
        case "color": return c.color;
        case "textDecorationLine":
            return (c.underline ? ["underline"] : []).concat(c.strike ? ["line-through"] : []).join(" ") || "none";
        case "fontWeight": return String(c.weight);
        case "fontStyle": return c.italic ? "italic" : "normal";
        case "display": return c.display;
        case "fontSize": return c.size + "px";
        case "fontFamily":
            var named = _vComputedOther(v, "font-family");
            var box = v.model && v.model.text ? _logicalParent(v.model.text) : null;
            return named === "" && box && _gHasCascade() ? _gComputed(box, "font-family") : named || c.family;
        case "backgroundColor": return c.bg;
        case "visibility": return c.visible ? "visible" : "hidden";
        case "textDecorationLine":
            return c.underline ? (c.strike ? "underline line-through" : "underline") : c.strike ? "line-through" : "none";
        default: return _vComputedOther(v, _camelToKebab(String(prop)));
        }
    }
    return new Proxy({ getPropertyValue: read }, {
        get: function(target, prop) {
            if (prop in target) {
                return target[prop];
            }
            return typeof prop === "string" ? read(prop) : undefined;
        }
    });
}

// What the inline painting has no use for still has a value to read.
function _vComputedOther(v, prop) {
    if (prop === "width" || prop === "height") {
        return "auto";
    }
    var rules = _gHasCascade() ? _gRules() : _vLoadRules();
    var m = _meta(v);
    var best = null;
    var weight = -1;
    for (var i = 0; i < rules.length; ++i) {
        if (_vRuleMatches(rules[i], v, m)) {
            for (var d = 0; d < rules[i].decls.length; ++d) {
                var decl = rules[i].decls[d];
                var w = _gWeight(rules[i], decl[2]);
                if (decl[0] === prop && w >= weight) {
                    best = decl[1];
                    weight = w;
                }
            }
        }
    }
    if (prop.indexOf("border-") === 0 && /-color$/.test(prop) && best === null) {
        return _vStyle(v).color;
    }
    if (best === null) {
        best = _G_INITIAL[prop] !== undefined ? _G_INITIAL[prop] : "";
    }
    if (/^border-.*-width$/.test(prop) && best === "medium") {
        return "0px";
    }
    return /^(margin|padding|border-.*-width)/.test(prop) && String(best) === "0" ? "0px" : String(best);
}

function _vHex(css) {
    var fn = /rgba?\((\d+), (\d+), (\d+)(?:, ([\d.]+))?\)/.exec(css);
    if (!fn) {
        return "#000000";
    }
    function two(n) {
        var s = Number(n).toString(16);
        return s.length === 1 ? "0" + s : s;
    }
    var rgb = two(fn[1]) + two(fn[2]) + two(fn[3]);
    return fn[4] !== undefined ? "#" + two(Math.round(parseFloat(fn[4]) * 255)) + rgb : "#" + rgb;
}

// Whether the node needs what only RichText has.
function _vNeedsHtml(kids, pc, family) {
    for (var i = 0; i < kids.length; ++i) {
        if (kids[i].kids) {
            var c = _vStyle(kids[i]);
            if ((c.bg !== "rgba(0, 0, 0, 0)" && c.bg !== "transparent") || (c.family !== family && c.family !== "")
                || _vNeedsHtml(kids[i].kids, c, family)) {
                return true;
            }
        }
    }
    return false;
}

function _vWriteText(st, data, owner) {
    var out = "";
    for (var i = 0; i < data.length; ++i) {
        var ch = data.charAt(i);
        if (st.pre) {
            out += ch === "\n" ? "<br>" : ch === " " ? "&nbsp;" : ch === "&" ? "&amp;" : ch === "<" ? "&lt;" : ch === ">" ? "&gt;" : ch;
        } else if (/[ \t\r\n\f]/.test(ch)) {
            if (!st.prevSpace) {
                out += " ";
                st.prevSpace = true;
            }
        } else {
            out += ch === "&" ? "&amp;" : ch === "<" ? "&lt;" : ch === ">" ? "&gt;" : ch;
            st.prevSpace = false;
        }
    }
    if (out === "") {
        return;
    }
    if (st.twin) {
        st.owners.push(owner);
        st.out += "<a href=\"eldur:" + (st.owners.length - 1) + "\">" + out + "</a>";
    } else {
        st.out += out;
    }
}

function _vWriteGen(st, gen, pc, owner) {
    _vWrap(st, gen.c, pc, false, null, function() {
        _vWriteText(st, gen.text, owner);
    });
}

function _vWrap(st, c, pc, isLink, href, body) {
    var close = "";
    function open(tag, attrs) {
        st.out += "<" + tag + (attrs || "") + ">";
        close = "</" + tag + ">" + close;
    }
    var wasLink = st.inLink;
    if (isLink && !st.twin) {
        if (st.bareLink) {
            st.inLink = false;
        } else {
            if (st.html) {
                open("a", " href=\"" + href + "\" style=\"color:" + _vHex(c.color) + ";text-decoration:none\"");
            } else {
                open("a", " href=\"" + href + "\"");
                if (st.linkColor === null) {
                    st.linkColor = c.color;
                }
            }
            st.inLink = true;
        }
    }
    if (c.weight >= 600 && pc.weight < 600) {
        open("b");
    }
    if (c.italic && !pc.italic) {
        open("i");
    }
    if (c.underline && !(pc.underline && pc.inlineLine) && !(st.inLink && !st.html) && !st.bareLink) {
        open("u");
    }
    if (c.strike && !(pc.strike && pc.inlineLine)) {
        open("s");
    }
    if (!st.twin && !c.visible) {
        open("font", " color=\"#00000000\"");
    } else if (!st.twin && c.color !== pc.color && !(st.inLink && !st.html)) {
        open("font", " color=\"" + _vHex(c.color) + "\"");
    }
    if (st.html) {
        var css = [];
        if (c.size !== pc.size) {
            css.push("font-size:" + c.size + "px");
        }
        if (c.family !== pc.family && c.family !== "") {
            css.push("font-family:'" + c.family + "'");
        }
        if (c.bg !== "rgba(0, 0, 0, 0)" && c.bg !== "transparent") {
            css.push("background-color:" + _vHex(c.bg));
        }
        if (css.length > 0) {
            open("span", " style=\"" + css.join(";") + "\"");
        }
    } else if (c.size > pc.size) {
        open("big");
    } else if (c.size < pc.size) {
        open("small");
    }
    body();
    st.inLink = wasLink;
    st.out += close;
}

function _vWriteKids(st, kids, pc, owner) {
    for (var i = 0; i < kids.length; ++i) {
        var k = kids[i];
        if (k.gen) {
            _vWriteGen(st, k.gen, pc, owner);
        } else if (k.vtext) {
            _vWriteText(st, k.data, owner);
        } else {
            _vWriteElement(st, k, pc);
        }
    }
}

function _vWriteElement(st, v, pc) {
    var c = _vStyle(v);
    if (c.display === "none") {
        return;
    }
    if (v.eldurTag === "br") {
        st.out += "<br>";
        st.prevSpace = true;
        return;
    }
    var href = v.eldurTag === "a" ? _vGetAttr(v, "href") : null;
    var lined = { color: c.color, weight: c.weight, italic: c.italic, size: c.size, family: c.family, bg: c.bg,
        underline: c.underline || (pc.underline && pc.inlineLine), strike: c.strike || (pc.strike && pc.inlineLine),
        inlineLine: true, visible: c.visible, display: c.display };
    _vWrap(st, c, pc, href !== null, href === null ? null : _vEscapeText(href).replace(/"/g, "&quot;"), function() {
        if (v.before) {
            _vWriteGen(st, v.before, lined, v);
        }
        _vWriteKids(st, v.kids, lined, v);
        if (v.after) {
            _vWriteGen(st, v.after, lined, v);
        }
    });
}

function _vMarkup(model, twin) {
    var base = _vBase(model);
    // A run between boxes starts and ends mid-line: the line takes its edge spaces.
    var inLine = model.text.parent && typeof model.text.parent.firstBaseline === "number";
    var st = {
        out: "", prevSpace: !inLine, pre: model.pre, html: model.html, twin: twin,
        owners: [], inLink: false, linkColor: null,
        bareLink: model.bare && model.kids.length === 1 && Boolean(model.kids[0].kids)
            && !_vStyle(model.kids[0]).underline
    };
    _vWriteKids(st, model.kids, base, null);
    if (!st.pre && !inLine) {
        st.out = st.out.replace(/ ((?:<\/[a-z]+>)*)$/, "$1");
    }
    return st;
}

// The tallest `line-height` an element in the model writes, in pixels; 0 if none.
function _vTallestLine(model) {
    var tallest = 0;
    _vEachElement(model.kids, function(v) {
        var own = /(?:^|;)\s*line-height:\s*([^;]+)/.exec(_vCssText(_meta(v)));
        var written = own ? own[1].trim() : _vComputedOther(v, "line-height");
        var n = parseFloat(written);
        if (!isNaN(n)) {
            tallest = Math.max(tallest, /px$/.test(written) ? n : n * _vStyle(v).size);
        }
    });
    return tallest;
}

// A `Text` has one line height, so a taller element is told to the line placing
// it or, alone in its block, stands as padding around the block's own lines: its
// words stay on their baseline, though every wrapped line of it is short of the
// room Chromium gives the one line holding the element.
function _vRaiseLine(model) {
    var text = model.text;
    var tallest = _vTallestLine(model);
    if (text.parent && typeof text.parent.setRunLineHeight === "function") {
        if (tallest > 0 || text.eldurOwnLine !== undefined) {
            text.parent.setRunLineHeight(text, tallest);
        }
        return;
    }
    var fixed = text.lineHeightMode === 1; // Text.FixedHeight; a library script has no `Text`
    var line = fixed ? text.lineHeight
        : model.lh !== undefined ? model.lh : Math.round(model.k.size * 1.15);
    var room = Math.max(0, tallest - line);
    if (room > 0 || model.raised) {
        text.topPadding = Math.floor(room / 2);
        text.bottomPadding = Math.ceil(room / 2);
        model.raised = room > 0;
    }
}

// The first write takes the `Text` over from what the converter emitted: a
// `Text` styled as its one element goes back to the block's style, as that
// element's style now comes from the markup.
var _vRendering = false;

function _vRender(model) {
    var text = model.text;
    if (model.hoisted) {
        var k = model.k;
        text.color = k.color;
        text.font.weight = k.weight;
        text.font.italic = k.italic;
        text.font.underline = model.underline;
        text.font.strikeout = model.strike;
        text.font.pixelSize = k.size;
        model.hoisted = false;
    }
    if (!model.html && _vNeedsHtml(model.kids, _vBase(model), model.k.family)) {
        model.html = true;
    }
    var st = _vMarkup(model, false);
    var markup = st.out;
    if (model.html && model.lh !== undefined) {
        markup = "<div style=\"line-height:" + model.lh + "px\">" + markup + "</div>";
        text.lineHeightMode = 0; // Text.ProportionalHeight; a library script has no `Text`
        text.lineHeight = 1;
    }
    text.textFormat = model.html ? 1 : 4;
    if (st.linkColor !== null) {
        text.linkColor = st.linkColor;
    }
    _vRendering = true;
    text.text = markup;
    _vRendering = false;
    _vRaiseLine(model);
    model.rendered = true;
    model.twinText = null;
    _layoutChanged(text);
}

// After any write a selector could notice. A `Text` nothing touched keeps
// what the converter wrote for it.
function _vChanged(model) {
    ++_vGen;
    for (var i = 0; i < _vModels.length; ++i) {
        var each = _vModels[i];
        if (each.select) {
            if (each === model) {
                _vSyncSelect(each);
            }
            continue;
        }
        var render = each === model || each.rendered;
        if (!render) {
            _vEachElement(each.kids, function(v) {
                _vStyle(v);
                render = render || !v.cleanNow;
            });
        }
        if (render) {
            _vRender(each);
        }
    }
}

// Text has `linkAt` and nothing like `positionAt`. Making every run of the
// painted text a link would change how it paints, so a hidden copy, laid out
// the same, has every run as a link naming its node. Links the page has are
// left out of the copy; what it reports for them is the same run.
function _vTwin(model) {
    var text = model.text;
    if (!model.twin) {
        model.twin = Qt.createQmlObject(
            "import QtQuick; Text { visible: false; width: parent.width; font: parent.font; "
            + "wrapMode: parent.wrapMode; lineHeight: parent.lineHeight; lineHeightMode: parent.lineHeightMode; "
            + "horizontalAlignment: parent.horizontalAlignment }", text, "eldur_twin");
        _meta(model.twin).isFragment = true;
    }
    if (model.twinText === null || model.twinText === undefined) {
        var st = _vMarkup(model, true);
        model.twinOwners = st.owners;
        model.twinText = st.out;
        model.twin.textFormat = model.html ? 1 : 4;
        model.twin.text = model.html && model.lh !== undefined
            ? "<div style=\"line-height:" + model.lh + "px\">" + st.out + "</div>"
            : st.out;
        model.twinRects = null;
    }
    return model.twin;
}

function _vHitAt(model, x, y) {
    var twin = _vTwin(model);
    // A plain `Text` draws the space its marked twin drops.
    var drawn = model.text;
    if (drawn.textFormat === 0 && String(drawn.text).charAt(0) === " ") {
        x -= _vMetrics(drawn).advanceWidth(" ");
    }
    var link = twin.linkAt(x, y);
    if (link.indexOf("eldur:") !== 0) {
        return null;
    }
    return model.twinOwners[Number(link.slice(6))] || null;
}

function _vRect(v) {
    var none = { x: 0, y: 0, width: 0, height: 0 };
    if (!v.model || v.model.select || v.vtext || v.model.text.visible === false) {
        return none;
    }
    for (var at = v; at; at = at.vparent) {
        if (_vStyle(at).display === "none") {
            return none;
        }
    }
    var twin = _vTwin(v.model);
    var line = Math.max(1, Math.floor((twin.font.pixelSize || 16) / 2));
    var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (var y = line / 2; y < v.model.text.height; y += line) {
        for (var x = 0; x < v.model.text.width; x += 2) {
            var owner = _vHitAt(v.model, x, y);
            for (var o = owner; o; o = o.vparent) {
                if (o === v) {
                    minX = Math.min(minX, x);
                    maxX = Math.max(maxX, x + 2);
                    minY = Math.min(minY, y - line / 2);
                    maxY = Math.max(maxY, y + line / 2);
                    break;
                }
            }
        }
    }
    // A box on the line cut out of it counts across the line, margins too; an
    // inline box's own height is its font's whatever sits on its baseline.
    _vCutBoxes(v).forEach(function(box) {
        var from = box.mapToItem(v.model.text, 0, 0).x;
        minX = Math.min(minX, from);
        maxX = Math.max(maxX, from + box.width);
    });
    if (maxX >= minX) {
        return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
    }
    // An element with nothing in it still has a place on its line.
    if (v.kids && v.kids.length === 0) {
        var chars = _vCharsBefore(v.model.kids, v);
        var height = v.model.text.contentHeight / Math.max(1, v.model.text.lineCount);
        return { x: Math.min(v.model.text.width, chars.width), y: 0, width: 0, height: height };
    }
    return none;
}

// How far along its `Text` the empty element `v` sits, read off the twin's
// marks is not possible with nothing to mark: the text before it, measured.
function _vCharsBefore(kids, v) {
    var metrics = _vMetrics(v.model.text);
    var text = "";
    function walk(list) {
        for (var i = 0; i < list.length; ++i) {
            if (list[i] === v) {
                return true;
            }
            if (list[i].vtext) {
                text += list[i].data;
            } else if (list[i].kids && walk(list[i].kids)) {
                return true;
            }
        }
        return false;
    }
    walk(kids);
    return { width: metrics.advanceWidth(text.replace(/\s+/g, " ").replace(/^ /, "")) };
}

function _vMetrics(text) {
    var m = _meta(text);
    if (!m.vMetrics) {
        m.vMetrics = Qt.createQmlObject("import QtQuick; FontMetrics {}", text, "eldur_vmetrics");
        m.vMetrics.font = text.font;
    }
    return m.vMetrics;
}

// The boxes on `v`'s line that are `v`'s in the page.
function _vCutBoxes(v) {
    var line = v.model.text.parent;
    if (v.eldurKey === undefined || !line || typeof line.firstBaseline !== "number") {
        return [];
    }
    var boxes = [];
    Array.from(line.children).forEach(function(kid) {
        var box = kid.eldurInlineOwner !== undefined ? kid : kid.children && kid.children[0];
        if (box && box.eldurInlineOwner === v.eldurKey && box.visible) {
            boxes.push(kid);
        }
    });
    return boxes;
}

// The element around a box on a line, in the run its words went to.
function _vCutOwner(raw) {
    var line = raw.parent;
    while (line && typeof line.firstBaseline !== "number") {
        line = line.parent;
    }
    if (!line) {
        return null;
    }
    var found = null;
    Array.from(line.children).forEach(function(kid) {
        var model = found ? null : _vModelOf(kid);
        if (model && !model.select) {
            _vEachElement(model.kids, function(v) {
                if (!found && v.eldurKey === raw.eldurInlineOwner) {
                    found = v;
                }
            });
        }
    });
    return found;
}

function _vClientRect(v) {
    var r = _vRect(v);
    var origin = v.model && !v.model.select && (r.width > 0 || r.height > 0)
        ? v.model.text.mapToItem(null, r.x, r.y) : { x: 0, y: 0 };
    return { x: origin.x, y: origin.y, left: origin.x, top: origin.y, right: origin.x + r.width,
        bottom: origin.y + r.height, width: r.width, height: r.height };
}

// One box a line: the same scan as `_vRect`, kept apart by the line it hit.
function _vClientRects(v, whole) {
    var text = v.model.text;
    var lineHeight = text.contentHeight / Math.max(1, text.lineCount);
    if (text.lineCount < 2 || !(lineHeight > 0)) {
        return [whole];
    }
    var step = Math.max(1, Math.floor((_vTwin(v.model).font.pixelSize || 16) / 2));
    var boxes = {};
    for (var y = step / 2; y < text.height; y += step) {
        for (var x = 0; x < text.width; x += 2) {
            for (var o = _vHitAt(v.model, x, y); o; o = o.vparent) {
                if (o === v) {
                    var at = Math.floor(y / lineHeight);
                    var box = boxes[at] || (boxes[at] = { min: x, max: x + 2 });
                    box.min = Math.min(box.min, x);
                    box.max = Math.max(box.max, x + 2);
                    break;
                }
            }
        }
    }
    var keys = Object.keys(boxes).map(Number).sort(function(a, b) { return a - b; });
    if (keys.length < 2) {
        return [whole];
    }
    var content = Math.min(lineHeight, _vMetrics(text).height || lineHeight);
    return keys.map(function(at) {
        var origin = text.mapToItem(null, boxes[at].min, at * lineHeight + (lineHeight - content) / 2);
        var width = boxes[at].max - boxes[at].min;
        return { x: origin.x, y: origin.y, left: origin.x, top: origin.y, right: origin.x + width,
            bottom: origin.y + content, width: width, height: content };
    });
}

// `_connectSignalOnce` for a node: what reaches it is its `Text`'s tap.
function _vConnect(v, type) {
    _meta(v).connected[type] = true;
    if (type === "click" && v.model && !v.model.select) {
        _vTapHandler(v.model.text);
    }
}

function _vTapHandler(text) {
    var m = _meta(text);
    if (!m.vTap) {
        m.vTap = Qt.createQmlObject("import QtQuick; TapHandler {}", text, "eldur_vtap");
        m.vTap.tapped.connect(function(eventPoint, btn) {
            _beginTap({
                position: eventPoint.position,
                scenePosition: eventPoint.scenePosition,
                button: btn
            });
        });
    }
}

function _hasListeners(m, type) {
    var bubble = m.listeners[type];
    var capture = m.listeners["capture:" + type];
    return Boolean((bubble && bubble.length > 0) || (capture && capture.length > 0));
}

function _vReconnect(node) {
    if (node.vtext || !node.model) {
        return;
    }
    var m = _meta(node);
    if (_hasListeners(m, "click")) {
        _vConnect(node, "click");
    }
    for (var i = 0; i < node.kids.length; ++i) {
        if (!node.kids[i].gen) {
            _vReconnect(node.kids[i]);
        }
    }
}

// A tap: not every handler under the point hears it (a link or a control takes
// the press for itself) and those that do hear it on their own, so the first
// to hear runs the whole path of a browser's one event, and the rest find their
// turn taken.
var _tap = null;

// `childAt` stops at a parent's bounds, where a dropdown's list has only started.
function _deepestUnder(item, scene) {
    if (!item.visible || typeof item.mapFromItem !== "function") {
        return null;
    }
    var local = item.mapFromItem(null, scene.x, scene.y);
    var inside = local.x >= 0 && local.y >= 0 && local.x < item.width && local.y < item.height;
    if (item.clip && !inside) {
        return null;
    }
    var kids = [];
    for (var i = 0; i < item.children.length; ++i) {
        kids.push({ at: i, kid: item.children[i] });
    }
    kids.sort(function(a, b) { return (b.kid.z - a.kid.z) || (b.at - a.at); });
    for (var k = 0; k < kids.length; ++k) {
        var found = _deepestUnder(kids[k].kid, scene);
        if (found) {
            return found;
        }
    }
    // Positioned boxes hang from the initial containing block; it paints nothing,
    // and lies over the page.
    if (inside && _isPlainTextItem(item) && _isLines(item.parent) && !item.parent.runCovers(item, local.x, local.y)) {
        return null;
    }
    return inside && item !== _root.eldurInitialBlock ? item : null;
}

function _deepestAt(scene) {
    var top = _root.contentItem || _root;
    return _deepestUnder(top, scene) || top;
}

// Pressing what takes no focus moves it to the page.
function _blurUnlessFocusable(target) {
    var raw = target;
    for (; raw; raw = _gParent(raw)) {
        if (raw.activeFocusOnTab === true || typeof raw.clicked === "function"
            || typeof raw.cursorPosition === "number" || getAttribute(raw, "tabindex") !== null
            || _nodeTag(raw) === "summary") {
            return;
        }
    }
    var active = document.activeElement;
    if (active && active.blur && active !== document.body) {
        active.blur();
    }
}

function _beginTap(qtEv, heardBy) {
    var scene = qtEv.scenePosition;
    if (_tap && _tap.x === scene.x && _tap.y === scene.y) {
        return _tap;
    }
    var deep = _deepestAt(scene);
    var t = { x: scene.x, y: scene.y, target: null, control: null, ev: null };
    _tap = t;
    Qt.callLater(_flushTap);
    for (var at = deep; at && !t.target; at = at.parent) {
        var model = _vModelOf(at);
        if (model && !model.select) {
            var local = at.mapFromItem(null, scene.x, scene.y);
            t.target = _vHitAt(model, local.x, local.y) || _vHostElement(model);
        } else if (!_isInternalHelperNode(at) && _vIsTagged(at)) {
            t.target = at;
            if (typeof at.clicked === "function") {
                t.control = at;
            }
        }
    }
    t.target = t.target || _unwrap(document.body) || heardBy;
    _blurUnlessFocusable(t.target);
    t.ev = event("click", wrap(t.target), qtEv);
    t.ev.target = wrap(t.target);
    t.ev.bubbles = true;
    // A control's own `clicked` starts its path, if the press comes to a click.
    if (!t.control) {
        _dispatchDynamic(t.target, "click", t.ev);
    }
    return t;
}

function _flushTap() {
    _tap = null;
}

// A link's own handler asks; a tap on it comes first.
function linkPrevented() {
    return Boolean(_tap && _tap.ev && _tap.ev.defaultPrevented);
}

function tapPrevented(eventPoint) {
    return Boolean(_beginTap(eventPoint).ev.defaultPrevented);
}

function _vClick(v) {
    _dispatchDynamic(v, "click", null);
}

// A control takes the press for itself, so no handler above it hears: the
// event goes up from here. Its toggle has happened; `preventDefault` undoes it.
function _clickedControl(raw) {
    var m = _meta(raw);
    var toggled = Boolean(m.toggledNow);
    m.toggledNow = false;
    var ev = event("click", wrap(raw), null);
    _dispatchDynamic(raw, "click", ev);
    if (!toggled) {
        return;
    }
    if (ev.defaultPrevented) {
        raw.checked = !raw.checked;
        return;
    }
    _toggleEvents(raw);
}

function _toggleEvents(raw) {
    var m = _meta(raw);
    if (m.connected.input) {
        _dispatchDynamic(raw, "input", null);
    }
    if (m.connected.change) {
        _dispatchDynamic(raw, "change", null);
    }
}

// Inserting: what a script made as a QML object becomes a node when it goes
// into a line of text. Its wrapper stays the script's handle on it.
function _vFromCreated(raw) {
    var m = _meta(raw);
    if (m.createdTag === "#text") {
        return _vLeaf(raw.text, null, null);
    }
    var tag = m.createdTag;
    var kidSource = m.flow ? m.flow.children : raw.children;
    var kids = [];
    if (typeof raw.text === "string" && raw.text !== "") {
        kids.push(_vLeaf(m.explicitText !== undefined ? m.explicitText : raw.text, null, null));
    } else if (typeof raw.text !== "string" && m.explicitText !== undefined && m.explicitText !== "") {
        kids.push(_vLeaf(m.explicitText, null, null));
    }
    for (var i = 0; kidSource && i < kidSource.length; ++i) {
        var ch = kidSource[i];
        if (_meta(ch).createdTag) {
            kids.push(_vFromCreated(ch));
        }
    }
    var v = _vElement(tag, [], null, null);
    _itemMeta.delete(v);
    _itemMeta.delete(raw);
    _itemMeta.set(v, m);
    m.raw = v;
    if (m._setRaw) {
        m._setRaw(v);
    }
    delete m.createdTag;
    m.connected = {};
    delete m.flow;
    if (raw.objectName) {
        v.objectName = raw.objectName;
    } else if (m.attrs.id) {
        v.objectName = String(m.attrs.id);
    }
    delete m.attrs.id;
    if (m.explicitHidden) {
        m.attrs.hidden = "";
    }
    v.eldurClasses = m.classes.join(" ");
    for (var k = 0; k < kids.length; ++k) {
        kids[k].vparent = v;
        v.kids.push(kids[k]);
    }
    if (typeof raw.destroy === "function") {
        raw.destroy();
    }
    return v;
}

// A node going where only real items are laid out.
function _vToReal(v) {
    if (v.vtext) {
        var plain = _unwrap(document.createTextNode(v.data));
        var said = _itemMeta.get(v);
        if (said && said.wrapper) {
            // Whoever holds the node still means this text.
            _itemMeta.delete(v);
            _itemMeta.set(plain, said);
            said.raw = plain;
            if (said._setRaw) {
                said._setRaw(plain);
            }
        }
        _gReplaced.set(v, plain);
        return plain;
    }
    var m = _meta(v);
    var made = _unwrap(document.createElement(v.eldurTag));
    var mm = _meta(made);
    var script = m.wrapper;
    var look = _vStyle(v);
    _vRemove(v);
    if (v.objectName) {
        made.objectName = v.objectName;
    }
    var kids = v.kids && v.kids.some(_vIsElement) ? v.kids.slice() : null;
    var text = kids ? "" : _vText(v);
    _itemMeta.delete(v);
    _itemMeta.set(made, m);
    // Whoever holds the node still means this element.
    _gReplaced.set(v, made);
    m.raw = made;
    m.createdTag = v.eldurTag;
    m.connected = {};
    if (m._setRaw) {
        m._setRaw(made);
    }
    if (script) {
        m.wrapper = script;
    }
    if (text !== "") {
        setText(made, text);
    }
    // What the markup showed was the model's; on a closed page no pass paints this.
    if (_isPlainTextItem(made)) {
        made.color = look.color;
        made.font.weight = look.weight;
        made.font.italic = look.italic;
        made.font.underline = look.underline;
        made.font.strikeout = look.strike;
        made.font.pixelSize = look.size;
    }
    if (kids && !_vTryInsert(made, kids, null)) {
        _insertChildren(made, kids, null);
    }
    var keys = Object.keys(m.styles);
    for (var i = 0; i < keys.length; ++i) {
        setStyle(made, keys[i], m.styles[keys[i]]);
    }
    var kinds = ["click", "dblclick"];
    for (var e = 0; e < kinds.length; ++e) {
        if (_hasListeners(m, kinds[e])) {
            _connectSignalOnce(made, kinds[e]);
        }
    }
    void mm;
    return made;
}

function _vRemove(node) {
    if (node.wsOwner) {
        var runs = _meta(node.wsOwner)[node.wsKey];
        runs.splice(runs.indexOf(node), 1);
        node.wsOwner = null;
        node.vparent = null;
        return;
    }
    var owner = node.vparent || node.model;
    if (!owner) {
        return;
    }
    var at = owner.kids.indexOf(node);
    if (at !== -1) {
        owner.kids.splice(at, 1);
    }
    var model = node.model;
    node.vparent = null;
    _vSetModel(node, null);
    if (model) {
        if (model.select) {
            _vEnsureSelected(model);
        }
        _vChanged(model);
    }
}

function _vAsNode(raw) {
    if (raw.__eldurInline) {
        return raw;
    }
    var m = _meta(raw);
    return m.createdTag ? _vFromCreated(raw) : null;
}

// Inserts `nodes` (raw, or wrappers' raws) into container `c` before `ref`.
function _vInsert(c, rawNodes, ref) {
    var model = _vModelFor(c);
    var list = c.kids;
    for (var i = 0; i < rawNodes.length; ++i) {
        var node = _vAsNode(rawNodes[i]);
        if (!node) {
            continue;
        }
        if (node === ref) {
            ref = null;
            var after = list.indexOf(node) + 1;
            if (after > 0 && after < list.length) {
                ref = list[after];
            }
        }
        if (node.vparent || node.model) {
            _vRemove(node);
        }
        var at = ref ? list.indexOf(ref) : -1;
        node.vparent = c.__eldurInline ? c : null;
        _vSetModel(node, model);
        if (at === -1) {
            list.push(node);
        } else {
            list.splice(at, 0, node);
        }
        _vReconnect(node);
        if (model && model.select && node.sel) {
            _vPick(model, node);
        }
    }
    if (model && model.select) {
        _vEnsureSelected(model);
    }
    _vChanged(model);
}

function _vClone(node, deep) {
    if (node.vtext) {
        return _vLeaf(node.data, null, null);
    }
    var m = _meta(node);
    var copy = _vElement(node.eldurTag, [], null, null);
    var cm = _meta(copy);
    copy.objectName = node.objectName;
    copy.attrOrder = node.attrOrder.slice();
    cm.classes = m.classes.slice();
    Object.keys(m.attrs).forEach(function(k) { cm.attrs[k] = m.attrs[k]; });
    Object.keys(m.dataset).forEach(function(k) { cm.dataset[k] = m.dataset[k]; });
    Object.keys(m.styles).forEach(function(k) { cm.styles[k] = m.styles[k]; });
    cm.explicitHidden = m.explicitHidden;
    if (deep) {
        node.kids.forEach(function(k) {
            if (!k.gen) {
                var kc = _vClone(k, true);
                kc.vparent = copy;
                copy.kids.push(kc);
            }
        });
    }
    return copy;
}

// Options. `selectedness` as HTML has it: the node keeps its own flag, and
// a select showing one line always has one on while it can.
function _vOptions(model) {
    var out = [];
    _vEachElement(model.kids, function(v) {
        if (v.eldurTag === "option") {
            out.push(v);
        }
    });
    return out;
}

function _vOptionText(v) {
    return _vText(v).replace(/[ \t\r\n\f]+/g, " ").trim();
}

function _vOptionValue(v) {
    var attr = _vGetAttr(v, "value");
    return attr !== null ? attr : _vOptionText(v);
}

function _vSelected(model) {
    var opts = _vOptions(model);
    var found = -1;
    for (var i = 0; i < opts.length; ++i) {
        if (opts[i].sel) {
            found = i;
        }
    }
    return found;
}

function _vEnsureSelected(model) {
    var opts = _vOptions(model);
    var keep = _vSelected(model);
    for (var i = 0; i < opts.length; ++i) {
        if (i !== keep) {
            opts[i].sel = false;
        }
    }
    if (keep === -1) {
        for (var j = 0; j < opts.length; ++j) {
            if (_vGetAttr(opts[j], "disabled") === null) {
                opts[j].sel = true;
                break;
            }
        }
    }
}

function _vPick(model, v) {
    var opts = _vOptions(model);
    for (var i = 0; i < opts.length; ++i) {
        opts[i].sel = opts[i] === v;
    }
}

function _vSyncSelect(model) {
    var opts = _vOptions(model);
    var combo = model.text;
    _vSyncing = true;
    combo.model = opts.map(function(o) { return { text: _vOptionText(o), value: _vOptionValue(o) }; });
    combo.currentIndex = _vSelected(model);
    _vSyncing = false;
    // A created select is as wide as its options, and they come after it.
    if (_meta(combo).createdTag && combo.parent) {
        var style = _gStyle(combo);
        _gControlWidth(combo, style, _gEdges(combo, style, combo.parent.width));
    }
    _layoutChanged(combo);
}

function _vSelectModel(raw) {
    var model = raw && !raw.__eldurInline ? _vModelOf(raw) : null;
    return model && model.select ? model : null;
}

// HTML's listed elements of a form, hidden or not, in tree order. Names and ids
// reach them as properties. `<input type=image>` is the standard's exception.
function _formElements(form) {
    var list = _elementsUnder(form, function(node) {
        var tag = _nodeTag(node);
        return /^(button|fieldset|input|object|output|select|textarea)$/.test(tag)
            && !(tag === "input" && wrap(node).type === "image");
    }, false).map(wrap);
    var col = _asHTMLCollection(list);
    list.forEach(function(el) {
        [el.id, el.name].forEach(function(key) {
            if (key && !(key in col)) {
                Object.defineProperty(col, key, { configurable: true, enumerable: false, value: el });
            }
        });
    });
    return col;
}

// What a form would submit: successful controls in tree order (HTML 4.10.21.4).
function _formEntries(form) {
    var entries = [];
    Array.prototype.forEach.call(_formElements(_unwrap(form)), function(el) {
        var tag = String(el.tagName).toLowerCase();
        var type = String(el.type || "").toLowerCase();
        if (!el.name || el.disabled || tag === "button" || tag === "fieldset" || tag === "output" || tag === "object"
            || /^(submit|button|reset|image)$/.test(type)) {
            return;
        }
        if ((type === "checkbox" || type === "radio")) {
            if (el.checked) {
                entries.push([el.name, getAttribute(_unwrap(el), "value") || "on"]);
            }
            return;
        }
        entries.push([el.name, String(el.value)]);
    });
    return entries;
}

function FormData(form) {
    this._entries = form ? _formEntries(form) : [];
}
FormData.prototype = {
    append: function(name, value) { this._entries.push([String(name), String(value)]); },
    delete: function(name) { this._entries = this._entries.filter(function(e) { return e[0] !== String(name); }); },
    get: function(name) {
        var hit = this._entries.filter(function(e) { return e[0] === String(name); });
        return hit.length > 0 ? hit[0][1] : null;
    },
    getAll: function(name) {
        return this._entries.filter(function(e) { return e[0] === String(name); }).map(function(e) { return e[1]; });
    },
    has: function(name) { return this.get(name) !== null; },
    set: function(name, value) {
        var key = String(name), done = false;
        this._entries = this._entries.filter(function(e) {
            if (e[0] !== key) {
                return true;
            }
            if (done) {
                return false;
            }
            done = true;
            e[1] = String(value);
            return true;
        });
        if (!done) {
            this._entries.push([key, String(value)]);
        }
    },
    entries: function() { return this._entries.map(function(e) { return [e[0], e[1]]; })[Symbol.iterator](); },
    keys: function() { return this._entries.map(function(e) { return e[0]; })[Symbol.iterator](); },
    values: function() { return this._entries.map(function(e) { return e[1]; })[Symbol.iterator](); },
    forEach: function(callback, self) {
        var data = this;
        this._entries.slice().forEach(function(e) { callback.call(self, e[1], e[0], data); });
    }
};
FormData.prototype[Symbol.iterator] = FormData.prototype.entries;

function Option(text, value, defaultSelected, selected) {
    var attrs = [];
    if (value !== undefined) {
        attrs.push(["value", String(value)]);
    }
    if (defaultSelected) {
        attrs.push(["selected", ""]);
    }
    var v = _vElement("option", attrs, null, null);
    v.sel = Boolean(selected);
    if (text !== undefined && String(text) !== "") {
        v.kids.push(_vLeaf(String(text), v, null));
    }
    return wrap(v);
}

// A script reading a size right after writing text expects it to count.
function _vSettle() {
    if (_vHasRules() && _layoutDirty.length > 0) {
        _settleLayouts();
    }
}

// The model already holding `raw`'s text.
function _vExisting(raw) {
    if (!_vHasRules() && typeof raw.eldurInline !== "string" && !_itemMeta.has(raw)) {
        return null;
    }
    var own = _vModelOf(raw);
    if (own) {
        return own;
    }
    var text = typeof raw.text === "string" ? null : _findTextChild(raw);
    return text ? _vModelOf(text) : null;
}

function _vInlineTag(tag) {
    return /^(#text|a|abbr|b|bdi|bdo|br|cite|code|data|del|dfn|em|font|i|ins|kbd|label|mark|q|s|samp|small|span|strike|strong|sub|sup|time|tt|u|var|wbr)$/.test(tag);
}

function _vStaysInline(node) {
    // An inline element holding a box is a line, not a node of a model.
    if (_isLines(node)) {
        return false;
    }
    var tag = node.__eldurInline ? node.eldurTag : String(_meta(node).createdTag || "");
    if (!_vInlineTag(tag)) {
        return false;
    }
    // A span the page's CSS makes a box is a box.
    if (!node.__eldurInline && tag !== "#text" && _gActive()) {
        ++_gGen;
        return _gStyle(node).display === "inline";
    }
    return true;
}

// Where `nodes` go, if a line of text takes them; else they are real items.
function _vHostOf(raw, nodes, ref) {
    if (!_vHasRules() && !raw.__eldurInline) {
        return null;
    }
    if (ref && ref.__eldurInline && nodes.every(function(n) { return n.__eldurInline || _vStaysInline(n); })) {
        return ref.vparent || ref.model;
    }
    if (raw.__eldurInline) {
        return raw;
    }
    if (_holdsLines(raw)) {
        return null;
    }
    // A block breaks out of a line of text; what is a node of a model has no box
    // to break out with.
    var joinsModel = function(n) {
        return n.__eldurInline || _vStaysInline(n);
    };
    var known = _vExisting(raw);
    if (known && nodes.every(joinsModel)) {
        return known;
    }
    if (!nodes.every(_vStaysInline)) {
        return null;
    }
    var hasNode = nodes.some(function(n) { return n.__eldurInline; });
    if (_meta(raw).createdTag && hasNode && _vStaysInline(raw)) {
        return _vFromCreated(raw);
    }
    // A line after the box's blocks: a model's one Text cannot stand below them.
    if (_gActive() && _gIsBox(raw) && _rawElementChildren(raw).length > 0) {
        return null;
    }
    return _vOwnModel(raw) || (_markupMakesElements(raw) ? null : _vBoxModel(raw));
}

// `true` when the nodes went into a line of text.
function _vTryInsert(raw, nodes, ref) {
    var host = _vHostOf(raw, nodes, ref);
    if (!host) {
        return false;
    }
    _vInsert(host, nodes, ref);
    return true;
}

// The children and, with text, the child nodes of `raw`; shared since both
// must agree where a model's nodes stand among a box's items.
function _rawChildNodes(raw) {
    if (raw.__eldurInline) {
        return raw.kids ? raw.kids.filter(_vIsRendered) : [];
    }
    var own = _vOwnModel(raw);
    if (own) {
        return own.kids.filter(_vIsRendered);
    }
    var out = [];
    var withLeaves = function(leaves, leaf) {
        leaves.forEach(function(l) {
            l.vparent = raw;
            out.push(l);
        });
        if (leaf) {
            out.push(leaf);
        }
    };
    _rawKids(raw, true).forEach(function(kid) { withLeaves(_wsBefore(kid), kid); });
    withLeaves(_wsEnd(raw), null);
    return out;
}

function _walkItems(node, visitor) {
    if (!node) {
        return false;
    }
    // An out-of-flow box is in the tree at its slot, and only then.
    if (node.eldurBox) {
        node = node.eldurBox;
    }
    if (visitor(node)) {
        return true;
    }
    var inlineModel = _vModelOf(node);
    if (inlineModel) {
        var inlineKids = inlineModel.kids.filter(_vIsElement);
        for (var v = 0; v < inlineKids.length; ++v) {
            if (_walkItems(inlineKids[v], visitor)) {
                return true;
            }
        }
    }
    if (node.children) {
        for (var i = 0; i < node.children.length; ++i) {
            if (node.children[i].eldurSlot !== undefined) {
                continue;
            }
            if (_walkItems(node.children[i], visitor)) {
                return true;
            }
        }
    } else if (node.contentItem && node.contentItem !== node) {
        if (_walkItems(node.contentItem, visitor)) {
            return true;
        }
    }
    return false;
}

function _isInternalHelperNode(node) {
    if (!node || typeof node !== "object") {
        return true;
    }
    var m = _meta(node);
    if (m.hostBox || m.isFragment) {
        return true;
    }
    if (!node.objectName && !m.createdTag && typeof node.eldurTag !== "string") {
        if (node.parent && (node.parent.contentItem === node || node.parent.background === node || node.parent.indicator === node)) {
            return true;
        }
        if (typeof node.contentX === "number" && typeof node.contentY === "number" && node.contentItem) {
            return true;
        }
        if (typeof node.position === "number" && typeof node.size === "number" && typeof node.active === "boolean") {
            return true;
        }
        if (_root && _root.eldurStyledClasses !== undefined) {
            return true;
        }
        if (_isPlainTextItem(node) && node.parent && (node.parent.color !== undefined || node.parent.clip === true)) {
            return true;
        }
        if (typeof node.spacing === "number" || typeof node.columnSpacing === "number") {
            return true;
        }
    }
    return false;
}

var _selectorCache = new Map();

function _selectorError(text) {
    var e = new Error("'" + text + "' is not a valid selector");
    e.name = "SyntaxError";
    return e;
}

// Pseudo-classes that take no argument; each says whether `node` has it.
var _plainPseudos = {
    "first-child": function(n) { return _elementSiblings(n)[0] === n; },
    "last-child": function(n) { var s = _elementSiblings(n); return s[s.length - 1] === n; },
    "only-child": function(n) { return _elementSiblings(n).length === 1; },
    "first-of-type": function(n) { return _siblingsOfType(n)[0] === n; },
    "last-of-type": function(n) { var s = _siblingsOfType(n); return s[s.length - 1] === n; },
    "only-of-type": function(n) { return _siblingsOfType(n).length === 1; },
    "empty": function(n) {
        return _rawElementChildren(n).length === 0 && !getText(n);
    },
    "root": function(n) { return n === (_taggedHtml() || _root); },
    "scope": function(n) { return n === _selectorScope; },
    "checked": function(n) {
        return n.checked === true || _meta(n).explicitChecked === true;
    },
    "disabled": function(n) { return _isFormControl(n) && n.enabled === false; },
    "enabled": function(n) { return _isFormControl(n) && n.enabled !== false; },
    "focus": function(n) { return n.activeFocus === true; },
    "focus-visible": function(n) { return n.activeFocus === true; },
    "focus-within": function(n) {
        return _walkItems(n, function(d) { return d.activeFocus === true; });
    },
    "hover": function(n) { return n.hovered === true || _meta(n).gHover === true; },
    "active": function(n) { return n.pressed === true || n.down === true || _meta(n).gPress === true; },
    "link": function(n) { return _isLink(n); },
    "any-link": function(n) { return _isLink(n); },
    "visited": function() { return false; },
    "target": function() { return false; },
    "defined": function() { return true; },
    "required": function(n) { return _isFormControl(n) && getAttribute(n, "required") !== null; },
    "optional": function(n) { return _isFormControl(n) && getAttribute(n, "required") === null; },
    "read-only": function(n) { return !_isEditable(n); },
    "read-write": function(n) { return _isEditable(n); },
    "indeterminate": function(n) { return n.checkState === Qt.PartiallyChecked; },
    "default": function(n) { return getAttribute(n, "checked") !== null || getAttribute(n, "selected") !== null; },
    "placeholder-shown": function(n) {
        return typeof n.placeholderText === "string" && n.placeholderText !== "" && n.text === "";
    },
    "valid": function(n) { return _isFormControl(n); },
    "invalid": function() { return false; }
};

function _nodeTag(node) {
    return (typeof node.eldurTag === "string" && node.eldurTag !== ""
        ? node.eldurTag
        : _inferTagName(node)).toLowerCase();
}

function _isFormControl(node) {
    return ["button", "input", "select", "textarea", "option", "optgroup", "fieldset"]
        .indexOf(_nodeTag(node)) !== -1;
}

function _isLink(node) {
    var tag = _nodeTag(node);
    return (tag === "a" || tag === "area") && getAttribute(node, "href") !== null;
}

function _isEditable(node) {
    var tag = _nodeTag(node);
    return (tag === "input" || tag === "textarea") && node.readOnly !== true && node.enabled !== false;
}

// The parent element: what lies between an element and the next one up (a
// scrolling viewport, the window) is not in a script's DOM.
function _elementParent(node) {
    var parent = _logicalParent(node);
    while (parent && _isInternalHelperNode(parent)) {
        parent = _logicalParent(parent);
    }
    return parent;
}

function _elementSiblings(node) {
    var parent = _elementParent(node);
    return parent ? _rawElementChildren(parent).filter(function(k) {
        return !_isInternalHelperNode(k);
    }) : [node];
}

function _siblingsOfType(node) {
    var tag = _nodeTag(node);
    return _elementSiblings(node).filter(function(k) { return _nodeTag(k) === tag; });
}

// The `html` element, when the converter tagged elements: below the window and
// the scrolling viewport, which are not elements.
function _taggedHtml() {
    var found = null;
    if (_root) {
        _walkItems(_root, function(node) {
            if (node.eldurTag === "html") {
                found = node;
                return true;
            }
            return false;
        });
    }
    return found;
}

function _nodeId(node) {
    if (typeof node.objectName === "string" && node.objectName !== "") {
        return node.objectName;
    }
    return _meta(node).attrs.id || "";
}

// A small recursive-descent reader for the Selectors level 4 syntax
// `querySelector` accepts. Anything it does not know is a `SyntaxError`, as in
// a browser, rather than a selector that silently matches nothing.
function _SelectorReader(text) {
    this.text = text;
    this.at = 0;
}

_SelectorReader.prototype.peek = function() {
    return this.text.charAt(this.at);
};

_SelectorReader.prototype.skipSpace = function() {
    var before = this.at;
    while (/\s/.test(this.peek()) && this.at < this.text.length) {
        this.at += 1;
    }
    return this.at > before;
};

_SelectorReader.prototype.fail = function() {
    return _selectorError(this.text);
};

_SelectorReader.prototype.name = function() {
    var out = "";
    while (this.at < this.text.length) {
        var ch = this.peek();
        if (ch === "\\") {
            this.at += 1;
            var hex = /^[0-9a-fA-F]{1,6}/.exec(this.text.slice(this.at));
            if (hex) {
                out += String.fromCodePoint(parseInt(hex[0], 16));
                this.at += hex[0].length;
                if (/\s/.test(this.peek())) {
                    this.at += 1;
                }
            } else {
                if (this.at >= this.text.length) {
                    throw this.fail();
                }
                out += this.peek();
                this.at += 1;
            }
        } else if (/[-_a-zA-Z0-9]/.test(ch) || ch.charCodeAt(0) >= 0x80) {
            out += ch;
            this.at += 1;
        } else {
            break;
        }
    }
    if (out === "" || /^(-?[0-9]|-$)/.test(out)) {
        throw this.fail();
    }
    return out;
};

_SelectorReader.prototype.string = function() {
    var quote = this.peek();
    this.at += 1;
    var out = "";
    while (this.at < this.text.length && this.peek() !== quote) {
        if (this.peek() === "\\") {
            this.at += 1;
        }
        out += this.peek();
        this.at += 1;
    }
    if (this.at >= this.text.length) {
        throw this.fail();
    }
    this.at += 1;
    return out;
};

_SelectorReader.prototype.attribute = function() {
    this.at += 1;
    this.skipSpace();
    var test = { kind: "attr", name: this.name().toLowerCase(), op: null, value: "", fold: false };
    this.skipSpace();
    var op = /^[~|^$*]?=/.exec(this.text.slice(this.at));
    if (op) {
        test.op = op[0];
        this.at += op[0].length;
        this.skipSpace();
        var q = this.peek();
        test.value = q === "\"" || q === "'" ? this.string() : this.name();
        this.skipSpace();
        if (/[iIsS]/.test(this.peek())) {
            test.fold = this.peek().toLowerCase() === "i";
            this.at += 1;
            this.skipSpace();
        }
    }
    if (this.peek() !== "]") {
        throw this.fail();
    }
    this.at += 1;
    return test;
};

// `an+b`, with `of <selector list>` in the `-child` forms.
_SelectorReader.prototype.nth = function(allowOf) {
    var rest = this.text.slice(this.at);
    var m = /^\s*(even|odd|([-+]?\d*)n\s*(?:([-+])\s*(\d+))?|[-+]?\d+)\s*/i.exec(rest);
    if (!m) {
        throw this.fail();
    }
    this.at += m[0].length;
    var a = 0;
    var b = 0;
    var form = m[1].toLowerCase();
    if (form === "even") {
        a = 2;
    } else if (form === "odd") {
        a = 2;
        b = 1;
    } else if (form.indexOf("n") !== -1) {
        var coef = m[2];
        a = coef === "" || coef === "+" ? 1 : (coef === "-" ? -1 : parseInt(coef, 10));
        b = m[4] ? parseInt(m[4], 10) * (m[3] === "-" ? -1 : 1) : 0;
    } else {
        b = parseInt(form, 10);
    }
    var of_ = null;
    if (allowOf && /^of\s/i.test(this.text.slice(this.at))) {
        this.at += 2;
        of_ = this.list(false, ")");
    }
    return { a: a, b: b, of: of_ };
};

_SelectorReader.prototype.pseudo = function() {
    this.at += 1;
    if (this.peek() === ":") {
        // A pseudo-element is no element: valid, and never found.
        this.at += 1;
        this.name();
        return { kind: "never" };
    }
    var name = this.name().toLowerCase();
    if (this.peek() !== "(") {
        if (["before", "after", "first-line", "first-letter"].indexOf(name) !== -1) {
            return { kind: "never" };
        }
        if (!Object.prototype.hasOwnProperty.call(_plainPseudos, name)) {
            throw this.fail();
        }
        return { kind: "plain", test: _plainPseudos[name] };
    }
    this.at += 1;
    var test;
    switch (name) {
    case "not":
    case "is":
    case "where":
    case "matches":
        test = { kind: name === "not" ? "not" : "is", list: this.list(false, ")") };
        break;
    case "has":
        test = { kind: "has", list: this.list(true, ")") };
        break;
    case "nth-child":
    case "nth-last-child":
        test = { kind: "nth", last: name === "nth-last-child", ofType: false, nth: this.nth(true) };
        break;
    case "nth-of-type":
    case "nth-last-of-type":
        test = { kind: "nth", last: name === "nth-last-of-type", ofType: true, nth: this.nth(false) };
        break;
    default:
        throw this.fail();
    }
    this.skipSpace();
    if (this.peek() !== ")") {
        throw this.fail();
    }
    this.at += 1;
    return test;
};

_SelectorReader.prototype.compound = function() {
    var tests = [];
    var ch = this.peek();
    if (ch === "*") {
        this.at += 1;
        tests.push({ kind: "any" });
    } else if (/[-_a-zA-Z\\]/.test(ch) || ch.charCodeAt(0) >= 0x80) {
        tests.push({ kind: "tag", name: this.name().toLowerCase() });
    }
    for (;;) {
        ch = this.peek();
        if (ch === "#") {
            this.at += 1;
            tests.push({ kind: "id", name: this.name() });
        } else if (ch === ".") {
            this.at += 1;
            tests.push({ kind: "class", name: this.name() });
        } else if (ch === "[") {
            tests.push(this.attribute());
        } else if (ch === ":") {
            tests.push(this.pseudo());
        } else {
            break;
        }
    }
    if (tests.length === 0) {
        throw this.fail();
    }
    return tests;
};

// A list of complex selectors, up to `end` (the end of the text when empty).
// `relative` allows the leading combinator of `:has()`.
_SelectorReader.prototype.list = function(relative, end) {
    var list = [];
    for (;;) {
        var chain = [];
        var comb = relative ? " " : null;
        this.skipSpace();
        for (;;) {
            if (/[>+~]/.test(this.peek()) && this.peek() !== "") {
                if (chain.length === 0 && !relative) {
                    throw this.fail();
                }
                comb = this.peek();
                this.at += 1;
                this.skipSpace();
            }
            chain.push({ comb: comb, tests: this.compound() });
            var spaced = this.skipSpace();
            var ch = this.peek();
            if (this.at >= this.text.length || ch === "," || ch === end) {
                break;
            }
            comb = spaced ? " " : null;
            if (comb === null && !/[>+~]/.test(ch)) {
                throw this.fail();
            }
        }
        list.push(chain);
        if (this.peek() !== ",") {
            break;
        }
        this.at += 1;
    }
    return list;
};

function _parseSelector(selector) {
    var text = String(selector);
    var known = _selectorCache.get(text);
    if (known) {
        return known;
    }
    var reader = new _SelectorReader(text);
    var list = reader.list(false, "");
    if (reader.at < text.length) {
        throw _selectorError(text);
    }
    _selectorCache.set(text, list);
    return list;
}

function _matchesAttrTest(node, test) {
    var actual = getAttribute(node, test.name);
    if (actual === null) {
        return false;
    }
    if (test.op === null) {
        return true;
    }
    var have = String(actual);
    var want = test.value;
    if (test.fold) {
        have = have.toLowerCase();
        want = want.toLowerCase();
    }
    switch (test.op) {
    case "=": return have === want;
    case "^=": return want !== "" && have.indexOf(want) === 0;
    case "$=": return want !== "" && have.slice(-want.length) === want;
    case "*=": return want !== "" && have.indexOf(want) !== -1;
    case "~=": return want !== "" && !/\s/.test(want) && have.trim().split(/\s+/).indexOf(want) !== -1;
    case "|=": return have === want || have.indexOf(want + "-") === 0;
    }
    return false;
}

function _matchesNth(node, test) {
    var nth = test.nth;
    var pool = test.ofType ? _siblingsOfType(node) : _elementSiblings(node);
    if (nth.of) {
        pool = pool.filter(function(k) { return _matchesList(k, nth.of); });
    }
    var index = pool.indexOf(node);
    if (index === -1) {
        return false;
    }
    var place = test.last ? pool.length - index : index + 1;
    if (nth.a === 0) {
        return place === nth.b;
    }
    var steps = (place - nth.b) / nth.a;
    return steps >= 0 && Math.floor(steps) === steps;
}

function _matchesHas(node, chains) {
    var found = false;
    var check = function(candidate) {
        for (var c = 0; c < chains.length; ++c) {
            if (_matchesChain(candidate, chains[c], chains[c].length - 1, node)) {
                found = true;
                return true;
            }
        }
        return false;
    };
    var below = function(start) {
        return _walkItems(start, function(d) { return d !== start && check(d); });
    };
    if (below(node)) {
        return true;
    }
    // `+` and `~` look to the right of the element, and then inside.
    var sibs = _elementSiblings(node);
    for (var i = sibs.indexOf(node) + 1; i > 0 && i < sibs.length; ++i) {
        if (check(sibs[i]) || below(sibs[i])) {
            return true;
        }
    }
    return found;
}

function _matchesTest(node, test) {
    switch (test.kind) {
    case "any": return true;
    case "never": return false;
    case "tag": return _nodeTag(node) === test.name;
    case "id": return _nodeId(node) === test.name;
    case "class": return classList(node).contains(test.name);
    case "attr": return _matchesAttrTest(node, test);
    case "plain": return Boolean(test.test(node));
    case "not": return !_matchesList(node, test.list);
    case "is": return _matchesList(node, test.list);
    case "has": return _matchesHas(node, test.list);
    case "nth": return _matchesNth(node, test);
    }
    return false;
}

// `anchor` is the element a `:has()` argument is relative to: the chain's
// first compound must stand in its leading combinator's relation to it.
function _matchesChain(node, chain, idx, anchor) {
    if (_isInternalHelperNode(node)) {
        return false;
    }
    var tests = chain[idx].tests;
    for (var t = 0; t < tests.length; ++t) {
        if (!_matchesTest(node, tests[t])) {
            return false;
        }
    }
    var comb = chain[idx].comb;
    var next = function(other) {
        if (idx === 0) {
            return other === anchor;
        }
        return _matchesChain(other, chain, idx - 1, anchor);
    };
    if (idx === 0 && !anchor) {
        return true;
    }
    if (comb === ">") {
        var parent = _elementParent(node);
        return parent ? next(parent) : false;
    }
    if (comb === "+" || comb === "~") {
        var sibs = _elementSiblings(node);
        for (var i = sibs.indexOf(node) - 1; i >= 0; --i) {
            if (next(sibs[i])) {
                return true;
            }
            if (comb === "+") {
                return false;
            }
        }
        return false;
    }
    var up = _elementParent(node);
    while (up) {
        if (next(up)) {
            return true;
        }
        up = _elementParent(up);
    }
    return false;
}

function _matchesList(node, list) {
    for (var i = 0; i < list.length; ++i) {
        if (_matchesChain(node, list[i], list[i].length - 1, null)) {
            return true;
        }
    }
    return false;
}

// What `:scope` means while a selector runs.
var _selectorScope = null;

function _matchesSelectorComplex(node, selector) {
    if (!node) {
        return false;
    }
    var list = _parseSelector(selector);
    if (_isInternalHelperNode(node)) {
        return false;
    }
    var saved = _selectorScope;
    _selectorScope = node;
    var result = _matchesList(node, list);
    _selectorScope = saved;
    return result;
}

// The elements under `startNode`, and `startNode` itself when it is the
// document's root element, in document order.
function _elementsUnder(startNode, keep, firstOnly) {
    var rootNode = startNode || _root;
    var results = [];
    if (!rootNode) {
        return results;
    }
    _walkItems(rootNode, function(node) {
        if ((node !== rootNode || !startNode) && !_isInternalHelperNode(node) && keep(node)) {
            results.push(node);
            return firstOnly;
        }
        return false;
    });
    return results;
}

function _queryTree(startNode, selector, firstOnly) {
    var list = _parseSelector(selector);
    var saved = _selectorScope;
    _selectorScope = startNode || _root;
    var results = _elementsUnder(startNode, function(node) {
        return _matchesList(node, list);
    }, firstOnly);
    _selectorScope = saved;
    return results;
}

function _byClassNames(startNode, classNames) {
    var names = String(classNames).trim().split(/\s+/).filter(Boolean);
    return _liveCollection(function() {
        if (names.length === 0) {
            return [];
        }
        return _elementsUnder(startNode, function(node) {
            var cl = classList(node);
            return names.every(function(c) { return cl.contains(c); });
        }, false);
    });
}

function _byTagName(startNode, tagName) {
    var tag = String(tagName).toLowerCase();
    return _liveCollection(function() {
        return _elementsUnder(startNode, function(node) {
            return tag === "*" || _nodeTag(node) === tag;
        }, false);
    });
}

function _findRootScroller() {
    if (!_root) {
        return null;
    }
    if (typeof _root.contentX === "number" && typeof _root.contentY === "number") {
        return _root;
    }
    var containers = [_root];
    if (_root.contentItem && _root.contentItem !== _root) {
        containers.push(_root.contentItem);
    }
    for (var c = 0; c < containers.length; ++c) {
        var kids = containers[c].children;
        if (!kids) {
            continue;
        }
        for (var i = 0; i < kids.length; ++i) {
            var ch = kids[i];
            if (ch && typeof ch.contentX === "number" && typeof ch.contentY === "number") {
                return ch;
            }
        }
    }
    return null;
}

function _scrollerMaxExtent(scroller, axis) {
    if (!scroller) {
        return 0;
    }
    var isY = axis === "y";
    var viewProp = isY ? "height" : "width";
    var contentProp = isY ? "contentHeight" : "contentWidth";
    var viewDim = (typeof scroller[viewProp] === "number" && scroller[viewProp] > 0)
        ? scroller[viewProp]
        : (_root && typeof _root[viewProp] === "number" ? _root[viewProp] : 0);
    var contentDim = typeof scroller[contentProp] === "number" ? scroller[contentProp] : 0;
    if (scroller.contentItem && scroller.contentItem.children) {
        var cKids = scroller.contentItem.children;
        for (var i = 0; i < cKids.length; ++i) {
            var ck = cKids[i];
            if (!ck) { continue; }
            var impProp = isY ? "implicitHeight" : "implicitWidth";
            var d = (typeof ck[viewProp] === "number" && ck[viewProp] > 0)
                ? ck[viewProp]
                : (typeof ck[impProp] === "number" ? ck[impProp] : 0);
            if (d > contentDim) {
                contentDim = d;
            }
        }
    }
    return (viewDim > 0 && contentDim > viewDim) ? Math.max(0, contentDim - viewDim) : 0;
}

function scrollTo(xOrOpts, y) {
    var scroller = _findRootScroller();
    if (!scroller) {
        return;
    }
    var curX = typeof scroller.contentX === "number" ? scroller.contentX : 0;
    var curY = typeof scroller.contentY === "number" ? scroller.contentY : 0;
    var nextX = curX;
    var nextY = curY;
    if (xOrOpts && typeof xOrOpts === "object") {
        if (typeof xOrOpts.left === "number" && !isNaN(xOrOpts.left)) { nextX = xOrOpts.left; }
        if (typeof xOrOpts.top === "number" && !isNaN(xOrOpts.top)) { nextY = xOrOpts.top; }
    } else {
        if (typeof xOrOpts === "number" && !isNaN(xOrOpts)) { nextX = xOrOpts; }
        if (typeof y === "number" && !isNaN(y)) { nextY = y; }
    }
    var maxX = _scrollerMaxExtent(scroller, "x");
    var maxY = _scrollerMaxExtent(scroller, "y");
    var clampedX = Math.max(0, Math.min(nextX, maxX));
    var clampedY = Math.max(0, Math.min(nextY, maxY));
    scroller.contentX = clampedX;
    scroller.contentY = clampedY;
    if ((clampedX !== curX || clampedY !== curY) && _root) {
        _dispatchDynamic(_root, "scroll", null);
    }
}

function scrollBy(xOrOpts, y) {
    var scroller = _findRootScroller();
    if (!scroller) {
        return;
    }
    var curX = typeof scroller.contentX === "number" ? scroller.contentX : 0;
    var curY = typeof scroller.contentY === "number" ? scroller.contentY : 0;
    if (xOrOpts && typeof xOrOpts === "object") {
        var dx = typeof xOrOpts.left === "number" && !isNaN(xOrOpts.left) ? xOrOpts.left : 0;
        var dy = typeof xOrOpts.top === "number" && !isNaN(xOrOpts.top) ? xOrOpts.top : 0;
        scrollTo(curX + dx, curY + dy);
    } else {
        var nx = typeof xOrOpts === "number" && !isNaN(xOrOpts) ? xOrOpts : 0;
        var ny = typeof y === "number" && !isNaN(y) ? y : 0;
        scrollTo(curX + nx, curY + ny);
    }
}

var scroll = scrollTo;

var document = {
    get title() { return getTitle(); },
    set title(v) { setTitle(v); },
    get baseURI() { return _getDocumentBaseUrl(); },
    get URL() { return _locState.href; },
    get documentURI() { return _locState.href; },
    get body() {
        var html = _taggedHtml();
        var kids = html ? _rawElementChildren(html) : [];
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].eldurTag === "body") {
                return wrap(kids[i]);
            }
        }
        return _root ? wrap(_root.contentItem || _root) : null;
    },
    get documentElement() {
        var html = _taggedHtml();
        return html ? wrap(html) : (_root ? wrap(_root) : null);
    },
    get head() { return _root ? wrap(_root) : null; },
    get hidden() { return false; },
    get visibilityState() { return "visible"; },
    get activeElement() {
        if (_focusHeld && (_focusHeld.visible === false || _focusHeld.activeFocus)) {
            return wrap(_focusHeld);
        }
        if (_activeElement && _focusGone(_activeElement)) {
            _activeElement = null;
        }
        if (_activeElement && (_activeElement.activeFocus || _activeElement.focus)) {
            return wrap(_activeElement);
        }
        var focused = null;
        if (_root) {
            _walkItems(_root, function(node) {
                if (node !== _root && (node.activeFocus || node.focus) && !_focusGone(node)) {
                    focused = node;
                }
                return false;
            });
        }
        if (focused) {
            return wrap(focused);
        }
        // A node with no focus of its own to ask, such as an inline one.
        if (_activeElement && typeof _activeElement.focus !== "boolean") {
            return wrap(_activeElement);
        }
        return this.body;
    },
    getElementById: function(id) {
        var targetId = String(id);
        if (targetId === "") {
            return null;
        }
        var found = _elementsUnder(null, function(node) {
            return _nodeId(node) === targetId;
        }, true);
        return found.length > 0 ? wrap(found[0]) : null;
    },
    querySelector: function(selector) {
        var matches = _queryTree(null, selector, true);
        return matches.length > 0 ? wrap(matches[0]) : null;
    },
    querySelectorAll: function(selector) {
        return _asNodeList(_queryTree(null, selector, false).map(wrap));
    },
    getElementsByClassName: function(classNames) {
        return _byClassNames(null, classNames);
    },
    getElementsByTagName: function(tagName) {
        return _byTagName(null, tagName);
    },
    createElement: function(tagName) {
        // An option never is a QML object: its control lists it.
        if (_vHasRules() && /^(option|optgroup)$/i.test(String(tagName))) {
            return wrap(_vElement(String(tagName).toLowerCase(), [], null, null));
        }
        var tag = String(tagName).toLowerCase();
        var qmlSrc = _genericElementSource(tag, "text");
        var created = Qt.createQmlObject(qmlSrc, _qobjectParent(), "eldur_el_" + tag);
        _meta(created).createdTag = tag;
        if (_gActive()) {
            _gLive.push(created);
        }
        if (tag === "input") {
            _meta(created).lastGenericInputSource = qmlSrc;
        }
        _connectControls(created);
        if (created && created.parent !== undefined) {
            created.parent = null;
        }
        return wrap(created);
    },
    createTextNode: function(text) {
        var created = Qt.createQmlObject("import QtQuick; Text { " + _NATIVE_TEXT + "}", _qobjectParent(), "eldur_text");
        if (created) {
            created.text = String(text);
            _meta(created).createdTag = "#text";
            if (created.parent !== undefined) {
                created.parent = null;
            }
        }
        return wrap(created);
    },
    createDocumentFragment: function() {
        var created = Qt.createQmlObject("import QtQuick; Item {}", _qobjectParent(), "eldur_frag");
        if (created) {
            _meta(created).isFragment = true;
            if (created.parent !== undefined) {
                created.parent = null;
            }
        }
        return wrap(created);
    },
    addEventListener: function(type, fn, opts) {
        listen(document, type, fn, opts);
    },
    removeEventListener: function(type, fn, opts) {
        unlisten(document, type, fn, opts);
    },
    dispatchEvent: function(ev) {
        return dispatchEvent(document, ev);
    },
    write: function() {
        console.warn("eldur: document.write() is unsupported");
    }
};

var window = {
    get innerWidth() {
        return _root && typeof _root.width === "number" ? _root.width : 0;
    },
    get innerHeight() {
        return _root && typeof _root.height === "number" ? _root.height : 0;
    },
    get scrollX() {
        _layoutForRead();
        var scroller = _findRootScroller();
        return scroller && typeof scroller.contentX === "number" ? scroller.contentX : 0;
    },
    get scrollY() {
        _layoutForRead();
        var scroller = _findRootScroller();
        return scroller && typeof scroller.contentY === "number" ? scroller.contentY : 0;
    },
    get pageXOffset() {
        return this.scrollX;
    },
    get pageYOffset() {
        return this.scrollY;
    },
    devicePixelRatio: 1,
    document: document,
    localStorage: localStorage,
    sessionStorage: sessionStorage,
    location: location,
    history: history,
    navigator: navigator,
    performance: performance,
    setTimeout: setTimeout,
    clearTimeout: clearTimeout,
    setInterval: setInterval,
    clearInterval: clearInterval,
    requestAnimationFrame: requestAnimationFrame,
    cancelAnimationFrame: cancelAnimationFrame,
    queueMicrotask: queueMicrotask,
    structuredClone: structuredClone,
    fetch: fetch,
    alert: alert,
    confirm: confirm,
    prompt: prompt,
    atob: atob,
    btoa: btoa,
    matchMedia: matchMedia,
    getComputedStyle: getComputedStyle,
    scrollTo: scrollTo,
    scrollBy: scrollBy,
    scroll: scroll,
    Event: Event,
    Option: Option,
    FormData: FormData,
    CustomEvent: CustomEvent,
    EventTarget: EventTarget,
    KeyboardEvent: KeyboardEvent,
    MouseEvent: MouseEvent,
    PointerEvent: PointerEvent,
    FocusEvent: FocusEvent,
    InputEvent: InputEvent,
    WheelEvent: WheelEvent,
    MutationObserver: MutationObserver,
    ResizeObserver: ResizeObserver,
    IntersectionObserver: IntersectionObserver,
    close: function() {
        if (_root && typeof _root.close === "function") {
            _root.close();
        } else {
            Qt.quit();
        }
    },
    open: function(url) {
        Qt.openUrlExternally(String(url));
    },
    addEventListener: function(type, fn, opts) {
        listen(window, type, fn, opts);
    },
    removeEventListener: function(type, fn, opts) {
        unlisten(window, type, fn, opts);
    },
    dispatchEvent: function(ev) {
        return dispatchEvent(window, ev);
    }
};

// ---------------------------------------------------------------------------
// Created elements (docs/spec/dom-api.md, Created elements).
//
// A script's elements take the page's CSS here, from the same rule table the
// inline nodes use: which rules match is only known when the script runs. Their
// boxes have one form and are placed by this pass, since a Qt layout has no
// margin collapsing, no baseline line boxes and no synchronous answer for a
// script reading a size back right after an insert. The boxes it places answer
// to the page's own layouts through their implicit size and `Layout.*`.
// ---------------------------------------------------------------------------

var _gDirty = false;
var _gGen = 0;
var _gLive = [];
var _gMetricsCache = {};
var _gRunning = false;
var _gStaticBase = null;
var _gClosedHidden = null;
var _gStaticRoot = "";
var _gReplaced = new Map();
var _gAdopted = new Map();
var _gLayoutReader = null;
var _gGrave = null;
var _gCascadeRules = null;
var _gPaintTable = null;
var _gWarned = {};
var _gMarginHelper = null;

var _G_CONTROL = /^(button|input|textarea|select|img|table|thead|tbody|tfoot|tr|td|th|caption|colgroup|col|canvas|video|audio|svg|iframe)$/;
var _G_INHERITED = ["color", "font-family", "font-weight", "font-style", "text-align", "visibility", "white-space"];
var _G_INITIAL = {
    "display": "inline", "position": "static", "box-sizing": "content-box", "opacity": "1",
    "background-color": "rgba(0, 0, 0, 0)", "flex-direction": "row", "flex-wrap": "nowrap",
    "justify-content": "normal", "align-items": "normal", "align-self": "auto", "flex-grow": "0",
    "flex-shrink": "1", "flex-basis": "auto", "order": "0", "row-gap": "normal", "column-gap": "normal",
    "width": "auto", "height": "auto", "min-width": "auto", "min-height": "auto", "max-width": "none",
    "max-height": "none", "top": "auto", "right": "auto", "bottom": "auto", "left": "auto", "z-index": "auto",
    "margin-top": "0px", "margin-right": "0px", "margin-bottom": "0px", "margin-left": "0px",
    "padding-top": "0px", "padding-right": "0px", "padding-bottom": "0px", "padding-left": "0px",
    "border-top-width": "medium", "border-right-width": "medium", "border-bottom-width": "medium",
    "border-left-width": "medium", "border-top-style": "none", "border-right-style": "none",
    "border-bottom-style": "none", "border-left-style": "none", "border-top-left-radius": "0px",
    "border-top-right-radius": "0px", "border-bottom-right-radius": "0px", "border-bottom-left-radius": "0px",
    "overflow-x": "visible", "overflow-y": "visible", "grid-template-columns": "none",
    "background-image": "none", "box-shadow": "none", "transform": "none", "transform-origin": "50% 50%",
    "filter": "none", "animation-name": "none", "transition-property": "all", "transition-duration": "0s",
    "transition-delay": "0s", "transition-timing-function": "ease", "background-size": "auto",
    "background-position-x": "0%", "background-position-y": "0%", "background-repeat": "repeat",
    "text-shadow": "none", "outline-style": "none", "outline-width": "medium",
    "outline-color": "currentcolor", "outline-offset": "0px", "object-fit": "fill", "object-position": "50% 50%", "background-origin": "padding-box", "background-clip": "border-box", "background-attachment": "scroll"
};
// The controls whose box the UA sheet in the rule table describes: padding and
// border are the control's insets.
var _G_FRAMED = { button: true, input: true, select: true, textarea: true };

// A settle may swap the item a read started out with.
function _gCurrent(raw) {
    while (raw && typeof raw === "object" && _gReplaced.has(raw)) {
        raw = _gReplaced.get(raw);
    }
    return raw;
}

function _gActive() {
    return Boolean(_root) && _root.eldurOpenElements === true;
}

// A page all of whose elements have templates still holds the rules when it
// has margins to work out around created elements or styles to read back.
function _gHasCascade() {
    return _gActive() || (Boolean(_root) && typeof _root.eldurCascadeRules === "string");
}

function _gRules() {
    if (_gActive()) {
        return _vLoadRules();
    }
    if (!_gCascadeRules) {
        _gCascadeRules = _gHasCascade() ? _vReadRules(_root.eldurCascadeRules) : [];
    }
    return _gCascadeRules;
}

function _gTouch() {
    _holdFocus();
    if (!_gHasCascade()) {
        return;
    }
    _gDirty = true;
    _layoutChanged(_root);
}

function _gIsBox(raw) {
    return Boolean(raw) && raw.eldurCreatedBox === true;
}

// The one form of a created element that is neither a control nor a line of an
// inline node. `eldurFlowForm` boxes wait in a slot: see `_leaveFlow`.
function _gBoxSource(tag, outOfFlow) {
    return "import QtQuick; import QtQuick.Layouts; Rectangle { color: \"transparent\"; "
        + "property bool eldurCreatedBox: true; "
        + (outOfFlow ? "property Item eldurSlot: null; property var eldurBlock: \"parent\"; " : "")
        + "property real eldurMt: 0; property real eldurMr: 0; property real eldurMb: 0; property real eldurMl: 0; "
        + "property real eldurPw: -1; property bool eldurFill: false; property int eldurAlign: 0; "
        + "property int eldurColorMs: 0; property int eldurColorWait: 0; property int eldurColorEase: 0; "
        + "property int eldurFadeMs: 0; property int eldurFadeWait: 0; property int eldurFadeEase: 0; "
        + "Behavior on color { enabled: eldurColorMs + eldurColorWait > 0; SequentialAnimation { "
        + "PauseAnimation { duration: eldurColorWait } ColorAnimation { duration: eldurColorMs; easing.type: eldurColorEase } } } "
        + "Behavior on opacity { enabled: eldurFadeMs + eldurFadeWait > 0; SequentialAnimation { "
        + "PauseAnimation { duration: eldurFadeWait } NumberAnimation { duration: eldurFadeMs; easing.type: eldurFadeEase } } } "
        + "property real eldurGapT: 0; property real eldurGapL: 0; property real eldurTrack: -1; "
        // A box narrower than its grid track keeps the track's width in its
        // margin, as the converter's own grid items do: the track has no other size.
        + "readonly property real eldurTrackLeft: eldurTrack < 0 ? 0 : eldurTrack - eldurMl - eldurMr; "
        + "Layout.topMargin: eldurMt + eldurGapT; Layout.rightMargin: eldurMr + (eldurPw >= 0 ? Math.max(0, eldurTrackLeft - eldurPw) : 0); "
        + "Layout.bottomMargin: eldurMb; Layout.leftMargin: eldurMl + eldurGapL; "
        + "Layout.minimumWidth: eldurTrack < 0 ? 0 : eldurPw >= 0 ? Math.min(eldurPw, eldurTrackLeft) : eldurTrackLeft; "
        + "Layout.maximumWidth: eldurTrack < 0 ? Number.POSITIVE_INFINITY : eldurPw >= 0 ? Math.min(eldurPw, eldurTrackLeft) : eldurTrackLeft; "
        + "Layout.fillWidth: eldurFill; Layout.preferredWidth: eldurPw; Layout.alignment: eldurAlign; "
        + "Text { " + _NATIVE_TEXT + "visible: text !== \"\"; wrapMode: Text.Wrap; color: \"black\"; font.pixelSize: 16 } }";
}

function _gWantsBox(tag) {
    return !_G_CONTROL.test(tag) && !/^(#text|option|optgroup|script|style|template)$/.test(tag);
}

// --- The cascade -----------------------------------------------------------

function _gMatched(raw) {
    var m = _meta(raw);
    if (m.gMatchGen === _gGen && m.gMatched) {
        return m.gMatched;
    }
    var rules = _gRules();
    var tag = _nodeTag(raw);
    var id = _elementIdOf(raw, m);
    var out = [];
    for (var i = 0; i < rules.length; ++i) {
        if (rules[i].sel.indexOf("::") === -1 && _vRuleMatchesFor(rules[i], raw, m, tag, id)) {
            out.push(i);
        }
    }
    m.gMatched = out;
    m.gMatchGen = _gGen;
    return out;
}

function _elementIdOf(raw, m) {
    if (raw.objectName) {
        return raw.objectName;
    }
    return m.attrs && m.attrs.id ? String(m.attrs.id) : "";
}

// The later of two declarations of equal weight wins, so `>=`.
function _gOffer(won, prop, value, weight) {
    if (!won[prop] || weight >= won[prop].weight) {
        won[prop] = { value: value, weight: weight };
    }
}

function _gWeight(rule, important) {
    var origin = important ? (rule.ua ? 4 : 3) : (rule.ua ? 0 : 1);
    // An `!important` declaration in an earlier layer beats a later one's.
    var layer = rule.layer === undefined ? (important ? 0 : 99) : (important ? 98 - rule.layer : rule.layer);
    return origin * _V_ORIGIN_BAND + layer * _V_LAYER_BAND + rule.rank;
}

function _gInlineDecls(raw, m) {
    var out = [];
    var keys = Object.keys(m.styles);
    if (m.staticStyle === undefined) {
        m.staticStyle = typeof raw.eldurStyle === "string" ? raw.eldurStyle : "";
    }
    var seen = {};
    var push = function(prop, text) {
        var important = /!\s*important\s*$/i.test(text);
        out.push([prop, text.replace(/!\s*important\s*$/i, "").trim(), important]);
    };
    var parts = m.staticStyle.split(";");
    for (var p = 0; p < parts.length; ++p) {
        var colon = parts[p].indexOf(":");
        if (colon > 0) {
            var staticProp = parts[p].slice(0, colon).trim().toLowerCase();
            // A script's write, or its clearing, replaced the attribute's.
            if (!Object.prototype.hasOwnProperty.call(m.styles, staticProp.indexOf("--") === 0 ? staticProp : _kebabToCamel(staticProp))) {
                push(staticProp, parts[p].slice(colon + 1).trim());
            }
        }
    }
    for (var s = 0; s < keys.length; ++s) {
        var kebab = keys[s].indexOf("--") === 0 ? keys[s] : _camelToKebab(keys[s]);
        if (!seen[kebab] && String(m.styles[keys[s]]) !== "") {
            seen[kebab] = true;
            push(kebab, String(m.styles[keys[s]]));
        }
    }
    return out;
}

function _gVars(raw, parent) {
    var inherited = parent ? _gStyle(parent).vars : {};
    var own = null;
    var rules = _gRules();
    var matched = _gMatched(raw);
    var won = {};
    for (var i = 0; i < matched.length; ++i) {
        var rule = rules[matched[i]];
        for (var d = 0; d < rule.decls.length; ++d) {
            if (rule.decls[d][0].indexOf("--") === 0) {
                _gOffer(won, rule.decls[d][0], rule.decls[d][1], _gWeight(rule, rule.decls[d][2]));
            }
        }
    }
    var inline = _gInlineDecls(raw, _meta(raw));
    for (var n = 0; n < inline.length; ++n) {
        if (inline[n][0].indexOf("--") === 0) {
            _gOffer(won, inline[n][0], inline[n][1], (inline[n][2] ? 3.5 : 2) * _V_ORIGIN_BAND);
        }
    }
    var names = Object.keys(won);
    if (names.length === 0) {
        return inherited;
    }
    own = Object.assign({}, inherited);
    for (var k = 0; k < names.length; ++k) {
        var written = won[names[k]].value;
        if (written === "inherit" || written === "unset" || written === "revert") {
            own[names[k]] = inherited[names[k]];
        } else if (written === "initial") {
            delete own[names[k]];
        } else {
            own[names[k]] = written;
        }
    }
    // A variable may name another declared on this element.
    for (var v = 0; v < names.length; ++v) {
        if (own[names[v]] !== undefined) {
            var resolved = _gSubstitute(own[names[v]], own, [names[v]]);
            if (resolved === null) {
                delete own[names[v]];
            } else {
                own[names[v]] = resolved;
            }
        }
    }
    return own;
}

// `var(--x, fallback)` written out; `null` when nothing is left of it, which
// makes the declaration "invalid at computed-value time".
function _gSubstitute(value, vars, path) {
    var text = String(value);
    for (var at = text.indexOf("var("); at !== -1; at = text.indexOf("var(")) {
        var depth = 0;
        var end = -1;
        var comma = -1;
        for (var i = at + 3; i < text.length; ++i) {
            if (text[i] === "(") {
                ++depth;
            } else if (text[i] === ")") {
                if (--depth === 0) {
                    end = i;
                    break;
                }
            } else if (text[i] === "," && depth === 1 && comma === -1) {
                comma = i;
            }
        }
        if (end === -1) {
            return null;
        }
        var name = text.slice(at + 4, comma === -1 ? end : comma).trim();
        var fallback = comma === -1 ? null : text.slice(comma + 1, end).trim();
        var got = null;
        if (vars[name] !== undefined && path.indexOf(name) === -1) {
            got = _gSubstitute(vars[name], vars, path.concat([name]));
        }
        if (got === null && fallback !== null) {
            got = _gSubstitute(fallback, vars, path);
        }
        if (got === null) {
            return null;
        }
        text = text.slice(0, at) + got + text.slice(end + 1);
    }
    return text;
}

function _gSplitValue(text) {
    var parts = [];
    var depth = 0;
    var cur = "";
    for (var i = 0; i < text.length; ++i) {
        var c = text[i];
        if (c === "(") {
            ++depth;
        } else if (c === ")") {
            --depth;
        }
        if (depth === 0 && /\s/.test(c)) {
            if (cur) {
                parts.push(cur);
            }
            cur = "";
        } else {
            cur += c;
        }
    }
    if (cur) {
        parts.push(cur);
    }
    return parts;
}

function _gSides(parts) {
    var a = parts[0];
    var b = parts[1] !== undefined ? parts[1] : a;
    var c = parts[2] !== undefined ? parts[2] : a;
    var d = parts[3] !== undefined ? parts[3] : b;
    return [a, b, c, d];
}

// Only a shorthand whose value held `var()` reaches the runtime whole: the
// converter expanded the others.
function _gExpand(prop, value) {
    var names = ["top", "right", "bottom", "left"];
    var out = [];
    var parts = _gSplitValue(value);
    if (prop === "margin" || prop === "padding") {
        var sides = _gSides(parts);
        for (var i = 0; i < 4; ++i) {
            out.push([prop + "-" + names[i], sides[i]]);
        }
    } else if (prop === "inset") {
        var insets = _gSides(parts);
        for (var n = 0; n < 4; ++n) {
            out.push([names[n], insets[n]]);
        }
    } else if (prop === "border-radius") {
        var corners = _gSides(parts);
        var cornerNames = ["top-left", "top-right", "bottom-right", "bottom-left"];
        for (var r = 0; r < 4; ++r) {
            out.push(["border-" + cornerNames[r] + "-radius", corners[r]]);
        }
    } else if (prop === "gap") {
        out.push(["row-gap", parts[0]], ["column-gap", parts[1] !== undefined ? parts[1] : parts[0]]);
    } else if (prop === "background") {
        var whole = _gPaintValue("background", value);
        if (whole) {
            out.push(["background-color", whole.color === null ? "rgba(0, 0, 0, 0)" : whole.color],
                ["background-image", whole.image === null ? "none" : whole.image]);
        } else {
            // `background: none` names no colour: the shorthand resets it.
            out.push(["background-color", /^none$/i.test(String(value).trim()) ? "rgba(0, 0, 0, 0)" : value]);
        }
    } else if (prop === "outline") {
        var lineWidth = "medium";
        var lineStyle = "none";
        var lineColor = "currentcolor";
        for (var lp = 0; lp < parts.length; ++lp) {
            if (/^(none|hidden|dotted|dashed|solid|double|groove|ridge|inset|outset|auto)$/.test(parts[lp])) {
                lineStyle = parts[lp];
            } else if (/^(thin|medium|thick|[-+.\d].*|calc\(.*)$/.test(parts[lp])) {
                lineWidth = parts[lp];
            } else {
                lineColor = parts[lp];
            }
        }
        out.push(["outline-width", lineWidth], ["outline-style", lineStyle], ["outline-color", lineColor]);
    } else if (/^border(-(top|right|bottom|left))?$/.test(prop)) {
        var which = prop === "border" ? names : [prop.slice(7)];
        var width = "medium";
        var style = "none";
        var color = "currentcolor";
        for (var p = 0; p < parts.length; ++p) {
            if (/^(none|hidden|dotted|dashed|solid|double|groove|ridge|inset|outset)$/.test(parts[p])) {
                style = parts[p];
            } else if (/^(thin|medium|thick|[-+.\d].*|calc\(.*)$/.test(parts[p])) {
                width = parts[p];
            } else {
                color = parts[p];
            }
        }
        for (var w = 0; w < which.length; ++w) {
            out.push(["border-" + which[w] + "-width", width], ["border-" + which[w] + "-style", style],
                ["border-" + which[w] + "-color", color]);
        }
    } else if (/^border-(width|style|color)$/.test(prop)) {
        var vals = _gSides(parts);
        for (var b = 0; b < 4; ++b) {
            out.push(["border-" + names[b] + prop.slice(6), vals[b]]);
        }
    } else {
        out.push([prop, value]);
    }
    return out;
}

function _gOfferDecl(won, prop, value, weight, vars) {
    var text = value;
    if (text.indexOf("var(") !== -1) {
        text = _gSubstitute(text, vars, []);
        if (text === null) {
            text = "unset";
        }
    }
    var longhands = _gExpand(prop, text);
    for (var i = 0; i < longhands.length; ++i) {
        _gOffer(won, longhands[i][0], longhands[i][1], weight);
    }
}

function _gNumber(text) {
    var n = parseFloat(text);
    return isNaN(n) ? 0 : n;
}

// `calc()` of lengths, in px. `basis` is what a percentage is of; `null` makes
// a percentage unresolvable, and the whole value with it.
function _gPx(value, basis, size, rootSize) {
    var text = String(value).trim().toLowerCase();
    if (text === "" || text === "auto" || text === "none" || text === "normal") {
        return null;
    }
    var pos = 0;
    var failed = false;
    function skip() {
        while (pos < text.length && /\s/.test(text[pos])) {
            ++pos;
        }
    }
    function atom() {
        skip();
        if (text[pos] === "(" ) {
            ++pos;
            var inner = sum();
            skip();
            ++pos;
            return inner;
        }
        if (text.indexOf("calc(", pos) === pos) {
            pos += 4;
            return atom();
        }
        var match = /^[-+]?(\d*\.\d+|\d+)(e[-+]?\d+)?([a-z%]*)/.exec(text.slice(pos));
        if (!match) {
            failed = true;
            pos = text.length;
            return 0;
        }
        pos += match[0].length;
        var n = parseFloat(match[0]);
        switch (match[3]) {
        case "": return { n: n, unitless: true };
        case "px": return { n: n };
        case "em": return { n: n * size };
        case "rem": return { n: n * rootSize };
        case "%":
            if (basis === null || basis === undefined) {
                failed = true;
                return { n: 0 };
            }
            return { n: n * basis / 100 };
        case "vw": return { n: n * _root.width / 100 };
        case "vh": return { n: n * _root.height / 100 };
        case "vmin": return { n: n * Math.min(_root.width, _root.height) / 100 };
        case "vmax": return { n: n * Math.max(_root.width, _root.height) / 100 };
        case "pt": return { n: n * 96 / 72 };
        case "pc": return { n: n * 16 };
        case "in": return { n: n * 96 };
        case "cm": return { n: n * 96 / 2.54 };
        case "mm": return { n: n * 96 / 25.4 };
        default:
            failed = true;
            return { n: 0 };
        }
    }
    function product() {
        var left = atom();
        for (;;) {
            skip();
            var op = text[pos];
            if (op !== "*" && op !== "/") {
                return left;
            }
            ++pos;
            var right = atom();
            left = { n: op === "*" ? left.n * right.n : left.n / right.n, unitless: left.unitless && right.unitless };
        }
    }
    function sum() {
        var left = product();
        for (;;) {
            skip();
            var op = text[pos];
            if (op !== "+" && op !== "-") {
                return left;
            }
            ++pos;
            var right = product();
            left = { n: op === "+" ? left.n + right.n : left.n - right.n };
        }
    }
    var got = sum();
    return failed ? null : got.n;
}

function _gWeightNumber(text) {
    if (text === "normal") {
        return 400;
    }
    if (text === "bold") {
        return 700;
    }
    return parseFloat(text) || 400;
}

function _gParent(raw) {
    var p = _logicalParent(raw);
    if (p && p.__eldurInline) {
        return null;
    }
    if (p && p === raw) {
        return null;
    }
    return p || null;
}

// What a script's element computes to, by DOM place.
function _gStyle(raw) {
    var m = _meta(raw);
    if (m.gStyle && m.gStyleGen === _gGen) {
        return m.gStyle;
    }
    var parent = _gParent(raw);
    var inherited = parent ? _gStyle(parent) : null;
    var vars = _gVars(raw, parent);
    var rules = _gRules();
    var matched = _gMatched(raw);
    var won = {};
    for (var i = 0; i < matched.length; ++i) {
        var rule = rules[matched[i]];
        for (var d = 0; d < rule.decls.length; ++d) {
            var decl = rule.decls[d];
            if (decl[0].indexOf("--") !== 0) {
                _gOfferDecl(won, decl[0], decl[1], _gWeight(rule, decl[2]), vars);
            }
        }
    }
    var inline = _gInlineDecls(raw, m);
    for (var n = 0; n < inline.length; ++n) {
        if (inline[n][0].indexOf("--") !== 0) {
            _gOfferDecl(won, inline[n][0], inline[n][1], (inline[n][2] ? 3.5 : 2) * _V_ORIGIN_BAND, vars);
        }
    }
    var tag = _nodeTag(raw);
    var s = { vars: vars, tag: tag, won: won };
    var value = function(prop) {
        var got = won[prop] ? won[prop].value : undefined;
        var inheritable = _G_INHERITED.indexOf(prop) !== -1 || prop === "font-size" || prop === "line-height";
        if (got === "revert" || got === "revert-layer" || got === "unset") {
            got = inheritable ? "inherit" : "initial";
        }
        if (got === undefined) {
            got = inheritable ? "inherit" : "initial";
        }
        return got;
    };
    // font-size first: every em below is of it.
    var parentSize = inherited ? inherited.fontSize : 16;
    var sizeText = value("font-size");
    var keywordSizes = { "xx-small": 9, "x-small": 10, small: 13, medium: 16, large: 18, "x-large": 24, "xx-large": 32, "xxx-large": 48 };
    var rootSize = _gRootFontSize(raw, parent);
    if (sizeText === "inherit") {
        s.fontSize = parentSize;
    } else if (sizeText === "initial") {
        s.fontSize = 16;
    } else if (keywordSizes[sizeText] !== undefined) {
        s.fontSize = keywordSizes[sizeText];
    } else if (sizeText === "smaller") {
        s.fontSize = parentSize / 1.2;
    } else if (sizeText === "larger") {
        s.fontSize = parentSize * 1.2;
    } else {
        var sized = _gPx(sizeText, parentSize, parentSize, rootSize);
        s.fontSize = sized === null ? parentSize : sized;
    }
    s.rootSize = rootSize === null || !parent ? s.fontSize : rootSize;
    var lineText = value("line-height");
    if (lineText === "inherit") {
        s.lineFactor = inherited ? inherited.lineFactor : null;
        s.linePx = inherited ? inherited.linePx : null;
    } else if (lineText === "normal" || lineText === "initial") {
        s.lineFactor = null;
        s.linePx = null;
    } else if (/^[-+.\d]+$/.test(lineText)) {
        s.lineFactor = parseFloat(lineText);
        s.linePx = null;
    } else {
        s.lineFactor = null;
        s.linePx = _gPx(lineText, s.fontSize, s.fontSize, s.rootSize);
    }
    for (var h = 0; h < _G_INHERITED.length; ++h) {
        var name = _G_INHERITED[h];
        var text = value(name);
        if (text === "initial") {
            text = { "color": "rgb(0, 0, 0)", "font-family": "", "font-weight": "400", "font-style": "normal", "text-align": "start", "visibility": "visible", "white-space": "normal" }[name];
        } else if (text === "inherit") {
            if (inherited) {
                text = inherited[name];
            } else {
                text = { "color": "rgb(0, 0, 0)", "font-family": "", "font-weight": "400", "font-style": "normal", "text-align": "start", "visibility": "visible", "white-space": "normal" }[name];
            }
        }
        s[name] = text;
    }
    s.color = s.color === undefined ? s["color"] : s.color;
    var color = _gColor(s["color"], inherited ? inherited["color"] : "rgb(0, 0, 0)");
    s["color"] = color;
    s["font-weight"] = String(_gResolveWeight(s["font-weight"], inherited ? inherited["font-weight"] : "400"));
    var initialNames = Object.keys(_G_INITIAL);
    for (var q = 0; q < initialNames.length; ++q) {
        var key = initialNames[q];
        var own = value(key);
        if (own === "inherit") {
            s[key] = inherited ? inherited[key] : _G_INITIAL[key];
        } else {
            s[key] = own === "initial" ? _G_INITIAL[key] : own;
        }
    }
    s["background-color"] = _gColor(s["background-color"], color);
    var sides = ["top", "right", "bottom", "left"];
    for (var b = 0; b < 4; ++b) {
        var side = sides[b];
        var borderColor = value("border-" + side + "-color");
        s["border-" + side + "-color"] = _gColor(borderColor === "initial" || borderColor === "inherit" ? "currentcolor" : borderColor, color);
        var style = s["border-" + side + "-style"];
        var widthText = s["border-" + side + "-width"];
        var known = { thin: 1, medium: 3, thick: 5 };
        var px = known[widthText] !== undefined ? known[widthText] : (_gPx(widthText, null, s.fontSize, s.rootSize) || 0);
        s["border-" + side + "-px"] = style === "none" || style === "hidden" ? 0 : px;
    }
    var flexParent = inherited && (inherited.display === "flex" || inherited.display === "grid" || inherited.display === "inline-flex" || inherited.display === "inline-grid");
    if (flexParent && (s.display === "inline" || s.display === "inline-block")) {
        s.display = "block";
    }
    if ((s.position === "absolute" || s.position === "fixed") && (s.display === "inline" || s.display === "inline-block")) {
        s.display = "block";
    }
    m.gStyle = s;
    m.gStyleGen = _gGen;
    return s;
}

function _gRootFontSize(raw, parent) {
    if (!parent) {
        return null;
    }
    var top = parent;
    for (var up = _gParent(top); up; up = _gParent(top)) {
        top = up;
    }
    return _gStyle(top).fontSize;
}

function _gResolveWeight(text, parentWeight) {
    if (text === "bolder") {
        return _gWeightNumber(parentWeight) >= 600 ? 900 : 700;
    }
    if (text === "lighter") {
        return _gWeightNumber(parentWeight) >= 600 ? 400 : 100;
    }
    return _gWeightNumber(text);
}

function _gColor(text, current) {
    var value = String(text).trim();
    if (value.toLowerCase() === "currentcolor") {
        return current;
    }
    if (value.toLowerCase() === "transparent") {
        return "rgba(0, 0, 0, 0)";
    }
    return _vColor(value, current);
}

// QML's colour strings have no `rgb()`, and take an unknown one as black.
function _gQtColor(text) {
    var m = /^rgba?\(([^)]*)\)$/.exec(String(text).trim());
    if (!m) {
        return String(text);
    }
    var v = m[1].split(/[\s,/]+/).map(Number);
    return Qt.rgba(v[0] / 255, v[1] / 255, v[2] / 255, v.length > 3 ? v[3] : 1);
}

function _gMetrics(s) {
    var family = s["font-family"] || "";
    var key = family + "|" + s.fontSize + "|" + s["font-weight"] + "|" + s["font-style"];
    if (!_gMetricsCache[key]) {
        var metrics = Qt.createQmlObject("import QtQuick; FontMetrics {}", _qobjectParent(), "eldur_metrics");
        metrics.font = _gFontOf(s);
        _gMetricsCache[key] = metrics;
    }
    return _gMetricsCache[key];
}

// `font.pixelSize` is an int, as the converter's static text has it.
function _gFontOf(s) {
    var family = _gFamily(s["font-family"]);
    var font = { pixelSize: Math.max(Math.round(s.fontSize), 1), weight: _gWeightNumber(s["font-weight"]), italic: s["font-style"] === "italic" };
    if (family) {
        font.family = family;
    }
    return Qt.font(font);
}

function _gFamily(list) {
    var first = String(list || "").split(",")[0].trim().replace(/^["']|["']$/g, "");
    return first;
}

function _gLineHeight(s) {
    if (s.linePx !== null && s.linePx !== undefined) {
        return s.linePx;
    }
    if (s.lineFactor !== null && s.lineFactor !== undefined) {
        return s.lineFactor * s.fontSize;
    }
    return null;
}

// --- Used values -----------------------------------------------------------

function _gEdges(raw, s, basis) {
    var at = function(prop) {
        return _gPx(s[prop], basis, s.fontSize, s.rootSize);
    };
    return {
        mt: at("margin-top"), mr: at("margin-right"), mb: at("margin-bottom"), ml: at("margin-left"),
        pt: at("padding-top") || 0, pr: at("padding-right") || 0, pb: at("padding-bottom") || 0, pl: at("padding-left") || 0,
        bt: s["border-top-px"], br: s["border-right-px"], bb: s["border-bottom-px"], bl: s["border-left-px"]
    };
}

function _gKids(raw) {
    return _rawKids(raw, false).filter(function(kid) {
        return !kid.__eldurInline;
    });
}

function _gOutOfFlow(s) {
    return s.position === "absolute" || s.position === "fixed";
}

function _gInlineLevel(s) {
    return s.display === "inline" || s.display === "inline-block" || s.display === "inline-flex" || s.display === "inline-grid";
}

function _gMerge(a, b) {
    if (a >= 0 && b >= 0) {
        return Math.max(a, b);
    }
    if (a <= 0 && b <= 0) {
        return Math.min(a, b);
    }
    return a + b;
}

function _gTextOf(raw) {
    if (!_gIsBox(raw)) {
        return null;
    }
    return _findTextChild(raw);
}

function _gIsFlex(s) {
    return s.display === "flex" || s.display === "inline-flex";
}

function _gIsGrid(s) {
    return s.display === "grid" || s.display === "inline-grid";
}

// The width of a box's content on one line.
function _gIntrinsic(raw, s) {
    var e = _gEdges(raw, s, 0);
    var frame = e.pl + e.pr + e.bl + e.br;
    if (!_gIsBox(raw)) {
        return _gPx(s.width, null, s.fontSize, s.rootSize) !== null
            ? _gOuterWidth(s, e, _gPx(s.width, null, s.fontSize, s.rootSize))
            : (raw.implicitWidth || 0) + (_gControlFrame(raw) ? frame : 0);
    }
    var fixed = _gPx(s.width, null, s.fontSize, s.rootSize);
    if (fixed !== null) {
        return _gOuterWidth(s, e, fixed);
    }
    var best = 0;
    var text = _gTextOf(raw);
    if (text && text.text !== "") {
        best = Math.ceil(text.implicitWidth);
    }
    var kids = _gBoxKids(raw);
    var row = _gIsFlex(s) && String(s["flex-direction"]).indexOf("row") === 0;
    var line = 0;
    var gap = _gPx(s["column-gap"], null, s.fontSize, s.rootSize) || 0;
    var count = 0;
    for (var i = 0; i < kids.length; ++i) {
        var ks = _gStyle(kids[i]);
        if (ks.display === "none" || _gOutOfFlow(ks)) {
            continue;
        }
        var ke = _gEdges(kids[i], ks, 0);
        var outer = _gIntrinsic(kids[i], ks) + (ke.ml || 0) + (ke.mr || 0);
        if (row || _gInlineLevel(ks)) {
            line += outer + (count > 0 && row ? gap : 0);
            ++count;
        } else {
            best = Math.max(best, line, outer);
            line = 0;
            count = 0;
        }
    }
    return Math.max(best, line) + frame;
}

// A button's label wraps, unless the white-space it inherits forbids it.
function _gLabelled(raw, s) {
    return s.tag === "button" && raw.contentItem && typeof raw.contentItem.lineCount === "number";
}

// As narrow as a button goes: its widest word, or its whole label on one line.
function _gMinContent(raw, s) {
    var whole = _gIntrinsic(raw, s);
    var white = s["white-space"];
    if (white === "nowrap" || white === "pre") {
        return whole;
    }
    var label = raw.contentItem;
    label.wrapMode = 1; // Text.WordWrap
    var metrics = _vMetrics(label);
    var widest = 0;
    String(raw.text).split(/\s+/).forEach(function(word) {
        widest = Math.max(widest, metrics.advanceWidth(word));
    });
    return whole - Math.max(0, metrics.advanceWidth(String(raw.text)) - widest);
}

function _gOuterWidth(s, e, content) {
    return s["box-sizing"] === "border-box" ? Math.max(content, e.pl + e.pr + e.bl + e.br) : content + e.pl + e.pr + e.bl + e.br;
}

function _gOuterHeight(s, e, content) {
    return s["box-sizing"] === "border-box" ? Math.max(content, e.pt + e.pb + e.bt + e.bb) : content + e.pt + e.pb + e.bt + e.bb;
}

function _gControlFrame(raw) {
    return !_gIsBox(raw) && _meta(raw).createdTag && !raw.background;
}

// Places a box in `avail` px of its container's width: sets its width, height
// and what's inside. `opts`: shrink (take the content's width), forceW / forceH
// (a flex or grid container says). Returns `{ mt, mb }`, the margins that show
// outside it once its first and last children's have collapsed through it.
function _gPlace(raw, avail, opts, heightBasis) {
    opts = opts || {};
    var s = _gStyle(raw);
    var e = _gEdges(raw, s, avail);
    var frameW = e.pl + e.pr + e.bl + e.br;
    var frameH = e.pt + e.pb + e.bt + e.bb;
    var size = function(prop, basis) {
        return _gPx(s[prop], basis, s.fontSize, s.rootSize);
    };
    var box = _gIsBox(raw);
    var width = size("width", avail);
    var used;
    if (opts.forceW !== undefined) {
        used = opts.forceW;
    } else if (width !== null) {
        used = _gOuterWidth(s, e, width);
    } else if (opts.shrink || !box && _gInlineLevel(s)) {
        used = Math.min(_gIntrinsic(raw, s), Math.max(avail - (e.ml || 0) - (e.mr || 0), 0));
        if (opts.shrink === "max") {
            used = _gIntrinsic(raw, s);
        } else if (_gLabelled(raw, s)) {
            used = Math.max(used, _gMinContent(raw, s));
        }
    } else {
        used = Math.max(avail - (e.ml || 0) - (e.mr || 0), frameW);
    }
    var maxW = size("max-width", avail);
    if (maxW !== null && opts.forceW === undefined) {
        used = Math.min(used, _gOuterWidth(s, e, maxW));
    }
    var minW = size("min-width", avail);
    if (minW !== null && opts.forceW === undefined) {
        used = Math.max(used, _gOuterWidth(s, e, minW));
    }
    if (raw.width !== used) {
        raw.width = used;
    }
    var innerW = Math.max(used - frameW, 0);
    var basisH = heightBasis === undefined ? null : heightBasis;
    var height = opts.forceH !== undefined ? null : size("height", basisH);
    var fixedH = opts.forceH !== undefined ? opts.forceH : (height !== null ? _gOuterHeight(s, e, height) : null);
    var margins = { mt: e.mt || 0, mb: e.mb || 0 };
    if (!box) {
        return _gPlaceLeaf(raw, s, e, used, fixedH, margins, frameH);
    }
    var content = { height: 0, top: 0, bottom: 0, through: false };
    var innerH = fixedH === null ? null : Math.max(fixedH - frameH, 0);
    if (_gIsFlex(s) || _gIsGrid(s)) {
        _gLinesLeave(raw);
    }
    if (_gIsFlex(s)) {
        content.height = _gLayoutFlex(raw, s, e, innerW, innerH);
    } else if (_gIsGrid(s)) {
        content.height = _gLayoutGrid(raw, s, e, innerW, innerH);
    } else {
        content = _gLayoutFlow(raw, s, e, innerW, fixedH === null, innerH);
    }
    var usedH = fixedH !== null ? fixedH : content.height + frameH;
    var maxH = size("max-height", basisH);
    if (maxH !== null && opts.forceH === undefined) {
        usedH = Math.min(usedH, _gOuterHeight(s, e, maxH));
    }
    var minH = size("min-height", basisH);
    if (minH !== null && opts.forceH === undefined) {
        usedH = Math.max(usedH, _gOuterHeight(s, e, minH));
    }
    if (raw.height !== usedH) {
        raw.height = usedH;
    }
    if (raw.implicitHeight !== usedH) {
        raw.implicitHeight = usedH;
    }
    if (raw.implicitWidth !== used) {
        raw.implicitWidth = used;
    }
    margins.mt = _gMerge(margins.mt, content.top);
    margins.mb = _gMerge(margins.mb, content.bottom);
    margins.empty = content.through && usedH === 0;
    _gPaintBox(raw, s, e);
    return margins;
}

function _gPlaceLeaf(raw, s, e, used, fixedH, margins, frameH) {
    _gPaintControl(raw, s, e);
    _gPaintPicture(raw, s, used, fixedH === null ? raw.height : fixedH);
    // A created inline element with only text is a `Text` itself.
    if (_meta(raw).createdTag && _isPlainTextItem(raw)) {
        _gPaintText(raw, s, used);
    }
    var height = fixedH;
    if (height === null) {
        height = raw.implicitHeight;
        if (_gLabelled(raw, s) && raw.contentItem.lineCount > 1) {
            var line = _gLineHeight(s);
            height += (raw.contentItem.lineCount - 1) * (line === null ? _gControlLine(s) : line);
        }
    }
    if (raw.height !== height) {
        raw.height = height;
    }
    return margins;
}

// What flows in a box, text nodes too: not its own `Text`, not virtual nodes.
function _gBoxKids(raw) {
    return _rawKids(raw, true).filter(function(kid) {
        return !kid.__eldurInline && (kid.objectName || _meta(kid).createdTag || !_isPlainTextItem(kid));
    });
}

// A box's kids in order: a run of what shares lines, or one block.
function _gFlowGroups(raw) {
    var kids = _gBoxKids(raw);
    var groups = [];
    var run = null;
    for (var i = 0; i < kids.length; ++i) {
        var ks = _gStyle(kids[i]);
        // Its slot is what sits here, and it is placed against its containing block.
        if (_gOutOfFlow(ks) && ks.display !== "none") {
            continue;
        }
        if (_gInlineLevel(ks)) {
            if (!run) {
                run = { run: [], lines: null };
                groups.push(run);
            }
            run.run.push(kids[i]);
        } else if (ks.display === "none" && run) {
            run.run.push(kids[i]);
        } else {
            run = null;
            groups.push({ kid: kids[i] });
        }
    }
    return groups;
}

// Whether the box's children are already what `groups` says. A line made here
// is anonymous, so the DOM still sees its items as the box's.
function _gLinesMatch(raw, groups) {
    var at = 0;
    for (var i = 0; i < raw.children.length; ++i) {
        var child = raw.children[i];
        var group = groups[at];
        if (_isAnonLines(child)) {
            if (!group || !group.run || child.children.length !== group.run.length) {
                return false;
            }
            for (var k = 0; k < group.run.length; ++k) {
                if (child.children[k] !== group.run[k]) {
                    return false;
                }
            }
            group.lines = child;
            ++at;
        } else if (group && group.kid === child) {
            ++at;
        } else if (groups.some(function(g) { return g.kid === child || g.run && g.run.indexOf(child) !== -1; })) {
            return false;
        }
    }
    return at === groups.length;
}

// A flex or grid container's children are its items, not a line's.
function _gLinesLeave(raw) {
    for (var i = raw.children.length - 1; i >= 0; --i) {
        if (_isAnonLines(raw.children[i])) {
            _gUnline(raw.children[i], raw);
        }
    }
}

function _gUnline(lines, parent) {
    while (lines.children.length > 0) {
        var kid = lines.children[0];
        kid.parent = null;
        kid.parent = parent;
    }
    lines.parent = null;
    lines.destroy();
}

function _gArrangeLines(raw, groups) {
    if (_gLinesMatch(raw, groups)) {
        return;
    }
    var old = [];
    for (var i = 0; i < raw.children.length; ++i) {
        if (_isAnonLines(raw.children[i])) {
            old.push(raw.children[i]);
        }
    }
    groups.forEach(function(group) {
        if (group.kid) {
            group.kid.parent = null;
            group.kid.parent = raw;
            return;
        }
        var lines = _newLines(raw);
        lines.parent = raw;
        group.run.forEach(function(kid) {
            kid.parent = null;
            kid.parent = lines;
        });
        group.lines = lines;
    });
    old.forEach(function(lines) { _gUnline(lines, raw); });
}

// One box's run of inline content on the one layouter there is. Returns its height.
function _gLayOutRun(group, s, innerW, x, y) {
    var lines = group.lines;
    var whitespace = s["white-space"];
    lines.font = _gFontOf(s);
    lines.color = _gQtColor(s.color);
    var line = _gLineHeight(s);
    lines.lineHeight = line === null ? 0 : line;
    lines.noWrap = whitespace === "nowrap" || whitespace === "pre";
    lines.preserveSpace = whitespace !== "normal" && whitespace !== "nowrap";
    var align = s["text-align"];
    // Text.AlignHCenter, AlignRight, AlignJustify, AlignLeft.
    lines.textAlign = align === "center" || align === "-webkit-center" ? 4
        : align === "right" || align === "end" || align === "-webkit-right" ? 2
        : align === "justify" ? 8 : 1;
    for (var i = 0; i < group.run.length; ++i) {
        var kid = group.run[i];
        if (_meta(kid).createdTag === "#text") {
            _dressLineText(lines, kid);
            continue;
        }
        var ks = _gStyle(kid);
        if (ks.display === "none") {
            _gHide(kid);
            continue;
        }
        kid.visible = true;
        var ke = _gEdges(kid, ks, innerW);
        _gPlace(kid, innerW, { shrink: true }, null);
        _gLineMargins(kid, ke);
        if (_isPlainTextItem(kid)) {
            // The line places it, on its natural height.
            _gSet(kid, "lineHeightMode", 0);
            _gSet(kid, "lineHeight", 1);
            _gSet(kid, "wrapMode", 1);
            lines.setRunLineHeight(kid, _gLineHeight(ks));
        }
    }
    lines.x = x;
    lines.y = y;
    lines.width = innerW;
    lines.layout();
    return lines.implicitHeight;
}

// Block children stack, with their margins collapsing; inline-level ones share
// lines. The box's own `Text` is the anonymous line before them.
function _gLayoutFlow(raw, s, e, innerW, autoH, innerH) {
    var x = e.bl + e.pl;
    var top = e.bt + e.pt;
    var y = top;
    var pending = 0;
    var first = true;
    var through = 0;
    var collapseTop = e.pt === 0 && e.bt === 0 && s["overflow-y"] === "visible" && s.display !== "inline-block" && s.display !== "flow-root";
    var collapseBottom = autoH && e.pb === 0 && e.bb === 0 && s["overflow-y"] === "visible" && s.display !== "inline-block" && s.display !== "flow-root";
    var text = _gTextOf(raw);
    if (text && text.text !== "") {
        _gPaintText(text, s, innerW);
        text.x = x;
        text.y = y + _gLineShift(s);
        y += text.height;
        first = false;
    } else if (text) {
        _gPaintText(text, s, innerW);
        text.x = x;
        text.y = y;
    }
    var groups = _gFlowGroups(raw);
    _gArrangeLines(raw, groups);
    for (var i = 0; i < groups.length; ++i) {
        var group = groups[i];
        if (group.run) {
            var height = _gLayOutRun(group, s, innerW, x, first && collapseTop ? y : y + pending);
            if (height > 0) {
                if (first && collapseTop) {
                    through = pending;
                } else {
                    y += pending;
                }
                pending = 0;
                first = false;
                y += height;
            }
            continue;
        }
        var kid = group.kid;
        var ks = _gStyle(kid);
        if (ks.display === "none") {
            _gHide(kid);
            continue;
        }
        kid.visible = true;
        var ke = _gEdges(kid, ks, innerW);
        var got = _gPlace(kid, innerW, null, innerH);
        var kidWidth = kid.width;
        var left = ke.ml === null && ke.mr === null ? (innerW - kidWidth) / 2
            : ke.ml === null ? innerW - kidWidth - (ke.mr || 0)
            : ke.ml;
        kid.x = x + left;
        var merged = _gMerge(pending, got.mt);
        if (got.empty) {
            kid.y = y + merged;
            pending = _gMerge(merged, got.mb);
            continue;
        }
        if (first && collapseTop) {
            through = merged;
            kid.y = y;
        } else {
            kid.y = y + merged;
        }
        y = kid.y + kid.height;
        pending = got.mb;
        first = false;
    }
    var bottom = 0;
    if (collapseBottom && !first) {
        bottom = pending;
    } else if (!first) {
        y += pending;
    } else if (collapseTop && collapseBottom) {
        through = pending;
    }
    return { height: y - top, top: through, bottom: bottom, through: first && collapseTop && collapseBottom };
}

// A browser centres the glyphs in a line on a whole-pixel baseline; a fixed QML
// line puts them at its top (`whole_baseline` in the converter's text.rs).
function _gLineShift(s) {
    var line = _gLineHeight(s);
    if (line === null) {
        return 0;
    }
    var metrics = _gMetrics(s);
    var ascent = Math.round(metrics.ascent);
    return Math.floor((line - ascent - Math.round(metrics.descent)) / 2);
}

function _gHide(kid) {
    kid.visible = false;
}

function _gLayoutFlex(raw, s, e, innerW, innerH) {
    var row = String(s["flex-direction"]).indexOf("row") === 0;
    var reversed = String(s["flex-direction"]).indexOf("reverse") !== -1;
    var wrap = s["flex-wrap"] !== "nowrap";
    var rowGap = _gPx(s["row-gap"], innerH, s.fontSize, s.rootSize) || 0;
    var colGap = _gPx(s["column-gap"], innerW, s.fontSize, s.rootSize) || 0;
    var mainGap = row ? colGap : rowGap;
    var crossGap = row ? rowGap : colGap;
    var mainSize = row ? innerW : innerH;
    var items = [];
    var kids = _gKids(raw);
    for (var i = 0; i < kids.length; ++i) {
        var ks = _gStyle(kids[i]);
        if (ks.display === "none") {
            _gHide(kids[i]);
            continue;
        }
        kids[i].visible = true;
        if (_gOutOfFlow(ks)) {
            continue;
        }
        var ke = _gEdges(kids[i], ks, innerW);
        var basis = _gPx(ks["flex-basis"], mainSize, ks.fontSize, ks.rootSize);
        var content = null;
        if (row) {
            var kw = basis !== null ? basis : _gPx(ks.width, innerW, ks.fontSize, ks.rootSize);
            content = kw !== null ? _gOuterWidth(ks, ke, kw) : _gIntrinsic(kids[i], ks);
        } else {
            var kh = basis !== null ? basis : _gPx(ks.height, innerH, ks.fontSize, ks.rootSize);
            if (kh !== null) {
                content = _gOuterHeight(ks, ke, kh);
            } else {
                _gPlace(kids[i], innerW, null, null);
                content = kids[i].height;
            }
        }
        var marginMain = row ? (ke.ml || 0) + (ke.mr || 0) : (ke.mt || 0) + (ke.mb || 0);
        items.push({
            kid: kids[i], s: ks, e: ke, size: content, margin: marginMain, order: _gNumber(ks.order), at: items.length,
            grow: _gNumber(ks["flex-grow"]), shrink: _gNumber(ks["flex-shrink"])
        });
    }
    items.sort(function(a, b) {
        return a.order - b.order || a.at - b.at;
    });
    if (reversed) {
        items.reverse();
    }
    var lines = [];
    var cur = [];
    var used = 0;
    for (var n = 0; n < items.length; ++n) {
        var need = items[n].size + items[n].margin;
        if (wrap && mainSize !== null && cur.length > 0 && used + mainGap + need > mainSize) {
            lines.push(cur);
            cur = [];
            used = 0;
        }
        used += need + (cur.length > 0 ? mainGap : 0);
        cur.push(items[n]);
    }
    lines.push(cur);
    var frameX = e.bl + e.pl;
    var frameY = e.bt + e.pt;
    var crossAt = 0;
    var mainEnd = 0;
    var align = s["align-items"];
    for (var l = 0; l < lines.length; ++l) {
        var line = lines[l];
        var taken = 0;
        var grows = 0;
        var shrinks = 0;
        for (var t = 0; t < line.length; ++t) {
            taken += line[t].size + line[t].margin + (t > 0 ? mainGap : 0);
            grows += line[t].grow;
            shrinks += line[t].shrink * line[t].size;
        }
        var free = mainSize === null ? 0 : mainSize - taken;
        for (var g = 0; g < line.length; ++g) {
            if (free > 0 && grows > 0) {
                line[g].size += free * line[g].grow / Math.max(grows, 1);
            } else if (free < 0 && shrinks > 0) {
                line[g].size = Math.max(line[g].size + free * line[g].shrink * line[g].size / shrinks, 0);
            }
        }
        var lineCross = 0;
        for (var c = 0; c < line.length; ++c) {
            var it = line[c];
            if (row) {
                _gPlace(it.kid, innerW, { forceW: it.size }, innerH);
                it.cross = it.kid.height;
                it.crossMargin = (it.e.mt || 0) + (it.e.mb || 0);
            } else {
                var fits = _gPx(it.s.width, innerW, it.s.fontSize, it.s.rootSize) === null
                    && (it.s["align-self"] === "auto" ? align : it.s["align-self"]);
                var stretch = !fits || fits === "normal" || fits === "stretch";
                _gPlace(it.kid, innerW, stretch ? { forceH: it.size } : { forceH: it.size, shrink: true }, innerH);
                it.cross = it.kid.width;
                it.crossMargin = (it.e.ml || 0) + (it.e.mr || 0);
            }
            lineCross = Math.max(lineCross, it.cross + it.crossMargin);
        }
        var crossRoom = row ? innerH : innerW;
        if (lines.length === 1 && crossRoom !== null) {
            lineCross = Math.max(lineCross, crossRoom);
        }
        var rest = mainSize === null ? 0 : mainSize - _gLineTaken(line, mainGap);
        var start = 0;
        var between = mainGap;
        var justify = s["justify-content"];
        var count = line.length;
        if (rest > 0) {
            if (justify === "center") {
                start = rest / 2;
            } else if (justify === "flex-end" || justify === "end" || justify === "right") {
                start = rest;
            } else if (justify === "space-between" && count > 1) {
                between = mainGap + rest / (count - 1);
            } else if (justify === "space-around") {
                start = rest / count / 2;
                between = mainGap + rest / count;
            } else if (justify === "space-evenly") {
                start = rest / (count + 1);
                between = mainGap + rest / (count + 1);
            }
        }
        var pos = start;
        for (var q = 0; q < line.length; ++q) {
            var item = line[q];
            var self = item.s["align-self"] === "auto" ? align : item.s["align-self"];
            var sizeAuto = row
                ? _gPx(item.s.height, innerH, item.s.fontSize, item.s.rootSize) === null
                : _gPx(item.s.width, innerW, item.s.fontSize, item.s.rootSize) === null;
            if ((self === "normal" || self === "stretch") && sizeAuto) {
                var want = lineCross - item.crossMargin;
                if (row && item.kid.height !== want) {
                    _gPlace(item.kid, innerW, { forceW: item.size, forceH: want }, innerH);
                } else if (!row && item.kid.width !== want) {
                    _gPlace(item.kid, innerW, { forceW: want, forceH: item.size }, innerH);
                }
                item.cross = row ? item.kid.height : item.kid.width;
            }
            var crossFree = lineCross - item.cross - item.crossMargin;
            var crossOffset = self === "center" ? crossFree / 2
                : self === "flex-end" || self === "end" ? crossFree : 0;
            if (row) {
                item.kid.x = frameX + pos + (item.e.ml || 0);
                item.kid.y = frameY + crossAt + crossOffset + (item.e.mt || 0);
                pos += item.size + item.margin + between;
            } else {
                item.kid.y = frameY + pos + (item.e.mt || 0);
                item.kid.x = frameX + crossAt + crossOffset + (item.e.ml || 0);
                pos += item.size + item.margin + between;
            }
        }
        mainEnd = Math.max(mainEnd, pos - (line.length > 0 ? between : 0));
        crossAt += lineCross + (l < lines.length - 1 ? crossGap : 0);
    }
    return row ? crossAt : mainEnd;
}

function _gLineTaken(line, gap) {
    var taken = 0;
    for (var i = 0; i < line.length; ++i) {
        taken += line[i].size + line[i].margin + (i > 0 ? gap : 0);
    }
    return taken;
}

function _gTracks(text, width, s, gap) {
    var tokens = _gSplitValue(String(text));
    var expanded = [];
    for (var i = 0; i < tokens.length; ++i) {
        var repeat = /^repeat\(\s*(\d+)\s*,(.*)\)$/.exec(tokens[i]);
        if (repeat) {
            var inner = _gSplitValue(repeat[2].trim());
            for (var r = 0; r < parseInt(repeat[1], 10); ++r) {
                expanded = expanded.concat(inner);
            }
        } else {
            expanded.push(tokens[i]);
        }
    }
    return expanded;
}

// Items take the next cell; `grid-column` and `grid-row` are not in the table.
function _gLayoutGrid(raw, s, e, innerW, innerH) {
    var rowGap = _gPx(s["row-gap"], innerH, s.fontSize, s.rootSize) || 0;
    var colGap = _gPx(s["column-gap"], innerW, s.fontSize, s.rootSize) || 0;
    var written = s["grid-template-columns"];
    var tracks = written === "none" ? ["1fr"] : _gTracks(written);
    var kids = [];
    var all = _gKids(raw);
    for (var i = 0; i < all.length; ++i) {
        var ks = _gStyle(all[i]);
        if (ks.display === "none") {
            _gHide(all[i]);
        } else if (!_gOutOfFlow(ks)) {
            all[i].visible = true;
            kids.push(all[i]);
        }
    }
    var n = tracks.length;
    var room = innerW - colGap * (n - 1);
    var sizes = [];
    var fr = 0;
    var fixed = 0;
    for (var t = 0; t < n; ++t) {
        var track = tracks[t];
        if (/fr$/.test(track)) {
            sizes.push(null);
            fr += parseFloat(track);
        } else if (track === "auto" || /^(min|max)-content$/.test(track)) {
            var widest = 0;
            for (var k = t; k < kids.length; k += n) {
                widest = Math.max(widest, _gIntrinsic(kids[k], _gStyle(kids[k])));
            }
            sizes.push(widest);
            fixed += widest;
        } else {
            var minmax = /^minmax\(.*,\s*(.*)\)$/.exec(track);
            var px = _gPx(minmax ? minmax[1] : track, innerW, s.fontSize, s.rootSize);
            sizes.push(px === null ? 0 : px);
            fixed += px === null ? 0 : px;
        }
    }
    var unit = fr > 0 ? Math.max(room - fixed, 0) / fr : 0;
    var lefts = [];
    var left = e.bl + e.pl;
    for (var c = 0; c < n; ++c) {
        if (sizes[c] === null) {
            sizes[c] = unit * parseFloat(tracks[c]);
        }
        lefts.push(left);
        left += sizes[c] + colGap;
    }
    var y = 0;
    var rows = Math.ceil(kids.length / n);
    for (var r = 0; r < rows; ++r) {
        var tallest = 0;
        for (var a = 0; a < n && r * n + a < kids.length; ++a) {
            var kid = kids[r * n + a];
            var kidStyle = _gStyle(kid);
            var kidEdges = _gEdges(kid, kidStyle, sizes[a]);
            _gPlace(kid, sizes[a], null, null);
            tallest = Math.max(tallest, kid.height + (kidEdges.mt || 0) + (kidEdges.mb || 0));
        }
        for (var b = 0; b < n && r * n + b < kids.length; ++b) {
            var cell = kids[r * n + b];
            var cellStyle = _gStyle(cell);
            var cellEdges = _gEdges(cell, cellStyle, sizes[b]);
            if (_gPx(cellStyle.height, null, cellStyle.fontSize, cellStyle.rootSize) === null) {
                _gPlace(cell, sizes[b], { forceH: tallest - (cellEdges.mt || 0) - (cellEdges.mb || 0) }, null);
            }
            cell.x = lefts[b] + (cellEdges.ml || 0);
            cell.y = e.bt + e.pt + y + (cellEdges.mt || 0);
        }
        y += tallest + (r < rows - 1 ? rowGap : 0);
    }
    return y;
}

// --- Paint -----------------------------------------------------------------

function _gSet(item, prop, value) {
    if (item[prop] !== value) {
        item[prop] = value;
    }
}

function _gPaintText(text, s, innerW) {
    text.font = _gFontOf(s);
    text.color = _gQtColor(s["color"]);
    var line = _gLineHeight(s);
    if (line !== null) {
        _gSet(text, "lineHeightMode", 1);
        _gSet(text, "lineHeight", line);
    } else {
        _gSet(text, "lineHeightMode", 0);
        _gSet(text, "lineHeight", 1);
    }
    var align = s["text-align"];
    _gSet(text, "horizontalAlignment", align === "center" ? 4 : align === "right" || align === "end" ? 2 : align === "justify" ? 8 : 1);
    _gSet(text, "width", innerW);
    // A Text that was hidden has not counted its lines.
    if (text.text !== "") {
        text.forceLayout();
    }
    // Qt gives a fixed line the font's height when that is taller; CSS doesn't.
    _gSet(text, "height", text.text === "" ? 0 : line !== null ? line * text.lineCount : text.implicitHeight);
}

function _gPaintBox(raw, s, e) {
    var painted = _meta(raw).gPainted === true;
    // The first paint is where the box starts, not a change to animate.
    if (painted) {
        _gPaintTimings(raw, s);
    }
    var radius = _gPx(s["border-top-left-radius"], raw.width, s.fontSize, s.rootSize) || 0;
    _gSet(raw, "radius", radius);
    _gSet(raw, "opacity", s.visibility === "hidden" ? 0 : _gNumber(s.opacity));
    var uniform = e.bt === e.br && e.bt === e.bb && e.bt === e.bl
        && s["border-top-color"] === s["border-right-color"] && s["border-top-color"] === s["border-bottom-color"]
        && s["border-top-color"] === s["border-left-color"];
    var drawn = _gPaintDrawnBorder(raw, s, e, uniform);
    _gSet(raw.border, "width", uniform && !drawn ? e.bt : 0);
    if (uniform && !drawn && e.bt > 0) {
        raw.border.color = _gQtColor(s["border-top-color"]);
    }
    _gPaintEdges(raw, s, e, !uniform && !drawn);
    _gPaintOutline(raw, s);
    var z = s["z-index"] === "auto" ? (s.position !== "static" ? 1 : 0) : _gNumber(s["z-index"]);
    _gSet(raw, "z", z);
    _gSet(raw, "clip", s["overflow-x"] !== "visible" || s["overflow-y"] !== "visible");
    _gPlaceRelative(raw, s);
    _gPaintDecor(raw, s, e, radius);
    if (!painted) {
        _meta(raw).gPainted = true;
        _gPaintTimings(raw, s);
    }
}

var _G_SIDES = ["top", "right", "bottom", "left"];

function _gDrawnSource(widths, colors, styles, radii, attrs) {
    return "import QtQuick; import \".\"; EldurBorder { " + attrs
        + "sideWidths: " + JSON.stringify(widths) + "; sideColors: " + JSON.stringify(colors)
        + "; sideStyles: " + JSON.stringify(styles) + "; radii: " + JSON.stringify(radii) + " }";
}

// The picture one Rectangle border and four square strips cannot give, as in
// `needs_drawn_border`. Says whether it took the box's border over.
function _gPaintDrawnBorder(raw, s, e, uniform) {
    var widths = [e.bt, e.br, e.bb, e.bl];
    var colors = _G_SIDES.map(function(side) { return String(_gQtColor(s["border-" + side + "-color"])); });
    var styles = _G_SIDES.map(function(side) { return s["border-" + side + "-style"]; });
    var radii = _gCorners(raw, s);
    var fancy = styles.some(function(style, i) { return widths[i] > 0 && style !== "solid"; });
    var meet = widths.some(function(w, i) {
        var next = (i + 1) % 4;
        return w > 0 && widths[next] > 0 && colors[i] !== colors[next];
    });
    var wanted = fancy || (!uniform && (meet || radii.some(function(r) { return r > 0; })));
    var source = wanted ? _gDrawnSource(widths, colors, styles, radii, "anchors.fill: parent; ") : null;
    _gReplaceShape(raw, "gBorder", source, source, raw, function() {}, Qt.resolvedUrl("eldur_paint"));
    return wanted;
}

// Above what the box holds, taking no room, and on the box's curve grown by its
// distance. A clip would cut it, as a child: a clipping box warns.
function _gPaintOutline(raw, s) {
    var known = { thin: 1, medium: 3, thick: 5 };
    var named = known[s["outline-width"]];
    var width = named !== undefined ? named : _gPx(s["outline-width"], null, s.fontSize, s.rootSize) || 0;
    var style = s["outline-style"];
    var shown = width > 0 && style !== "none" && style !== undefined && style !== "hidden";
    var source = null;
    if (shown) {
        var out = (_gPx(s["outline-offset"], null, s.fontSize, s.rootSize) || 0) + width;
        var radii = _gCorners(raw, s).map(function(r) { return r > 0 ? r + out : 0; });
        var color = String(_gQtColor(_gColor(s["outline-color"], s.color)));
        var solid = style === "solid" || style === "auto";
        source = _gDrawnSource([width, width, width, width], [color, color, color, color],
            [0, 1, 2, 3].map(function() { return solid ? "solid" : style; }), radii,
            "anchors.fill: parent; anchors.margins: " + (-out) + "; z: 1; ");
        if (raw.clip) {
            _gWarn(raw, "outline", "the box clips, and that cuts its outline too");
        }
    }
    _gReplaceShape(raw, "gOutline", source, source, raw, function() {}, Qt.resolvedUrl("eldur_paint"));
}

// A Rectangle's border is one width and colour; unequal sides are four strips.
function _gPaintEdges(raw, s, e, shown) {
    var m = _meta(raw);
    if (!shown && !m.gEdges) {
        return;
    }
    if (!m.gEdges) {
        // `eldurSlot: null` keeps it out of the DOM, as for an out-of-flow box.
        m.gEdges = Qt.createQmlObject("import QtQuick; Item { property Item eldurSlot: null; anchors.fill: parent; "
            + "Rectangle { id: t; anchors { left: parent.left; right: parent.right; top: parent.top } } "
            + "Rectangle { id: r; anchors { right: parent.right; top: parent.top; bottom: parent.bottom } } "
            + "Rectangle { id: b; anchors { left: parent.left; right: parent.right; bottom: parent.bottom } } "
            + "Rectangle { id: l; anchors { left: parent.left; top: parent.top; bottom: parent.bottom } } "
            + "function set(side, size, shade) { const strip = [t, r, b, l][side]; strip.color = shade; "
            + "if (side % 2 === 0) strip.height = size; else strip.width = size; strip.visible = size > 0; } }", raw, "eldur_edges");
    }
    m.gEdges.visible = shown;
    if (shown) {
        m.gEdges.set(0, e.bt, _gQtColor(s["border-top-color"]));
        m.gEdges.set(1, e.br, _gQtColor(s["border-right-color"]));
        m.gEdges.set(2, e.bb, _gQtColor(s["border-bottom-color"]));
        m.gEdges.set(3, e.bl, _gQtColor(s["border-left-color"]));
    }
}

// --- What the converter parsed for a box's paint ----------------------------

// A shadow in `em` is held at 100px to the `em`: see `em_as_hundreds`.
function _gShadowsAt(prop, text, fontSize) {
    var held = _gPaintValue(prop, text);
    if (!held || !held.em) {
        return held;
    }
    var k = fontSize / 100;
    return {
        css: held.css.replace(/-?[\d.]+px/g, function(px) { return parseFloat(px) * k + "px"; }),
        list: held.list.map(function(one) {
            return {
                x: one.x * k, y: one.y * k, spread: one.spread * k, blur: one.blur * k,
                inset: one.inset, c: one.c, a: one.a,
                bands: one.bands.map(function(band) { return [band[0] * k, band[1]]; })
            };
        })
    };
}

function _gPaintValue(prop, text) {
    if (_gPaintTable === null) {
        var written = _root ? _root.eldurPaintValues : undefined;
        _gPaintTable = typeof written === "string" && written !== "" ? JSON.parse(written) : {};
    }
    return _gPaintTable[prop + ":" + String(text).replace(/\s+/g, "")];
}

function _gDescribe(raw) {
    var m = _meta(raw);
    var id = _elementIdOf(raw, m);
    return "<" + _nodeTag(raw) + (id ? " #" + id : "") + (m.classes && m.classes.length ? " ." + m.classes.join(".") : "") + ">";
}

function _gWarn(raw, prop, why) {
    var text = "eldur: " + _gDescribe(raw) + " " + prop + ": " + why;
    if (!_gWarned[text]) {
        _gWarned[text] = true;
        console.warn(text);
    }
}

function _gStopsSource(stops, length) {
    var at = stops.map(function(p) { return p.frac + (length > 0 ? p.px / length : 0); });
    var first = at[0];
    var last = at[at.length - 1];
    // A gradient's stops are 0 to 1: the line is cut to where they are.
    var span = last > first ? last - first : 1;
    // Qt sorts stops by position, and two at one place, a hard edge, may swap.
    var before = -1;
    var parts = stops.map(function(p, i) {
        var position = Math.max((at[i] - first) / span, before + 0.0001);
        before = position;
        return "GradientStop { position: " + Math.min(position, 1) + "; color: \"" + p.c + "\" } ";
    });
    return { first: first, last: last > first ? last : first + 1, text: parts.join("") };
}

function _gGradientPath(layer, w, h, radii) {
    var fill;
    var extra = "";
    var spread = layer.repeating ? "spread: ShapeGradient.RepeatSpread; " : "";
    if (layer.kind === "linear" || layer.kind === "corner") {
        var dx, dy, length;
        if (layer.kind === "linear") {
            var angle = layer.angle * Math.PI / 180;
            dx = Math.sin(angle);
            dy = -Math.cos(angle);
            length = Math.abs(w * dx) + Math.abs(h * dy);
        } else {
            var hyp = Math.max(1, Math.hypot(w, h));
            dx = (layer.right ? h : -h) / hyp;
            dy = (layer.bottom ? w : -w) / hyp;
            length = 2 * w * h / hyp;
        }
        var line = _gStopsSource(layer.stops, length);
        var at = function(f) { return [w / 2 + dx * length * (f - 0.5), h / 2 + dy * length * (f - 0.5)]; };
        var from = at(line.first);
        var to = at(line.last);
        fill = "LinearGradient { " + spread + "x1: " + from[0] + "; y1: " + from[1] + "; x2: " + to[0] + "; y2: " + to[1] + "; " + line.text + "} ";
    } else if (layer.kind === "conic") {
        var turn = layer.stops.slice().reverse().map(function(p) {
            return "GradientStop { position: " + (1 - Math.max(0, Math.min(1, p.frac))) + "; color: \"" + p.c + "\" } ";
        });
        fill = "ConicalGradient { centerX: " + (layer.cx[0] + layer.cx[1] * w / 100) + "; centerY: " + (layer.cy[0] + layer.cy[1] * h / 100)
            + "; angle: " + ((90 - layer.from) % 360 + 360) % 360 + "; " + turn.join("") + "} ";
    } else {
        var cx = layer.cx[0] + layer.cx[1] * w / 100;
        var cy = layer.cy[0] + layer.cy[1] * h / 100;
        var rx = Math.max(cx, w - cx);
        var ry = Math.max(cy, h - cy);
        var r = layer.circle ? Math.hypot(rx, ry) : rx * Math.SQRT2;
        var round = _gStopsSource(layer.stops, r);
        fill = "RadialGradient { " + spread + "centerX: " + cx + "; centerY: " + cy + "; centerRadius: " + r * round.last
            + "; focalX: " + cx + "; focalY: " + cy + "; focalRadius: " + r * round.first + "; " + round.text + "} ";
        if (!layer.circle && rx > 0) {
            extra = "fillTransform: PlanarTransform.fromScale(1, " + ry / rx + ", " + cx + ", " + cy + "); ";
        }
    }
    return "ShapePath { strokeWidth: -1; fillGradient: " + fill + extra + _gRectSource(0, 0, w, h, radii) + "} ";
}

function _gCommaList(text) {
    var out = [];
    var depth = 0;
    var part = "";
    String(text).split("").forEach(function(c) {
        depth += c === "(" ? 1 : c === ")" ? -1 : 0;
        if (c === "," && depth === 0) {
            out.push(part.trim());
            part = "";
        } else {
            part += c;
        }
    });
    out.push(part.trim());
    return out;
}

// Whether one Shape over the padding box can draw this layer.
function _gLayerIsPlain(layer, at) {
    return layer.kind !== "image" && at.size === "auto" && /^0(%|px)?$/.test(at.x) && /^0(%|px)?$/.test(at.y)
        && at.origin === "padding-box" && at.clip === "border-box" && at.attachment !== "fixed";
}

function _gLayerAt(s, l) {
    function pick(name) {
        var list = _gCommaList(s[name]);
        return list[l % list.length];
    }
    return {
        size: pick("background-size"), x: pick("background-position-x"), y: pick("background-position-y"),
        repeat: pick("background-repeat"), origin: pick("background-origin"), clip: pick("background-clip"),
        attachment: pick("background-attachment")
    };
}

// The same item the converter writes for a layer in the page.
function _gLayerSource(raw, s, e, layer, at, outerRadii) {
    var w = raw.width;
    var h = raw.height;
    function padding(side) {
        return _gPx(s["padding-" + side], w, s.fontSize, s.rootSize) || 0;
    }
    var border = { top: e.bt, right: e.br, bottom: e.bb, left: e.bl };
    var content = {
        top: e.bt + padding("top"), right: e.br + padding("right"),
        bottom: e.bb + padding("bottom"), left: e.bl + padding("left")
    };
    var none = { top: 0, right: 0, bottom: 0, left: 0 };
    if (at.clip === "text") {
        _gWarn(raw, "background-clip", "`text` is not converted; the layer fills the box behind the text");
    }
    var clip = at.clip === "padding-box" ? border : at.clip === "content-box" ? content : none;
    var origin = at.origin === "border-box" ? none : at.origin === "content-box" ? content : border;
    var areaW = w - origin.left - origin.right;
    var areaH = h - origin.top - origin.bottom;
    var parts = "anchors.fill: parent; anchors.topMargin: " + clip.top + "; anchors.rightMargin: " + clip.right
        + "; anchors.bottomMargin: " + clip.bottom + "; anchors.leftMargin: " + clip.left
        + "; areaTop: " + (origin.top - clip.top) + "; areaRight: " + (origin.right - clip.right)
        + "; areaBottom: " + (origin.bottom - clip.bottom) + "; areaLeft: " + (origin.left - clip.left) + "; ";
    var given = at.size.split(/\s+/);
    var tileW = areaW;
    var tileH = areaH;
    if (at.size === "cover" || at.size === "contain") {
        parts += "sizing: " + (at.size === "cover" ? 1 : 2) + "; ";
    } else if (at.size !== "auto") {
        parts += "sizing: 3; ";
        var side = function(text, px, percent, basis) {
            if (text === undefined || text === "auto") {
                return null;
            }
            if (/%$/.test(text)) {
                var share = parseFloat(text);
                parts += px + ": 0; " + percent + ": " + share + "; ";
                return share / 100 * basis;
            }
            var value = _gPx(text, basis, s.fontSize, s.rootSize) || 0;
            parts += px + ": " + value + "; ";
            return value;
        };
        var gw = side(given[0], "sizeWidth", "sizeWidthPercent", areaW);
        var gh = side(given[1], "sizeHeight", "sizeHeightPercent", areaH);
        tileW = gw === null ? areaW : gw;
        tileH = gh === null ? areaH : gh;
    }
    var named = { left: "0%", top: "0%", center: "50%", right: "100%", bottom: "100%" };
    function position(written, px, percent, basis) {
        var text = named[written] || written;
        if (/%$/.test(text)) {
            parts += percent + ": " + parseFloat(text) + "; ";
        } else if (/^calc|^\d|^-/.test(text) && !/%/.test(text)) {
            parts += px + ": " + (_gPx(text, basis, s.fontSize, s.rootSize) || 0) + "; ";
        } else {
            _gWarn(raw, "background-position", "`" + text + "` is placed at 0: only lengths, percentages and edge words place a layer here");
        }
    }
    position(at.x, "positionX", "positionXPercent", areaW);
    position(at.y, "positionY", "positionYPercent", areaH);
    var repeat = at.repeat.split(/\s+/);
    var acrossX = repeat[0] !== "no-repeat" && repeat[0] !== "repeat-y";
    var acrossY = (repeat[1] || repeat[0]) !== "no-repeat" && repeat[0] !== "repeat-x";
    parts += "repeatX: " + acrossX + "; repeatY: " + acrossY + "; spaced: " + (repeat[0] === "space")
        + "; rounded: " + (repeat[0] === "round") + "; fixedToWindow: " + (at.attachment === "fixed") + "; ";
    var less = function(r, a, b) { return Math.max(r - Math.max(a, b), 0); };
    parts += "topLeftRadius: " + less(outerRadii[0], clip.top, clip.left)
        + "; topRightRadius: " + less(outerRadii[1], clip.top, clip.right)
        + "; bottomRightRadius: " + less(outerRadii[2], clip.bottom, clip.right)
        + "; bottomLeftRadius: " + less(outerRadii[3], clip.bottom, clip.left) + "; ";
    if (layer.kind === "image") {
        return "EldurBackgroundLayer { " + parts + "source: Qt.resolvedUrl(\"" + layer.source + "\") } ";
    }
    return "EldurBackgroundLayer { " + parts + "isGradient: true; Shape { anchors.fill: parent; "
        + _gGradientPath(layer, tileW, tileH, [0, 0, 0, 0]) + "} } ";
}

function _gRectSource(x, y, w, h, radii) {
    return "PathRectangle { x: " + x + "; y: " + y + "; width: " + Math.max(w, 0) + "; height: " + Math.max(h, 0)
        + "; topLeftRadius: " + Math.max(radii[0], 0) + "; topRightRadius: " + Math.max(radii[1], 0)
        + "; bottomRightRadius: " + Math.max(radii[2], 0) + "; bottomLeftRadius: " + Math.max(radii[3], 0) + " } ";
}

function _gBandColor(c, alpha) {
    var a = Math.max(0, Math.min(255, Math.round(alpha * 255))).toString(16);
    return "#" + (a.length < 2 ? "0" + a : a) + c.slice(3);
}

// A hole has to stay clear for a box with no background of its own, and an odd
// count of edges only says that where the shadow holds the whole box.
// A box with a fill to hide what is under it takes the blur the static page has.
// The formula is the one in `emit/decoration.rs`, fitted there to Chromium's.
function _gOuterBlur(shadow, radii) {
    var corners = radii[0] === radii[1] && radii[1] === radii[2] && radii[2] === radii[3]
        ? "radius: " + radii[0] + "; "
        : "topLeftRadius: " + radii[0] + "; topRightRadius: " + radii[1]
            + "; bottomRightRadius: " + radii[2] + "; bottomLeftRadius: " + radii[3] + "; ";
    return "RectangularShadow { anchors.fill: parent; offset: Qt.vector2d(" + shadow.x + ", " + shadow.y + "); "
        + "blur: " + (shadow.blur > 0 ? 1.2 * shadow.blur + 2 : 0) + "; spread: " + shadow.spread + "; "
        + "color: \"" + _gBandColor(shadow.c, shadow.a) + "\"; " + corners + "} ";
}

function _gOuterRing(shadow, band, w, h, radii) {
    var g = shadow.spread + band[0];
    var x = shadow.x - g;
    var y = shadow.y - g;
    var sw = w + 2 * g;
    var sh = h + 2 * g;
    if (sw <= 0 || sh <= 0) {
        return "";
    }
    var color = _gBandColor(shadow.c, band[1]);
    var head = "ShapePath { strokeWidth: -1; fillColor: \"" + color + "\"; ";
    var grown = radii.map(function(r) { return r > 0 ? r + g : 0; });
    var holds = x <= 0 && y <= 0 && x + sw >= w && y + sh >= h;
    var square = radii.every(function(r) { return r === 0; });
    if (holds || !square) {
        return head + "fillRule: ShapePath.OddEvenFill; " + _gRectSource(x, y, sw, sh, grown) + _gRectSource(0, 0, w, h, radii) + "} ";
    }
    var none = [0, 0, 0, 0];
    var parts = "";
    var left = Math.max(x, 0);
    var right = Math.min(x + sw, w);
    if (y < 0) {
        parts += _gRectSource(x, y, sw, Math.min(-y, sh), none);
    }
    if (y + sh > h) {
        var below = Math.max(y, h);
        parts += _gRectSource(x, below, sw, y + sh - below, none);
    }
    var midTop = Math.max(y, 0);
    var midBottom = Math.min(y + sh, h);
    if (midBottom > midTop) {
        if (x < 0) {
            parts += _gRectSource(x, midTop, Math.min(-x, sw), midBottom - midTop, none);
        }
        if (x + sw > w) {
            var after = Math.max(x, w);
            parts += _gRectSource(after, midTop, x + sw - after, midBottom - midTop, none);
        }
    }
    return parts === "" || right < left - sw ? "" : head + parts + "} ";
}

function _gInsetRing(shadow, band, w, h, radii) {
    var g = shadow.spread + band[0];
    var top = Math.max(shadow.y + g, 0);
    var right = Math.max(g - shadow.x, 0);
    var bottom = Math.max(g - shadow.y, 0);
    var left = Math.max(shadow.x + g, 0);
    if (top + right + bottom + left === 0) {
        return "";
    }
    var shrunk = radii.map(function(r) { return r - Math.max(g, 0); });
    return "ShapePath { strokeWidth: -1; fillColor: \"" + _gBandColor(shadow.c, band[1]) + "\"; fillRule: ShapePath.OddEvenFill; "
        + _gRectSource(0, 0, w, h, radii) + _gRectSource(left, top, w - left - right, h - top - bottom, shrunk) + "} ";
}

function _gShapeSource(attrs, paths) {
    return "import QtQuick; import QtQuick.Effects; import QtQuick.Shapes; import QtQuick.Layouts; Shape { property Item eldurSlot: null; " + attrs + paths + "}";
}

function _gReplaceShape(raw, slot, signature, source, host, place, url) {
    var m = _meta(raw);
    if (m[slot + "Sig"] === signature) {
        return m[slot];
    }
    m[slot + "Sig"] = signature;
    if (m[slot]) {
        m[slot].visible = false;
        m[slot].destroy();
        m[slot] = null;
    }
    if (source !== null) {
        m[slot] = Qt.createQmlObject(source, host, url || "eldur_paint");
        place(m[slot]);
    }
    return m[slot];
}

function _gCorners(raw, s) {
    var names = ["top-left", "top-right", "bottom-right", "bottom-left"];
    return names.map(function(n) {
        return _gPx(s["border-" + n + "-radius"], raw.width, s.fontSize, s.rootSize) || 0;
    });
}

function _gPaintDecor(raw, s, e, radius) {
    var w = raw.width;
    var h = raw.height;
    var pw = w - e.bl - e.br;
    var ph = h - e.bt - e.bb;
    var outerRadii = _gCorners(raw, s);
    if (!outerRadii.every(function(r) { return r === radius; })) {
        _gSet(raw, "topLeftRadius", outerRadii[0]);
        _gSet(raw, "topRightRadius", outerRadii[1]);
        _gSet(raw, "bottomRightRadius", outerRadii[2]);
        _gSet(raw, "bottomLeftRadius", outerRadii[3]);
    }
    var inner = outerRadii.map(function(r) { return Math.max(r - Math.max(e.bt, e.bl), 0); });
    var imageText = s["background-image"];
    var paths = "";
    var placed = [];
    var plainOnly = true;
    if (imageText !== "none") {
        var images = _gPaintValue("background-image", imageText);
        if (!images) {
            _gWarn(raw, "background-image", "`" + imageText + "` is not painted, so only the background colour shows: the converter never saw this value; write it in a style rule");
        } else {
            for (var l = images.layers.length - 1; l >= 0; --l) {
                var layer = images.layers[l];
                var layerAt = _gLayerAt(s, l);
                if (layer.kind === "unpainted") {
                    _gWarn(raw, "background-image", layer.what + " paints nothing");
                } else if (layer.kind !== "none") {
                    // Each drawn in order, so one that needs an item of its
                    // own makes them all items.
                    placed.push(_gLayerIsPlain(layer, layerAt)
                        ? "Shape { x: " + e.bl + "; y: " + e.bt + "; width: " + Math.max(pw, 0) + "; height: " + Math.max(ph, 0) + "; "
                            + _gGradientPath(layer, pw, ph, inner) + "} "
                        : _gLayerSource(raw, s, e, layer, layerAt, outerRadii));
                    plainOnly = plainOnly && _gLayerIsPlain(layer, layerAt);
                    if (plainOnly) {
                        paths += _gGradientPath(layer, pw, ph, inner);
                    }
                }
            }
        }
    }
    var at = "x: " + e.bl + "; y: " + e.bt + "; width: " + Math.max(pw, 0) + "; height: " + Math.max(ph, 0) + "; ";
    var shadowText = s["box-shadow"];
    var outer = "";
    var outerBlurs = "";
    var outerSteps = "";
    var insets = "";
    if (shadowText !== "none") {
        var shadows = _gShadowsAt("box-shadow", shadowText, s.fontSize);
        if (!shadows) {
            _gWarn(raw, "box-shadow", "`" + shadowText + "` is not painted, so the box has no shadow: the converter never saw this value, or it mixes `em` with `px`; write it in a style rule, in one unit");
        } else {
            var opaque = _gQtColor(s["background-color"]).a === 1 || imageText !== "none";
            for (var i = shadows.list.length - 1; i >= 0; --i) {
                var shadow = shadows.list[i];
                for (var b = 0; b < shadow.bands.length; ++b) {
                    if (shadow.inset) {
                        insets += _gInsetRing(shadow, shadow.bands[b], pw, ph, inner);
                    } else if (opaque) {
                        outerSteps += _gOuterRing(shadow, shadow.bands[b], w, h, outerRadii);
                    } else {
                        outer += _gOuterRing(shadow, shadow.bands[b], w, h, outerRadii);
                    }
                }
                if (!shadow.inset && opaque) {
                    outerBlurs += _gOuterBlur(shadow, outerRadii);
                }
                var round = outerRadii.some(function(r) { return r > 0; });
                if (!shadow.inset && round && !opaque && (shadow.x !== 0 || shadow.y !== 0)) {
                    _gWarn(raw, "box-shadow", "an offset shadow of a round box with no background shows through it");
                }
            }
        }
    }
    // Only a child with a negative z is below what the box holds, and that is
    // below the box's own fill too: with layers to show, they take the fill over.
    var shade = _gQtColor(s["background-color"]);
    var clips = _gCommaList(s["background-clip"]);
    var colourClip = clips[clips.length - 1];
    var colourKept = colourClip === "padding-box" || colourClip === "content-box";
    plainOnly = plainOnly && !colourKept;
    var layered = paths !== "" || insets !== "" || !plainOnly;
    raw.color = layered ? "transparent" : shade;
    var under = "ShapePath { strokeWidth: -1; fillColor: \"" + shade + "\"; "
        + _gRectSource(-e.bl, -e.bt, w, h, outerRadii) + "} ";
    var layers = under + paths + insets;
    var attrs = "z: -1; " + at;
    if (plainOnly) {
        _gReplaceShape(raw, "gLayers", layers + at, layered ? _gShapeSource(attrs, layers) : null, raw, function() {});
    } else {
        var byClip = colourClip === "content-box"
            ? function(side) { return (_gPx(s["padding-" + side], w, s.fontSize, s.rootSize) || 0); }
            : function() { return 0; };
        var bottomColour = colourKept
            ? "Rectangle { color: \"" + shade + "\"; x: " + (e.bl + byClip("left")) + "; y: " + (e.bt + byClip("top"))
                + "; width: " + Math.max(w - e.bl - e.br - byClip("left") - byClip("right"), 0)
                + "; height: " + Math.max(h - e.bt - e.bb - byClip("top") - byClip("bottom"), 0) + " } "
            : "Rectangle { anchors.fill: parent; color: \"" + shade + "\"; topLeftRadius: " + outerRadii[0]
                + "; topRightRadius: " + outerRadii[1] + "; bottomRightRadius: " + outerRadii[2]
                + "; bottomLeftRadius: " + outerRadii[3] + " } ";
        var over = insets === "" ? "" : "Shape { x: " + e.bl + "; y: " + e.bt + "; width: " + Math.max(pw, 0)
            + "; height: " + Math.max(ph, 0) + "; " + insets + "} ";
        _gReplaceShape(raw, "gLayers", placed.join("") + bottomColour + over + w + "x" + h,
            "import QtQuick; import QtQuick.Shapes; import \".\"; Item { property Item eldurSlot: null; z: -1; width: " + w + "; height: " + h + "; "
                + bottomColour + placed.join("") + over + "}",
            raw, function() {}, Qt.resolvedUrl("eldur_paint"));
    }
    // The software scene graph draws no `RectangularShadow`, and steps are what it can.
    if (outerBlurs !== "") {
        outer += "Item { anchors.fill: parent; visible: GraphicsInfo.api !== GraphicsInfo.Software; " + outerBlurs + "} "
            + "Shape { anchors.fill: parent; visible: GraphicsInfo.api === GraphicsInfo.Software; " + outerSteps + "} ";
    }
    _gPaintOuterShadow(raw, outer, w, h);
    _gPaintMatrix(raw, s, w, h);
    if (s["text-shadow"] !== "none" && s["text-shadow"] !== undefined) {
        _gWarn(raw, "text-shadow", "the text of a box a script made or restyled is drawn with no shadow; only text the page holds as written has one");
    }
    if (s["animation-name"] !== "none") {
        _gWarn(raw, "animation-name", "`" + s["animation-name"] + "` does not run on a created box: it stays as its rules leave it");
    }
    if (!_gPaintPicture(raw, s, w, h)) {
        _gPaintLayer(raw, s, outerRadii,
            raw.clip && _rawElementChildren(raw).length > 0 && outerRadii.some(function(r) { return r > 0; }));
    }
}

// The layer a box needs. QML's `clip` is square, so a clip to rounded corners is
// a mask; a filter is the same kind of effect; a fading group is the layer alone.
// An effect holds its values as written, so new values make a new one.
function _gPaintLayer(raw, s, radii, clips) {
    var m = _meta(raw);
    var source = "";
    if (s.filter !== "none" && s.filter !== undefined) {
        var filter = _gPaintValue("filter", s.filter);
        if (!filter) {
            _gWarn(raw, "filter", "`" + s.filter + "` is not painted, so the box shows unfiltered: it has a `url()`, or the converter never saw this value; write its functions in a style rule");
        } else {
            source = "EldurFilter { " + filter.source + " }";
            if (clips) {
                _gWarn(raw, "filter", "the box clips to square corners under a filter: one layer holds one effect");
            }
        }
    }
    if (source === "" && clips) {
        source = "EldurRoundClip { topLeftRadius: " + radii[0] + "; topRightRadius: " + radii[1]
            + "; bottomRightRadius: " + radii[2] + "; bottomLeftRadius: " + radii[3] + " }";
    }
    if (m.gLayerEffect !== source) {
        if (source !== "" || m.gLayerEffect !== undefined) {
            raw.layer.effect = source === "" ? null : Qt.createQmlObject(
                "import QtQuick; import \".\"; QtObject { property Component effect: " + source + " }",
                raw, Qt.resolvedUrl("eldur_paint")).effect;
        }
        m.gLayerEffect = source;
    }
    // QML fades each thing drawn; CSS fades the box and what it holds as one.
    var fades = _gNumber(s.opacity) < 1 && _rawElementChildren(raw).length > 0;
    _gSet(raw.layer, "enabled", source !== "" || fades);
}

// A page's `<img>` is an `Image` in an item that keeps the CSS size; a created
// one is the `Image`.
function _gPictureOf(raw) {
    if (raw.source !== undefined) {
        return raw;
    }
    var kids = raw.eldurTag === "img" ? raw.children : [];
    for (var i = 0; i < kids.length; ++i) {
        if (kids[i].source !== undefined && kids[i].fillMode !== undefined) {
            return kids[i];
        }
    }
    return null;
}

function _gPaintPicture(raw, s, w, h) {
    var picture = _gPictureOf(raw);
    if (!picture) {
        return false;
    }
    var radii = _gCorners(raw, s);
    _gPaintLayer(raw, s, radii, radii.some(function(r) { return r > 0; }));
    var fit = s["object-fit"];
    var file = picture.sourceSize;
    // The enums' values: a library script cannot see the QML type `Image`.
    var modes = { fill: 0, contain: 1, cover: 2, none: 6 };
    var mode = modes[fit];
    if (fit === "scale-down") {
        mode = file.width > w || file.height > h ? modes.contain : modes.none;
    }
    if (mode !== undefined) {
        _gSet(picture, "fillMode", mode);
    }
    var names = { left: "0%", center: "50%", right: "100%", top: "0%", bottom: "100%" };
    var at = s["object-position"].trim().split(/\s+/).map(function(part) {
        return names[part] || part;
    });
    var pick = function(value, start, middle, end) {
        var number = parseFloat(value);
        if (number === 0) {
            return start;
        }
        if (value === "50%") {
            return middle;
        }
        if (value === "100%") {
            return end;
        }
        _gWarn(raw, "object-position", "`" + value + "` on an image a script restyled is centred: "
            + "only an edge or the middle can be placed at run time");
        return middle;
    };
    _gSet(picture, "horizontalAlignment", pick(at[0], 1, 4, 2));
    _gSet(picture, "verticalAlignment", pick(at[1] || "50%", 0x20, 0x80, 0x40));
    _gSet(picture, "clip", fit === "none" || fit === "cover");
    return true;
}

// Shadows lie outside the box, where its own clip would take them: a clipping
// box's goes next to it. A layout leaves alone a child with no size to give.
function _gPaintOuterShadow(raw, paths, w, h) {
    var m = _meta(raw);
    var signature = paths + w + "x" + h + raw.clip;
    if (m.gOuterSig === signature) {
        return;
    }
    var source = null;
    if (paths !== "") {
        // A layout moves its children, so the shape sits in an item of no size
        // it can move as it likes, and follows the box from there.
        source = raw.clip && raw.parent
            ? "import QtQuick; import QtQuick.Effects; import QtQuick.Shapes; import QtQuick.Layouts; Item { property Item eldurSlot: null; "
                + "property Item box: null; property alias shape: drawn; Layout.maximumWidth: 0; Layout.maximumHeight: 0; "
                + "z: box ? box.z : 0; opacity: box ? box.opacity : 1; visible: box ? box.visible : false; "
                + "Shape { id: drawn; x: box ? box.x - parent.x : 0; y: box ? box.y - parent.y : 0; width: " + w + "; height: " + h + "; " + paths + "} }"
            : _gShapeSource("z: -2; width: " + w + "; height: " + h + "; ", paths);
    }
    _gReplaceShape(raw, "gOuter", signature, source, raw.clip && raw.parent ? raw.parent : raw, function(item) {
        if (raw.clip) {
            item.box = raw;
        }
    });
    _gSetTransforms(raw);
}

function _gSetTransforms(raw) {
    var m = _meta(raw);
    var list = [];
    if (m.gMatrix) {
        list.push(m.gMatrix);
    }
    if (m.gShift) {
        list.push(m.gShift);
    }
    raw.transform = list;
    // What stands in for the box moves with it.
    if (m.gOuter && m.gOuter.shape) {
        m.gOuter.shape.transform = list;
    }
}

function _gMultiply(a, b) {
    return [a[0] * b[0] + a[2] * b[1], a[1] * b[0] + a[3] * b[1], a[0] * b[2] + a[2] * b[3], a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4], a[1] * b[4] + a[3] * b[5] + a[5]];
}

// The value as `matrix()`, with percentages of this box; `null` for `none`.
function _gMatrixOf(raw, s, w, h, warn) {
    var text = s.transform;
    if (text === "none") {
        return null;
    }
    var value = _gPaintValue("transform", text);
    if (!value) {
        if (warn) {
            _gWarn(raw, "transform", "`" + text + "` is not painted, so the box stays unmoved: the converter never saw this value; write it in a style rule");
        }
        return null;
    }
    var out = [1, 0, 0, 1, 0, 0];
    for (var i = 0; i < value.fns.length; ++i) {
        var f = value.fns[i];
        if (f.t) {
            out = _gMultiply(out, [1, 0, 0, 1,
                f.t[0][0] + f.t[0][1] * w / 100 + (f.t[0][2] || 0) * s.fontSize,
                f.t[1][0] + f.t[1][1] * h / 100 + (f.t[1][2] || 0) * s.fontSize]);
        } else if (f.m) {
            out = _gMultiply(out, f.m);
        } else if (warn) {
            _gWarn(raw, "transform", "`" + f.u + "` has no 2-D form and is left out; the rest of the transform still applies");
        }
    }
    return out;
}

function _gOrigin(s, w, h) {
    var parts = _gSplitValue(s["transform-origin"]);
    var word = { left: "0%", top: "0%", center: "50%", right: "100%", bottom: "100%" };
    var axis = function(text, size) {
        var px = _gPx(word[text] || text, size, s.fontSize, s.rootSize);
        return px === null ? size / 2 : px;
    };
    var first = parts[0] === undefined ? "50%" : parts[0];
    var second = parts[1] === undefined ? "50%" : parts[1];
    if (first === "top" || first === "bottom" || second === "left" || second === "right") {
        var swap = first;
        first = second;
        second = swap;
    }
    return [axis(first, w), axis(second, h)];
}

function _gApplyMatrix(raw, v) {
    _meta(raw).gMatrixNow = v;
    _meta(raw).gMatrix.matrix = Qt.matrix4x4(v[0], v[2], 0, v[4], v[1], v[3], 0, v[5], 0, 0, 1, 0, 0, 0, 0, 1);
}

function _gPaintMatrix(raw, s, w, h) {
    var m = _meta(raw);
    var written = _gMatrixOf(raw, s, w, h, true);
    if (!written && !m.gMatrix) {
        return;
    }
    var origin = _gOrigin(s, w, h);
    var about = _gMultiply(_gMultiply([1, 0, 0, 1, origin[0], origin[1]], written || [1, 0, 0, 1, 0, 0]),
        [1, 0, 0, 1, -origin[0], -origin[1]]);
    var first = !m.gMatrix;
    if (first) {
        m.gMatrix = Qt.createQmlObject("import QtQuick; Matrix4x4 {}", raw, "eldur_matrix");
        _gApplyMatrix(raw, about);
        _gSetTransforms(raw);
        return;
    }
    var now = m.gMatrixNow;
    if (about.every(function(v, i) { return v === (m.gMatrixGoal || now)[i]; })) {
        return;
    }
    m.gMatrixGoal = about;
    if (m.gTweenMs + m.gTweenWait > 0) {
        _gTween(raw, now.slice(), about);
    } else {
        if (m.gTween) {
            m.gTween.stop();
        }
        _gApplyMatrix(raw, about);
    }
}

function _gTween(raw, from, to) {
    var m = _meta(raw);
    if (!m.gTween) {
        m.gTween = Qt.createQmlObject("import QtQuick; Item { id: tween; property Item eldurSlot: null; property real t: 1; "
            + "property int wait: 0; property int ms: 0; property int ease: 0; "
            + "SequentialAnimation { id: run; PauseAnimation { duration: tween.wait } "
            + "NumberAnimation { target: tween; property: \"t\"; from: 0; to: 1; duration: tween.ms; easing.type: tween.ease } } "
            + "function go() { run.restart(); } function stop() { run.stop(); } }", raw, "eldur_tween");
        m.gTween.tChanged.connect(function() {
            var t = m.gTween.t;
            _gApplyMatrix(raw, m.gTweenFrom.map(function(v, i) { return v + (m.gTweenTo[i] - v) * t; }));
        });
    }
    m.gTweenFrom = from;
    m.gTweenTo = to;
    m.gTween.wait = m.gTweenWait;
    m.gTween.ms = m.gTweenMs;
    m.gTween.ease = m.gTweenEase;
    m.gTween.go();
}

function _gSeconds(text) {
    var n = parseFloat(text);
    return isNaN(n) ? 0 : /ms\s*$/.test(text) ? n : n * 1000;
}

function _gPaintTimings(raw, s) {
    var m = _meta(raw);
    var names = String(s["transition-property"]).split(",");
    var times = String(s["transition-duration"]).split(",");
    var waits = String(s["transition-delay"]).split(",");
    var curves = String(s["transition-timing-function"]).split(/,(?![^(]*\))/);
    // Easing.Linear, .InQuad, .OutQuad, .InOutQuad: Qt's nearest to CSS's curves.
    var easeOf = function(text) {
        return { linear: 0, "ease-in": 1, "ease-out": 2 }[text.trim()] || (text.trim() === "linear" ? 0 : 3);
    };
    var got = { color: [0, 0, 3], fade: [0, 0, 3], move: [0, 0, 3] };
    for (var i = 0; i < names.length; ++i) {
        var name = names[i].trim();
        var ms = _gSeconds(times[i % times.length]);
        var wait = _gSeconds(waits[i % waits.length]);
        if (name === "none" || ms + wait <= 0) {
            continue;
        }
        var timing = [ms, wait, easeOf(curves[i % curves.length])];
        if (name === "all" || name === "background-color" || name === "background") {
            got.color = timing;
        }
        if (name === "all" || name === "opacity") {
            got.fade = timing;
        }
        if (name === "all" || name === "transform") {
            got.move = timing;
        }
        if (!/^(all|background-color|background|opacity|transform)$/.test(name)) {
            _gWarn(raw, "transition", "`" + name + "` changes at once on a created box");
        }
    }
    _gSet(raw, "eldurColorMs", got.color[0]);
    _gSet(raw, "eldurColorWait", got.color[1]);
    _gSet(raw, "eldurColorEase", got.color[2]);
    _gSet(raw, "eldurFadeMs", got.fade[0]);
    _gSet(raw, "eldurFadeWait", got.fade[1]);
    _gSet(raw, "eldurFadeEase", got.fade[2]);
    m.gTweenMs = got.move[0];
    m.gTweenWait = got.move[1];
    m.gTweenEase = got.move[2];
}

// --- :hover and the like ----------------------------------------------------

var _G_STATE_WORD = /:(hover|focus-within|focus-visible|focus|active|checked)\b/g;
var _gStateRules = null;

// Each compound that names a state, without it: what has to be watched.
function _gLoadStateRules() {
    if (_gStateRules) {
        return _gStateRules;
    }
    _gStateRules = [];
    var rules = _gRules();
    for (var i = 0; i < rules.length; ++i) {
        var compounds = rules[i].sel.split(/\s*[>+~\s]\s*(?![^(]*\))/);
        for (var c = 0; c < compounds.length; ++c) {
            var kinds = compounds[c].match(_G_STATE_WORD);
            if (!kinds) {
                continue;
            }
            try {
                _gStateRules.push({
                    list: _parseSelector(compounds[c].replace(_G_STATE_WORD, "") || "*"),
                    kinds: kinds.map(function(k) { return k.slice(1); })
                });
            } catch (e) {
                // A selector no element can match has nothing to watch.
            }
        }
    }
    return _gStateRules;
}

function _gWatch(raw, kind) {
    var m = _meta(raw);
    m.gWatching = m.gWatching || {};
    if (m.gWatching[kind]) {
        return;
    }
    m.gWatching[kind] = true;
    var changed = function() { _gTouch(); };
    if (kind === "hover") {
        if (typeof raw.hovered === "boolean") {
            if (typeof raw.hoverEnabled === "boolean") {
                raw.hoverEnabled = true;
            }
            raw.hoveredChanged.connect(changed);
        } else {
            var hover = Qt.createQmlObject("import QtQuick; HoverHandler { blocking: false }", raw, "eldur_hover");
            hover.hoveredChanged.connect(function() {
                m.gHover = hover.hovered;
                _gTouch();
            });
        }
    } else if (kind === "active") {
        if (typeof raw.pressed === "boolean") {
            raw.pressedChanged.connect(changed);
        } else {
            var tap = Qt.createQmlObject("import QtQuick; TapHandler { gesturePolicy: TapHandler.DragThreshold }", raw, "eldur_press");
            tap.pressedChanged.connect(function() {
                m.gPress = tap.pressed;
                _gTouch();
            });
        }
    } else if (kind === "checked") {
        if (raw.checkedChanged) {
            raw.checkedChanged.connect(changed);
        }
    } else if (raw.activeFocusChanged) {
        raw.activeFocusChanged.connect(changed);
    }
}

function _gWatchStates() {
    var rules = _gLoadStateRules();
    if (rules.length === 0) {
        return;
    }
    _walkItems(_root, function(item) {
        if (_gIsElement(item)) {
            for (var r = 0; r < rules.length; ++r) {
                var matches = false;
                try {
                    matches = _matchesList(item, rules[r].list);
                } catch (e) {
                    matches = false;
                }
                if (matches) {
                    rules[r].kinds.forEach(function(kind) { _gWatch(item, kind); });
                }
            }
        }
        return false;
    });
}

function _gPlaceRelative(raw, s) {
    var m = _meta(raw);
    var dx = 0;
    var dy = 0;
    if (s.position === "relative" || s.position === "sticky") {
        var parentWidth = raw.parent ? raw.parent.width : 0;
        var left = _gPx(s.left, parentWidth, s.fontSize, s.rootSize);
        var right = _gPx(s.right, parentWidth, s.fontSize, s.rootSize);
        var top = _gPx(s.top, null, s.fontSize, s.rootSize);
        var bottom = _gPx(s.bottom, null, s.fontSize, s.rootSize);
        dx = left !== null ? left : right !== null ? -right : 0;
        dy = top !== null ? top : bottom !== null ? -bottom : 0;
    }
    if (!m.gShift && dx === 0 && dy === 0) {
        return;
    }
    if (!m.gShift) {
        m.gShift = Qt.createQmlObject("import QtQuick; Translate {}", raw, "eldur_shift");
        _gSetTransforms(raw);
    }
    m.gShift.x = dx;
    m.gShift.y = dy;
}

// A control keeps its form: the cascade reaches what it has.
function _gPaintControl(raw, s, e) {
    if (!_meta(raw).createdTag) {
        return;
    }
    if (raw.background !== undefined && raw.background !== null && raw.font !== undefined) {
        if (_G_FRAMED[s.tag]) {
            _gSet(raw, "topPadding", e.pt + e.bt);
            _gSet(raw, "bottomPadding", e.pb + e.bb);
            _gSet(raw, "leftPadding", e.pl + e.bl);
            _gSet(raw, "rightPadding", e.pr + e.br);
            _gSet(raw, "topInset", 0);
            _gSet(raw, "bottomInset", 0);
            _gSet(raw, "leftInset", 0);
            _gSet(raw, "rightInset", 0);
            _gControlWidth(raw, s, e);
        }
        var inLayout = raw.parent && _gIsLayout(raw.parent);
        if (inLayout && (s.width !== "auto" || s.height !== "auto")) {
            var boxed = s["box-sizing"] === "border-box";
            var wide = _gPx(s.width, raw.parent.width, s.fontSize, s.rootSize);
            var tall = _gPx(s.height, null, s.fontSize, s.rootSize);
            var across = wide === null ? -1 : wide + (boxed ? 0 : e.pl + e.pr + e.bl + e.br);
            var down = tall === null ? -1 : tall + (boxed ? 0 : e.pt + e.pb + e.bt + e.bb);
            _gMarginTool().size(raw, across, down);
        }
        var line = _gLineHeight(s);
        if (raw.contentItem && raw.contentItem.font !== undefined) {
            _gPaintFontOnly(raw, s);
            if (line === null) {
                line = _gControlLine(s);
                // As `emit/controls.rs`: a fallback font's glyph makes the line taller.
                if (/[^\u0000-\u00ff]/.test(String(raw.text))) {
                    line = Math.max(line, raw.contentItem.implicitHeight);
                }
            }
            var frame = e.pt + e.pb + e.bt + e.bb;
            _gSet(raw, "implicitHeight", line + frame);
        }
        // Native until the page sets a border or background, as with
        // `appearance: auto`: the source's own line then stands for the UA's.
        if (_gAuthorBox(s)) {
            if (raw.background.color !== undefined) {
                raw.background.color = _gQtColor(s["background-color"]);
            }
            if (raw.background.border !== undefined) {
                raw.background.border.color = _gQtColor(s["border-top-color"]);
                raw.background.border.width = e.bt;
            }
            if (typeof raw.background.radius === "number") {
                raw.background.radius = _gPx(s["border-top-left-radius"], raw.width, s.fontSize, s.rootSize) || 0;
            }
        }
    }
}

// The converter's `CONTROL_FONTS`: what `size` and `cols` count in this family.
function _gControlFont(s) {
    var table = _root && _root.eldurControlFonts ? JSON.parse(_root.eldurControlFonts) : [];
    var family = _gFamily(s["font-family"]);
    for (var i = 0; i < table.length; ++i) {
        if (table[i].names.length === 0 || table[i].names.indexOf(family) !== -1) {
            return (s.fontSize < 14.5 ? table[i].small : table[i].large);
        }
    }
    return null;
}

// A created control's width when nothing else says: as `emit/controls.rs` works
// out a static one's.
function _gControlWidth(raw, s, e) {
    var frame = e.pl + e.pr + e.bl + e.br;
    var font = _gControlFont(s);
    var whole = Math.max(Math.round(s.fontSize), 1);
    if (s.tag === "select") {
        var metrics = _gMetrics(s);
        var options = _vSelectModel(raw);
        var widest = (options ? _vOptions(options) : []).reduce(function(w, o) {
            return Math.max(w, metrics.advanceWidth(_vOptionText(o)));
        }, 0);
        // Its frame around the text is 22 with the arrow; 2 of that is the border.
        raw.rightPadding = e.pr + e.br + 20;
        _gSet(raw, "implicitWidth", Math.ceil(widest * s.fontSize / whole) + frame + 20);
        return;
    }
    if (!font) {
        return;
    }
    var attr = function(name, fallback) {
        var n = parseInt(getAttribute(raw, name), 10);
        return n > 0 ? n : fallback;
    };
    if (s.tag === "textarea") {
        _gSet(raw, "implicitWidth", attr("cols", 20) * font[0] * s.fontSize + 15 + frame);
        var line = _gLineHeight(s);
        _gSet(raw, "implicitHeight", attr("rows", 2) * (line === null ? _gControlLine(s) : line) + e.pt + e.pb + e.bt + e.bb);
    } else if (s.tag === "input" && !/^(checkbox|radio|range|submit|button|reset)$/.test(_meta(raw).inputType || "")) {
        _gSet(raw, "implicitWidth", Math.round((attr("size", 20) * font[0] + font[1]) * s.fontSize) + frame);
    }
}

// Whether a rule of the page, not the UA sheet, sets `prop`.
function _gAuthor(s, prop) {
    return Boolean(s.won[prop]) && s.won[prop].weight >= _V_ORIGIN_BAND;
}

function _gAuthorBox(s) {
    var props = ["background-color", "background-image"];
    var sides = ["top", "right", "bottom", "left"];
    for (var i = 0; i < 4; ++i) {
        props.push("border-" + sides[i] + "-width", "border-" + sides[i] + "-style", "border-" + sides[i] + "-color");
    }
    return props.some(function(prop) {
        return _gAuthor(s, prop);
    });
}

// A line of a control's text at "normal" line height, as a browser rounds it.
function _gControlLine(s) {
    var metrics = _gMetrics(s);
    return Math.round(metrics.ascent) + Math.round(metrics.descent);
}

function _gPaintFontOnly(raw, s) {
    raw.font = _gFontOf(s);
    if (raw.palette) {
        raw.palette.buttonText = _gQtColor(s["color"]);
    }
}

// --- The pass --------------------------------------------------------------

function _gPrune() {
    var seen = new Set();
    _gLive = _gLive.filter(function(raw) {
        var alive = Boolean(raw) && !seen.has(raw);
        seen.add(raw);
        return alive;
    });
}

// A kid's margins the layout already has come from the conversion, which knew
// nothing of what a script adds: all of a column's gaps are worked out afresh.
function _gMarginTool() {
    if (!_gMarginHelper) {
        _gMarginHelper = Qt.createQmlObject("import QtQuick; import QtQuick.Layouts; QtObject { "
            + "function set(i, t, b) { i.Layout.topMargin = t; i.Layout.bottomMargin = b; } "
            + "function side(i, l, r) { i.Layout.leftMargin = l; i.Layout.rightMargin = r; } "
            + "function size(i, w, h) { i.Layout.preferredWidth = w; i.Layout.minimumWidth = w < 0 ? 0 : w; i.Layout.maximumWidth = w < 0 ? Infinity : w; "
            + "i.Layout.fillWidth = false; i.Layout.preferredHeight = h; i.Layout.minimumHeight = h < 0 ? 0 : h; i.Layout.maximumHeight = h < 0 ? Infinity : h; } "
            + "function shut(i, shut) { i.Layout.maximumHeight = shut ? 0 : Infinity; i.Layout.preferredHeight = shut ? 0 : -1; } "
            // A cell's rounding goes to the next item, as the converter's own
            // `eldurCarry` does (`emit/carry.rs`), and a height can change after.
            // A column's own height is whole: what its last cell was rounded
            // up by is not the box's. The converter knows that of its own items.
            + "function column(p) { if (typeof p.spacing === 'number') return p; "
            + "for (var k = 0; k < p.children.length; ++k) { var c = p.children[k]; "
            + "if (typeof c.spacing === 'number' && c.eldurTag === undefined) return c; } return null; } "
            + "function excess(p) { var c = column(p); if (!c) return 0; "
            + "for (var k = c.children.length - 1; k >= 0; --k) { var i = c.children[k]; "
            + "if (i.visible && i.eldurSlot === undefined) return i.eldurCarry === undefined ? carry(i) : 0; } return 0; } "
            + "function carry(p) { if (!p || !p.parent) return 0; if (p.eldurCarry !== undefined) return p.eldurCarry + excess(p); "
            + "var w = p.Layout.preferredHeight; var fixed = w >= 0 && w !== p.implicitHeight; var h = fixed ? w : p.implicitHeight - excess(p); "
            + "var m = p.Layout.topMargin + p.Layout.bottomMargin; var c = Math.ceil(m + (fixed ? h : Math.ceil(h))) - m - h; "
            + "return c > 1e-6 ? c + 1e-6 : 0; } "
            + "function hold(i, t, b, p) { i.Layout.topMargin = p ? Qt.binding(function() { return t - carry(p); }) : t; i.Layout.bottomMargin = b; } "
            + "function top(i) { return i.Layout.topMargin; } "
            + "function bottom(i) { return i.Layout.bottomMargin; } }", _root, "eldur_margins");
    }
    return _gMarginHelper;
}

function _gIsElement(raw) {
    return typeof raw.eldurTag === "string" || Boolean(_meta(raw).createdTag);
}

function _gIsLayout(item) {
    return Boolean(item) && (typeof item.spacing === "number" || typeof item.columnSpacing === "number");
}

function _gCollapses(raw, s) {
    return (s.display === "block" || s.display === "list-item") && !_gOutOfFlow(s)
        && s["overflow-x"] === "visible" && s["overflow-y"] === "visible" && s.tag !== "html";
}

function _gHasOwnText(raw) {
    if (typeof raw.text === "string" && raw.text !== "") {
        return true;
    }
    for (var i = 0; i < raw.children.length; ++i) {
        var kid = raw.children[i];
        if (typeof kid.text === "string" && kid.text !== "" && !_gIsElement(kid)) {
            return true;
        }
    }
    return false;
}

function _gColumnOf(raw) {
    if (_gIsLayout(raw)) {
        return raw;
    }
    for (var i = 0; i < raw.children.length; ++i) {
        var kid = raw.children[i];
        if (_gIsLayout(kid) && !_gIsElement(kid)) {
            return kid;
        }
    }
    return null;
}

function _gFlowKids(column) {
    var out = [];
    for (var i = 0; i < column.children.length; ++i) {
        var kid = column.children[i];
        if (!kid || kid.eldurSlot !== undefined || _isLayoutSpacer(kid) || !kid.visible) {
            continue;
        }
        if (_gIsElement(kid) && _gOutOfFlow(_gStyle(kid))) {
            continue;
        }
        out.push(kid);
    }
    return out;
}

// What a box's outer edges collapse to, its first and last children's included.
function _gOuter(kid) {
    // A line has no margins, and the one before it still has to be paid.
    if (_isLines(kid) && _isAnonLines(kid)) {
        return { mt: 0, mb: 0, empty: false };
    }
    if (!_gIsElement(kid) || _gStyle(kid).display === "inline") {
        var tool = _gMarginTool();
        return { mt: tool.top(kid), mb: tool.bottom(kid), empty: false, foreign: true };
    }
    if (_gIsBox(kid) && _meta(kid).gMargins) {
        return _meta(kid).gMargins;
    }
    var s = _gStyle(kid);
    var e = _gEdges(kid, s, kid.parent ? kid.parent.width : 0);
    var out = { mt: e.mt || 0, mb: e.mb || 0, empty: false };
    if (!_gCollapses(kid, s) || _gHasOwnText(kid)) {
        return out;
    }
    var column = _gColumnOf(kid);
    var kids = column ? _gFlowKids(column) : [];
    var heightless = _gPx(s.height, null, s.fontSize, s.rootSize);
    var autoHeight = heightless === null && s.height === "auto";
    var i;
    if (e.pt + e.bt === 0) {
        for (i = 0; i < kids.length; ++i) {
            var first = _gOuter(kids[i]);
            out.mt = _gMerge(out.mt, first.mt);
            if (!first.empty) {
                break;
            }
            out.mt = _gMerge(out.mt, first.mb);
        }
    }
    if (e.pb + e.bb === 0 && autoHeight) {
        for (i = kids.length - 1; i >= 0; --i) {
            var last = _gOuter(kids[i]);
            out.mb = _gMerge(out.mb, last.mb);
            if (!last.empty) {
                break;
            }
            out.mb = _gMerge(out.mb, last.mt);
        }
    }
    var allEmpty = kids.every(function(k) { return _gOuter(k).empty; });
    out.empty = allEmpty && e.pt + e.bt + e.pb + e.bb === 0 && (autoHeight || heightless === 0)
        && _G_CONTROL.test(s.tag) === false;
    return out;
}

function _gSetMargins(kid, top, bottom) {
    if (_gIsBox(kid)) {
        _gSet(kid, "eldurMt", top);
        _gSet(kid, "eldurMb", bottom);
        return;
    }
    _gMarginTool().set(kid, top, bottom);
}

// A gap a binding keeps, once. What conversion declared stays until a gap is
// worked out for the kid, as its binding knows only its old neighbour's carry.
function _gHoldGap(kid, top, bottom, prev) {
    if (_gIsBox(kid)) {
        _gSetMargins(kid, top - _gMarginTool().carry(prev), bottom);
        return;
    }
    var held = _meta(kid).gHeld;
    if (held && held.top === top && held.bottom === bottom && held.prev === prev) {
        return;
    }
    _meta(kid).gHeld = { top: top, bottom: bottom, prev: prev };
    _gMarginTool().hold(kid, top, bottom, prev);
}

function _gMarginTop(kid) {
    return _gIsBox(kid) ? kid.eldurMt || 0 : _gMarginTool().top(kid);
}

function _gMarginBottom(kid) {
    return _gIsBox(kid) ? kid.eldurMb || 0 : _gMarginTool().bottom(kid);
}

function _gApplyGaps(gaps, trailing) {
    var before = 0;
    var prev = null;
    for (var i = 0; i < gaps.length; ++i) {
        var kid = gaps[i].kid;
        if (gaps[i].want !== null) {
            var end = i === gaps.length - 1;
            // A bottom this pass wrote before is no page's own.
            var keep = gaps[i].net !== undefined ? gaps[i].net
                : end ? trailing : _gIsBox(kid) || _meta(kid).gHeld ? 0 : _gMarginBottom(kid);
            _gHoldGap(kid, gaps[i].want - before, keep, prev);
        }
        before = _gMarginBottom(kid);
        prev = kid;
    }
}

function _gColumnMargins(column, owner) {
    // Between a table's parts is border-spacing, which `_tableLayout` sets.
    if (_TABLE_PARTS[_meta(column).createdTag]) {
        return;
    }
    var kids = _gFlowKids(column);
    // A template's Text keeps a line for text that hasn't come; a box has none.
    for (var t = 0; t < kids.length; ++t) {
        if (_meta(kids[t]).createdTag && typeof kids[t].lineCount === "number" && _gStyle(kids[t]).height === "auto") {
            var shut = kids[t].text === "";
            // Its height may be a binding: only one this closed is opened again.
            if (shut || _meta(kids[t]).gShut) {
                _gMarginTool().shut(kids[t], shut);
                _meta(kids[t]).gShut = shut;
            }
        }
    }
    var os = owner ? _gStyle(owner) : null;
    var oe = owner ? _gEdges(owner, os, owner.parent ? owner.parent.width : 0) : null;
    var row = column.eldurPackRow === true || typeof column.columnSpacing === "number"
        || String(column).indexOf("RowLayout") !== -1;
    var collapses = Boolean(owner) && _gCollapses(owner, os) && !_gHasOwnText(owner);
    if (row || (os && /(flex|grid)$/.test(os.display))) {
        // Margins don't collapse across a row, and a template in one was probed
        // with the ones it has wherever it lands: only a box is told.
        for (var k = 0; k < kids.length; ++k) {
            if (_gIsBox(kids[k])) {
                var got = _gOuter(kids[k]);
                var ke = _gEdges(kids[k], _gStyle(kids[k]), column.width);
                _gSetMargins(kids[k], got.mt, got.mb);
                _gSet(kids[k], "eldurMl", ke.ml || 0);
                _gSet(kids[k], "eldurMr", ke.mr || 0);
            }
        }
        return;
    }
    var throughTop = collapses && oe.pt + oe.bt === 0;
    var throughBottom = collapses && oe.pb + oe.bb === 0 && os.height === "auto";
    var pending = 0;
    var paid = 0;
    var seen = false;
    var gaps = [];
    var netted = 0;
    // A column's first: nothing before it can have moved it.
    var lastOwn = true;
    for (var i = 0; i < kids.length; ++i) {
        var kid = kids[i];
        var o = _gOuter(kid);
        if (_gIsBox(kid)) {
            var be = _gEdges(kid, _gStyle(kid), column.width);
            _gSet(kid, "eldurMl", be.ml || 0);
            _gSet(kid, "eldurMr", be.mr || 0);
        }
        // Conversion got right the gap between two of the page's own elements,
        // to the fraction, unless a script's child's margin passes through one.
        var own = !_meta(kid).createdTag && !_gIsBox(kid) && !o.foreign;
        var settled = own && !_gEdgeCreated(kid);
        var inPlace = settled && lastOwn && !_meta(kid).gHeld;
        lastOwn = settled;
        if (o.foreign) {
            lastOwn = false;
            gaps.push({ kid: kid, want: null });
            pending = o.mb;
            paid = o.mb;
            seen = true;
            netted = 0;
            continue;
        }
        if (o.empty) {
            // It sits where its own bottom would start the next box's gap, so
            // that margin of its is still to come.
            var at = _gMerge(pending, o.mt);
            pending = _gMerge(at, o.mb);
            if (!seen && throughTop) {
                gaps.push({ kid: kid, want: 0 });
                paid = pending;
            } else {
                gaps.push({ kid: kid, want: at - paid });
                paid = at;
            }
            netted += 1;
            continue;
        }
        var total = _gMerge(pending, o.mt);
        gaps.push({ kid: kid, want: inPlace ? null : !seen && throughTop ? 0 : total - paid });
        pending = o.mb;
        paid = 0;
        netted = 0;
        seen = true;
    }
    // Past the last box the margin is the parent's, not a gap in it: a box
    // margins collapse through still sits past them, without growing the parent.
    // Qt won't shrink a column by a cell, so each such cell is empty on its own.
    if (throughBottom) {
        var sum = 0;
        for (var n = gaps.length - netted; n < gaps.length; ++n) {
            sum += gaps[n].want;
            gaps[n].net = -sum;
        }
    }
    _gApplyGaps(gaps, throughBottom ? 0 : pending - paid);
}

function _gEdgeCreated(kid) {
    var column = _gColumnOf(kid);
    var kids = column ? _gFlowKids(column) : [];
    if (kids.length === 0) {
        return false;
    }
    var first = kids[0];
    var last = kids[kids.length - 1];
    return Boolean(_meta(first).createdTag) || Boolean(_meta(last).createdTag) || _gEdgeCreated(first)
        || (first !== last && _gEdgeCreated(last));
}

function _gMarginPass() {
    var found = [];
    var add = function(column, owner) {
        for (var f = 0; f < found.length; ++f) {
            if (found[f].column === column) {
                return;
            }
        }
        found.push({ column: column, owner: owner });
    };
    var visit = function(raw) {
        if (!raw.parent || _gIsBox(raw.parent) || _isLines(raw.parent) || !_gIsLayout(raw.parent) || !_isConnected(raw) || raw.eldurSlot !== undefined) {
            return;
        }
        if (_gStyle(raw).display === "none" || _gOutOfFlow(_gStyle(raw))) {
            return;
        }
        var owner = _gParent(raw);
        add(raw.parent, owner);
        // Its margins can pass through each ancestor to that one's own column.
        while (owner && !_gIsBox(owner) && owner.parent && _gIsLayout(owner.parent) && _gCollapses(owner, _gStyle(owner))) {
            var next = _gParent(owner);
            add(owner.parent, next);
            owner = next;
        }
    };
    if (_gActive()) {
        _gLive.forEach(visit);
    } else {
        _walkItems(_root, function(item) {
            if (_meta(item).createdTag) {
                visit(item);
            }
            return false;
        });
    }
    for (var c = 0; c < found.length; ++c) {
        _gColumnMargins(found[c].column, found[c].owner);
    }
}

// A box the page's own layout holds says what it wants through `Layout.*`.
function _gAttached(raw, s, e) {
    var parent = raw.parent;
    if (!parent) {
        return;
    }
    var inLayout = typeof parent.spacing === "number" || typeof parent.columnSpacing === "number";
    var avail = parent.width;
    var width = _gPx(s.width, avail, s.fontSize, s.rootSize);
    var shrinks = _gInlineLevel(s) || parent.eldurPackRow === true
        || (typeof parent.spacing === "number" && typeof parent.columnSpacing !== "number"
            && String(parent).indexOf("RowLayout") !== -1);
    if (!inLayout) {
        return;
    }
    if (width !== null) {
        var least = _gPx(s["min-width"], avail, s.fontSize, s.rootSize);
        var most = _gPx(s["max-width"], avail, s.fontSize, s.rootSize);
        var clamped = most === null ? width : Math.min(width, most);
        _gSet(raw, "eldurFill", false);
        _gSet(raw, "eldurPw", _gOuterWidth(s, e, least === null ? clamped : Math.max(clamped, least)));
    } else if (shrinks) {
        _gSet(raw, "eldurFill", _gNumber(s["flex-grow"]) > 0);
        _gSet(raw, "eldurPw", _gIntrinsic(raw, s));
    } else {
        var maxW = _gPx(s["max-width"], avail, s.fontSize, s.rootSize);
        _gSet(raw, "eldurFill", maxW === null);
        _gSet(raw, "eldurPw", maxW === null ? -1 : Math.min(avail, _gOuterWidth(s, e, maxW)));
    }
    var centred = e.ml === null && e.mr === null;
    // Qt.AlignHCenter, Qt.AlignRight, Qt.AlignLeft.
    // A grid item starts at its cell's top; a Layout would centre it.
    var top = typeof parent.columnSpacing === "number" ? 32 : 0;
    _gSet(raw, "eldurAlign", (centred ? 4 : e.ml === null ? 2 : 0) | top);
}

// Margins of a box on a line do not collapse: the line reads them as they are.
function _gLineMargins(raw, e) {
    var tool = _gMarginTool();
    if (_gIsBox(raw)) {
        _gSet(raw, "eldurMt", e.mt || 0);
        _gSet(raw, "eldurMb", e.mb || 0);
        _gSet(raw, "eldurMl", e.ml || 0);
        _gSet(raw, "eldurMr", e.mr || 0);
        return;
    }
    tool.set(raw, e.mt || 0, e.mb || 0);
    tool.side(raw, e.ml || 0, e.mr || 0);
}

function _gLayoutRoots() {
    var containers = [];
    var outOfFlow = [];
    for (var i = 0; i < _gLive.length; ++i) {
        var raw = _gLive[i];
        if (!raw.parent || _gIsBox(raw.parent) || !_isConnected(raw)
            || _isAnonLines(raw.parent) && _gIsBox(raw.parent.parent)) {
            continue;
        }
        var s = _gStyle(raw);
        if (!_gIsBox(raw)) {
            _gPaintControl(raw, s, _gEdges(raw, s, raw.parent.width));
            if (_isLines(raw.parent)) {
                _gLineMargins(raw, _gEdges(raw, s, raw.parent.width));
            }
            continue;
        }
        if (s.display === "none") {
            raw.visible = false;
            continue;
        }
        raw.visible = true;
        var abs = _gOutOfFlow(s);
        // A new line has its width once the layout holding it is polished.
        var avail = _isLines(raw.parent) && raw.parent.width === 0 && raw.parent.parent
            ? raw.parent.parent.width : raw.parent.width;
        var e = _gEdges(raw, s, avail);
        if (abs) {
            outOfFlow.push(raw);
            continue;
        }
        _gAttached(raw, s, e);
        var inLayout = typeof raw.parent.spacing === "number" || typeof raw.parent.columnSpacing === "number";
        var fill = inLayout && raw.eldurFill;
        var given = avail - (e.ml || 0) - (e.mr || 0);
        var opts = _gInlineLevel(s) || (inLayout && raw.eldurPw >= 0) ? { shrink: true } : null;
        if (fill) {
            opts = { forceW: Math.max(given, 0) };
        }
        var track = _gTrackWidth(raw);
        if (track !== null) {
            opts = { forceW: track };
        }
        var margins = _gPlace(raw, Math.max(avail, 0), opts, null);
        _meta(raw).gMargins = margins;
        if (_isLines(raw.parent)) {
            _gLineMargins(raw, e);
        }
        if (containers.indexOf(raw.parent) === -1) {
            containers.push(raw.parent);
        }
    }
    _gMarginPass();
    return outOfFlow;
}

// A filled grid's item is as wide as its track, which only the grid knows.
function _gTrackWidth(raw) {
    var grid = raw.parent;
    var tracks = grid.eldurTracks;
    if (!tracks || tracks.length === 0) {
        return null;
    }
    if (raw.eldurPw >= 0 && !raw.eldurFill) {
        return null;
    }
    var column = _rawElementChildren(grid).indexOf(raw) % tracks.length;
    if (tracks[column] >= 0) {
        return tracks[column];
    }
    var free = grid.width - grid.columnSpacing * (tracks.length - 1);
    var parts = 0;
    for (var i = 0; i < tracks.length; ++i) {
        if (tracks[i] >= 0) {
            free -= tracks[i];
        } else {
            parts -= tracks[i];
        }
    }
    return Math.max(free * -tracks[column] / parts, 0);
}

function _gContainingBlock(raw, s) {
    if (s.position === "fixed") {
        return null;
    }
    for (var p = _gParent(raw); p; p = _gParent(p)) {
        if (_gStyle(p).position !== "static") {
            return p;
        }
    }
    return null;
}

// Out of flow: in the coordinates of the item its slot's gate is the size of.
function _gPlaceOutOfFlow(raw) {
    var s = _gStyle(raw);
    var m = _meta(raw);
    var block = _gContainingBlock(raw, s);
    var wanted = s.position === "fixed" ? "window" : (block || "initial");
    if (raw.eldurBlock !== wanted) {
        raw.eldurBlock = wanted;
        _leaveFlow(raw);
    }
    var gate = m.outOfFlowGate;
    var cs = block ? _gStyle(block) : null;
    var ce = block ? _gEdges(block, cs, 0) : { bl: 0, bt: 0, br: 0, bb: 0 };
    var w = (gate ? gate.width : _root.width) - ce.bl - ce.br;
    var h = (gate ? gate.height : _root.height) - ce.bt - ce.bb;
    var at = function(prop, basis) {
        return _gPx(s[prop], basis, s.fontSize, s.rootSize);
    };
    var e = _gEdges(raw, s, w);
    var left = at("left", w);
    var right = at("right", w);
    var top = at("top", h);
    var bottom = at("bottom", h);
    var opts = at("width", w) === null && !(left !== null && right !== null) ? { shrink: true } : null;
    if (at("width", w) === null && left !== null && right !== null) {
        opts = { forceW: Math.max(w - left - right - (e.ml || 0) - (e.mr || 0), 0) };
    }
    _gPlace(raw, w, opts, h);
    raw.x = ce.bl + (left !== null ? left + (e.ml || 0) : right !== null ? w - right - raw.width - (e.mr || 0) : (e.ml || 0));
    raw.y = ce.bt + (top !== null ? top + (e.mt || 0) : bottom !== null ? h - bottom - raw.height - (e.mb || 0) : (e.mt || 0));
}

// A box leaves the flow or returns to it by changing form: only an out-of-flow
// box declares the slot the rest of the runtime looks for.
function _gSyncFlowForm(raw) {
    var m = _meta(raw);
    var s = _gStyle(raw);
    var wantOut = _gOutOfFlow(s) && s.display !== "none";
    var isOut = raw.eldurBlock !== undefined;
    if (wantOut === isOut) {
        return raw;
    }
    var slot = raw.eldurSlot;
    var kept = wantOut ? null : slot;
    var next = _swapRawItem(raw, null, _gBoxSource(m.createdTag, wantOut));
    _gLive.push(next);
    if (wantOut) {
        var place = _slotFor(next);
        var container = next.parent;
        next.parent = null;
        _insertInFlow(container, [place], null);
        next.eldurBlock = "initial";
        _leaveFlow(next);
    } else if (kept) {
        var holder = kept.parent;
        _insertInFlow(holder, [next], kept);
        kept.parent = null;
        kept.destroy();
    }
    return next;
}

function _gSettle() {
    if (_gRunning || !_gHasCascade() || !_gDirty) {
        return;
    }
    if (!_gActive()) {
        // Templates paint a closed page's elements; only their margins are left.
        _gRunning = true;
        _gDirty = false;
        ++_gGen;
        _gMarginPass();
        // Templates paint the states, but what a script reads comes from the rules.
        _gWatchStates();
        _gClosedHiddenBoxes();
        _tableRepaintAll();
        _gRunning = false;
        return;
    }
    // Only a read right after a pass asks; holding more keeps dead items alive.
    _gReplaced.clear();
    _gRunning = true;
    _gDirty = false;
    ++_gGen;
    _gPrune();
    _gClosedHiddenBoxes();
    _gStaticChanges();
    var snapshot = _gLive.slice();
    for (var i = 0; i < snapshot.length; ++i) {
        if (_isConnected(snapshot[i])) {
            _gFillsLine(snapshot[i]);
        }
    }
    _gPrune();
    snapshot = _gLive.slice();
    for (var j = 0; j < snapshot.length; ++j) {
        if (_gIsBox(snapshot[j]) && _isConnected(snapshot[j])) {
            _gSyncFlowForm(snapshot[j]);
        }
    }
    _gPrune();
    _gWatchStates();
    _tableRepaintAll();
    _gReposition();
    _gRunning = false;
}

// Lines are only made inside a box. A page's empty container that a script
// fills with inline-level children becomes one, as nothing of its own is lost.
function _gFillsLine(raw) {
    if (!raw.parent || _gIsBox(raw.parent) || _isLines(raw.parent) || !_gInlineLevel(_gStyle(raw))) {
        return;
    }
    var parent = _gParent(raw);
    if (!parent || _gIsBox(parent) || _meta(parent).createdTag || /^(body|html)$/.test(_nodeTag(parent))) {
        return;
    }
    var all = _rawElementChildren(parent);
    for (var i = 0; i < all.length; ++i) {
        if (!_meta(all[i]).createdTag) {
            return;
        }
    }
    _gAdopt(parent);
}

function _gReposition() {
    var outs = _gLayoutRoots();
    for (var i = 0; i < _gLive.length; ++i) {
        var raw = _gLive[i];
        if (raw.eldurBlock !== undefined && _isConnected(raw) && _gStyle(raw).display !== "none") {
            _gPlaceOutOfFlow(raw);
        }
    }
    return outs;
}

// Widths a layout hands down arrive only once it is polished.
function _gAfterPolish() {
    if (_gRunning || !_gActive() || _gLive.length === 0) {
        return;
    }
    _gRunning = true;
    _gPrune();
    _gReposition();
    _gRunning = false;
}

// --- The page's own elements ------------------------------------------------

// Taken before a script runs: which rules each of the page's elements was
// converted under.
function _gSnapshotStatics() {
    if (!_gActive() || _gStaticBase !== null) {
        return;
    }
    _gStaticBase = [];
    var html = _taggedHtml();
    _gStaticRoot = html ? _gRootAttrs(html) : "";
    _walkItems(_root, function(item) {
        if (typeof item.eldurTag === "string" && !_meta(item).createdTag) {
            _gStaticBase.push({ raw: item, hiddenBox: item.visible === false, sig: _gStaticSig(item), classes: _meta(item).classes.slice() });
        }
        return false;
    });
}

// A rule can match the same and still say something else, through a variable
// an ancestor's rules changed.
function _gStaticSig(raw) {
    var rules = _gRules();
    var matched = _gMatched(raw);
    var uses = false;
    for (var i = 0; i < matched.length && !uses; ++i) {
        uses = JSON.stringify(rules[matched[i]].decls).indexOf("var(") !== -1;
    }
    return matched.join(",") + (uses ? JSON.stringify(_gStyle(raw).vars) : "") + _gBoxInline(raw);
}

var _G_IN_PLACE = /^(display|visibility|opacity|color|zIndex|left|top|right|bottom|position|cursor|pointerEvents)$/;

// What a script wrote that the item as converted cannot answer to.
function _gBoxInline(raw) {
    var styles = _meta(raw).styles;
    return Object.keys(styles).filter(function(k) {
        return !_G_IN_PLACE.test(k) && k.indexOf("-") === -1;
    }).sort().map(function(k) { return k + ":" + styles[k]; }).join(";");
}

function _gRootAttrs(html) {
    var m = _meta(html);
    return m.classes.join(" ") + JSON.stringify(m.dataset);
}

// A class no variant of the converter's reaches, on this element, an ancestor
// or an earlier sibling, is what a selector can only have learned now.
function _gUncovered(entry) {
    var raw = entry.raw;
    if (_gBoxInline(raw) !== "") {
        return true;
    }
    var now = _meta(raw).classes;
    var changed = now.filter(function(c) { return entry.classes.indexOf(c) === -1; })
        .concat(entry.classes.filter(function(c) { return now.indexOf(c) === -1; }));
    for (var i = 0; i < changed.length; ++i) {
        if (_gMentioned(changed[i])
            && (typeof raw[_sanitizeClassProp(changed[i])] !== "boolean" || !_gVariantSupports(raw, changed[i]))) {
            return true;
        }
    }
    return false;
}

// A class no selector names changes what no rule matches.
function _gMentioned(name) {
    var rules = _gRules();
    var word = "." + name;
    for (var i = 0; i < rules.length; ++i) {
        var at = rules[i].sel.indexOf(word);
        if (at !== -1 && !/[\w-]/.test(rules[i].sel.charAt(at + word.length))) {
            return true;
        }
    }
    return false;
}

// The converter also declares a variant for a class it only saw computed; such
// a variant cannot say a box's size, so only a literal one counts.
function _gVariantSupports(raw, name) {
    var literal = _root.eldurLiteralClasses;
    return literal !== undefined && literal.indexOf(name) !== -1;
}

// Shows or hides an item as the rules now say. The converter's binding answers
// for all it knows; only where it disagrees does something hold over it.
function _gShowAsRules(raw) {
    var display = _gStyle(raw).display;
    if (raw.eldurSlot && raw.eldurBlock === undefined) {
        var parent = raw.parent;
        if ((display === "none") === (raw.visible === true) && (!parent || parent.visible !== false)) {
            _setShown(raw, display !== "none");
        }
    } else {
        _holdFocus();
        _layoutChanged(raw.parent);
        _inlineRelease(raw, raw, "visible");
        var written = String(_meta(raw).styles.display || "") !== "";
        if (raw.parent && raw.parent.visible === false) {
            // `visible` reads false under a hidden parent, whatever it was told.
            if (display === "none") {
                _inlineHold(raw, raw, "visible", false);
            }
            _gWatchShown(raw);
        } else if (written || (display === "none") === (raw.visible === true)) {
            // A class change must not reach what an inline display decides.
            _inlineHold(raw, raw, "visible", display !== "none");
            if (!written) {
                _gWatchShown(raw);
            }
        }
    }
    _gShownInline(raw, display === "inline");
}

// The converter made a hidden element a box, not knowing what it would display
// as: an inline one is as wide as its text and as tall as its font.
function _gShownInline(raw, inline) {
    if (!_vInlineTag(_nodeTag(raw)) || _meta(raw).createdTag) {
        return;
    }
    var m = _meta(raw);
    if (inline && !m.gShownInline && raw.visible !== false) {
        m.gShownInline = true;
        _gFillHelper(raw).fill(raw, false);
    } else if (!inline && m.gShownInline) {
        m.gShownInline = false;
        _gFillHelper(raw).fill(raw, true);
    }
}

function _gFillHelper(raw) {
    var m = _meta(raw);
    if (!m.gFill) {
        m.gFill = Qt.createQmlObject("import QtQuick; import QtQuick.Layouts; QtObject { "
            + "function fill(i, on) { i.Layout.fillWidth = on; } }", raw, "eldur_fill");
    }
    return m.gFill;
}

// A closed page's templates cover what a class can show but an inline element,
// which their variants cannot make a box of.
// The rules may say something else about it after any later change.
function _gWatchShown(raw) {
    _gClosedHidden = _gClosedHidden || [];
    if (_gClosedHidden.indexOf(raw) === -1) {
        _gClosedHidden.push(raw);
    }
}

function _gClosedHiddenBoxes() {
    (_gClosedHidden || []).slice().forEach(function(raw) {
        if (!_gAdopted.has(raw) && raw.parent) {
            _gShowAsRules(raw);
        }
    });
}

function _gSnapshotClosedHidden() {
    _gClosedHidden = [];
    _walkItems(_root, function(item) {
        if (typeof item.eldurTag === "string" && item.visible === false && _vInlineTag(item.eldurTag)) {
            _gClosedHidden.push(item);
        }
        return false;
    });
}

function _gStaticChanges() {
    if (_gStaticBase === null) {
        return;
    }
    ++_gGen;
    for (var h = 0; h < _gStaticBase.length; ++h) {
        if (_gStaticBase[h].hiddenBox) {
            _gShowAsRules(_gCurrent(_gStaticBase[h].raw));
        }
    }
    var open = [];
    for (var i = 0; i < _gStaticBase.length; ++i) {
        open.push(_gStaticBase[i]);
    }
    var byRaw = new Map();
    for (var u = 0; u < open.length; ++u) {
        byRaw.set(open[u].raw, _gUncovered(open[u]));
    }
    var causes = function(raw) {
        for (var p = _gParent(raw); p; p = _gParent(p)) {
            if (byRaw.get(p) === true) {
                return true;
            }
        }
        var parent = _gParent(raw);
        var sibs = parent ? _rawElementChildren(parent) : [];
        for (var k = 0; k < sibs.length && sibs[k] !== raw; ++k) {
            if (byRaw.get(sibs[k]) === true) {
                return true;
            }
        }
        return false;
    };
    var html = _taggedHtml();
    var rootMoved = Boolean(html) && _gRootAttrs(html) !== _gStaticRoot;
    var keep = [];
    for (var n = 0; n < open.length; ++n) {
        var entry = open[n];
        if (entry.raw.parent === null) {
            continue;
        }
        var sig = _gStaticSig(entry.raw);
        if (sig !== entry.sig && rootMoved) {
            _gRepaintStatic(entry.raw);
            entry.sig = sig;
            keep.push(entry);
        } else if (sig !== entry.sig && (byRaw.get(entry.raw) === true || causes(entry.raw))) {
            _gAdopt(entry.raw);
        } else {
            keep.push(entry);
        }
    }
    _gStaticBase = keep;
    if (rootMoved) {
        _gStaticRoot = _gRootAttrs(html);
        _gRepaintCanvas(html);
    }
    ++_gGen;
}

// The root's attributes change what a whole page says, usually its colours. The
// page keeps the form it was converted to, children, text and all, and takes
// the new values.
function _gRepaintStatic(raw) {
    var s = _gStyle(raw);
    if (raw.__eldurInline || s.display === "none") {
        return;
    }
    var e = _gEdges(raw, s, raw.parent ? raw.parent.width : 0);
    if (raw.background !== undefined && raw.background !== null) {
        if (raw.background.color !== undefined && s.won["background-color"]) {
            raw.background.color = _gQtColor(s["background-color"]);
        }
        if (raw.background.border !== undefined && s.won["border-top-color"]) {
            raw.background.border.color = _gQtColor(s["border-top-color"]);
        }
        if (raw.contentItem && raw.contentItem.color !== undefined) {
            raw.contentItem.color = _gQtColor(s["color"]);
        }
        return;
    }
    if (typeof raw.text === "string" && raw.color !== undefined) {
        raw.color = _gQtColor(s["color"]);
        if (raw.linkColor !== undefined) {
            raw.linkColor = _gLinkColor(raw);
        }
        return;
    }
    if (raw.border === undefined || raw.color === undefined) {
        return;
    }
    if (_nodeTag(raw) !== "body" && _nodeTag(raw) !== "html") {
        raw.color = _gQtColor(s["background-color"]);
    }
    if (e.bt > 0) {
        raw.border.color = _gQtColor(s["border-top-color"]);
    }
    var kids = raw.children;
    for (var i = 0; i < kids.length; ++i) {
        var kid = kids[i];
        if (typeof kid.eldurTag === "string") {
            continue;
        }
        if (kid.spacing !== undefined) {
            _gRepadStatic(raw, kid, e);
        } else if (kid.color !== undefined && kid.children.length === 0) {
            // A strip for one side: which one, its shape and place say.
            var across = kid.width >= kid.height;
            var side = across ? (kid.y > 0 ? "bottom" : "top") : (kid.x > 0 ? "right" : "left");
            kid.color = _gQtColor(s["border-" + side + "-color"]);
        }
    }
}

function _gRepadStatic(raw, content, e) {
    var top = e.pt + e.bt;
    var bottom = e.pb + e.bb;
    var left = e.pl + e.bl;
    var right = e.pr + e.br;
    content.anchors.topMargin = top;
    content.anchors.leftMargin = left;
    content.anchors.rightMargin = right;
    raw.implicitHeight = Qt.binding(function() { return content.implicitHeight + top + bottom; });
    raw.implicitWidth = Qt.binding(function() { return content.implicitWidth + left + right; });
}

// A static paragraph holds its links in markup, where no element answers.
function _gLinkColor(raw) {
    var rules = _gRules();
    var won = {};
    for (var i = 0; i < rules.length; ++i) {
        if (rules[i].sel.indexOf("::") !== -1 || rules[i].lastName !== "a") {
            continue;
        }
        for (var d = 0; d < rules[i].decls.length; ++d) {
            if (rules[i].decls[d][0] === "color") {
                _gOfferDecl(won, "color", rules[i].decls[d][1], _gWeight(rules[i], rules[i].decls[d][2]), _gStyle(raw).vars);
            }
        }
    }
    return won.color ? _gQtColor(won.color.value) : raw.linkColor;
}

// A root or body background paints the whole canvas.
function _gRepaintCanvas(html) {
    var body = _rawElementChildren(html).filter(function(k) { return _nodeTag(k) === "body"; })[0];
    var from = _gStyle(html).won["background-color"] ? _gStyle(html) : body ? _gStyle(body) : null;
    if (from && _root.color !== undefined) {
        _root.color = from.won["background-color"] ? _gQtColor(from["background-color"]) : "#ffffff";
    }
}

// From now on the element is the same as one a script made. The item stays, for
// bindings that read it by id, but nothing is left of it on screen.
function _gAdopt(raw) {
    var m = _meta(raw);
    var tag = _nodeTag(raw);
    if (!_gWantsBox(tag) || _rawHasNoFlowParent(raw)) {
        return;
    }
    var place = _gConvertedPlace(raw);
    m.createdTag = tag;
    var name = raw.objectName;
    m.gKeepReplaced = true;
    var next = _swapRawItem(raw, null, _gBoxSource(tag, false));
    delete m.gKeepReplaced;
    _gAdopted.set(raw, next);
    next.objectName = name;
    next.eldurGapT = place.top;
    next.eldurGapL = place.left;
    next.eldurTrack = place.track;
    if (place.cell) {
        _gLayoutCell(next, place.cell);
    }
    _gLive.push(next);
    return next;
}

// What the converter wrote on an item for its parent's gaps and grid tracks,
// which are no rule of the element's and which the box has to go on keeping.
function _gConvertedPlace(raw) {
    var place = { top: 0, left: 0, track: -1 };
    var parent = _gParent(raw);
    if (!parent || !raw.parent) {
        return place;
    }
    var s = _gStyle(parent);
    if (_gIsFlex(s)) {
        var row = String(s["flex-direction"]).indexOf("row") === 0;
        var gap = _gPx(s[row ? "column-gap" : "row-gap"], null, s.fontSize, s.rootSize) || 0;
        var written = _gLayoutMargins(raw);
        place.left = row ? Math.min(gap, written.left) : 0;
        place.top = row ? 0 : Math.min(gap, written.top);
    } else if (typeof raw.parent.columns === "number") {
        var cell = _gLayoutMargins(raw);
        place.cell = cell;
        place.track = !cell.fill && cell.wide >= 0 ? cell.wide + cell.left + cell.right
            : isFinite(cell.max) ? cell.max : -1;
    }
    return place;
}

function _gLayoutMargins(raw) {
    if (!_gLayoutReader) {
        _gLayoutReader = Qt.createQmlObject("import QtQuick; import QtQuick.Layouts; QtObject { "
            + "function read(i) { return { top: i.Layout.topMargin, left: i.Layout.leftMargin, right: i.Layout.rightMargin, max: i.Layout.maximumWidth, "
            + "wide: i.Layout.preferredWidth, fill: i.Layout.fillWidth, tall: i.Layout.fillHeight, row: i.Layout.row, column: i.Layout.column, "
            + "rows: i.Layout.rowSpan, columns: i.Layout.columnSpan }; } "
            + "function cell(i, c) { i.Layout.row = c.row; i.Layout.column = c.column; i.Layout.rowSpan = c.rows; i.Layout.columnSpan = c.columns; "
            + "i.Layout.fillHeight = c.tall; } }",
            _qobjectParent(), "eldur_layout_reader");
    }
    return _gLayoutReader.read(raw);
}

function _gLayoutCell(raw, cell) {
    _gLayoutReader.cell(raw, cell);
}

function _rawHasNoFlowParent(raw) {
    return !raw.parent;
}

// --- Reading it back --------------------------------------------------------

// What the converter moved a static line by to put its glyphs where a browser
// does; the box stays where it was.
function _gGlyphShift(raw) {
    if (typeof raw.lineHeightMode !== "number" || !raw.transform || raw.transform.length < 1
        || raw.transform[0].y === undefined || raw.transform[0].objectName === "eldurCss"
        || _gHasCascade() && (raw.transform.length !== 1 || _gStyle(raw).won["transform"])) {
        return 0;
    }
    return raw.transform[0].y;
}

function _gNumberText(n) {
    return String(Number(n.toPrecision(6)));
}

function _gComputed(raw, prop) {
    if (raw.__eldurInline || typeof raw.visible !== "boolean") {
        return null;
    }
    var name = prop.indexOf("--") === 0 ? prop : _camelToKebab(prop);
    var s = _gStyle(raw);
    if (name.indexOf("--") === 0) {
        return s.vars[name] !== undefined ? String(s.vars[name]).trim() : "";
    }
    var parentWidth = raw.parent ? raw.parent.width : 0;
    var length = function(written) {
        var px = _gPx(written, parentWidth, s.fontSize, s.rootSize);
        return px === null ? String(written) : _gNumberText(px) + "px";
    };
    if (name === "width" || name === "height") {
        if (s.display === "none" || s.display === "inline") {
            return s.display === "none" ? length(s[name]) : "auto";
        }
        var frame = 0;
        if (s["box-sizing"] !== "border-box") {
            var edges = _gEdges(raw, s, parentWidth);
            frame = name === "width" ? edges.pl + edges.pr + edges.bl + edges.br : edges.pt + edges.pb + edges.bt + edges.bb;
        }
        return _gNumberText(raw[name] - frame) + "px";
    }
    if (name === "opacity") {
        return _gNumberText(_gNumber(s.opacity));
    }
    if (name === "text-decoration-line") {
        var decoration = s.won["text-decoration-line"] || s.won["text-decoration"];
        var line = decoration ? String(decoration.value).split(/\s+/).filter(function(w) {
            return w === "underline" || w === "line-through" || w === "overline";
        }).join(" ") : "";
        return line === "" ? "none" : line;
    }
    if (name === "background-image" || name === "box-shadow") {
        var parsed = s[name] === "none" ? null : _gShadowsAt(name, s[name], s.fontSize);
        return parsed ? parsed.css : String(s[name]);
    }
    if (name === "transform") {
        var matrix = _gMatrixOf(raw, s, raw.width, raw.height, false);
        return matrix ? "matrix(" + matrix.map(function(v) {
            return _gNumberText(Number(v.toFixed(6)));
        }).join(", ") + ")" : "none";
    }
    if (name === "font-family") {
        return s[name] === "" ? "\"Times New Roman\"" : String(s[name]).split(",").map(function(f) {
            return f.trim().replace(/^'(.*)'$/, "\"$1\"");
        }).join(", ");
    }
    if (name === "font-size") {
        return _gNumberText(s.fontSize) + "px";
    }
    if (name === "line-height") {
        var line = _gLineHeight(s);
        return line === null ? "normal" : _gNumberText(line) + "px";
    }
    if (/^border-(top|right|bottom|left)-width$/.test(name)) {
        return _gNumberText(s[name.replace("width", "px")]) + "px";
    }
    if (/^(margin|padding)-/.test(name)) {
        var used = _gPx(s[name], parentWidth, s.fontSize, s.rootSize);
        return _gNumberText(used === null ? 0 : used) + "px";
    }
    if (/radius$/.test(name) || /^(top|right|bottom|left)$/.test(name) || /^(min|max)-/.test(name)
        || name === "row-gap" || name === "column-gap" || name === "flex-basis") {
        return length(s[name]);
    }
    return s[name] === undefined ? null : String(s[name]);
}
