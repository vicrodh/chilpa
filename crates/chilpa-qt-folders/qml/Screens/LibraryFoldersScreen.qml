import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.folders
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "../Components"
import "../Patterns"

// Settings > Library > Library folders: add a folder by browsing or by its
// path, see when each was scanned and whether it is there, scan again or
// remove. A scan runs in the background and its progress shows here.
Item {
    id: page

    readonly property real contentWidth: Math.min(Metrics.settingsMaxWidth + Metrics.settingsNav, width - Metrics.contentX * 2) // derived: padding on both sides
    readonly property bool canStart: !LibraryFolders.scanning && !LibraryFolders.canResume

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

    PillButton {
        anchors.right: add.right
        anchors.verticalCenter: title.verticalCenter
        objectName: "checkAllLibraryFolders"
        text: I18n.tr("Check all folders", I18n.rev)
        glyph: Lucide.rotateCcw
        iconFamily: Lucide.font
        enabled: LibraryFolders.count > 0 && page.canStart
        onClicked: LibraryFolders.rescanAll()
    }

    Item {
        id: add
        enabled: page.canStart
        x: Metrics.contentX
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section
        width: page.contentWidth
        height: Math.max(Metrics.settingsRow, explanation.implicitHeight)

        Column {
            id: explanation
            anchors.left: parent.left
            anchors.right: path.left
            anchors.rightMargin: Metrics.gap16
            anchors.verticalCenter: parent.verticalCenter
            Text { width: parent.width; text: I18n.tr("Add a folder", I18n.rev); wrapMode: Text.WordWrap; font.pointSize: Type.bodyLarge; font.weight: Type.medium; color: Theme.color.textPrimary }
            Text { width: parent.width; text: I18n.tr("Browse for a music folder, or type its full path.", I18n.rev); wrapMode: Text.WordWrap; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        }

        SearchField {
            id: path
            anchors.right: browse.left
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.selectWidth
            placeholder: I18n.tr("Folder path", I18n.rev)
            onAccepted: page.addTyped()
        }
        PillButton {
            id: browse
            anchors.right: confirm.left
            anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.folder
            text: I18n.tr("Browse…", I18n.rev)
            onClicked: LibraryFolders.browse()
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
        anchors.top: hint.bottom
        anchors.topMargin: Metrics.gap12
        width: page.contentWidth
    }

    Text {
        id: hint
        x: Metrics.contentX
        anchors.top: add.bottom
        anchors.topMargin: Metrics.gap12
        width: page.contentWidth
        text: I18n.tr("Scheduled checks run while Chilpa is open. Startup checks run once when the app opens. Unchanged files are skipped.", I18n.rev)
            + " " + I18n.tr("Closing stops a scan. Saved progress can be continued here after reopening.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
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
        ScrollBar.vertical: ViewportScrollBar { gutter: (page.width - Metrics.contentX - page.contentWidth) / 2 } // derived: the middle of the room to its right

        delegate: FolderRow {
            width: ListView.view.width
            onRescan: LibraryFolders.rescan(path)
            onForget: LibraryFolders.forget(path)
            onPolicyChosen: policy => LibraryFolders.choosePolicy(path, policy)
        }
    }

    function addTyped() {
        if (path.text.trim() === "")
            return
        LibraryFolders.add(path.text)
        path.text = ""
    }
}
