import QtQuick
import "../Theme"

// What a row of Settings says: its name and, under it, what it does. A line
// of a report under a setting has a smaller name in plain weight.
Column {
    id: said

    property string title
    property string description
    property bool line: false
    // The narrowest it can be without breaking a word in two, and how wide
    // it runs with each text on one line: what its owner weighs against a
    // control beside it.
    readonly property real least: Math.max(titleWords.contentWidth, descriptionWords.contentWidth)
    readonly property real natural: Math.max(name.implicitWidth, detail.implicitWidth)

    spacing: Metrics.gap2

    Text {
        id: name
        width: parent.width
        text: said.title
        wrapMode: Text.Wrap
        font.pointSize: said.line ? Type.row : Type.bodyLarge
        font.weight: said.line ? Type.regular : Type.medium
        color: Theme.color.textPrimary
    }
    Text {
        id: detail
        width: parent.width
        visible: said.description !== ""
        text: said.description
        wrapMode: Text.Wrap
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }
    // Each text again, a word to a line: as wide as its longest word.
    Text { id: titleWords; visible: false; width: 1; wrapMode: Text.WordWrap; text: said.title; font: name.font }
    Text { id: descriptionWords; visible: false; width: 1; wrapMode: Text.WordWrap; text: said.description; font: detail.font }
}
