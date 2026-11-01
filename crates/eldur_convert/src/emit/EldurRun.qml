// One inline run of an EldurLines. What every run has in common is written once:
// Qt compiles an enum on each object of a document at a cost that grows with how
// many it has, and a page has many runs.
import QtQuick

Text {
    textFormat: Text.PlainText
    wrapMode: Text.WordWrap
    renderType: Text.NativeRendering
}
