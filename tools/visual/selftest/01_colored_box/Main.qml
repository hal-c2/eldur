import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

ApplicationWindow {
    width: 1280
    height: 800
    visible: true
    title: "Colored Box"
    color: "#f1f5f9"

    Rectangle {
        x: 64
        y: 56
        width: 560
        implicitHeight: cardLayout.implicitHeight + 68
        height: implicitHeight
        color: "#1e293b"
        border.width: 2
        border.color: "#3b82f6"
        radius: 14

        ColumnLayout {
            id: cardLayout
            anchors.fill: parent
            anchors.margins: 34
            spacing: 14

            Rectangle {
                Layout.preferredWidth: 124
                Layout.preferredHeight: 28
                radius: 6
                color: "#2563eb"

                Text {
                    anchors.centerIn: parent
                    text: "STATUS READY"
                    font.family: "Liberation Sans"
                    font.pixelSize: 13
                    font.weight: Font.Bold
                    color: "#ffffff"
                }
            }

            Text {
                Layout.fillWidth: true
                text: "Build Pipeline Summary"
                font.family: "Liberation Sans"
                font.pixelSize: 26
                font.weight: Font.Bold
                lineHeight: 32
                lineHeightMode: Text.FixedHeight
                verticalAlignment: Text.AlignVCenter
                color: "#f8fafc"
                wrapMode: Text.Wrap
            }

            Text {
                Layout.fillWidth: true
                text: "Static HTML and CSS boxes map to idiomatic Qt Quick layouts with matching geometry and typography."
                font.family: "Liberation Sans"
                font.pixelSize: 16
                lineHeight: 24
                lineHeightMode: Text.FixedHeight
                verticalAlignment: Text.AlignVCenter
                color: "#cbd5e1"
                wrapMode: Text.Wrap
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 12
                Layout.topMargin: 4
                radius: 6
                color: "#334155"

                Rectangle {
                    width: 320
                    height: 12
                    radius: 6
                    color: "#38bdf8"
                }
            }
        }
    }
}
