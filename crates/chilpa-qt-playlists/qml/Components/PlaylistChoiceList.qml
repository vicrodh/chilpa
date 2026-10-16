import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The list the picker's search opens, as QBZ's: "+ Create new playlist"
// always first, the playlist last added to pinned under it, then the
// playlists the search leaves, at most eight, and how many more there are.
Rectangle {
    id: list
    objectName: "playlistTargets"

    property var rows: []
    property var lastUsed: null
    property bool searching: false
    readonly property int shown: Math.min(rows.length, Metrics.playlistChoiceRows)
    readonly property int more: Math.max(0, rows.length - Metrics.playlistChoiceRows)
    readonly property int count: shown
    signal createAsked()
    signal chosen(var target)

    function itemAtIndex(index) { return repeater.itemAt(index) }

    height: Math.min(inner.implicitHeight + Metrics.hairline * 2, Metrics.playlistChoiceList) // derived: the rows and the border
    radius: Metrics.radiusControl
    color: Theme.color.surfaceRaised
    border.width: Metrics.hairline
    border.color: Theme.color.strokeHairline
    clip: true

    // A click on the list's own chrome stays in the list.
    // no cursor: it only keeps the click from the picker under the list
    MouseArea { anchors.fill: parent }

    Flickable {
        anchors.fill: parent
        anchors.margins: Metrics.hairline
        contentWidth: width
        contentHeight: inner.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: inner
            width: parent.width

            Rectangle {
                objectName: "createPlaylistRow"
                width: parent.width
                height: Metrics.playlistChoiceRow
                color: createHover.hovered ? Theme.color.stateHover : "transparent"
                Text {
                    anchors.fill: parent
                    anchors.leftMargin: Metrics.gap12
                    anchors.rightMargin: Metrics.gap12
                    verticalAlignment: Text.AlignVCenter
                    text: I18n.tr("+ Create new playlist", I18n.rev)
                    elide: Text.ElideRight
                    font.pointSize: Type.body
                    font.weight: Type.medium
                    color: Theme.color.accent
                }
                HoverHandler { id: createHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: list.createAsked() }
            }
            Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }

            PlaylistTargetRow {
                visible: list.lastUsed !== null
                width: parent.width
                height: visible ? Metrics.playlistChoiceRow : 0
                target: list.lastUsed || ({ playlist: { name: "" } })
                pinned: true
                caption: I18n.tr("Last playlist", I18n.rev)
                onChosen: list.chosen(list.lastUsed)
            }
            Rectangle { visible: list.lastUsed !== null; width: parent.width; height: visible ? Metrics.hairline : 0; color: Theme.color.strokeHairline }

            Repeater {
                id: repeater
                model: list.rows.slice(0, Metrics.playlistChoiceRows)
                PlaylistTargetRow {
                    required property var modelData
                    width: inner.width
                    target: modelData
                    onChosen: list.chosen(modelData)
                }
            }

            Text {
                visible: list.rows.length === 0 && list.searching
                width: parent.width
                height: visible ? Metrics.playlistChoiceRow : 0
                text: I18n.tr("No playlists found", I18n.rev)
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            Rectangle { visible: list.more > 0; width: parent.width; height: visible ? Metrics.hairline : 0; color: Theme.color.strokeHairline }
            Text {
                visible: list.more > 0
                width: parent.width
                height: visible ? Metrics.rowCompact : 0
                text: I18n.trCount("{} more playlist...", "{} more playlists...", list.more, I18n.rev)
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                font.pointSize: Type.small
                font.italic: true
                color: Theme.color.textTertiary
            }
        }
    }
}
