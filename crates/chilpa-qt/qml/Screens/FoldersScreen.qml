import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// A virtual root and lazy tree on the left; the selected directory's child
// folders, records or loose tracks on the right.
Item {
    id: page

    FolderModel {
        id: current
        path: Navigation.routeKey
        content: true
    }
    TrackListModel {
        id: tracks
        kind: "folder"
        filterRev: current.rev
        playingPath: Player.path
    }
    AlbumGridModel {
        id: albums
        kind: "folder"
        contentRev: current.rev
        columns: Math.max(1, Math.floor((body.width - Metrics.folderTreeWidth + Metrics.gridGap) / Metrics.cardMinWidth))
    }

    Item {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        height: Metrics.shelfHeader

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Folders", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8

            PillButton {
                visible: current.path !== ""
                kind: "primary"
                glyph: Icons.playerPlayFilled
                filled: true
                text: I18n.tr("Play folder", I18n.rev)
                onClicked: current.playAll()
            }
            PillButton {
                glyph: Icons.folderPlus
                text: I18n.tr("Open a folder…", I18n.rev)
                onClicked: current.pick()
            }
            IconButton {
                visible: current.hasCd
                tip: I18n.tr("Play the CD", I18n.rev)
                glyph: Icons.disc
                round: true
                onClicked: current.playCd()
            }
        }
    }

    Item {
        id: body
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width

        FolderTree {
            id: tree
            width: Math.min(Metrics.folderTreeWidth, body.width / 3) // derived: keep two thirds for content
            height: parent.height - Metrics.contentBottom
            activePath: current.path
        }

        FolderContent {
            anchors.left: tree.right
            anchors.leftMargin: Metrics.gridGap
            anchors.right: parent.right
            height: parent.height
            folder: current
            albums: albums
            tracks: tracks
        }
    }
}
