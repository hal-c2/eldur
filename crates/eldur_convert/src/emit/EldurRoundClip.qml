// The effect of a layer that clips a box and all it holds to rounded corners.
// A layer is needed because QML's own `clip` is square.
import QtQuick
import QtQuick.Effects

MultiEffect {
    id: rounded

    property real radius: 0
    property real topLeftRadius: radius
    property real topRightRadius: radius
    property real bottomRightRadius: radius
    property real bottomLeftRadius: radius

    maskEnabled: true
    maskSource: shape
    maskThresholdMin: 0.5
    maskSpreadAtMin: 1

    Rectangle {
        id: shape
        visible: false
        width: rounded.width
        height: rounded.height
        topLeftRadius: rounded.topLeftRadius
        topRightRadius: rounded.topRightRadius
        bottomRightRadius: rounded.bottomRightRadius
        bottomLeftRadius: rounded.bottomLeftRadius
        layer.enabled: true
        layer.smooth: true
    }
}
