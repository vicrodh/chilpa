import QtQuick
import "../Theme"

// What a row of Settings says: its name and, under it, what it does. A line
// of a report under a setting has a smaller name in plain weight.
Column {
    id: said

    property string title
    property string description
    property bool line: false

    spacing: Metrics.gap2

    Text {
        width: parent.width
        text: said.title
        wrapMode: Text.Wrap
        font.pointSize: said.line ? Type.row : Type.bodyLarge
        font.weight: said.line ? Type.regular : Type.medium
        color: Theme.color.textPrimary
    }
    Text {
        width: parent.width
        visible: said.description !== ""
        text: said.description
        wrapMode: Text.Wrap
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }
}
