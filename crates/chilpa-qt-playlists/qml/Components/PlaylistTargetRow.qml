import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// One playlist in the picker's list, as QBZ's: what marks it -- Chilpa's own
// mark on ours, the source's on a source's -- its name, and how many tracks
// it has. A click chooses it; the footer's button adds.
Rectangle {
    id: row
    required property var target
    // Said in place of the count: "Last playlist" on the one pinned above.
    property string caption: ""
    property bool pinned: false
    signal chosen()

    height: Metrics.playlistChoiceRow
    color: hover.hovered ? Theme.color.stateHover : "transparent"

    Icon {
        id: clock
        visible: row.pinned
        x: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.history
        size: Metrics.iconSm
        color: Theme.color.textTertiary
    }
    SourceIcon {
        id: mark
        visible: !row.pinned
        x: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        sourceInfo: ({ icon: row.target.icon || "" })
        color: Theme.color.textSecondary
    }
    Text {
        anchors.left: mark.right
        anchors.leftMargin: Metrics.gap8
        anchors.right: count.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        text: row.target.playlist.name
        elide: Text.ElideRight
        font.pointSize: Type.body
        color: Theme.color.textPrimary
    }
    Text {
        id: count
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        text: row.caption !== "" ? row.caption : I18n.trCount("{} track", "{} tracks", row.target.playlist.track_count || 0, I18n.rev)
        font.pointSize: Type.small
        font.italic: row.caption !== ""
        color: Theme.color.textTertiary
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: row.chosen() }
}
