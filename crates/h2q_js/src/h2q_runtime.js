.pragma library
.import QtQuick.LocalStorage 2.0 as QmlLocalStorage

// html-2-qml JavaScript runtime shim (h2q_runtime.js)
//
// Imported by generated page.js files via:
//   .import "h2q_runtime.js" as H2Q
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
// 3. Dynamic DOM fallback shim (H2Q.document, H2Q.classList, H2Q.dataset,
//    H2Q.setStyle, H2Q.setAttribute, H2Q.setInnerHTML, H2Q.listen) and DOM-like
//    event normalization (H2Q.event).
//
// Converter assumption for dynamic DOM lookups:
//   H2Q.document.getElementById(id) and H2Q.document.querySelector("#" + id)
//   walk the QML item tree from the root passed to H2Q.init(rootItem) and match
//   items whose `objectName` equals the HTML element's `id`. Tag and class
//   queries match optional `h2qTag` / `h2qClasses` properties or runtime
//   classList state recorded through H2Q.classList(item).

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

// Per-item sidecar storage keyed by raw QML QObject (or synthetic node).
// QML QObjects reject ad-hoc JS property assignments, whereas ES6 Map keys
// preserve QObject identity across direct id references and tree walks.
var _itemMeta = new Map();
var _warnedMissingTemplates = {};
var _componentProbeCache = new Map();

function init(rootItem, baseUrl) {
    if (rootItem) {
        _root = rootItem;
    }
    _warnedMissingTemplates = {};
    _componentProbeCache = new Map();
    if (typeof baseUrl === "string" && baseUrl !== "") {
        _explicitBaseUrl = baseUrl;
        _updateLocationFromHref(baseUrl, false);
    } else if (_root && typeof _root.baseUrl === "string" && _root.baseUrl !== "") {
        _explicitBaseUrl = _root.baseUrl;
        _updateLocationFromHref(_root.baseUrl, false);
    }
    _startTime = Date.now();
}

