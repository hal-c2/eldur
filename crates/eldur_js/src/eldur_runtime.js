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
    if (_vHasRules() && _root) {
        // What the converter wrote for a `Text` holds until the rules that
        // match its nodes change, so they must be read before any write.
        _walkItems(_root, function() { return false; });
    }
    _startTime = Date.now();
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
    _syncEldurClasses(curRaw, m);
    if (_isCreatedElement(curRaw) && _isConnected(curRaw)) {
        _connectSubtree(curRaw);
    }
}

function classList(item) {
    var raw = _unwrap(item);
    var m = _meta(raw);
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
    return _meta(item).dataset;
}

// The item appended children go in. A box's inner layout, when it has one.
// An empty box has only the `Text` for its `textContent`, and the column
// made here takes its place in the box's height.
function _appendTarget(raw) {
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
        return raw;
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

// Qt delivers its once-per-frame hover before it polishes layouts, so an
// item a script puts under a resting cursor is hit-tested at its size
// before layout and stays unhovered until the mouse moves. A browser lays
// out before it hit-tests: when the script's turn ends, polish the changed
// subtrees (children first: a layout's implicit size waits for its polish)
// and their ancestors, ahead of the frame.
var _layoutDirty = [];

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
    if (typeof item.ensurePolished === "function") {
        item.ensurePolished();
    }
}

