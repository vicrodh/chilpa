// Everything that opens music, with its dialogs inside.
//
// Every dialog starts at the last folder opened: hunting for the music from
// home on every startup is the kind of friction that is felt every day.
//
// The dialogs live here and not in the window: that way a layout that wants
// these buttons elsewhere just instantiates this panel, without dragging
// loose dialogs along or depending on the window's ids.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.blitzfc.chilpa

// A Flow and not a Row: at a narrow window the buttons wrap instead of
// running off the edge. A RowLayout with no fillWidth takes its implicit
// width and keeps it, so the last buttons end up outside the window with no
// way to reach them.
//
// The height follows the rows it ends up with, so whoever places it does not
// have to guess how many there are.
Flow {
    id: root
    spacing: 8
    // A Flow wraps against its width, so it has to be given one; the height
    // then follows the rows it ends up with, and whoever places it does not
    // have to guess how many there are.
    Layout.fillWidth: true
    Layout.preferredHeight: implicitHeight

    // The dialogs ask for a URL, not a path.
    readonly property url from_:
        Chilpa.lastFolder === "" ? "" : "file://" + Chilpa.lastFolder

    Component.onCompleted: Chilpa.refreshCdDrives()

    Shortcut {
        sequence: "Ctrl+O"
        onActivated: fileDialog.open()
    }

    FileDialog {
        id: fileDialog
        currentFolder: root.from_
        title: Chilpa.translate("Open an audio file", Chilpa.trRev)
        nameFilters: [
            "Audio (*.flac *.wav *.m4a *.mp3 *.ogg *.oga *.aiff *.aif *.dsf *.dff)",
            Chilpa.translate("SACD images", Chilpa.trRev) + " (*.iso *.dat)",
            Chilpa.translate("Everything", Chilpa.trRev) + " (*)"
        ]
        onAccepted: Chilpa.play(selectedFile)
    }

    FolderDialog {
        id: folderDialog
        currentFolder: root.from_
        title: Chilpa.translate("Open a folder", Chilpa.trRev)
        onAccepted: Chilpa.playFolder(selectedFolder)
    }

    FileDialog {
        id: playlistDialog
        currentFolder: root.from_
        title: Chilpa.translate("Open a playlist", Chilpa.trRev)
        nameFilters: [Chilpa.translate("Playlists", Chilpa.trRev) + " (*.m3u *.m3u8)"]
        onAccepted: Chilpa.playPlaylist(selectedFile)
    }

    FileDialog {
        id: saveDialog
        currentFolder: root.from_
        title: Chilpa.translate("Save the queue", Chilpa.trRev)
        fileMode: FileDialog.SaveFile
        defaultSuffix: "m3u"
        nameFilters: [Chilpa.translate("Playlists", Chilpa.trRev) + " (*.m3u)"]
        onAccepted: Chilpa.saveQueue(selectedFile)
    }

    FolderDialog {
        id: scanDialog
        currentFolder: root.from_
        title: Chilpa.translate("Scan a folder into the library", Chilpa.trRev)
        onAccepted: Chilpa.scanFolder(selectedFolder)
    }

    // Opening a radio: an address and that is it.
    Dialog {
        id: streamDialog
        title: Chilpa.translate("Open a stream", Chilpa.trRev)
        anchors.centerIn: Overlay.overlay
        width: 420
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: if (address.text !== "") Chilpa.playUrl(address.text)

        ColumnLayout {
            anchors.fill: parent
            TextField {
                id: address
                Layout.fillWidth: true
                placeholderText: "http://..."
            }
        }
    }

    Button {
        text: Chilpa.translate("Open file", Chilpa.trRev)
        onClicked: fileDialog.open()
    }
    Button {
        text: Chilpa.translate("Open folder", Chilpa.trRev)
        onClicked: folderDialog.open()
    }
    Button {
        text: Chilpa.translate("Open playlist", Chilpa.trRev)
        onClicked: playlistDialog.open()
    }
    Button {
        text: Chilpa.translate("Save queue", Chilpa.trRev)
        enabled: Chilpa.queue.length > 0
        onClicked: saveDialog.open()
    }
    Button {
        text: Chilpa.translate("Clear queue", Chilpa.trRev)
        enabled: Chilpa.queue.length > 0
        onClicked: Chilpa.clearQueue()
    }
    Button {
        text: Chilpa.translate("Open stream", Chilpa.trRev)
        onClicked: streamDialog.open()
    }
    // It only appears if there is a drive: a button that will never be
    // any use gets in the way more than it helps.
    Button {
        text: Chilpa.translate("Play CD", Chilpa.trRev)
        visible: Chilpa.cdDrives.length > 0
        onClicked: Chilpa.playCd(Chilpa.cdDrives[0])
    }
    Button {
        text: Chilpa.translate("Scan into the library", Chilpa.trRev)
        onClicked: scanDialog.open()
    }
    Button {
        text: Chilpa.translate("Rescan", Chilpa.trRev)
        enabled: !Chilpa.scanning
        onClicked: Chilpa.rescan()
    }
}
