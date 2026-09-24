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

// The panel takes up what its buttons take up. Whoever places it decides
// whether there is space to spare beside it; a filler in here would eat the
// bar of a layout that puts something else on the same line.
RowLayout {
    id: root
    spacing: 8

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
        title: "Abrir un archivo de audio"
        nameFilters: [
            "Audio (*.flac *.wav *.m4a *.mp3 *.ogg *.oga *.aiff *.aif *.dsf *.dff)",
            "Imágenes de SACD (*.iso *.dat)",
            "Todo (*)"
        ]
        onAccepted: Chilpa.play(selectedFile)
    }

    FolderDialog {
        id: folderDialog
        currentFolder: root.from_
        title: "Abrir una carpeta"
        onAccepted: Chilpa.playFolder(selectedFolder)
    }

    FileDialog {
        id: playlistDialog
        currentFolder: root.from_
        title: "Abrir una lista"
        nameFilters: ["Listas (*.m3u *.m3u8)"]
        onAccepted: Chilpa.playPlaylist(selectedFile)
    }

    FileDialog {
        id: saveDialog
        currentFolder: root.from_
        title: "Guardar la cola"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "m3u"
        nameFilters: ["Listas (*.m3u)"]
        onAccepted: Chilpa.saveQueue(selectedFile)
    }

    FolderDialog {
        id: scanDialog
        currentFolder: root.from_
        title: "Escanear una carpeta a la biblioteca"
        onAccepted: Chilpa.scanFolder(selectedFolder)
    }

    // Opening a radio: an address and that is it.
    Dialog {
        id: streamDialog
        title: "Abrir un stream"
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
        text: "Abrir archivo"
        onClicked: fileDialog.open()
    }
    Button {
        text: "Abrir carpeta"
        onClicked: folderDialog.open()
    }
    Button {
        text: "Abrir lista"
        onClicked: playlistDialog.open()
    }
    Button {
        text: "Guardar cola"
        enabled: Chilpa.queue.length > 0
        onClicked: saveDialog.open()
    }
    Button {
        text: "Vaciar cola"
        enabled: Chilpa.queue.length > 0
        onClicked: Chilpa.clearQueue()
    }
    Button {
        text: "Abrir stream"
        onClicked: streamDialog.open()
    }
    // It only appears if there is a drive: a button that will never be
    // any use gets in the way more than it helps.
    Button {
        text: "Reproducir CD"
        visible: Chilpa.cdDrives.length > 0
        onClicked: Chilpa.playCd(Chilpa.cdDrives[0])
    }
    Button {
        text: "Escanear a la biblioteca"
        onClicked: scanDialog.open()
    }
    Button {
        text: "Volver a escanear"
        enabled: !Chilpa.scanning
        onClicked: Chilpa.rescan()
    }
}
