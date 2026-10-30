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

// Per-item sidecar storage keyed by raw QML QObject (or synthetic node).
// QML QObjects reject ad-hoc JS property assignments, whereas ES6 Map keys
// preserve QObject identity across direct id references and tree walks.
var _itemMeta = new Map();

function init(rootItem) {
    if (rootItem) {
        _root = rootItem;
    }
    _startTime = Date.now();
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

function atob(encoded) {
    return Qt.atob(String(encoded));
}

function btoa(raw) {
    return Qt.btoa(String(raw));
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

var location = {
    href: "",
    protocol: "file:",
    host: "",
    hostname: "",
    port: "",
    pathname: "/",
    search: "",
    hash: "",
    origin: "null",
    assign: function(url) {
        this.href = String(url);
    },
    replace: function(url) {
        this.href = String(url);
    },
    reload: function() {
        console.warn("H2Q: location.reload() is unsupported");
    },
    toString: function() {
        return this.href;
    }
};

var history = {
    length: 1,
    state: null,
    pushState: function(state, _unused, url) {
        this.state = state;
        if (url !== undefined && url !== null) {
            location.href = String(url);
        }
    },
    replaceState: function(state, _unused, url) {
        this.state = state;
        if (url !== undefined && url !== null) {
            location.href = String(url);
        }
    },
    back: function() {},
    forward: function() {},
    go: function() {}
};

var navigator = {
    userAgent: "html-2-qml/0.1.0",
    onLine: true
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
    if (item && typeof item === "object" && item.__h2qWrapper) {
        return item._item;
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

function classList(item) {
    var raw = _unwrap(item);
    var m = _meta(raw);
    return {
        add: function() {
            for (var i = 0; i < arguments.length; ++i) {
                var cls = String(arguments[i]);
                if (cls && m.classes.indexOf(cls) === -1) {
                    m.classes.push(cls);
                }
                _syncClassProp(raw, cls, true);
            }
        },
        remove: function() {
            for (var i = 0; i < arguments.length; ++i) {
                var cls = String(arguments[i]);
                var idx = m.classes.indexOf(cls);
                if (idx !== -1) {
                    m.classes.splice(idx, 1);
                }
                _syncClassProp(raw, cls, false);
            }
        },
        toggle: function(clsName, force) {
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
            _syncClassProp(raw, cls, shouldHave);
            return shouldHave;
        },
        contains: function(clsName) {
            var cls = String(clsName);
            if (m.classes.indexOf(cls) !== -1) {
                return true;
            }
            if (raw && typeof raw === "object") {
                var prop = _sanitizeClassProp(cls);
                if (typeof raw[prop] === "boolean") {
                    return raw[prop];
                }
            }
            return false;
        },
        replace: function(oldCls, newCls) {
            var oldStr = String(oldCls);
            var newStr = String(newCls);
            var idx = m.classes.indexOf(oldStr);
            if (idx === -1) {
                return false;
            }
            m.classes[idx] = newStr;
            _syncClassProp(raw, oldStr, false);
            _syncClassProp(raw, newStr, true);
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
    for (var i = 0; i < m.classes.length; ++i) {
        _syncClassProp(raw, m.classes[i], false);
    }
    m.classes.length = 0;
    var trimmed = String(value).trim();
    if (trimmed.length > 0) {
        var parts = trimmed.split(/\s+/);
        for (var j = 0; j < parts.length; ++j) {
            if (parts[j] && m.classes.indexOf(parts[j]) === -1) {
                m.classes.push(parts[j]);
                _syncClassProp(raw, parts[j], true);
            }
        }
    }
}

function getClassName(item) {
    return _meta(item).classes.join(" ");
}

function dataset(item) {
    return _meta(item).dataset;
}

function _findTextChild(raw) {
    if (!raw || !raw.children) {
        return null;
    }
    for (var i = 0; i < raw.children.length; ++i) {
        var child = raw.children[i];
        if (child && typeof child.text === "string") {
            return child;
        }
    }
    return null;
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
        return textChild.text;
    }
    return "";
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
    if (typeof raw.text === "string") {
        raw.text = str;
        return;
    }
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
    var name = String(prop);
    var str = value === null || value === undefined ? "" : String(value);
    _meta(raw).styles[name] = str;

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

function getStyle(item, prop) {
    var m = _meta(item);
    var name = String(prop);
    if (Object.prototype.hasOwnProperty.call(m.styles, name)) {
        return m.styles[name];
    }
    var raw = _unwrap(item);
    if (name === "display" && raw && typeof raw.visible === "boolean") {
        return raw.visible ? "" : "none";
    }
    return "";
}

function setAttribute(item, attrName, attrValue) {
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
    if (name === "class") {
        setClassName(raw, val);
    } else if (name === "disabled" && typeof raw.enabled === "boolean") {
        raw.enabled = false;
    } else if (name === "hidden" && typeof raw.visible === "boolean") {
        raw.visible = false;
    } else if (name === "value") {
        if (typeof raw.find === "function" && typeof raw.currentIndex === "number") {
            raw.currentIndex = raw.find(val);
        } else if (typeof raw.text === "string") {
            raw.text = val;
        } else if (typeof raw.value === "number") {
            raw.value = Number(val) || 0;
        }
    } else if (name === "src" && raw.source !== undefined) {
        raw.source = val;
    } else if (name === "placeholder" && typeof raw.placeholderText === "string") {
        raw.placeholderText = val;
    }
}

function getAttribute(item, attrName) {
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
    if (Object.prototype.hasOwnProperty.call(m.attrs, name)) {
        return m.attrs[name];
    }
    if (name === "id" && typeof raw.objectName === "string" && raw.objectName.length > 0) {
        return raw.objectName;
    }
    if (name === "class" && m.classes.length > 0) {
        return m.classes.join(" ");
    }
    return null;
}

function removeAttribute(item, attrName) {
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
    } else if (name === "class") {
        setClassName(raw, "");
    }
}

function hasAttribute(item, attrName) {
    return getAttribute(item, attrName) !== null;
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

    var domEvent = {
        type: String(type),
        target: targetItem || null,
        currentTarget: targetItem || null,
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
    return e;
}

function CustomEvent(type, opts) {
    return Event(type, opts);
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
        if (typeof raw.textChanged === "function") {
            raw.textChanged.connect(function() { _dispatchDynamic(raw, "input", null); });
            m.connected[type] = true;
            return true;
        }
    } else if (type === "change") {
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
               typeof raw.activeFocusChanged === "function") {
        raw.activeFocusChanged.connect(function() {
            if (raw.activeFocus && (type === "focus" || type === "focusin")) {
                _dispatchDynamic(raw, type, null);
            } else if (!raw.activeFocus && (type === "blur" || type === "focusout")) {
                _dispatchDynamic(raw, type, null);
            }
        });
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
    } else if (type === "scroll" && typeof raw.contentYChanged === "function") {
        raw.contentYChanged.connect(function() { _dispatchDynamic(raw, "scroll", null); });
        m.connected[type] = true;
        return true;
    } else if (type === "resize" && typeof raw.widthChanged === "function") {
        raw.widthChanged.connect(function() { _dispatchDynamic(raw, "resize", null); });
        if (typeof raw.heightChanged === "function") {
            raw.heightChanged.connect(function() { _dispatchDynamic(raw, "resize", null); });
        }
        m.connected[type] = true;
        return true;
    }

    return false;
}

function listen(item, eventType, handler, options) {
    if (!item || typeof handler !== "function") {
        return;
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
    if (!_connectSignalOnce(raw, type)) {
        console.warn("H2Q: dynamic listener for '" + type + "' cannot bind to a QML signal");
    }
}

function unlisten(item, eventType, handler) {
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
        return;
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
}

function dispatchEvent(item, domEv) {
    if (!item || !domEv || !domEv.type) {
        return true;
    }
    return _dispatchDynamic(_unwrap(item), String(domEv.type), domEv);
}

// ---------------------------------------------------------------------------
// 9. Minimal Element wrapper and H2Q.document tree lookup
// ---------------------------------------------------------------------------

function wrap(rawItem) {
    if (!rawItem) {
        return null;
    }
    if (rawItem.__h2qWrapper) {
        return rawItem;
    }
    var m = _meta(rawItem);
    if (m.wrapper) {
        return m.wrapper;
    }

    var styleProxy = new Proxy({}, {
        get: function(_t, prop) {
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
        _item: rawItem,
        style: styleProxy,
        get id() {
            return typeof rawItem.objectName === "string" ? rawItem.objectName : "";
        },
        set id(v) {
            if (typeof rawItem.objectName === "string") {
                rawItem.objectName = String(v);
            }
        },
        get textContent() { return getText(rawItem); },
        set textContent(v) { setText(rawItem, v); },
        get innerText() { return getText(rawItem); },
        set innerText(v) { setText(rawItem, v); },
        get innerHTML() { return getInnerHTML(rawItem); },
        set innerHTML(v) { setInnerHTML(rawItem, v); },
        get value() {
            if (typeof rawItem.currentText === "string") {
                return rawItem.currentText;
            }
            if (typeof rawItem.text === "string") {
                return rawItem.text;
            }
            if (typeof rawItem.value === "number") {
                return rawItem.value;
            }
            return "";
        },
        set value(v) {
            if (typeof rawItem.find === "function" && typeof rawItem.currentIndex === "number") {
                rawItem.currentIndex = rawItem.find(String(v));
            } else if (typeof rawItem.text === "string") {
                rawItem.text = String(v);
            } else if (typeof rawItem.value === "number") {
                rawItem.value = Number(v) || 0;
            }
        },
        get checked() { return Boolean(rawItem.checked); },
        set checked(v) {
            if (typeof rawItem.checked === "boolean") {
                rawItem.checked = Boolean(v);
            }
        },
        get disabled() { return rawItem.enabled !== undefined ? !rawItem.enabled : false; },
        set disabled(v) {
            if (typeof rawItem.enabled === "boolean") {
                rawItem.enabled = !v;
            }
        },
        get hidden() { return rawItem.visible !== undefined ? !rawItem.visible : false; },
        set hidden(v) {
            if (typeof rawItem.visible === "boolean") {
                rawItem.visible = !v;
            }
        },
        get src() { return rawItem.source !== undefined ? String(rawItem.source) : ""; },
        set src(v) {
            if (rawItem.source !== undefined) {
                rawItem.source = v;
            }
        },
        get placeholder() { return typeof rawItem.placeholderText === "string" ? rawItem.placeholderText : ""; },
        set placeholder(v) {
            if (typeof rawItem.placeholderText === "string") {
                rawItem.placeholderText = String(v);
            }
        },
        get className() { return getClassName(rawItem); },
        set className(v) { setClassName(rawItem, v); },
        get classList() { return classList(rawItem); },
        get dataset() { return dataset(rawItem); },
        get offsetWidth() { return typeof rawItem.width === "number" ? rawItem.width : 0; },
        get offsetHeight() { return typeof rawItem.height === "number" ? rawItem.height : 0; },
        get parentNode() { return rawItem.parent ? wrap(rawItem.parent) : null; },
        get children() {
            var out = [];
            if (rawItem.children) {
                for (var i = 0; i < rawItem.children.length; ++i) {
                    out.push(wrap(rawItem.children[i]));
                }
            }
            return out;
        },
        setAttribute: function(k, v) { setAttribute(rawItem, k, v); },
        getAttribute: function(k) { return getAttribute(rawItem, k); },
        removeAttribute: function(k) { removeAttribute(rawItem, k); },
        hasAttribute: function(k) { return hasAttribute(rawItem, k); },
        addEventListener: function(t, fn, opts) { listen(rawItem, t, fn, opts); },
        removeEventListener: function(t, fn) { unlisten(rawItem, t, fn); },
        dispatchEvent: function(ev) { return dispatchEvent(rawItem, ev); },
        focus: function() {
            if (typeof rawItem.forceActiveFocus === "function") {
                rawItem.forceActiveFocus();
            }
        },
        blur: function() {
            if (typeof rawItem.focus === "boolean") {
                rawItem.focus = false;
            }
        },
        getBoundingClientRect: function() { return getBoundingClientRect(rawItem); },
        appendChild: function(child) {
            var rawChild = _unwrap(child);
            if (rawChild && rawChild.parent !== undefined) {
                var container = rawItem.contentItem || rawItem;
                rawChild.parent = container;
            }
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
            var rawNew = _unwrap(newChild);
            var rawRef = _unwrap(refChild);
            var container = rawItem.contentItem || rawItem;
            if (!rawNew || rawNew.parent === undefined) {
                return newChild;
            }
            if (!rawRef || !container.children) {
                rawNew.parent = container;
                return newChild;
            }
            var kids = [];
            for (var i = 0; i < container.children.length; ++i) {
                kids.push(container.children[i]);
            }
            var idx = kids.indexOf(rawRef);
            if (idx === -1) {
                rawNew.parent = container;
                return newChild;
            }
            var tail = kids.slice(idx);
            for (var j = 0; j < tail.length; ++j) {
                tail[j].parent = null;
            }
            rawNew.parent = container;
            for (var k = 0; k < tail.length; ++k) {
                tail[k].parent = container;
            }
            return newChild;
        },
        querySelector: function(sel) {
            var matches = _queryTree(rawItem, String(sel), true);
            return matches.length > 0 ? wrap(matches[0]) : null;
        },
        querySelectorAll: function(sel) {
            return _queryTree(rawItem, String(sel), false).map(wrap);
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

function _matchesSimpleSelector(node, sel) {
    var s = sel.trim();
    if (!s) {
        return false;
    }
    if (s === "*") {
        return true;
    }
    if (s.charAt(0) === "#") {
        return typeof node.objectName === "string" && node.objectName === s.slice(1);
    }
    if (s.charAt(0) === ".") {
        var reqClasses = s.slice(1).split(".");
        var cl = classList(node);
        for (var i = 0; i < reqClasses.length; ++i) {
            if (reqClasses[i] && !cl.contains(reqClasses[i])) {
                return false;
            }
        }
        return true;
    }
    var dotIdx = s.indexOf(".");
    var tagPart = dotIdx === -1 ? s : s.slice(0, dotIdx);
    var classPart = dotIdx === -1 ? "" : s.slice(dotIdx);
    var nodeTag = typeof node.h2qTag === "string" ? node.h2qTag.toLowerCase() : "";
    if (tagPart && nodeTag !== tagPart.toLowerCase()) {
        return false;
    }
    if (classPart && !_matchesSimpleSelector(node, classPart)) {
        return false;
    }
    return true;
}

function _queryTree(startNode, selector, firstOnly) {
    var rootNode = startNode || _root;
    var results = [];
    if (!rootNode) {
        return results;
    }
    var s = String(selector).trim();
    if (/[>+~\[\]:]/.test(s) || /\s/.test(s)) {
        console.warn("H2Q: complex runtime selector is unsupported:", s);
        return results;
    }
    _walkItems(rootNode, function(node) {
        if (node !== rootNode && _matchesSimpleSelector(node, s)) {
            results.push(node);
            if (firstOnly) {
                return true;
            }
        }
        return false;
    });
    return results;
}

var document = {
    get title() { return getTitle(); },
    set title(v) { setTitle(v); },
    get body() { return _root ? wrap(_root.contentItem || _root) : null; },
    get documentElement() { return _root ? wrap(_root) : null; },
    get hidden() { return false; },
    getElementById: function(id) {
        var targetId = String(id);
        var found = null;
        if (_root) {
            _walkItems(_root, function(node) {
                if (typeof node.objectName === "string" && node.objectName === targetId) {
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
        return _queryTree(_root, selector, false).map(wrap);
    },
    getElementsByClassName: function(classNames) {
        var parts = String(classNames).trim().split(/\s+/).filter(Boolean);
        if (parts.length === 0) {
            return [];
        }
        return _queryTree(_root, "." + parts.join("."), false).map(wrap);
    },
    getElementsByTagName: function(tagName) {
        return _queryTree(_root, String(tagName), false).map(wrap);
    },
    createElement: function(tagName) {
        var tag = String(tagName).toLowerCase();
        var qmlSrc = "import QtQuick; Item {}";
        if (tag === "span" || tag === "p" || tag === "label" || tag === "h1" || tag === "h2" || tag === "h3") {
            qmlSrc = "import QtQuick; Text {}";
        } else if (tag === "div" || tag === "section" || tag === "article") {
            qmlSrc = "import QtQuick; Rectangle { color: \"transparent\" }";
        } else if (tag === "img") {
            qmlSrc = "import QtQuick; Image {}";
        } else if (tag === "button") {
            qmlSrc = "import QtQuick.Controls.Basic; Button {}";
        } else if (tag === "input") {
            qmlSrc = "import QtQuick.Controls.Basic; TextField {}";
        }
        var created = Qt.createQmlObject(qmlSrc, _qobjectParent(), "h2q_el_" + tag);
        if (created && created.parent !== undefined) {
            created.parent = null;
        }
        return wrap(created);
    },
    createTextNode: function(text) {
        var created = Qt.createQmlObject("import QtQuick; Text {}", _qobjectParent(), "h2q_text");
        if (created) {
            created.text = String(text);
            if (created.parent !== undefined) {
                created.parent = null;
            }
        }
        return wrap(created);
    },
    createDocumentFragment: function() {
        var created = Qt.createQmlObject("import QtQuick; Item {}", _qobjectParent(), "h2q_frag");
        if (created && created.parent !== undefined) {
            created.parent = null;
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
        return _root && typeof _root.contentX === "number" ? _root.contentX : 0;
    },
    get scrollY() {
        return _root && typeof _root.contentY === "number" ? _root.contentY : 0;
    },
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
    Event: Event,
    CustomEvent: CustomEvent,
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
    }
};
