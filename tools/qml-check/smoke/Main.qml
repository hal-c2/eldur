// A page that passes qml:check, for tasks:smoke: it tests the task, not the
// converter. The converter's output is checked by `mise run corpus`, then
// `mise run qml:check`.
import QtQuick

Rectangle {
    width: 200
    height: 100
    color: "white"

    Text {
        anchors.centerIn: parent
        text: "qml:check"
    }
}
