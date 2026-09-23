// Las listas guardadas.
//
// Cada una dice de dónde viene. Hoy todas son de aquí; cuando un plugin
// traiga las suyas, se van a ver en la misma lista y se van a poder separar
// por esa columna, que es de lo que se trata.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    id: raiz
    spacing: 8

    // Cada renglón viene como "id, nombre, de dónde, pistas".
    function campos(renglon) {
        return renglon.split("\t")
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        TextField {
            id: nombre
            Layout.preferredWidth: 220
            placeholderText: "Nombre de la lista"
            onAccepted: raiz.guardar()
        }
        Button {
            text: "Guardar la cola"
            enabled: Chilpa.queue.length > 0 && nombre.text.trim() !== ""
            onClicked: raiz.guardar()
        }
        Item { Layout.fillWidth: true }
    }

    function guardar() {
        if (nombre.text.trim() === "") return
        Chilpa.saveQueueAs(nombre.text)
        nombre.text = ""
    }

    Label {
        visible: Chilpa.playlists.length === 0
        text: "Todavía no hay listas guardadas"
        color: "#7a411d1f"
    }

    ListView {
        id: lista
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: Chilpa.playlists

        delegate: Rectangle {
            width: lista.width
            height: 40
            radius: 8
            color: fila.containsMouse ? "#f6f1ec" : "transparent"

            readonly property var datos: raiz.campos(modelData)

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 6
                spacing: 10

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Label {
                        Layout.fillWidth: true
                        text: datos[1]
                        elide: Text.ElideRight
                        font.pixelSize: 13
                        color: "#411d1f"
                    }
                    Label {
                        text: datos[3] + (datos[3] === "1" ? " pista · " : " pistas · ") + datos[2]
                        font.pixelSize: 10
                        color: "#7a411d1f"
                    }
                }
                Button {
                    text: "Reproducir"
                    onClicked: Chilpa.playSavedPlaylist(parseInt(datos[0]))
                }
                Button {
                    text: "Borrar"
                    onClicked: Chilpa.deletePlaylist(parseInt(datos[0]))
                }
            }

            MouseArea {
                id: fila
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
            }
        }
    }
}
