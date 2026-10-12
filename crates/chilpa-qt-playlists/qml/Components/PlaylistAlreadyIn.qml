import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The picker's second column, as QBZ's "Already in": the playlists that
// already hold the tracks being added, and "2 of 5" when they hold some of
// them. Choosing one is how the missing ones go in. A source's playlist is
// not read for this: only ours say what they hold.
Column {
    id: column
    objectName: "playlistAlreadyIn"

    property var rows: []
    property int trackCount: 0
    signal chosen(var target)

    spacing: Metrics.gap8

    Text {
        text: I18n.tr("Already in", I18n.rev)
        font.pointSize: Type.small
        font.weight: Type.medium
        color: Theme.color.textSecondary
    }
    Repeater {
        model: column.rows
        Rectangle {
            id: entry
            required property var modelData
            width: column.width
            height: Metrics.rowCompact
            radius: Metrics.radiusItem
            color: hover.hovered ? Theme.color.stateHover : "transparent"
            SourceIcon {
                id: mark
                x: Metrics.gap6
                anchors.verticalCenter: parent.verticalCenter
                sourceInfo: ({ icon: entry.modelData.icon || "" })
                color: Theme.color.textSecondary
            }
            Text {
                anchors.left: mark.right
                anchors.leftMargin: Metrics.gap6
                anchors.right: part.left
                anchors.rightMargin: Metrics.gap6
                anchors.verticalCenter: parent.verticalCenter
                text: entry.modelData.playlist.name
                elide: Text.ElideRight
                font.pointSize: Type.small
                color: Theme.color.textPrimary
            }
            Text {
                id: part
                // All of them, or the one track, is said by being listed.
                visible: column.trackCount > 1 && entry.modelData.exact < column.trackCount
                anchors.right: parent.right
                anchors.rightMargin: Metrics.gap6
                anchors.verticalCenter: parent.verticalCenter
                text: visible ? I18n.tr("{} of {}", I18n.rev).replace("{}", entry.modelData.exact).replace("{}", column.trackCount) : ""
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: column.chosen(entry.modelData) }
        }
    }
}
