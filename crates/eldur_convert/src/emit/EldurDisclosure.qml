// The triangle of a `<details>`' summary, as Chromium draws it: its bottom edge
// sits on the baseline. The proportions are measured from Chromium at 13, 16, 24
// and 40px; the runtime turns it as the `<details>` opens and closes.

import QtQuick
import QtQuick.Shapes

Item {
    id: marker

    property bool open: false
    property color color: "black"
    property real em: 16

    readonly property bool eldurDisclosure: true
    // In a line it sits on the baseline with its bottom edge, as a box with no text.
    readonly property string eldurBaseline: "none"

    width: em * 1.05859
    height: Math.round(em * 0.7)

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: marker.color
            strokeColor: "transparent"
            startX: marker.em * (marker.open ? 0.015 : 0)
            startY: marker.height - marker.em * (marker.open ? 0.625 : 0.645)
            PathLine {
                x: marker.em * (marker.open ? 0.67 : 0.56)
                y: marker.height - marker.em * (marker.open ? 0.625 : 0.33)
            }
            PathLine {
                x: marker.em * (marker.open ? 0.335 : 0)
                y: marker.height - marker.em * (marker.open ? 0.035 : 0)
            }
            PathLine {
                x: marker.em * (marker.open ? 0.015 : 0)
                y: marker.height - marker.em * (marker.open ? 0.625 : 0.645)
            }
        }
    }
}
