// One `background` layer that is more than a fill of the whole box: an image,
// or a gradient with a size, a position, a repeat or a box of its own. It sits
// on the box it is clipped to. One ShapePath paints it, whatever it holds, so
// that tiles and rounded corners take one draw call however many tiles show.
import QtQuick
import QtQuick.Shapes

Item {
    id: layer

    property url source
    // A gradient, drawn into one tile.
    default property alias gradient: gradientBox.data
    property bool isGradient: false

    // The box the layer is positioned against, as distances in from this item.
    property real areaLeft: 0
    property real areaTop: 0
    property real areaRight: 0
    property real areaBottom: 0
    // `background-attachment: fixed`: the window is that box.
    property bool fixedToWindow: false

    // 0 auto, 1 cover, 2 contain, 3 the sizes below. A side below 0 is `auto`.
    property int sizing: 0
    property real sizeWidth: -1
    property real sizeWidthPercent: 0
    property real sizeHeight: -1
    property real sizeHeightPercent: 0

    property real positionX: 0
    property real positionXPercent: 0
    property real positionY: 0
    property real positionYPercent: 0

    property bool repeatX: true
    property bool repeatY: true
    property bool spaced: false
    property bool rounded: false

    property real radius: 0
    property real topLeftRadius: radius
    property real topRightRadius: radius
    property real bottomRightRadius: radius
    property real bottomLeftRadius: radius

    property point windowAt: Qt.point(0, 0)
    function findWindowAt() {
        windowAt = layer.mapToItem(null, 0, 0)
    }
    Component.onCompleted: if (fixedToWindow) Qt.callLater(findWindowAt)

    readonly property real areaX: fixedToWindow ? -windowAt.x : areaLeft
    readonly property real areaY: fixedToWindow ? -windowAt.y : areaTop
    readonly property real areaWidth: fixedToWindow && Window.window
        ? Window.window.width : width - areaLeft - areaRight
    readonly property real areaHeight: fixedToWindow && Window.window
        ? Window.window.height : height - areaTop - areaBottom

    readonly property real naturalWidth: isGradient ? 0 : picture.sourceSize.width
    readonly property real naturalHeight: isGradient ? 0 : picture.sourceSize.height
    readonly property real fitScale: naturalWidth > 0 && naturalHeight > 0
        ? (sizing === 1 ? Math.max(areaWidth / naturalWidth, areaHeight / naturalHeight)
                        : Math.min(areaWidth / naturalWidth, areaHeight / naturalHeight))
        : 1
    readonly property real givenWidth: sizeWidth + sizeWidthPercent / 100 * areaWidth
    readonly property real givenHeight: sizeHeight + sizeHeightPercent / 100 * areaHeight
    readonly property bool hasWidth: sizing === 3 && (sizeWidth >= 0 || sizeWidthPercent > 0)
    readonly property bool hasHeight: sizing === 3 && (sizeHeight >= 0 || sizeHeightPercent > 0)
    // CSS Backgrounds 3 §3.9: an `auto` side keeps the image's ratio, and an
    // image with no size of its own, a gradient, takes the box.
    readonly property real sizedWidth: {
        if (sizing === 1 || sizing === 2)
            return naturalWidth > 0 ? naturalWidth * fitScale : areaWidth
        if (hasWidth)
            return givenWidth
        if (naturalWidth <= 0)
            return areaWidth
        return hasHeight && naturalHeight > 0 ? naturalWidth * givenHeight / naturalHeight : naturalWidth
    }
    readonly property real sizedHeight: {
        if (sizing === 1 || sizing === 2)
            return naturalHeight > 0 ? naturalHeight * fitScale : areaHeight
        if (hasHeight)
            return givenHeight
        if (naturalHeight <= 0)
            return areaHeight
        return hasWidth && naturalWidth > 0 ? naturalHeight * givenWidth / naturalWidth : naturalHeight
    }
    // `round` fits a whole number of tiles.
    readonly property real tileWidth: rounded && sizedWidth > 0
        ? areaWidth / Math.max(1, Math.round(areaWidth / sizedWidth)) : sizedWidth
    readonly property real tileHeight: rounded && sizedHeight > 0
        ? areaHeight / Math.max(1, Math.round(areaHeight / sizedHeight)) : sizedHeight
    // `space` puts the leftover between whole tiles; with room for one only,
    // the tile stays where `background-position` puts it, unrepeated.
    readonly property int spacedColumns: tileWidth > 0 ? Math.floor(areaWidth / tileWidth) : 0
    readonly property int spacedRows: tileHeight > 0 ? Math.floor(areaHeight / tileHeight) : 0
    readonly property bool spacesX: spaced && spacedColumns > 1
    readonly property bool spacesY: spaced && spacedRows > 1
    readonly property real stepX: spacesX ? (areaWidth - tileWidth) / (spacedColumns - 1) : tileWidth
    readonly property real stepY: spacesY ? (areaHeight - tileHeight) / (spacedRows - 1) : tileHeight

    readonly property bool tilesX: spaced ? spacesX : repeatX
    readonly property bool tilesY: spaced ? spacesY : repeatY
    readonly property real tileX: spacesX ? areaX
        : areaX + positionX + positionXPercent / 100 * (areaWidth - tileWidth)
    readonly property real tileY: spacesY ? areaY
        : areaY + positionY + positionYPercent / 100 * (areaHeight - tileHeight)

    // What shows of the tiles: all of this item, or what one row, column or
    // tile leaves of it.
    readonly property real paintLeft: tilesX ? 0 : Math.max(0, tileX)
    readonly property real paintTop: tilesY ? 0 : Math.max(0, tileY)
    readonly property real paintRight: tilesX ? width : Math.min(width, tileX + tileWidth)
    readonly property real paintBottom: tilesY ? height : Math.min(height, tileY + tileHeight)
    readonly property bool atLeft: paintLeft <= 0
    readonly property bool atTop: paintTop <= 0
    readonly property bool atRight: paintRight >= width
    readonly property bool atBottom: paintBottom >= height

    readonly property bool drawsFile: !isGradient && !spaced && !tilesX && !tilesY
    readonly property real fileScaleX: drawsFile && naturalWidth > 0 ? tileWidth / naturalWidth : 1
    readonly property real fileScaleY: drawsFile && naturalHeight > 0 ? tileHeight / naturalHeight : 1

    Item {
        id: cell
        visible: false
        width: layer.stepX
        height: layer.stepY
        layer.enabled: layer.isGradient || layer.spaced
        layer.smooth: true
        layer.wrapMode: ShaderEffectSource.Repeat

        Item {
            id: gradientBox
            width: layer.tileWidth
            height: layer.tileHeight
        }
        Image {
            id: spacedPicture
            source: layer.spaced ? layer.source : ""
            width: layer.tileWidth
            height: layer.tileHeight
        }
    }
    Image {
        id: picture
        visible: false
        source: layer.source
        width: layer.tileWidth
        height: layer.tileHeight
        smooth: true
        // Only a repeat needs a texture that wraps; one tile draws the file's.
        layer.enabled: !layer.isGradient && !layer.spaced && (layer.tilesX || layer.tilesY)
        layer.smooth: true
        layer.wrapMode: ShaderEffectSource.Repeat
    }
    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        visible: layer.paintRight > layer.paintLeft && layer.paintBottom > layer.paintTop
        ShapePath {
            strokeWidth: -1
            fillItem: layer.isGradient || layer.spaced ? cell : picture
            // A layer's texture is the tile already; the file's is its own size.
            fillTransform: Qt.matrix4x4(layer.fileScaleX, 0, 0, layer.tileX,
                                        0, layer.fileScaleY, 0, layer.tileY,
                                        0, 0, 1, 0,
                                        0, 0, 0, 1)
            PathRectangle {
                x: layer.paintLeft
                y: layer.paintTop
                width: layer.paintRight - layer.paintLeft
                height: layer.paintBottom - layer.paintTop
                topLeftRadius: layer.atLeft && layer.atTop ? layer.topLeftRadius : 0
                topRightRadius: layer.atRight && layer.atTop ? layer.topRightRadius : 0
                bottomRightRadius: layer.atRight && layer.atBottom ? layer.bottomRightRadius : 0
                bottomLeftRadius: layer.atLeft && layer.atBottom ? layer.bottomLeftRadius : 0
            }
        }
    }
}
