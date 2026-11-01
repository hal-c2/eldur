// One `text-shadow`: the text again, behind itself.
import QtQuick
import QtQuick.Effects

Text {
    id: copy

    required property Text source
    property real blurDeviation: 0

    z: -1
    width: source.width
    height: source.height
    text: source.text
    font: source.font
    textFormat: source.textFormat
    wrapMode: source.wrapMode
    elide: source.elide
    lineHeight: source.lineHeight
    lineHeightMode: source.lineHeightMode
    horizontalAlignment: source.horizontalAlignment
    verticalAlignment: source.verticalAlignment

    layer.enabled: blurDeviation > 0
    layer.effect: MultiEffect {
        autoPaddingEnabled: true
        blurEnabled: true
        blurMax: Math.ceil(copy.blurDeviation * 4)
        blur: 1
    }
}