function _isQObject(obj) {
    if (!obj || typeof obj !== "object") {
        return false;
    }
    if (obj.__h2qWrapper || obj.__h2qRaw) {
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
    if (obj === window || obj === document || obj === _root || obj.__h2qWrapper || obj.__h2qRaw || obj.__h2qEventTarget) {
        return true;
    }
    return _isQObject(obj);
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
        _holder = Qt.createQmlObject("import QtQuick; QtObject {}", Qt.application, "h2q_holder");
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
        "h2q_timer_" + id
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
        "h2q_raf"
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
                "h2q_local_storage",
                "1.0",
                "html-2-qml localStorage",
                1048576
            );
            db.transaction(function(tx) {
                tx.executeSql("CREATE TABLE IF NOT EXISTS h2q_kv (k TEXT PRIMARY KEY, v TEXT NOT NULL)");
            });
            return db;
        } catch (err) {
            console.warn("H2Q: LocalStorage unavailable, falling back to memory:", err);
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
                var rs = tx.executeSql("SELECT v FROM h2q_kv WHERE k = ?", [k]);
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
                tx.executeSql("INSERT OR REPLACE INTO h2q_kv (k, v) VALUES (?, ?)", [k, v]);
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
                tx.executeSql("DELETE FROM h2q_kv WHERE k = ?", [k]);
            });
        },
        clear: function() {
            var handle = getDb();
            if (!handle) {
                fallback.clear();
                return;
            }
            handle.transaction(function(tx) {
                tx.executeSql("DELETE FROM h2q_kv");
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
                var rs = tx.executeSql("SELECT k FROM h2q_kv ORDER BY rowid ASC LIMIT 1 OFFSET ?", [i]);
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
                var rs = tx.executeSql("SELECT COUNT(*) AS c FROM h2q_kv");
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
    console.warn("H2Q: synchronous confirm() is unsupported:", message);
    return false;
}

function prompt(message, defaultValue) {
    console.warn("H2Q: synchronous prompt() is unsupported:", message, defaultValue);
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
    console.warn("H2Q: unsupported browser API:", feature);
    return undefined;
}

// Note on document base URL resolution for `a.href`:
// `H2Q.init(rootItem)` receives only the root QML Item (`Main.qml` calls
// `Page.__h2q_init(root)`), and the HTML-to-QML converter does not embed a
// source document URL into `Main.qml` or `page.js`. Consequently,
// `location.href` starts as `""`.
// - When `rawHref` is an absolute URL (e.g. `https://example.com` or
//   `https://example.com/a/b/../c`), `_resolveUrl` normalizes it per the
//   WHATWG URL / DOM `HTMLAnchorElement.href` rules (ensuring a trailing `/`
//   on bare origins and collapsing `.` / `..` path segments, which Qt 6.11's
//   built-in `new URL()` does not do).
// - When `rawHref` is a relative URL and a base URL is available via
//   `H2Q.init(rootItem, baseUrl)` or `_root.baseUrl`, `_resolveUrl` resolves `rawHref`
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

function _getDocumentBaseUrl() {
    if (_explicitBaseUrl !== "") {
        return _explicitBaseUrl;
    }
    if (_root && typeof _root.baseUrl === "string" && _root.baseUrl !== "") {
        return _root.baseUrl;
    }
    return "";
}

function _resolveUrl(rawHref) {
    if (rawHref === null || rawHref === undefined || rawHref === "") {
        return _getDocumentBaseUrl();
    }
    var str = String(rawHref).trim();
    if (str === "") {
        return _getDocumentBaseUrl();
    }
    var parsedAbs = _parseHierarchicalUrl(str);
    if (parsedAbs) {
        return parsedAbs.href;
    }
    if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(str)) {
        return str;
    }
    var baseStr = _getDocumentBaseUrl();
    var base = baseStr ? _parseHierarchicalUrl(baseStr) : null;
    if (!base) {
        // Runtime has no document base URL when converted statically unless
        // `H2Q.init(root, baseUrl)` or `root.baseUrl` was populated; return the raw
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
    console.warn("H2Q: " + method + " navigation is unsupported in QML: " + s);
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
        console.warn("H2Q: location.reload() is unsupported");
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
    userAgent: "html-2-qml/0.1.0",
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
        if (item.__h2qWrapper) {
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
    if (!obj || typeof obj !== "object" || obj.__h2qWrapper || _isQObject(obj)) {
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
        if (typeof raw.h2qClasses === "string" && raw.h2qClasses.length > 0) {
            var parts = raw.h2qClasses.trim().split(/\s+/);
            for (var i = 0; i < parts.length; ++i) {
                if (parts[i] && initialClasses.indexOf(parts[i]) === -1) {
                    initialClasses.push(parts[i]);
                }
            }
        }
        entry = {
            classes: initialClasses,
            attrs: {},
            dataset: {},
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

function _syncH2qClasses(raw, m) {
    if (raw && typeof raw === "object" && typeof raw.h2qClasses === "string") {
        var joined = m.classes.join(" ");
        if (raw.h2qClasses !== joined) {
            raw.h2qClasses = joined;
        }
    }
}

function _afterClassMutation(raw, m) {
    var curRaw = (m && m.raw) ? m.raw : raw;
    _syncH2qClasses(curRaw, m);
    if (_isCreatedElement(curRaw) && _isConnected(curRaw)) {
        _materializeSubtree(curRaw);
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
            && !boxChild.objectName && !_meta(boxChild).createdTag && typeof boxChild.h2qTag !== "string") {
            _meta(boxChild).hostBox = raw;
            return boxChild;
        }
    }
    var text = _findTextChild(raw);
    if (!text) {
        return raw;
    }
    var flow = Qt.createQmlObject("import QtQuick; import QtQuick.Layouts; ColumnLayout { spacing: 0 }", raw, "h2q_flow");
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

function _clearAppended(raw) {
    var flow = _meta(raw).flow;
    while (flow && flow.children.length > 0) {
        // `destroy` waits for the event loop, so a handler of a child being
        // removed can run to its end.
        var child = flow.children[0];
        child.parent = null;
        child.destroy();
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
    if (typeof raw.contentHeight === "number" && raw.contentItem) {
        return raw.contentItem;
    }
    if (typeof raw.spacing === "number" || typeof raw.columnSpacing === "number") {
        return raw;
    }
    var m = _meta(raw);
    if (m.flow) {
        return m.flow;
    }
    if ((raw.color !== undefined || raw.clip === true) && raw.children) {
        for (var i = 0; i < raw.children.length; ++i) {
            var ch = raw.children[i];
            if (ch && (typeof ch.spacing === "number" || typeof ch.columnSpacing === "number")
                && !ch.objectName && !_meta(ch).createdTag && typeof ch.h2qTag !== "string") {
                _meta(ch).hostBox = raw;
                return ch;
            }
        }
    }
    return raw;
}

function _rawElementChildren(raw) {
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
        if (!isLayout && _isPlainTextItem(child) && !child.objectName && !_meta(child).createdTag) {
            continue;
        }
        out.push(child);
    }
    return out;
}

function _logicalParent(raw) {
    if (!raw || !raw.parent) {
        return null;
    }
    var p = raw.parent;
    var pm = _meta(p);
    if (pm.hostBox) {
        return pm.hostBox;
    }
    if ((typeof p.spacing === "number" || typeof p.columnSpacing === "number")
        && !p.objectName && !pm.createdTag && typeof p.h2qTag !== "string"
        && p.parent && (p.parent.color !== undefined || p.parent.clip === true)
        && typeof p.parent.spacing !== "number" && typeof p.parent.columnSpacing !== "number") {
        return p.parent;
    }
    return p;
}

function _inferTagName(raw) {
    if (!raw) {
        return "";
    }
    if (typeof raw.h2qTag === "string" && raw.h2qTag !== "") {
        return raw.h2qTag.toUpperCase();
    }
    var m = _meta(raw);
    if (m.createdTag) {
        return String(m.createdTag).toUpperCase();
    }
    if (typeof raw.h2qTag === "string" && raw.h2qTag !== "") {
        return raw.h2qTag.toUpperCase();
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
    if (_isPlainJsObject(raw) && ("textContent" in raw || "innerText" in raw)) {
        return raw.textContent !== undefined ? raw.textContent : raw.innerText;
    }
    if (typeof raw.text === "string") {
        return raw.text;
    }
    var textChild = _findTextChild(raw);
    if (textChild) {
        return String(textChild.text);
    }
    var m = _meta(raw);
    return m.explicitText !== undefined ? String(m.explicitText) : "";
}

function setText(item, value) {
    var raw = _unwrap(item);
    if (!raw) {
        return;
    }
    if (_isPlainJsObject(raw)) {
        raw.textContent = value;
        return;
    }
    var str = value === null || value === undefined ? "" : String(value);
    var m = _meta(raw);
    m.explicitText = str;
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
        console.warn("H2Q: <script> and <style> inside innerHTML are unsupported");
        return;
    }
    var target = typeof raw.text === "string" ? raw : _findTextChild(raw);
    if (target && typeof target.text === "string") {
        if (typeof target.textFormat === "number") {
            // Text.StyledText === 4 in QtQuick
            target.textFormat = 4;
        }
        target.text = str;
        _meta(raw).innerHTML = str;
        return;
    }
    console.warn("H2Q: innerHTML assignment on non-text QML item is unsupported");
}

function getInnerHTML(item) {
    var m = _meta(item);
    if (m.innerHTML !== undefined) {
        return m.innerHTML;
    }
    return getText(item);
}

function setStyle(item, prop, value) {
    var raw = _unwrap(item);
    if (!raw) {
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
            raw.visible = str !== "none";
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
        raw.visible = false;
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
    var name = String(attrName);
    var m = _meta(raw);
    delete m.attrs[name];
    if (name.indexOf("data-") === 0 && name.length > 5) {
        delete m.dataset[_kebabToCamel(name.slice(5))];
    } else if (name === "disabled" && typeof raw.enabled === "boolean") {
        raw.enabled = true;
    } else if (name === "hidden" && typeof raw.visible === "boolean") {
        raw.visible = true;
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
        __h2qEventTarget: true,
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
    var m = _meta(raw);
    var target = m.wrapper || raw;
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
    if (_isPlainJsObject(raw)) {
        m.connected[type] = true;
        return true;
    }

    if (type === "click") {
        if (typeof raw.clicked === "function") {
            raw.clicked.connect(function() { _dispatchDynamic(raw, "click", null); });
            m.connected[type] = true;
            return true;
        }
        if (typeof raw.mapToItem === "function") {
            var tap = Qt.createQmlObject("import QtQuick; TapHandler {}", raw, "h2q_tap");
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
            var dtap = Qt.createQmlObject("import QtQuick; TapHandler {}", raw, "h2q_dbltap");
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
        var keySignal = type === "keydown" ? raw.h2qKeyDown : raw.h2qKeyUp;
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
        var hover = Qt.createQmlObject("import QtQuick; HoverHandler {}", raw, "h2q_hover");
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
        var moveHover = Qt.createQmlObject("import QtQuick; HoverHandler {}", raw, "h2q_move");
        moveHover.pointChanged.connect(function() {
            _dispatchDynamic(raw, "mousemove", {
                position: moveHover.point.position,
                scenePosition: moveHover.point.scenePosition
            });
        });
        m.connected[type] = true;
        return true;
    } else if (type === "wheel" && typeof raw.mapToItem === "function") {
        var wheelH = Qt.createQmlObject("import QtQuick; WheelHandler {}", raw, "h2q_wheel");
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
    } else if (type === "keypress" && typeof raw.h2qKeyDown === "function") {
        raw.h2qKeyDown.connect(function(qtEv) { _dispatchDynamic(raw, "keypress", qtEv); });
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
    if (_addListener(item, eventType, handler, options)
        && !_connectSignalOnce(_unwrap(item), String(eventType))) {
        console.warn("H2Q: dynamic listener for '" + eventType + "' cannot bind to a QML signal");
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
            console.warn("H2Q: dynamic on" + type + " handler cannot bind to a QML signal");
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
// 9. Minimal Element wrapper and H2Q.document tree lookup
// ---------------------------------------------------------------------------

var _NATIVE_IMPORTS = "import QtQuick; import QtQuick.Controls.Basic; ";

// Keys handlers accept the key unless told otherwise, and an accepted key
// never reaches the field; `preventDefault` accepts it.
var _KEY_SIGNALS = "signal h2qKeyDown(var event); signal h2qKeyUp(var event); "
    + "Keys.onPressed: (event) => { event.accepted = false; h2qKeyDown(event); } "
    + "Keys.onReleased: (event) => { event.accepted = false; h2qKeyUp(event); } ";

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
                : "Text { anchors.centerIn: parent; text: \"\\u2713\"; color: \"white\"; font.pixelSize: Math.max(1, control.height - 2); visible: control.checked } ")
            + "} }";
    }
    if (type === "range") {
        return _NATIVE_IMPORTS + "Slider { from: 0; to: 100; value: 50; stepSize: 1; snapMode: Slider.SnapAlways; " + _KEY_SIGNALS + "}";
    }
    if (type === "number") {
        return _NATIVE_IMPORTS + "SpinBox { editable: true; from: -2147483648; to: 2147483647; " + _KEY_SIGNALS + "}";
    }
    if (type === "submit" || type === "button" || type === "reset") {
        return _NATIVE_IMPORTS + "Button { " + _KEY_SIGNALS + "palette.buttonText: \"black\"; background: Rectangle { color: \"#efefef\"; border.color: \"#767676\"; radius: 3 } }";
    }
    return _NATIVE_IMPORTS + "TextField { " + _KEY_SIGNALS + "color: \"black\"; "
        + "background: Rectangle { color: \"white\"; border.color: \"#767676\"; radius: 2 } }";
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

function _insertRawNodesBefore(container, rawNodes, rawRef) {
    if (!container || rawNodes.length === 0) {
        return;
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
            _materializeSubtree(rawNodes[mIdx]);
        }
    }
}

function _genericElementSource(tag, inputType) {
    if (tag === "input") {
        return _inputSource(inputType || "text");
    }
    if (tag === "textarea") {
        return _NATIVE_IMPORTS + "TextArea { color: \"black\"; background: Rectangle { color: \"white\"; border.color: \"#767676\" } }";
    }
    if (tag === "li") {
        return "import QtQuick; import QtQuick.Layouts; RowLayout { Layout.fillWidth: true; spacing: 8 }";
    }
    if (tag === "ul" || tag === "ol") {
        return "import QtQuick; import QtQuick.Layouts; ColumnLayout { Layout.fillWidth: true; spacing: 0 }";
    }
    if (tag === "span" || tag === "p" || tag === "label" || tag === "h1" || tag === "h2" || tag === "h3" || tag === "a") {
        return "import QtQuick; Text { color: \"black\"; font.pixelSize: 16 }";
    }
    if (tag === "div" || tag === "section" || tag === "article" || tag === "header" || tag === "footer" || tag === "main" || tag === "nav") {
        return "import QtQuick; Rectangle { color: \"transparent\" }";
    }
    if (tag === "img") {
        return "import QtQuick; Image {}";
    }
    if (tag === "button") {
        return _NATIVE_IMPORTS + "Button { palette.buttonText: \"black\"; background: Rectangle { color: \"#efefef\"; border.color: \"#767676\"; radius: 3 } }";
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
        cur = _logicalParent(cur) || cur.parent || null;
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
        hasClasses: Boolean(probe && typeof probe.h2qClasses === "string"),
        upRanks: [],
        templateName: (probe && typeof probe.h2qTemplate === "string") ? probe.h2qTemplate : ""
    };
    if (probe) {
        for (var k in probe) {
            if (k.indexOf("cls_") === 0 && typeof probe[k] === "boolean") {
                schema.boolProps[k] = true;
            } else {
                var upMatch = /^h2qUp([1-9][0-9]*)$/.exec(k);
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
    var tag = String(m.createdTag || (typeof raw.h2qTag === "string" ? raw.h2qTag : "")).toLowerCase();
    var inputType = tag === "input" ? String(m.inputType || m.attrs.type || "").toLowerCase() : "";
    var tagPart = (tag === "input" && inputType !== "") ? ("input[type=" + inputType + "]") : tag;

    var styled = (_root && _root.h2qStyledClasses !== undefined && _root.h2qStyledClasses !== null && typeof _root.h2qStyledClasses.length === "number")
        ? _root.h2qStyledClasses
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
        if (typeof p.h2qTemplate === "string" && p.h2qTemplate !== "") {
            context = p.h2qTemplate;
        } else if (typeof p.h2qKey === "string" && p.h2qKey !== "") {
            context = p.h2qKey;
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
    if (!_root || !_root.h2qTemplates || typeof _root.h2qTemplates !== "object") {
        return null;
    }
    var templates = _root.h2qTemplates;
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
                console.warn("h2q: no style template for " + exact + "; using " + cand);
            }
            return { component: templates[cand], exactKey: exact, step: cand };
        }
    }
    if (!_warnedMissingTemplates[exact]) {
        _warnedMissingTemplates[exact] = true;
        console.warn("h2q: no style template for " + exact + "; using generic");
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
            raw["h2qUp" + rank] = anc;
        }
    } else {
        for (var n = 1; n <= 8; ++n) {
            var prop = "h2qUp" + n;
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
            initProps.h2qClasses = m.classes.join(" ");
        }
        for (var u = 0; u < schema.upRanks.length; ++u) {
            var rank = schema.upRanks[u];
            var anc = _logicalAncestorAt(oldRaw, rank);
            if (anc) {
                initProps["h2qUp" + rank] = anc;
            }
        }
        next = component.createObject(parentContainer || _qobjectParent(), initProps);
        if (!parentContainer && next && next.parent !== undefined) {
            next.parent = null;
        }
        m.templateComponent = component;
    } else {
        var tag = String(m.createdTag || "div").toLowerCase();
        var src = fallbackQmlSrc || _genericElementSource(tag, m.inputType);
        next = Qt.createQmlObject(src, _qobjectParent(), "h2q_el_" + tag);
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

    if (typeof next.h2qClasses === "string" && next.h2qClasses !== m.classes.join(" ")) {
        next.h2qClasses = m.classes.join(" ");
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
        var nextTarget = _appendTarget(next);
        for (var c = 0; c < childElements.length; ++c) {
            if (childElements[c] && childElements[c].parent !== undefined) {
                childElements[c].parent = null;
                childElements[c].parent = nextTarget;
            }
        }
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

function _materializeSubtree(raw) {
    if (!raw || typeof raw !== "object") {
        return raw;
    }
    var cur = raw;
    if (_isCreatedElement(cur)) {
        var m = _meta(cur);
        var res = _lookupTemplate(cur);
        if (res) {
            if (res.component) {
                var schema = _getComponentSchema(res.component);
                var sameComp = (m.templateComponent === res.component)
                    || (typeof cur.h2qTemplate === "string" && cur.h2qTemplate !== "" && schema.templateName !== "" && cur.h2qTemplate === schema.templateName);
                if (!sameComp) {
                    cur = _swapRawItem(cur, res.component, null);
                } else {
                    m.templateComponent = res.component;
                    _syncUpPointersOnItem(cur);
                }
            } else {
                var wasTemplate = Boolean(m.templateComponent || (typeof cur.h2qTemplate === "string" && cur.h2qTemplate !== ""));
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
    if (rawItem.__h2qWrapper) {
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
        __h2qWrapper: true,
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
            if (_isConnected(rawItem) && _root && _root.h2qTemplates && typeof _root.h2qTemplates === "object") {
                _materializeSubtree(rawItem);
                return;
            }
            if (_inputSource(ty) === _inputSource(from) && !m.templateComponent) {
                return;
            }
            m.lastGenericInputSource = _inputSource(ty);
            _swapRawItem(rawItem, null, _inputSource(ty));
        },
        get id() {
            if (typeof rawItem.objectName === "string" && rawItem.objectName !== "") {
                return rawItem.objectName;
            }
            return m.attrs.id || "";
        },
        set id(v) {
            var s = String(v);
            m.attrs.id = s;
            if (typeof rawItem.objectName === "string") {
                rawItem.objectName = s;
            }
        },
        get tagName() { return _inferTagName(rawItem); },
        get nodeName() { return _inferTagName(rawItem); },
        get textContent() { return getText(rawItem); },
        set textContent(v) { setText(rawItem, v); },
        get innerText() { return getText(rawItem); },
        set innerText(v) { setText(rawItem, v); },
        get innerHTML() { return getInnerHTML(rawItem); },
        set innerHTML(v) { setInnerHTML(rawItem, v); },
        get value() {
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
            return typeof rawItem.currentIndex === "number" ? rawItem.currentIndex : -1;
        },
        set selectedIndex(v) {
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
        get disabled() { return rawItem.enabled !== undefined ? !rawItem.enabled : Boolean(m.explicitDisabled); },
        set disabled(v) {
            m.explicitDisabled = Boolean(v);
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
            if (typeof rawItem.visible === "boolean") {
                rawItem.visible = !v;
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
                return "";
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
        get offsetWidth() { return typeof rawItem.width === "number" ? rawItem.width : 0; },
        get offsetHeight() { return typeof rawItem.height === "number" ? rawItem.height : 0; },
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
        get parentNode() {
            var p = _logicalParent(rawItem);
            return p ? wrap(p) : null;
        },
        get parentElement() {
            var p = _logicalParent(rawItem);
            return p ? wrap(p) : null;
        },
        get children() {
            return _asHTMLCollection(_rawElementChildren(rawItem).map(wrap));
        },
        get childNodes() {
            return _asNodeList(_rawElementChildren(rawItem).map(wrap));
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
            var kids = _rawElementChildren(rawItem);
            return kids.length > 0 ? wrap(kids[0]) : null;
        },
        get lastChild() {
            var kids = _rawElementChildren(rawItem);
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
            return this.nextElementSibling;
        },
        get previousSibling() {
            return this.previousElementSibling;
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
                cur = _logicalParent(cur) || cur.parent || null;
            }
            return false;
        },
        appendChild: function(child) {
            var rawNodes = _expandInsertRawNodes([child]);
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, null);
            return child;
        },
        removeChild: function(child) {
            var rawChild = _unwrap(child);
            if (rawChild && rawChild.parent !== undefined) {
                rawChild.parent = null;
            }
            return child;
        },
        remove: function() {
            if (rawItem.parent !== undefined) {
                rawItem.parent = null;
            }
        },
        insertBefore: function(newChild, refChild) {
            var rawNodes = _expandInsertRawNodes([newChild]);
            var rawRef = _unwrap(refChild);
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, rawRef);
            return newChild;
        },
        replaceChild: function(newChild, oldChild) {
            var rawOld = _unwrap(oldChild);
            if (!rawOld) {
                return oldChild;
            }
            var rawNodes = _expandInsertRawNodes([newChild]);
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, rawOld);
            if (rawOld.parent !== undefined) {
                rawOld.parent = null;
            }
            return oldChild;
        },
        append: function() {
            var rawNodes = _expandInsertRawNodes(arguments);
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, null);
        },
        prepend: function() {
            var rawNodes = _expandInsertRawNodes(arguments);
            var kids = _rawElementChildren(rawItem);
            var first = kids.length > 0 ? kids[0] : null;
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, first);
        },
        before: function() {
            var p = _logicalParent(rawItem);
            if (!p) {
                return;
            }
            var rawNodes = _expandInsertRawNodes(arguments);
            _insertRawNodesBefore(_appendTarget(p), rawNodes, rawItem);
        },
        after: function() {
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
            var p = _logicalParent(rawItem);
            if (!p) {
                return;
            }
            var rawNodes = _expandInsertRawNodes(arguments);
            _insertRawNodesBefore(_appendTarget(p), rawNodes, rawItem);
            if (rawItem.parent !== undefined) {
                rawItem.parent = null;
            }
        },
        replaceChildren: function() {
            var existing = _rawElementChildren(rawItem);
            for (var i = 0; i < existing.length; ++i) {
                if (existing[i].parent !== undefined) {
                    existing[i].parent = null;
                }
            }
            var rawNodes = _expandInsertRawNodes(arguments);
            _insertRawNodesBefore(_appendTarget(rawItem), rawNodes, null);
        },
        cloneNode: function(deep) {
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
            var parts = String(classNames).trim().split(/\s+/).filter(Boolean);
            if (parts.length === 0) {
                return _asHTMLCollection([]);
            }
            return _asHTMLCollection(_queryTree(rawItem, "." + parts.join("."), false).map(wrap));
        },
        getElementsByTagName: function(tagName) {
            return _asHTMLCollection(_queryTree(rawItem, String(tagName), false).map(wrap));
        }
    };

    m.wrapper = wrapper;
    return wrapper;
}

function _walkItems(node, visitor) {
    if (!node) {
        return false;
    }
    if (visitor(node)) {
        return true;
    }
    if (node.contentItem && node.contentItem !== node) {
        if (_walkItems(node.contentItem, visitor)) {
            return true;
        }
    }
    if (node.children) {
        for (var i = 0; i < node.children.length; ++i) {
            if (_walkItems(node.children[i], visitor)) {
                return true;
            }
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
    if (_isPlainTextItem(node) && !node.objectName && !m.createdTag && typeof node.h2qTag !== "string"
        && node.parent && (node.parent.color !== undefined || node.parent.clip === true)) {
        return true;
    }
    if ((typeof node.spacing === "number" || typeof node.columnSpacing === "number")
        && !node.objectName && !m.createdTag && typeof node.h2qTag !== "string") {
        return true;
    }
    return false;
}

function _matchesSimpleSelector(node, sel) {
    var s = sel.trim();
    if (!s || _isInternalHelperNode(node)) {
        return false;
    }
    if (s === "*") {
        return true;
    }
    var attrFilters = [];
    var withoutAttrs = s.replace(/\[\s*([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*(?:([~|^$*]?=)\s*(?:"([^"]*)"|'([^']*)'|([^\]]*?)))?\s*\]/g, function(_all, attrName, op, dq, sq, uq) {
        var val = dq !== undefined ? dq : (sq !== undefined ? sq : (uq !== undefined ? uq.trim() : ""));
        attrFilters.push({ name: attrName, op: op || null, val: val });
        return "";
    });
    for (var a = 0; a < attrFilters.length; ++a) {
        var af = attrFilters[a];
        var actual = getAttribute(node, af.name);
        if (af.op === null) {
            if (actual === null) {
                return false;
            }
        } else if (actual === null) {
            return false;
        } else if (af.op === "=" && actual !== af.val) {
            return false;
        } else if (af.op === "^=" && actual.indexOf(af.val) !== 0) {
            return false;
        } else if (af.op === "$=" && (af.val === "" || actual.slice(-af.val.length) !== af.val)) {
            return false;
        } else if (af.op === "*=" && (af.val === "" || actual.indexOf(af.val) === -1)) {
            return false;
        } else if (af.op === "~=" && actual.trim().split(/\s+/).indexOf(af.val) === -1) {
            return false;
        }
    }

    var rest = withoutAttrs.trim();
    if (!rest) {
        return attrFilters.length > 0;
    }

    var idFilter = null;
    rest = rest.replace(/#([A-Za-z0-9_-]+)/g, function(_all, idVal) {
        idFilter = idVal;
        return "";
    });
    if (idFilter !== null) {
        var nodeId = (typeof node.objectName === "string" && node.objectName !== "")
            ? node.objectName
            : (_meta(node).attrs.id || "");
        if (nodeId !== idFilter) {
            return false;
        }
    }

    var classFilters = [];
    rest = rest.replace(/\.([A-Za-z0-9_-]+)/g, function(_all, clsVal) {
        classFilters.push(clsVal);
        return "";
    });
    if (classFilters.length > 0) {
        var cl = classList(node);
        for (var i = 0; i < classFilters.length; ++i) {
            if (!cl.contains(classFilters[i])) {
                return false;
            }
        }
    }

    var tagPart = rest.trim();
    if (tagPart && tagPart !== "*") {
        var nodeTag = (typeof node.h2qTag === "string" && node.h2qTag !== "")
            ? node.h2qTag.toLowerCase()
            : _inferTagName(node).toLowerCase();
        if (nodeTag !== tagPart.toLowerCase()) {
            return false;
        }
    }
    return true;
}

function _splitSelectorList(selector) {
    var list = [];
    var cur = "";
    var depth = 0;
    var quote = null;
    for (var i = 0; i < selector.length; ++i) {
        var ch = selector.charAt(i);
        if (quote) {
            if (ch === quote) {
                quote = null;
            }
            cur += ch;
        } else if (ch === "\"" || ch === "'") {
            quote = ch;
            cur += ch;
        } else if (ch === "[") {
            depth += 1;
            cur += ch;
        } else if (ch === "]" && depth > 0) {
            depth -= 1;
            cur += ch;
        } else if (ch === "," && depth === 0) {
            if (cur.trim()) {
                list.push(cur.trim());
            }
            cur = "";
        } else {
            cur += ch;
        }
    }
    if (cur.trim()) {
        list.push(cur.trim());
    }
    return list;
}

function _tokenizeCombinators(sel) {
    var tokens = [];
    var cur = "";
    var depth = 0;
    var quote = null;
    var pendingComb = " ";
    for (var i = 0; i < sel.length; ++i) {
        var ch = sel.charAt(i);
        if (quote) {
            if (ch === quote) {
                quote = null;
            }
            cur += ch;
            continue;
        }
        if (ch === "\"" || ch === "'") {
            quote = ch;
            cur += ch;
            continue;
        }
        if (ch === "[") {
            depth += 1;
            cur += ch;
            continue;
        }
        if (ch === "]" && depth > 0) {
            depth -= 1;
            cur += ch;
            continue;
        }
        if (depth === 0 && (ch === ">" || /\s/.test(ch))) {
            if (cur.trim()) {
                tokens.push({ comb: pendingComb, compound: cur.trim() });
                cur = "";
                pendingComb = ch === ">" ? ">" : " ";
            } else if (ch === ">") {
                pendingComb = ">";
            }
            continue;
        }
        cur += ch;
    }
    if (cur.trim()) {
        tokens.push({ comb: pendingComb, compound: cur.trim() });
    }
    return tokens;
}

function _matchesChain(node, tokens, idx) {
    if (!_matchesSimpleSelector(node, tokens[idx].compound)) {
        return false;
    }
    if (idx === 0) {
        return true;
    }
    var comb = tokens[idx].comb;
    var p = _logicalParent(node);
    if (comb === ">") {
        return p ? _matchesChain(p, tokens, idx - 1) : false;
    }
    while (p) {
        if (_matchesChain(p, tokens, idx - 1)) {
            return true;
        }
        p = _logicalParent(p);
    }
    return false;
}

function _matchesSelectorComplex(node, selector) {
    if (!node || !selector) {
        return false;
    }
    var parts = _splitSelectorList(String(selector));
    for (var i = 0; i < parts.length; ++i) {
        var part = parts[i];
        if (/[+~:]/.test(part.replace(/\[[^\]]*\]/g, ""))) {
            console.warn("H2Q: complex runtime selector is unsupported:", part);
            continue;
        }
        var tokens = _tokenizeCombinators(part);
        if (tokens.length > 0 && _matchesChain(node, tokens, tokens.length - 1)) {
            return true;
        }
    }
    return false;
}

function _queryTree(startNode, selector, firstOnly) {
    var rootNode = startNode || _root;
    var results = [];
    if (!rootNode) {
        return results;
    }
    var s = String(selector).trim();
    if (!s) {
        return results;
    }
    _walkItems(rootNode, function(node) {
        if (node !== rootNode && _matchesSelectorComplex(node, s)) {
            results.push(node);
            if (firstOnly) {
                return true;
            }
        }
        return false;
    });
    return results;
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
    get body() { return _root ? wrap(_root.contentItem || _root) : null; },
    get documentElement() { return _root ? wrap(_root) : null; },
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
        var found = null;
        if (_root) {
            _walkItems(_root, function(node) {
                if ((typeof node.objectName === "string" && node.objectName === targetId)
                    || _meta(node).attrs.id === targetId) {
                    found = node;
                    return true;
                }
                return false;
            });
        }
        return found ? wrap(found) : null;
    },
    querySelector: function(selector) {
        var matches = _queryTree(_root, selector, true);
        return matches.length > 0 ? wrap(matches[0]) : null;
    },
    querySelectorAll: function(selector) {
        return _asNodeList(_queryTree(_root, selector, false).map(wrap));
    },
    getElementsByClassName: function(classNames) {
        var parts = String(classNames).trim().split(/\s+/).filter(Boolean);
        if (parts.length === 0) {
            return _asHTMLCollection([]);
        }
        return _asHTMLCollection(_queryTree(_root, "." + parts.join("."), false).map(wrap));
    },
    getElementsByTagName: function(tagName) {
        return _asHTMLCollection(_queryTree(_root, String(tagName), false).map(wrap));
    },
    createElement: function(tagName) {
        var tag = String(tagName).toLowerCase();
        var qmlSrc = _genericElementSource(tag, "text");
        var created = Qt.createQmlObject(qmlSrc, _qobjectParent(), "h2q_el_" + tag);
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
        var created = Qt.createQmlObject("import QtQuick; Text {}", _qobjectParent(), "h2q_text");
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
        var created = Qt.createQmlObject("import QtQuick; Item {}", _qobjectParent(), "h2q_frag");
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
        console.warn("H2Q: document.write() is unsupported");
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
