// Written by Eldur beside Main.qml: what an inline element paints of its own box,
// behind the run of text that is its content. EldurLines says where the run's
// lines are; each line holds one fragment, cut where the line breaks
// (box-decoration-break: slice, CSS Fragmentation 3 §5.4).
import QtQuick
import "eldur_fonts.js" as EldurFonts

Item {
    id: box

    property var fragments: []
    property font font
    property color fill: "transparent"
    // Corners top-left, top-right, bottom-right, bottom-left; sides top, right,
    // bottom, left.
    property var radii: [0, 0, 0, 0]
    property var borderWidths: [0, 0, 0, 0]
    property var borderColors: ["transparent", "transparent", "transparent", "transparent"]
    property real paddingTop: 0
    property real paddingBottom: 0
    // How far the box reaches past the text beside the run's first and last line.
    property real reachLeft: 0
    property real reachRight: 0
    // A box cut by an atomic inline has no edge on the side of the cut.
    property bool opens: true
    property bool closes: true

    readonly property bool even: borderWidths[0] === borderWidths[1]
        && borderWidths[1] === borderWidths[2] && borderWidths[2] === borderWidths[3]
        && Qt.colorEqual(borderColors[0], borderColors[1])
        && Qt.colorEqual(borderColors[1], borderColors[2])
        && Qt.colorEqual(borderColors[2], borderColors[3])
    readonly property var extents: EldurFonts.extents(font)
    readonly property real overhang: Math.max(radii[0], radii[1], radii[2], radii[3],
                                              borderWidths[1], borderWidths[3]) + 2

    z: -1

    Repeater {
        model: box.fragments.length

        Item {
            id: fragment

            required property int index
            readonly property var at: box.fragments[index] || { x: 0, width: 0, ascent: 0 }
            readonly property bool first: index === 0 && box.opens
            readonly property bool last: index === box.fragments.length - 1 && box.closes

            x: at.x - (first ? box.reachLeft : 0)
            y: at.ascent - box.extents.ascent + (at.y || 0) - box.paddingTop - box.borderWidths[0]
            width: at.width + (first ? box.reachLeft : 0) + (last ? box.reachRight : 0)
            height: box.extents.ascent + box.extents.descent + box.paddingTop
                + box.paddingBottom + box.borderWidths[0] + box.borderWidths[2]
            clip: !first || !last

            // Past a cut the box goes on, so its corners and side stay out of view.
            Rectangle {
                x: fragment.first ? 0 : -box.overhang
                width: fragment.width + (fragment.first ? 0 : box.overhang) + (fragment.last ? 0 : box.overhang)
                height: fragment.height
                color: box.fill
                topLeftRadius: box.radii[0]
                topRightRadius: box.radii[1]
                bottomRightRadius: box.radii[2]
                bottomLeftRadius: box.radii[3]
                border.width: box.even ? box.borderWidths[0] : 0
                border.color: box.borderColors[0]
            }
            Repeater {
                model: box.even ? 0 : 4

                Rectangle {
                    required property int index
                    readonly property real across: box.borderWidths[index]

                    visible: across > 0 && (index === 0 || index === 2
                        || index === 1 && fragment.last || index === 3 && fragment.first)
                    color: box.borderColors[index]
                    x: index === 1 ? fragment.width - across : 0
                    y: index === 2 ? fragment.height - across : 0
                    width: index === 1 || index === 3 ? across : fragment.width
                    height: index === 0 || index === 2 ? across : fragment.height
                }
            }
        }
    }
}
