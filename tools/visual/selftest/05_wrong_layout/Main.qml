// Deliberately mismatched layout and palette to verify that compare.py penalises
// structural and colour errors in --selftest.
import QtQuick
import QtQuick.Controls.Basic

ApplicationWindow {
    width: 1280
    height: 800
    visible: true
    title: "Mismatched Layout"
    color: "#fef3c7"

    Rectangle {
        x: 760
        y: 440
        width: 380
        height: 240
        radius: 4
        color: "#dc2626"

        Text {
            anchors.centerIn: parent
            text: "Collapsed Box"
            font.family: "Liberation Sans"
            font.pixelSize: 20
            font.weight: Font.Bold
            color: "#ffffff"
        }
    }
}