// Structural pseudo-classes (`:first-child`, `:nth-child()`, …) read an
// element's place among its siblings from `eldurIndex`/`eldurCount` (and the
// same-tag `eldurTypeIndex`/`eldurTypeCount`), which the converter declares
// only on the children of containers scripts change.
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
    var dirty = _layoutDirty;
    _layoutDirty = [];
    var done = [];
    var numbered = [];
    for (var n = 0; n < dirty.length; ++n) {
        // A dirty item is a container that lost or gained children, or an
        // item that was swapped or refilled in its own container.
        var pair = [dirty[n], dirty[n].parent];
        for (var q = 0; q < pair.length; ++q) {
            if (pair[q] && numbered.indexOf(pair[q]) < 0) {
                numbered.push(pair[q]);
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

// Takes a node out of the DOM. Never destroys it: a script may hold it and put
// it back, as with a modal, and bindings of the page may read it by id.
function _detachNode(raw) {
    if (raw.__eldurInline) {
        _vRemove(raw);
        return;
    }
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
    _layoutChanged(raw.parent);
    var slot = raw.eldurSlot;
    if (slot && raw.eldurBlock === undefined) {
        raw.visible = Qt.binding(function() {
            return shown && (slot.parent ? slot.parent.visible : false);
        });
    } else {
        raw.visible = shown;
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
    if (typeof base.spacing === "number" || typeof base.columnSpacing === "number") {
        return base;
    }
    var m = _meta(raw);
    if (m.flow) {
        return m.flow;
    }
    var isContentItem = Boolean(raw.parent && raw.parent.contentItem === raw);
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

function _rawKids(raw, withText) {
    var container = _childContainer(raw);
    if (!container || !container.children) {
        return [];
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
        if (_vAnonText(child)) {
            var runKids = _vModelOf(child).kids.filter(withText ? _vIsRendered : _vIsElement);
            for (var r = 0; r < runKids.length; ++r) {
                out.push(runKids[r]);
            }
            continue;
        }
        if (!isLayout && _isPlainTextItem(child) && !child.objectName && !_meta(child).createdTag) {
            if (withText && _vHasRules()) {
                out.push(child);
            }
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

function _logicalParent(raw) {
    if (raw && raw.__eldurInline) {
        return raw.parent;
    }
    if (raw && raw.eldurSlot) {
        raw = raw.eldurSlot;
    }
    if (!raw || !raw.parent || raw.eldurSlot === null) {
        return null;
    }
    var p = raw.parent;
    var pm = _meta(p);
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
    if (vRun && _vAnonText(vRun)) {
        return _rawKids(raw, true).map(getText).join("");
    }
    if (_isPlainJsObject(raw) && ("textContent" in raw || "innerText" in raw)) {
        return raw.textContent !== undefined ? raw.textContent : raw.innerText;
    }
    var run = _meta(raw).runText;
    if (run !== undefined) {
        return run;
    }
    if (typeof raw.text === "string") {
        return raw.text;
    }
    var textChild = _findTextChild(raw);
    if (textChild) {
        return String(textChild.text);
    }
    var m = _meta(raw);
    if (m.explicitText !== undefined) {
        return String(m.explicitText);
    }
    // `textContent` is the text of everything inside.
    return _rawElementChildren(raw).map(getText).join("");
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
    m.explicitText = str;
    if (m.runText !== undefined) {
        m.runText = str;
        raw.text = _runText(str);
        return;
    }
    if (typeof raw.text === "string") {
        raw.text = str;
        return;
    }
    _clearAppended(raw);
    var textChild = _findTextChild(raw);
    if (textChild) {
        textChild.text = str;
    }
}

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
    var vHtml = raw.__eldurInline ? raw : _vExisting(raw);
    if (!vHtml && _vHasRules() && !_markupMakesElements(raw) && typeof raw.text === "string") {
        vHtml = _vOwnModel(raw);
    }
    if (vHtml) {
        _vSetInnerHTML(vHtml, str);
        return;
    }
    if (_markupMakesElements(raw)) {
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
    var nodes = _expandInsertRawNodes(_parseHtmlFragmentNodes(str));
    _insertRawNodesBefore(_appendTarget(raw), nodes, null);
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

    if (_isPlainJsObject(raw)) {
        if (!raw.style) {
            raw.style = {};
        }
        raw.style[name] = str;
        return;
    }

    if (name === "display") {
        if (typeof raw.visible === "boolean") {
            _setShown(raw, str !== "none");
        }
    } else if (name === "visibility") {
        var visible = str !== "hidden";
        if (typeof raw.opacity === "number") {
            raw.opacity = visible ? 1 : 0;
        }
        if (typeof raw.enabled === "boolean") {
            raw.enabled = visible;
        }
    } else if (name === "opacity") {
        var op = parseFloat(str);
        if (!isNaN(op) && typeof raw.opacity === "number") {
            raw.opacity = op;
        }
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
    } else if (name === "width") {
        var w = parseFloat(str);
        if (!isNaN(w) && typeof raw.width === "number") {
            raw.width = w;
        }
    } else if (name === "height") {
        var h = parseFloat(str);
        if (!isNaN(h) && typeof raw.height === "number") {
            raw.height = h;
        }
    } else if (name === "backgroundColor" || name === "background") {
        if (raw.background && raw.background.color !== undefined) {
            raw.background.color = str;
        } else if (raw.color !== undefined && typeof raw.text !== "string") {
            raw.color = str;
        }
    } else if (name === "color") {
        if (typeof raw.text === "string" && raw.color !== undefined) {
            raw.color = str;
        } else {
            var tc = _findTextChild(raw);
            if (tc && tc.color !== undefined) {
                tc.color = str;
            }
        }
    } else if (name === "fontSize") {
        var px = parseFloat(str);
        if (!isNaN(px)) {
            var fontTarget = raw.font !== undefined ? raw : _findTextChild(raw);
            if (fontTarget && fontTarget.font !== undefined) {
                fontTarget.font.pixelSize = px;
            }
        }
    } else if (name === "borderRadius" && typeof raw.radius === "number") {
        var rad = parseFloat(str);
        if (!isNaN(rad)) {
            raw.radius = rad;
        }
    } else if (name === "borderColor" && raw.border !== undefined) {
        raw.border.color = str;
    } else if (name === "borderWidth" && raw.border !== undefined) {
        var bw = parseFloat(str);
        if (!isNaN(bw)) {
            raw.border.width = bw;
        }
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
    if (name === "id" && typeof raw.objectName === "string") {
        raw.objectName = val;
    } else if (name === "type" && m.createdTag === "input") {
        wrap(raw).type = val;
    } else if (name === "class") {
        setClassName(raw, val);
    } else if (name === "disabled" && typeof raw.enabled === "boolean") {
        raw.enabled = false;
    } else if (name === "hidden" && typeof raw.visible === "boolean") {
        _setShown(raw, false);
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
    if (name === "class" && m.classes.length > 0) {
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
        _setShown(raw, true);
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

function focus(item) {
    if (item && !_isDomTarget(item) && typeof item.focus === "function") {
        return item.focus();
    }
    var raw = _unwrap(item);
    if (!raw) {
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

function getBoundingClientRect(item) {
    var raw = _unwrap(item);
    if (raw && raw.__eldurInline) {
        return _vClientRect(raw);
    }
    if (!raw) {
        return { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 };
    }
    var w = typeof raw.width === "number" ? raw.width : 0;
    var h = typeof raw.height === "number" ? raw.height : 0;
    var x = typeof raw.x === "number" ? raw.x : 0;
    var y = typeof raw.y === "number" ? raw.y : 0;
    if (typeof raw.mapToItem === "function") {
        var pt = raw.mapToItem(null, 0, 0);
        if (pt) {
            x = pt.x;
            y = pt.y;
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

function event(type, targetItem, qtEvent) {
    if (type === "click" && qtEvent && qtEvent.scenePosition && _vHasRules()) {
        var shared = _beginTap(qtEvent);
        if (shared.ev) {
            shared.ev.currentTarget = targetItem;
            return shared.ev;
        }
    }
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
        bubbles: true,
        cancelable: true,
        defaultPrevented: false,
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

function _dispatchDynamic(raw, type, qtEv) {
    // A real item's turn in a tap: see `_beginTap`.
    if (type === "click" && !raw.__eldurInline && qtEv && qtEv.scenePosition && _vHasRules()) {
        var t = _beginTap(qtEv);
        if (t.control || t.ran.indexOf(raw) !== -1 || t.ev._propagationStopped) {
            return !t.ev.defaultPrevented;
        }
        t.ran.push(raw);
        qtEv = t.ev;
    }
    var m = _meta(raw);
    var target = wrap(raw);
    var domEv = qtEv && qtEv.preventDefault ? qtEv : event(type, target, qtEv);
    domEv.currentTarget = target;
    if (!domEv.target) {
        domEv.target = target;
    }

    var propHandler = m.onHandlers[type];
    if (typeof propHandler === "function") {
        if (propHandler.call(target, domEv) === false) {
            domEv.preventDefault();
        }
        if (domEv._immediateStopped) {
            return !domEv.defaultPrevented;
        }
    }

    var list = m.listeners[type];
    var onceList = m.onceListeners[type];
    if (list && list.length > 0) {
        var copy = list.slice();
        for (var i = 0; i < copy.length; ++i) {
            var fn = copy[i];
            if (onceList) {
                var onceIdx = onceList.indexOf(fn);
                if (onceIdx !== -1) {
                    onceList.splice(onceIdx, 1);
                    var listIdx = list.indexOf(fn);
                    if (listIdx !== -1) {
                        list.splice(listIdx, 1);
                    }
                }
            }
            fn.call(target, domEv);
            if (domEv._immediateStopped) {
                break;
            }
        }
    }
    return !domEv.defaultPrevented;
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
            raw.clicked.connect(function() {
                if (_vHasRules()) {
                    _clickedControl(raw);
                } else {
                    _dispatchDynamic(raw, "click", null);
                }
            });
            m.connected[type] = true;
            return true;
        }
        if (typeof raw.mapToItem === "function") {
            var tap = Qt.createQmlObject("import QtQuick; TapHandler {}", raw, "eldur_tap");
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
            raw.textChanged.connect(function() { _dispatchDynamic(raw, "input", null); });
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
            raw.editingFinished.connect(function() { _dispatchDynamic(raw, "change", null); });
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
                if (_activeElement === raw) {
                    _activeElement = null;
                }
                if (type === "blur" || type === "focusout") {
                    _dispatchDynamic(raw, type, null);
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
    // An event no signal delivers (a custom one, say) can still be dispatched.
    if (_addListener(item, eventType, handler, options)) {
        _connectSignalOnce(_unwrap(item), String(eventType));
    }
}

// For events QML handlers already deliver (keys, a form's submit): the
// generated handler calls `dispatchEvent`, there is nothing to connect.
function listenBound(item, eventType, handler, options) {
    _addListener(item, eventType, handler, options);
}

function _addListener(item, eventType, handler, options) {
    if (!item || typeof handler !== "function") {
        return false;
    }
    var raw = _unwrap(item);
    var type = String(eventType);
    var m = _meta(raw);
    if (!m.listeners[type]) {
        m.listeners[type] = [];
    }
    if (m.listeners[type].indexOf(handler) === -1) {
        m.listeners[type].push(handler);
        if (options && typeof options === "object" && Boolean(options.once)) {
            if (!m.onceListeners[type]) {
                m.onceListeners[type] = [];
            }
            if (m.onceListeners[type].indexOf(handler) === -1) {
                m.onceListeners[type].push(handler);
            }
        }
    }
    return true;
}

// A listener on a known element can get both a connection and the generated
// QML handler; the handler asks this first, so one action fires once.
function connected(item, eventType) {
    return Boolean(_meta(item).connected[String(eventType)]);
}

function unlisten(item, eventType, handler) {
    if (item && !_isDomTarget(item) && typeof item.removeEventListener === "function") {
        return item.removeEventListener(eventType, handler);
    }
    if (!item || typeof handler !== "function") {
        return;
    }
    var m = _meta(item);
    var type = String(eventType);
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
    var raw = _unwrap(item);
    var type = String(eventType);
    var m = _meta(raw);
    m.onHandlers[type] = typeof handler === "function" ? handler : null;
    if (typeof handler === "function") {
        if (!_connectSignalOnce(raw, type)) {
            console.warn("eldur: dynamic on" + type + " handler cannot bind to a QML signal");
        }
    }
    return handler;
}

function dispatchEvent(item, domEv) {
    if (item && !_isDomTarget(item) && typeof item.dispatchEvent === "function") {
        return item.dispatchEvent(domEv);
    }
    if (!item || !domEv || !domEv.type) {
        return true;
    }
    return _dispatchDynamic(_unwrap(item), String(domEv.type), domEv);
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
        return _buttonSource(_KEY_SIGNALS);
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

function _insertRawNodesBefore(container, rawNodes, rawRef) {
    rawNodes = _forRealItems(rawNodes);
    if (!container || rawNodes.length === 0) {
        return;
    }
    var places = [];
    for (var n = 0; n < rawNodes.length; ++n) {
        var node = rawNodes[n];
        places.push(node && node.eldurBlock !== undefined ? _slotFor(node) : (node && node.eldurSlot) || node);
    }
    _insertInFlow(container, places, (rawRef && rawRef.eldurSlot) || rawRef);
    for (var o = 0; o < rawNodes.length; ++o) {
        if (rawNodes[o] && rawNodes[o].eldurBlock !== undefined) {
            _leaveFlow(rawNodes[o]);
        }
    }
}

function _insertInFlow(container, rawNodes, rawRef) {
    _layoutChanged(container);
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
    // Before the script reads styles back; removals settle with the layouts.
    _renumber(container);
}

function _genericElementSource(tag, inputType) {
    if (tag === "input") {
        return _inputSource(inputType || "text");
    }
    if (tag === "textarea") {
        return _NATIVE_IMPORTS + "TextArea { " + _NATIVE_TEXT + "color: \"black\"; background: Rectangle { color: \"white\"; border.color: \"#767676\" } }";
    }
    if (tag === "li") {
        return "import QtQuick; import QtQuick.Layouts; RowLayout { Layout.fillWidth: true; spacing: 8 }";
    }
    if (tag === "ul" || tag === "ol") {
        return "import QtQuick; import QtQuick.Layouts; ColumnLayout { Layout.fillWidth: true; spacing: 0 }";
    }
    if (tag === "span" || tag === "p" || tag === "label" || tag === "h1" || tag === "h2" || tag === "h3" || tag === "a") {
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
    var templates = _root.eldurTemplates;
    var keys = _buildTemplateKeys(raw);
    var exact = keys[0];
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
    if (!_warnedMissingTemplates[exact]) {
        _warnedMissingTemplates[exact] = true;
        console.warn("eldur: no style template for " + exact + "; using generic");
    }
    return { component: null, exactKey: exact, step: "generic" };
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

    _itemMeta.delete(oldRaw);
    _itemMeta.set(next, m);
    m.raw = next;
    if (typeof m._setRaw === "function") {
        m._setRaw(next);
    }
    oldRaw.destroy();
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
        _insertRawNodesBefore(_appendTarget(next), childElements, null);
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
            eventTypes[lKeys[ei]] = true;
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
        if (res) {
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

function _parseHtmlFragmentNodes(html) {
    var src = String(html === null || html === undefined ? "" : html);
    var nodes = [];
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
            }
            break;
        }
        if (m.index > pos) {
            var lead = src.slice(pos, m.index);
            if (lead.trim()) {
                nodes.push(document.createTextNode(_unescapeHtmlText(lead)));
            }
        }
        var tag = m[1].toLowerCase();
        var attrStr = m[2] || "";
        var selfClosing = m[3] === "/" || Boolean(voidTags[tag]);
        var el = document.createElement(tag);
        _applyAttrsFromHtml(el, attrStr);

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
        get type() { return m.inputType || getAttribute(rawItem, "type") || ""; },
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
        get innerText() { return getText(rawItem); },
        set innerText(v) { setText(rawItem, v); },
        get outerHTML() { return _serializeElement(rawItem); },
        set outerHTML(v) {
            this.replaceWith.apply(this, _parseHtmlFragmentNodes(v));
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
        get hidden() { return rawItem.visible !== undefined ? !rawItem.visible : Boolean(m.explicitHidden); },
        set hidden(v) {
            m.explicitHidden = Boolean(v);
            _reflectFlag(m, "hidden", v);
            if (rawItem.__eldurInline) {
                _vChanged(rawItem.model);
            } else if (typeof rawItem.visible === "boolean") {
                _setShown(rawItem, !v);
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
        get className() { return getClassName(rawItem); },
        set className(v) { setClassName(rawItem, v); },
        get classList() { return classList(rawItem); },
        get dataset() { return dataset(rawItem); },
        get offsetWidth() { _vSettle(); return typeof rawItem.width === "number" ? rawItem.width : 0; },
        get offsetHeight() { _vSettle(); return typeof rawItem.height === "number" ? rawItem.height : 0; },
        get clientWidth() { return typeof rawItem.width === "number" ? rawItem.width : 0; },
        get clientHeight() { return typeof rawItem.height === "number" ? rawItem.height : 0; },
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
            return _computeScrollDimension("x");
        },
        get scrollHeight() {
            return _computeScrollDimension("y");
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
        removeEventListener: function(t, fn) { unlisten(rawItem, t, fn); },
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
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, null);
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
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, rawRef);
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
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, rawOld);
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
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, null);
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
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, first);
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
            _insertRawNodesBefore(_appendTarget(p), rawNodes, rawItem);
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
            _insertRawNodesBefore(_appendTarget(p), rawNodes, nextRef);
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
            _insertRawNodesBefore(_appendTarget(p), rawNodes, rawItem);
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
            var rawNodes = _expandInsertRawNodes(arguments);
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, null);
        },
        cloneNode: function(deep) {
            if (rawItem.__eldurInline) {
                return wrap(_vClone(rawItem, Boolean(deep)));
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
                        clone.appendChild(wrap(kids[c]).cloneNode(true));
                    }
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
    if (_vHasRules() && _isPlainTextItem(raw) && _vIsTagged(raw) && !_meta(raw).createdTag) {
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
    if (!text || _meta(raw).flow) {
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
    _vRules = [];
    var table = _vHasRules() && _root.eldurInlineRules !== "" ? JSON.parse(_root.eldurInlineRules) : [];
    for (var i = 0; i < table.length; ++i) {
        var decls = table[i].decls.filter(function(d) { return _vProps.indexOf(d[0]) !== -1; });
        if (decls.length > 0 && table[i].sel.indexOf("::") === -1) {
            var last = /([.#]?)([\w-]+)$/.exec(table[i].sel);
            _vRules.push({
                sel: table[i].sel,
                rank: table[i].spec[0] * 1000000 + table[i].spec[1] * 1000 + table[i].spec[2],
                ua: table[i].ua,
                decls: decls,
                lastKind: last ? last[1] : null,
                lastName: last ? last[2] : null,
                list: undefined
            });
        }
    }
    return _vRules;
}

function _vRuleMatches(rule, v, m) {
    if (rule.lastKind === "") {
        if (rule.lastName !== v.eldurTag) {
            return false;
        }
    } else if (rule.lastKind === ".") {
        if (m.classes.indexOf(rule.lastName) === -1) {
            return false;
        }
    } else if (rule.lastKind === "#") {
        if (rule.lastName !== v.objectName) {
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
        if (_vRuleMatches(rules[i], v, m)) {
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

function _vCascade(v, pc) {
    var rules = _vLoadRules();
    var matched = _vMatched(v);
    var won = {};
    function offer(prop, value, weight) {
        if (!won[prop] || weight >= won[prop].weight) {
            won[prop] = { value: value, weight: weight };
        }
    }
    var band = 10000000000;
    for (var i = 0; i < matched.length; ++i) {
        var rule = rules[matched[i]];
        for (var d = 0; d < rule.decls.length; ++d) {
            var decl = rule.decls[d];
            var origin = decl[2] ? (rule.ua ? 4 : 3) : (rule.ua ? 0 : 1);
            offer(decl[0], decl[1], origin * band + rule.rank);
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
        case "fontWeight": return String(c.weight);
        case "fontStyle": return c.italic ? "italic" : "normal";
        case "display": return c.display;
        case "fontSize": return c.size + "px";
        case "fontFamily": return c.family;
        case "backgroundColor": return c.bg;
        case "visibility": return c.visible ? "visible" : "hidden";
        case "textDecorationLine":
            return c.underline ? (c.strike ? "underline line-through" : "underline") : c.strike ? "line-through" : "none";
        default: return "";
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
    var st = {
        out: "", prevSpace: true, pre: model.pre, html: model.html, twin: twin,
        owners: [], inLink: false, linkColor: null,
        bareLink: model.bare && model.kids.length === 1 && Boolean(model.kids[0].kids)
            && !_vStyle(model.kids[0]).underline
    };
    _vWriteKids(st, model.kids, base, null);
    if (!st.pre) {
        st.out = st.out.replace(/ ((?:<\/[a-z]+>)*)$/, "$1");
    }
    return st;
}

// The first write takes the `Text` over from what the converter emitted: a
// `Text` styled as its one element goes back to the block's style, as that
// element's style now comes from the markup.
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
        text.lineHeightMode = Text.ProportionalHeight;
        text.lineHeight = 1;
    }
    text.textFormat = model.html ? 1 : 4;
    if (st.linkColor !== null) {
        text.linkColor = st.linkColor;
    }
    text.text = markup;
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
    var link = twin.linkAt(x, y);
    if (link.indexOf("eldur:") !== 0) {
        return null;
    }
    return model.twinOwners[Number(link.slice(6))] || null;
}

function _vRect(v) {
    var none = { x: 0, y: 0, width: 0, height: 0 };
    if (!v.model || v.model.select || v.vtext) {
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
    return maxX < minX ? none : { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

function _vClientRect(v) {
    var r = _vRect(v);
    var origin = v.model && !v.model.select && r.width > 0
        ? v.model.text.mapToItem(null, r.x, r.y) : { x: 0, y: 0 };
    return { x: origin.x, y: origin.y, left: origin.x, top: origin.y, right: origin.x + r.width,
        bottom: origin.y + r.height, width: r.width, height: r.height };
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

function _vReconnect(node) {
    if (node.vtext || !node.model) {
        return;
    }
    var m = _meta(node);
    if ((m.listeners.click && m.listeners.click.length > 0) || m.onHandlers.click) {
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

function _deepestAt(scene) {
    var cur = _root.contentItem || _root;
    var found = cur;
    while (cur && typeof cur.childAt === "function") {
        var local = cur.mapFromItem(null, scene.x, scene.y);
        var next = cur.childAt(local.x, local.y);
        if (!next) {
            break;
        }
        found = next;
        cur = next;
    }
    return found;
}

function _beginTap(qtEv) {
    var scene = qtEv.scenePosition;
    if (_tap && _tap.x === scene.x && _tap.y === scene.y) {
        return _tap;
    }
    var deep = _deepestAt(scene);
    var t = { x: scene.x, y: scene.y, deep: deep, target: null, control: null, ev: null, ran: [] };
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
    t.target = t.target || _unwrap(document.body);
    t.ev = event("click", wrap(t.target), qtEv);
    t.ev.target = wrap(t.target);
    t.ev.bubbles = true;
    // A control's own `clicked` starts its path, if the press comes to a click.
    if (!t.control) {
        _runPath(t.target, t.ev, t.ran);
    }
    return t;
}

function _runPath(from, ev, ran) {
    for (var at = from; at && !ev._propagationStopped; at = _logicalParent(at)) {
        if (_isDomTarget(at) && !_isInternalHelperNode(at)) {
            if (ran) {
                ran.push(at);
            }
            _dispatchDynamic(at, "click", ev);
        }
    }
}

function _flushTap() {
    _tap = null;
}

// A link's own handler asks; a tap on it comes first.
function linkPrevented() {
    return Boolean(_tap && _tap.ev && _tap.ev.defaultPrevented);
}

function tapPrevented(eventPoint) {
    return _vHasRules() && Boolean(_beginTap(eventPoint).ev.defaultPrevented);
}

function _vClick(v) {
    var ev = event("click", wrap(v), null);
    ev.target = wrap(v);
    _runPath(v, ev, null);
}

// A control takes the press for itself, so no handler above it hears: the
// event goes up from here.
function _clickedControl(raw) {
    var ev = event("click", wrap(raw), null);
    ev.target = wrap(raw);
    _runPath(raw, ev, null);
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
        return _unwrap(document.createTextNode(v.data));
    }
    var m = _meta(v);
    var made = _unwrap(document.createElement(v.eldurTag));
    var mm = _meta(made);
    var script = m.wrapper;
    _vRemove(v);
    if (v.objectName) {
        made.objectName = v.objectName;
    }
    var text = _vText(v);
    _itemMeta.delete(v);
    _itemMeta.set(made, m);
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
    var keys = Object.keys(m.styles);
    for (var i = 0; i < keys.length; ++i) {
        setStyle(made, keys[i], m.styles[keys[i]]);
    }
    var kinds = ["click", "dblclick"];
    for (var e = 0; e < kinds.length; ++e) {
        if ((m.listeners[kinds[e]] && m.listeners[kinds[e]].length > 0) || m.onHandlers[kinds[e]]) {
            _connectSignalOnce(made, kinds[e]);
        }
    }
    void mm;
    return made;
}

function _vRemove(node) {
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
    _layoutChanged(combo);
}

function _vSelectModel(raw) {
    var model = raw && !raw.__eldurInline ? _vModelOf(raw) : null;
    return model && model.select ? model : null;
}

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

function _vStaysInline(node) {
    var tag = node.__eldurInline ? node.eldurTag : String(_meta(node).createdTag || "");
    return /^(#text|a|abbr|b|bdi|bdo|br|cite|code|data|del|dfn|em|font|i|ins|kbd|label|mark|q|s|samp|small|span|strike|strong|sub|sup|time|tt|u|var|wbr)$/.test(tag);
}

// Where `nodes` go, if a line of text takes them; else they are real items.
function _vHostOf(raw, nodes, ref) {
    if (!_vHasRules() && !raw.__eldurInline) {
        return null;
    }
    if (ref && ref.__eldurInline) {
        return ref.vparent || ref.model;
    }
    if (raw.__eldurInline) {
        return raw;
    }
    var known = _vExisting(raw);
    if (known) {
        return known;
    }
    if (!nodes.every(_vStaysInline)) {
        return null;
    }
    var hasNode = nodes.some(function(n) { return n.__eldurInline; });
    if (_meta(raw).createdTag && hasNode && _vStaysInline(raw)) {
        return _vFromCreated(raw);
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
    return _rawKids(raw, true);
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
    "hover": function(n) { return n.hovered === true; },
    "active": function(n) { return n.pressed === true || n.down === true; },
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
        if (_activeElement && (_activeElement.activeFocus || _activeElement.focus)) {
            return wrap(_activeElement);
        }
        var focused = null;
        if (_root) {
            _walkItems(_root, function(node) {
                if (node !== _root && (node.activeFocus || node.focus)) {
                    focused = node;
                }
                return false;
            });
        }
        if (focused) {
            return wrap(focused);
        }
        if (_activeElement) {
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
        if (tag === "input") {
            _meta(created).lastGenericInputSource = qmlSrc;
        }
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
        if (_root) {
            listen(_root, type, fn, opts);
        }
    },
    removeEventListener: function(type, fn) {
        if (_root) {
            unlisten(_root, type, fn);
        }
    },
    dispatchEvent: function(ev) {
        return _root ? dispatchEvent(_root, ev) : true;
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
        var scroller = _findRootScroller();
        return scroller && typeof scroller.contentX === "number" ? scroller.contentX : 0;
    },
    get scrollY() {
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
        if (_root) {
            listen(_root, type, fn, opts);
        }
    },
    removeEventListener: function(type, fn) {
        if (_root) {
            unlisten(_root, type, fn);
        }
    },
    dispatchEvent: function(ev) {
        return _root ? dispatchEvent(_root, ev) : true;
    }
};
