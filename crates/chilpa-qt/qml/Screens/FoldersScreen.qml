import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// Settings > Library > Library folders: add a folder by browsing or by its
// path, see when each was scanned and whether it is there, scan again or
// remove. A scan runs in the background and its progress shows here.
Item {
    id: page

    readonly property real contentWidth: Math.min(Metrics.settingsMaxWidth + Metrics.settingsNav, width - Metrics.contentX * 2) // derived: padding on both sides

    Text {
        id: title
        x: Metrics.contentX
        y: Metrics.contentTop
        text: I18n.tr("Library folders", I18n.rev)
        font.pointSize: Type.title
        font.weight: Type.bold
        font.letterSpacing: Type.titleTracking
        color: Theme.color.textPrimary
    }

    IconButton {
        anchors.right: add.right
        anchors.verticalCenter: title.verticalCenter
        tip: I18n.tr("Scan every folder again", I18n.rev)
        glyph: Icons.refresh
        enabled: LibraryFolders.count > 0 && !LibraryFolders.scanning
        onClicked: LibraryFolders.rescanAll()
    }

    Item {
        id: add
        x: Metrics.contentX
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section
        width: page.contentWidth
        height: Metrics.settingsRow

        Column {
            anchors.left: parent.left
            anchors.right: path.left
            anchors.rightMargin: Metrics.gap16
            anchors.verticalCenter: parent.verticalCenter
            Text { text: I18n.tr("Add a folder", I18n.rev); font.pointSize: Type.bodyLarge; font.weight: Type.medium; color: Theme.color.textPrimary }
            Text { text: I18n.tr("Browse for a music folder, or type its full path.", I18n.rev); font.pointSize: Type.meta; color: Theme.color.textSecondary }
        }

        SearchField {
            id: path
            anchors.right: browse.left
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.selectWidth
            placeholder: "/home/you/Music"
            onAccepted: page.addTyped()
        }
        PillButton {
            id: browse
            anchors.right: confirm.left
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.folder
            text: I18n.tr("Browse…", I18n.rev)
            onClicked: dialog.open()
        }
        PillButton {
            id: confirm
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            kind: "primary"
            glyph: Icons.folderPlus
            text: I18n.tr("Add", I18n.rev)
            onClicked: page.addTyped()
        }
    }

    ScanProgress {
        id: progress
        x: Metrics.contentX
        anchors.top: add.bottom
        anchors.topMargin: Metrics.gap12
        width: page.contentWidth
    }

    ListView {
        x: Metrics.contentX
        anchors.top: progress.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        width: page.contentWidth
        clip: true
        model: FolderListModel { rev: LibraryFolders.rev }
        bottomMargin: Metrics.contentBottom
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        delegate: FolderRow {
            width: ListView.view.width
            onRescan: LibraryFolders.rescan(path)
            onForget: LibraryFolders.forget(path)
        }
    }

    FolderDialog {
        id: dialog
        title: I18n.tr("Add a folder to the library", I18n.rev)
        onAccepted: LibraryFolders.add(selectedFolder.toString())
    }

    function addTyped() {
        if (path.text.trim() === "")
            return
        LibraryFolders.add(path.text)
        path.text = ""
    }
}
