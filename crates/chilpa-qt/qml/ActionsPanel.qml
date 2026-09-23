// Todo lo que abre música, con sus diálogos adentro.
//
// Cada diálogo empieza en la última carpeta que se abrió: buscar la música
// desde el home en cada arranque es el tipo de roce que se nota todos los
// días.
//
// Los diálogos viven aquí y no en la ventana: así un layout que quiera estos
// botones en otro lado solo instancia este panel, sin arrastrar diálogos
// sueltos ni depender de ids de la ventana.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.blitzfc.chilpa

// El panel ocupa lo que ocupan sus botones. Quien lo coloca decide si le
// sobra espacio a un lado; un relleno aquí adentro se comería la barra de
// un acomodo que ponga algo más en el mismo renglón.
RowLayout {
    id: root
    spacing: 8

    // Los diálogos piden una URL, no una ruta.
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

    // Abrir una radio: una dirección y ya.
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
    // Solo aparece si hay una unidad: un botón que nunca va a servir
    // estorba más de lo que ayuda.
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
