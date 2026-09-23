// Todo lo que abre música, con sus diálogos adentro.
//
// Los diálogos viven aquí y no en la ventana: así un layout que quiera estos
// botones en otro lado solo instancia este panel, sin arrastrar diálogos
// sueltos ni depender de ids de la ventana.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.blitzfc.chilpa

RowLayout {
    spacing: 8

    Shortcut {
        sequence: "Ctrl+O"
        onActivated: dialogoArchivo.open()
    }

    FileDialog {
        id: dialogoArchivo
        title: "Abrir un archivo de audio"
        nameFilters: ["Audio (*.flac *.wav *.m4a *.mp3 *.ogg *.aiff *.dsf)"]
        onAccepted: Chilpa.play(selectedFile)
    }

    FolderDialog {
        id: dialogoCarpeta
        title: "Abrir una carpeta"
        onAccepted: Chilpa.playFolder(selectedFolder)
    }

    FileDialog {
        id: dialogoLista
        title: "Abrir una lista"
        nameFilters: ["Listas (*.m3u *.m3u8)"]
        onAccepted: Chilpa.playPlaylist(selectedFile)
    }

    FileDialog {
        id: dialogoGuardar
        title: "Guardar la cola"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "m3u"
        nameFilters: ["Listas (*.m3u)"]
        onAccepted: Chilpa.saveQueue(selectedFile)
    }

    FolderDialog {
        id: dialogoEscaneo
        title: "Escanear una carpeta a la biblioteca"
        onAccepted: Chilpa.scanFolder(selectedFolder)
    }

    // Abrir una radio: una dirección y ya.
    Dialog {
        id: dialogoStream
        title: "Abrir un stream"
        anchors.centerIn: Overlay.overlay
        width: 420
        standardButtons: Dialog.Ok | Dialog.Cancel
        onAccepted: if (direccion.text !== "") Chilpa.playUrl(direccion.text)

        ColumnLayout {
            anchors.fill: parent
            TextField {
                id: direccion
                Layout.fillWidth: true
                placeholderText: "http://..."
            }
        }
    }

    Button {
        text: "Abrir archivo"
        onClicked: dialogoArchivo.open()
    }
    Button {
        text: "Abrir carpeta"
        onClicked: dialogoCarpeta.open()
    }
    Button {
        text: "Abrir lista"
        onClicked: dialogoLista.open()
    }
    Button {
        text: "Guardar cola"
        enabled: Chilpa.queue.length > 0
        onClicked: dialogoGuardar.open()
    }
    Button {
        text: "Abrir stream"
        onClicked: dialogoStream.open()
    }
    Button {
        text: "Escanear a la biblioteca"
        onClicked: dialogoEscaneo.open()
    }
    Item { Layout.fillWidth: true }
}
