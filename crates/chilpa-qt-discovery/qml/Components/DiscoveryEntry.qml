import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Button {
    id: card
    property var entry: ({})
    property string detail: ""
    signal openAlbum(int album)
    readonly property bool recording: entry.kind === "recording"
    implicitHeight: Metrics.discoveryEntryHeight
    padding: Metrics.gap16
    hoverEnabled: true
    Accessible.name: entry.name || ""
    Accessible.description: detail
    background: Rectangle {
        radius: Metrics.radiusCard
        color: card.down ? Theme.color.stateSelected : card.hovered ? Theme.color.stateHover : Theme.color.surfacePanel
        Edge {}
        FocusRing { visible: card.activeFocus }
    }
    contentItem: Item {
        Icon {
            id: icon
            anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
            glyph: card.entry.kind === "genre" ? Icons.hierarchy2 : card.entry.kind === "composer" ? Icons.pencil : card.recording ? Icons.disc : Icons.music
            size: Metrics.iconMd; color: Theme.color.accent
        }
        Column {
            anchors.left: icon.right; anchors.leftMargin: Metrics.gap12
            anchors.right: arrow.left; anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap4
            Text { width: parent.width; text: card.entry.name || ""; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary }
            Text { width: parent.width; text: card.detail; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        }
        Icon { id: arrow; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; glyph: Icons.chevronRight; size: Metrics.iconSm; color: Theme.color.textTertiary }
    }
}
