// Near-miss layout pair: preserves overall structure, colours, and text, but
// simulates subtle converter defects (8px uncollapsed body offset on topbar and
// columns plus smaller 13px/17px font sizes) so --selftest verifies that the
// composite score ranks subtle layout regressions strictly between matching
// pairs (> 85) and a grossly wrong layout.
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts

ApplicationWindow {
    width: 1280
    height: 800
    visible: true
    title: "Near Miss Reference"
    color: "#f8fafc"

    Rectangle {
        x: 8
        y: 8
        width: parent.width - 16
        height: 64
        color: "#0f172a"

        Text {
            anchors.left: parent.left
            anchors.leftMargin: 48
            anchors.verticalCenter: parent.verticalCenter
            text: "Two-Column Workspace"
            font.family: "Liberation Sans"
            font.pixelSize: 17
            font.weight: Font.Bold
            color: "#f8fafc"
        }
    }

    RowLayout {
        x: 56
        y: 108
        width: parent.width - 96
        spacing: 24

        Rectangle {
            Layout.preferredWidth: 360
            Layout.preferredHeight: 420
            Layout.alignment: Qt.AlignTop
            color: "#ffffff"
            border.width: 2
            border.color: "#cbd5e1"
            radius: 12

            ColumnLayout {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 26
                spacing: 20

                Text {
                    Layout.fillWidth: true
                    text: "Navigation"
                    font.family: "Liberation Sans"
                    font.pixelSize: 17
                    font.weight: Font.Bold
                    color: "#0f172a"
                }

                Rectangle {
                    Layout.preferredWidth: 64
                    Layout.preferredHeight: 4
                    radius: 2
                    color: "#2563eb"
                }

                Text {
                    Layout.fillWidth: true
                    text: "Sidebar column sized to a fixed track width beside a flexible main panel."
                    font.family: "Liberation Sans"
                    font.pixelSize: 13
                    color: "#475569"
                    wrapMode: Text.Wrap
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 76
                    radius: 8
                    color: "#eff6ff"
                    border.width: 1
                    border.color: "#bfdbfe"

                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 17
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Overview Metrics"
                        font.family: "Liberation Sans"
                        font.pixelSize: 13
                        font.weight: Font.Bold
                        color: "#1e3a8a"
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 76
                    radius: 8
                    color: "#f1f5f9"
                    border.width: 1
                    border.color: "#cbd5e1"

                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: 17
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Component Tree"
                        font.family: "Liberation Sans"
                        font.pixelSize: 13
                        font.weight: Font.Bold
                        color: "#334155"
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 420
            Layout.alignment: Qt.AlignTop
            color: "#ffffff"
            border.width: 2
            border.color: "#cbd5e1"
            radius: 12

            ColumnLayout {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 26
                spacing: 20

                Text {
                    Layout.fillWidth: true
                    text: "Primary Content Panel"
                    font.family: "Liberation Sans"
                    font.pixelSize: 17
                    font.weight: Font.Bold
                    color: "#0f172a"
                }

                Text {
                    Layout.fillWidth: true
                    text: "Flexible main column expands to fill the remaining horizontal viewport space beside the sidebar."
                    font.family: "Liberation Sans"
                    font.pixelSize: 13
                    color: "#475569"
                    wrapMode: Text.Wrap
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 184
                    Layout.topMargin: 8
                    radius: 10
                    color: "#ecfdf5"
                    border.width: 2
                    border.color: "#10b981"

                    ColumnLayout {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 24
                        spacing: 12

                        Text {
                            Layout.fillWidth: true
                            text: "Layout Parity Check"
                            font.family: "Liberation Sans"
                            font.pixelSize: 15
                            font.weight: Font.Bold
                            color: "#065f46"
                        }

                        Text {
                            Layout.fillWidth: true
                            text: "RowLayout stretch factors and spacing reproduce CSS flexbox two-column geometry across the viewport."
                            font.family: "Liberation Sans"
                            font.pixelSize: 13
                            color: "#047857"
                            wrapMode: Text.Wrap
                        }
                    }
                }
            }
        }
    }
}
