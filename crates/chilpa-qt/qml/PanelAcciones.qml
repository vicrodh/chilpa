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
    id: raiz
    spacing: 8

    // Los diálogos piden una URL, no una ruta.
    readonly property url desde:
        Chilpa.lastFolder === "" ? "" : "file://" + Chilpa.lastFolder

    Component.onCompleted: Chilpa.refreshCdDrives()

    Shortcut {
        sequence: "Ctrl+O"
        onActivated: dialogoArchivo.open()
    }

    FileDialog {
        id: dialogoArchivo
        currentFolder: raiz.desde
        title: "Abrir un archivo de audio"
        nameFilters: ["Audio (*.flac *.wav *.m4a *.mp3 *.ogg *.aiff *.dsf)"]
        onAccepted: Chilpa.play(selectedFile)
    }

    FolderDialog {
        id: dialogoCarpeta
        currentFolder: raiz.desde
        title: "Abrir una carpeta"
        onAccepted: Chilpa.playFolder(selectedFolder)
    }

    FileDialog {
        id: dialogoLista
        currentFolder: raiz.desde
        title: "Abrir una lista"
        nameFilters: ["Listas (*.m3u *.m3u8)"]
        onAccepted: Chilpa.playPlaylist(selectedFile)
    }

    FileDialog {
        id: dialogoGuardar
        currentFolder: raiz.desde
        title: "Guardar la cola"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "m3u"
        nameFilters: ["Listas (*.m3u)"]
        onAccepted: Chilpa.saveQueue(selectedFile)
    }

    FolderDialog {
        id: dialogoEscaneo
        currentFolder: raiz.desde
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
        text: "Vaciar cola"
        enabled: Chilpa.queue.length > 0
        onClicked: Chilpa.clearQueue()
    }
    Button {
        text: "Abrir stream"
        onClicked: dialogoStream.open()
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
        onClicked: dialogoEscaneo.open()
    }
    Button {
        text: "Volver a escanear"
        enabled: !Chilpa.scanning
        onClicked: Chilpa.rescan()
    }
}
