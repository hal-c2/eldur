// The effect of a layer for a CSS `filter`. The shader is eldur_filter.frag:
//   qsb --glsl "100 es,120,150" --hlsl 50 --msl 12 -o eldur_filter.frag.qsb eldur_filter.frag
// Blur and shadow come after the colours whatever order the CSS has them in.
import QtQuick
import QtQuick.Effects

ShaderEffect {
    id: filter

    property var source
    property matrix4x4 colorMatrix: Qt.matrix4x4()
    property vector4d colorOffset: Qt.vector4d(0, 0, 0, 0)

    // `blur()`'s standard deviation.
    property real blurDeviation: 0
    property bool hasDrop: false
    property real dropX: 0
    property real dropY: 0
    property real dropDeviation: 0
    property color dropColor: "black"

    fragmentShader: "eldur_filter.frag.qsb"

    layer.enabled: blurDeviation > 0 || hasDrop
    layer.effect: MultiEffect {
        autoPaddingEnabled: true
        blurEnabled: filter.blurDeviation > 0
        blurMax: Math.ceil(Math.max(filter.blurDeviation, filter.dropDeviation) * 4)
        blur: 1
        shadowEnabled: filter.hasDrop
        shadowHorizontalOffset: filter.dropX
        shadowVerticalOffset: filter.dropY
        shadowColor: filter.dropColor
        shadowBlur: filter.dropDeviation > 0 ? 1 : 0
    }
}
