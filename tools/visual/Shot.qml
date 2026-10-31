// Wrapper loaded by render_qml.py to screenshot a target Main.qml at WxH.
//
// Why QtObject is the root: Main.qml may itself have an ApplicationWindow (or
// Window) root. Using a non-window root avoids opening a second top-level
// window when Main.qml is already a Window, while hostWindowComp provides a
// host Window only when Main.qml's root is an Item.
import QtQuick
import QtQuick.Window

QtObject {
    id: root

    // Temporary graphical parent for createObject() so QQuickItem roots do not
    // emit "Created graphical object was not placed in the graphics scene"
    // before we inspect instanceof and reparent into hostWindowComp.
    property Item stagingItem: Item {}

    property Window targetWindow: null
    property Item captureRoot: null
    property QtObject loadedObject: null
    property string outputPath: ""
    property int reqWidth: 1280
    property int reqHeight: 800
    property int quietMs: 250
    property int maxWaitMs: 4000
    // "software" or "gl": the scene graph render_qml.py asked for. Checked at
    // grab time so a Qt that quietly picked another one fails the render
    // instead of producing scores that are not comparable between runs.
    property string expectedBackend: ""
    property Item backdrop: null
    property int totalTicks: 0
    property bool imagesReady: false
    property bool grabStarted: false
    // Settling: a frame is swapped only when something in the scene changed
    // (QQuickItem::update() / polish schedule one); a static scene swaps no
    // frames. So the page has settled once no frame has been swapped for
    // quietMs, after at least one frame at the requested size.
    property bool settling: false
    property bool frameAtSize: false
    property double settleStart: 0
    property double lastActivity: 0

    property Component hostWindowComp: Component {
        Window {
            visible: true
            color: "#ffffff"
        }
    }

    // QQuickItem::grabToImage renders the item subtree onto a transparent buffer
    // without QQuickWindow::color; placing a low-z backdrop Rectangle preserves
    // the window/canvas background colour in the captured PNG.
    property Component backdropComp: Component {
        Rectangle {
            // GraphicsInfo is only known inside a window once its scene graph
            // is initialised; the backdrop is the item Shot.qml owns there.
            readonly property int graphicsApi: GraphicsInfo.api
            anchors.fill: parent
            z: -1000000
        }
    }

    property Connections windowConn: Connections {
        target: root.targetWindow
        function onFrameSwapped() {
            if (!root.settling)
                return;
            if (root.targetWindow.width === root.reqWidth && root.targetWindow.height === root.reqHeight)
                root.frameAtSize = true;
            root.lastActivity = Date.now();
        }
        function onWidthChanged() { root.sizeChanged(); }
        function onHeightChanged() { root.sizeChanged(); }
    }

    // Only watches the clock; it never requests frames, which would keep the
    // scene from ever looking quiet.
    property Timer settleWatch: Timer {
        interval: 16
        repeat: true
        onTriggered: {
            root.totalTicks += 1;
            const now = Date.now();
            if (!root.imagesReady) {
                if (!root.hasLoadingImages(root.captureRoot) || root.totalTicks > 120)
                    root.startSettling();
                return;
            }
            if (root.hasLoadingImages(root.captureRoot))
                root.lastActivity = now;
            if (root.frameAtSize && now - root.lastActivity >= root.quietMs) {
                root.triggerGrab();
            } else if (now - root.settleStart >= root.maxWaitMs) {
                console.warn("Shot.qml: page did not settle within " + root.maxWaitMs
                             + " ms (frames kept changing); capturing anyway");
                root.triggerGrab();
            }
        }
    }

    function startSettling() {
        root.imagesReady = true;
        root.settling = true;
        root.frameAtSize = false;
        root.settleStart = Date.now();
        root.lastActivity = root.settleStart;
        // One frame so a scene that is already static is drawn at least once.
        if (root.targetWindow)
            root.targetWindow.requestUpdate();
    }

    function sizeChanged() {
        if (!root.settling)
            return;
        root.frameAtSize = false;
        root.lastActivity = Date.now();
        root.targetWindow.requestUpdate();
    }

    function hasLoadingImages(item: Item): bool {
        if (!item)
            return false;
        if (item instanceof Image) {
            const img = item as Image;
            if (img.status === Image.Loading)
                return true;
        }
        const kids = item.children;
        for (let i = 0; i < kids.length; ++i) {
            if (root.hasLoadingImages(kids[i]))
                return true;
        }
        return false;
    }

    // ApplicationWindow overrides contentItem with an internal C++-created
    // QQuickContentItem that has no QQmlEngine context, causing grabToImage()
    // to fail unless called on the owning QQuickRootItem at the top of the
    // visual parent chain.
    function topVisualRoot(item: Item): Item {
        let cur = item;
        while (cur && cur.parent) {
            cur = cur.parent;
        }
        return cur;
    }

    function triggerGrab() {
        if (root.grabStarted || !root.captureRoot || !root.targetWindow)
            return;
        root.grabStarted = true;
        settleWatch.stop();

        // grabToImage scales to targetSize, so a window the platform resized
        // (a tiling compositor, a too-small screen) would come out stretched
        // instead of failing.
        if (root.targetWindow.width !== root.reqWidth || root.targetWindow.height !== root.reqHeight) {
            console.error("Shot.qml: window is " + root.targetWindow.width + "x" + root.targetWindow.height
                          + ", expected " + root.reqWidth + "x" + root.reqHeight);
            Qt.exit(2);
            return;
        }

        const api = root.backdrop ? root.backdrop.graphicsApi : GraphicsInfo.Unknown;
        const wantApi = root.expectedBackend === "gl" ? GraphicsInfo.OpenGL
                      : root.expectedBackend === "software" ? GraphicsInfo.Software : api;
        if (api !== wantApi) {
            console.error("Shot.qml: scene graph API is " + api + ", expected " + wantApi
                          + " for backend " + root.expectedBackend);
            Qt.exit(3);
            return;
        }

        const targetSize = Qt.size(root.targetWindow.width, root.targetWindow.height);
        const ok = root.captureRoot.grabToImage(function(result) {
            if (!result.saveToFile(root.outputPath)) {
                console.error("Shot.qml: failed to write PNG to " + root.outputPath);
                Qt.exit(2);
                return;
            }
            Qt.quit();
        }, targetSize);

        if (!ok) {
            console.error("Shot.qml: grabToImage() returned false");
            Qt.exit(2);
        }
    }

    function instantiateComponent(comp: Component) {
        if (comp.status === Component.Error) {
            console.error(comp.errorString());
            Qt.exit(1);
            return;
        }
        if (comp.status !== Component.Ready)
            return;

        const obj = comp.createObject(root.stagingItem);
        if (obj === null) {
            console.error("Shot.qml: createObject() failed: " + comp.errorString());
            Qt.exit(1);
            return;
        }
        root.loadedObject = obj;

        const w = root.reqWidth;
        const h = root.reqHeight;

        if (obj instanceof Window) {
            const win = obj as Window;
            win.width = w;
            win.height = h;
            win.visible = true;
            // Explicit show() bypasses QQuickWindowQmlImpl's parent-item
            // visibility deferral caused by stagingItem not being in a window.
            win.show();
            root.targetWindow = win;

            const rootItem = root.topVisualRoot(win.contentItem);
            const bg = backdropComp.createObject(rootItem) as Rectangle;
            root.backdrop = bg;
            bg.color = Qt.binding(function() { return win.color; });
            root.captureRoot = rootItem;
        } else if (obj instanceof Item) {
            const item = obj as Item;
            const win = hostWindowComp.createObject(root, { width: w, height: h }) as Window;
            const rootItem = win.contentItem;
            root.backdrop = backdropComp.createObject(rootItem, { color: "#ffffff" }) as Rectangle;
            item.parent = rootItem;
            item.width = w;
            item.height = h;
            root.targetWindow = win;
            root.captureRoot = rootItem;
        } else {
            console.error("Shot.qml: unsupported root type (expected Window or Item): " + obj);
            Qt.exit(1);
            return;
        }

        if (!root.hasLoadingImages(root.captureRoot))
            root.startSettling();
        settleWatch.start();
    }

    Component.onCompleted: {
        const args = Qt.application.arguments;
        let sepIdx = -1;
        for (let i = 0; i < args.length; ++i) {
            if (args[i] === "--") {
                sepIdx = i;
                break;
            }
        }
        const tail = sepIdx >= 0 ? args.slice(sepIdx + 1) : args.slice(Math.max(0, args.length - 4));
        if (tail.length < 4) {
            console.error("Shot.qml: expected arguments -- <mainUrl> <width> <height> <outPath> [quietMs maxWaitMs [software|gl]], got: " + JSON.stringify(args));
            Qt.exit(2);
            return;
        }

        const mainUrl = tail[0];
        const w = parseInt(tail[1], 10);
        const h = parseInt(tail[2], 10);
        const outPath = tail[3];

        if (!(w > 0) || !(h > 0) || !outPath) {
            console.error("Shot.qml: invalid width/height/outPath: " + JSON.stringify(tail));
            Qt.exit(2);
            return;
        }

        root.reqWidth = w;
        root.reqHeight = h;
        root.outputPath = outPath;
        if (tail.length >= 6) {
            root.quietMs = parseInt(tail[4], 10);
            root.maxWaitMs = parseInt(tail[5], 10);
        }
        if (tail.length >= 7)
            root.expectedBackend = tail[6];

        const comp = Qt.createComponent(mainUrl);
        if (comp.status === Component.Loading) {
            comp.statusChanged.connect(function() {
                root.instantiateComponent(comp);
            });
        } else {
            root.instantiateComponent(comp);
        }
    }
}
