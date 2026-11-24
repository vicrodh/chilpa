import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// One place an import can write to: whether it is chosen, its mark, what it
// is and what it keeps. The whole row chooses it, with the circle a
// selected media row shows.
Item {
    id: row
    property string title
    property string detail
    property var sourceInfo: ({})
    property bool checked: false
    signal toggled()
    implicitHeight: Math.max(Metrics.playlistRowHeight, words.implicitHeight + Metrics.gap8 * 2) // derived: padding above and below
    activeFocusOnTab: enabled
    opacity: enabled ? 1 : Metrics.disabledOpacity
    Accessible.role: Accessible.CheckBox
    Accessible.name: title
    Accessible.checked: checked
    Keys.onSpacePressed: row.toggled()
    ListRowSurface { anchors.fill: parent; listMode: false; radius: Metrics.radiusRow; hovered: area.containsMouse; selected: row.checked }
    Icon {
        id: check
        x: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        glyph: row.checked ? Icons.circleCheckFilled : Icons.circle
        filled: row.checked
        size: Metrics.iconMd
        color: row.checked || row.activeFocus ? Theme.color.accent : Theme.color.textSecondary
    }
    SourceIcon {
        id: mark
        x: check.x + check.width + Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.iconMd; height: width
        sourceInfo: row.sourceInfo
    }
    Column {
        id: words
        anchors.left: mark.right; anchors.leftMargin: Metrics.gap10
        anchors.right: parent.right; anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2
        Text { width: parent.width; text: row.title; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textPrimary }
        Text { width: parent.width; visible: text !== ""; text: row.detail; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.small; color: Theme.color.textSecondary }
    }
    MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; enabled: row.enabled; onClicked: row.toggled() }
}
