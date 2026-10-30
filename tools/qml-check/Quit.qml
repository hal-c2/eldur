// Loaded by `mise run qml:check` next to each Main.qml. Quitting through the
// engine, rather than killing qml6, lets warnings at engine destruction
// (such as items still incubating) reach stderr.
import QtQml

Timer {
    interval: 1000
    running: true
    onTriggered: Qt.quit()
}
