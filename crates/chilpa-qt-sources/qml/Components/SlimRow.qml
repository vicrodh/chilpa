import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// One row of the Home's compact grid, as QBZ's (TrackSlimRow and SlimCard):
// a small cover, the title and who it is by, and a hover that fills the row.
Rectangle {
    id: row
    property url art
    property string title
    property string subtitle
    property bool playing: false
    property bool available: true
    // A small count at the row's end, as Most played's plays; empty, none.
    property string count: ""
    signal activated()
    signal menuAsked(real x, real y)

    radius: Metrics.radiusCard
    color: hover.hovered && available ? Theme.color.stateHover : "transparent"
    opacity: available ? 1 : Metrics.disabledOpacity
    Accessible.role: Accessible.Button
    Accessible.name: title

    ArtTile {
        id: cover
        x: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.slimArt; height: width
        radius: Metrics.radiusRow
        source: row.art
    }
    Column {
        anchors.left: cover.right; anchors.leftMargin: Metrics.gap12
        anchors.right: plays.left; anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2
        Text {
            width: parent.width
            text: row.title; textFormat: Text.PlainText; elide: Text.ElideRight
            font.pointSize: Type.slimTitle; font.weight: Type.medium
            color: row.playing ? Theme.color.accent : Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: row.subtitle; textFormat: Text.PlainText; elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
    }
    Row {
        id: plays
        objectName: "slimRowCount"
        visible: row.count !== ""
        width: visible ? implicitWidth : 0
        anchors.right: parent.right; anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4
        Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.playerPlay; size: Metrics.iconXs; color: Theme.color.textTertiary }
        Text { anchors.verticalCenter: parent.verticalCenter; text: row.count; font.pointSize: Type.small; color: Theme.color.textTertiary }
    }
    HoverHandler { id: hover; cursorShape: row.available ? Qt.PointingHandCursor : Qt.ArrowCursor }
    TapHandler {
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onTapped: (point, button) => {
            if (!row.available) return
            if (button === Qt.RightButton) row.menuAsked(point.position.x, point.position.y)
            else row.activated()
        }
    }
}
