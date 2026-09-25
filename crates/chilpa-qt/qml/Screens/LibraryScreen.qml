import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// Library (design/02 9, design/06): the four filter columns in cascade and
// the tracks they leave. A fixed layout, with no scroll of its own: only the
// columns and the list scroll.
Item {
    id: page

    TrackListModel {
        id: tracks
        kind: "library"
        filterRev: Library.rev
        playingPath: Player.path
    }

    Item {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        height: Metrics.shelfHeader

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Library", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Text {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: Library.summary
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }

    LibraryCrumbs {
        id: crumbs
        anchors.top: head.bottom
        x: head.x
        width: head.width
        height: Metrics.breadcrumb
    }

    Rectangle {
        id: panes
        anchors.top: crumbs.bottom
        x: head.x
        width: head.width
        height: Metrics.panesHeight
        radius: Metrics.radiusMenu
        color: Theme.color.strokeHairline
        border.color: Theme.color.strokeHairline
        clip: true

        Row {
            anchors.fill: parent
            anchors.margins: 1
            spacing: 1

            Repeater {
                model: [
                    { column: "year", title: I18n.tr("Year", I18n.rev) },
                    { column: "genre", title: I18n.tr("Genre", I18n.rev) },
                    { column: "artist", title: I18n.tr("Artist", I18n.rev) },
                    { column: "album", title: I18n.tr("Album", I18n.rev) }
                ]

                PaneColumn {
                    required property int index
                    required property var modelData
                    width: (panes.width - Metrics.paneSeams) * Metrics.paneShares[index] / Metrics.paneShareTotal
                    height: parent.height
                    column: modelData.column
                    title: modelData.title
                }
            }
        }
    }

    Row {
        id: toolbar
        anchors.top: panes.bottom
        anchors.topMargin: Metrics.gap12
        x: head.x
        spacing: Metrics.gap8

        PillButton {
            kind: "primary"
            glyph: Icons.playerPlayFilled
            filled: true
            text: I18n.tr("Play", I18n.rev)
            onClicked: tracks.playAll(false)
        }
        PillButton {
            glyph: Icons.arrowsShuffle
            text: I18n.tr("Shuffle", I18n.rev)
            onClicked: tracks.playAll(true)
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: Metrics.gap8
            text: tracks.summary
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }

    TrackList {
        anchors.top: toolbar.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width
        bottomInset: Metrics.contentBottom
        model: tracks
    }
}
