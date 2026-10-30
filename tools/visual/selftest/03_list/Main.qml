import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

ApplicationWindow {
    width: 1280
    height: 800
    visible: true
    title: "List Layout"
    color: "#f1f5f9"

    Rectangle {
        x: 64
        y: 48
        width: 680
        implicitHeight: panelCol.implicitHeight + 60
        height: implicitHeight
        color: "#ffffff"
        border.width: 2
        border.color: "#cbd5e1"
        radius: 12

        ColumnLayout {
            id: panelCol
            anchors.fill: parent
            anchors.margins: 30
            spacing: 20

            Text {
                Layout.fillWidth: true
                text: "Conversion Pipeline Steps"
                font.family: "Liberation Sans"
                font.pixelSize: 24
                font.weight: Font.Bold
                lineHeight: 30
                lineHeightMode: Text.FixedHeight
                verticalAlignment: Text.AlignVCenter
                color: "#0f172a"
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 12

                Repeater {
                    model: ListModel {
                        ListElement { dotColor: "#2563eb"; itemText: "Parse HTML5 document into arena DOM" }
                        ListElement { dotColor: "#0284c7"; itemText: "Resolve CSS cascade and computed values" }
                        ListElement { dotColor: "#10b981"; itemText: "Build block and inline formatting boxes" }
                        ListElement { dotColor: "#6366f1"; itemText: "Emit idiomatic Qt Quick 6.11 QML" }
                    }

                    delegate: Rectangle {
                        id: rowBox
                        required property string dotColor
                        required property string itemText

                        Layout.fillWidth: true
                        Layout.preferredHeight: 56
                        color: "#f8fafc"
                        border.width: 1
                        border.color: "#cbd5e1"
                        radius: 8

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 19
                            anchors.rightMargin: 19
                            spacing: 14

                            Rectangle {
                                Layout.preferredWidth: 12
                                Layout.preferredHeight: 12
                                radius: 6
                                color: rowBox.dotColor
                            }

                            Text {
                                Layout.fillWidth: true
                                text: rowBox.itemText
                                font.family: "Liberation Sans"
                                font.pixelSize: 16
                                color: "#1e293b"
                            }
                        }
                    }
                }
            }
        }
    }
}
