import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The head of the Folders screen: the folder's name, where it is, and what
// can be done from it -- play it, open another from the disk, play a CD.
Column {
    id: head

    property FolderModel folder

    readonly property bool atTop: folder.path === ""

    topPadding: Metrics.contentTop
    bottomPadding: Metrics.gap12
    spacing: Metrics.gap8

    Item {
        width: parent.width
        height: Metrics.shelfHeader

        Text {
            anchors.left: parent.left
            anchors.right: actions.left
            anchors.rightMargin: Metrics.gap16
            anchors.verticalCenter: parent.verticalCenter
            text: head.atTop ? I18n.tr("Folders", I18n.rev) : head.folder.path.split("/").pop()
            elide: Text.ElideMiddle
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }

        Row {
            id: actions
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8

            PillButton {
                visible: !head.atTop
                kind: "primary"
                glyph: Icons.playerPlayFilled
                filled: true
                text: I18n.tr("Play folder", I18n.rev)
                onClicked: head.folder.playAll()
            }
            PillButton {
                glyph: Icons.folderPlus
                text: I18n.tr("Open a folder…", I18n.rev)
                onClicked: head.folder.pick()
            }
            PillButton {
                visible: head.folder.hasCd
                glyph: Icons.disc
                text: I18n.tr("Play the CD", I18n.rev)
                onClicked: head.folder.playCd()
            }
        }
    }

    FolderCrumbs {
        visible: !head.atTop
        width: parent.width
        path: head.folder.path
        onOpened: (path) => Navigation.openFolder(path)
    }

    Text {
        visible: !head.folder.inLibrary && !head.atTop
        text: I18n.tr("Not in the library: this is the folder as it is on disk.", I18n.rev)
        font.pointSize: Type.meta
        color: Theme.color.textTertiary
    }

    Column {
        visible: head.atTop && head.folder.folders === 0 && !head.folder.loading
        spacing: Metrics.gap8
        topPadding: Metrics.section

        Text {
            text: I18n.tr("No folders in the library yet.", I18n.rev)
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        PillButton {
            kind: "primary"
            glyph: Icons.folderPlus
            text: I18n.tr("Add a folder", I18n.rev)
            onClicked: Navigation.go("libraryFolders", 0)
        }
    }
}
