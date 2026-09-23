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

    // El análisis: lo que llena las listas de tempo.
    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        Button {
            text: Chilpa.analyzing ? "Detener el análisis" : "Analizar la biblioteca"
            onClicked: Chilpa.analyzing ? Chilpa.cancelAnalysis() : Chilpa.analyzeLibrary()
        }
        Rectangle {
            visible: Chilpa.analyzing
            Layout.preferredWidth: 140
            Layout.preferredHeight: 6
            radius: 3
            color: "#e6ddd5"
            Rectangle {
                height: parent.height
                width: parent.width * Math.max(0, Math.min(1, Chilpa.analysisProgress))
                radius: parent.radius
                color: "#d7272b"
            }
        }
        Label {
            Layout.fillWidth: true
            text: Chilpa.analysisStatus
            elide: Text.ElideRight
            font.pixelSize: 11
            color: "#7a411d1f"
        }
    }

    // Las inteligentes primero: no se guardan, se vuelven a resolver cada
    // vez que se miran, así que siempre traen lo que cumple hoy.
    Label {
        text: "Inteligentes"
        font.pixelSize: 12
        font.weight: Font.DemiBold
        color: "#7a411d1f"
    }

    Flow {
        Layout.fillWidth: true
        spacing: 8

        Repeater {
            model: Chilpa.smartLists
            delegate: Button {
                readonly property var datos: modelData.split("\t")
                text: datos[0] + "  (" + datos[1] + ")"
                enabled: datos[1] !== "0"
                onClicked: Chilpa.playSmartList(datos[0])
            }
        }
    }

    Label {
        Layout.topMargin: 6
        visible: Chilpa.genres.length > 0
        text: "Géneros"
        font.pixelSize: 12
        font.weight: Font.DemiBold
        color: "#7a411d1f"
    }

    Flow {
        Layout.fillWidth: true
        spacing: 8
        visible: Chilpa.genres.length > 0

        Repeater {
            model: Chilpa.genres
            delegate: Button {
                readonly property var datos: modelData.split("\t")
                text: datos[0] + "  (" + datos[1] + ")"
                onClicked: Chilpa.playGenre(datos[0])
            }
        }
    }

    Label {
        Layout.topMargin: 6
        text: "Guardadas"
        font.pixelSize: 12
        font.weight: Font.DemiBold
        color: "#7a411d1f"
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
