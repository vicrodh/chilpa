// Los resultados de la búsqueda.
//
// Una lista simple: título, artista y duración. Un clic reproduce desde ahí,
// encolando lo que sigue, que es como se espera de una búsqueda. Un clic
// con el botón de en medio encola sin cortar lo que suena.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: root
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"


    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 8

        // El encabezado solo aparece cuando se abrió un álbum.
        RowLayout {
            Layout.fillWidth: true
            visible: Chilpa.openAlbum !== ""
            spacing: 12

            Rectangle {
                Layout.preferredWidth: 64
                Layout.preferredHeight: 64
                radius: 8
                color: "#f0eae4"
                visible: Chilpa.openAlbumArt !== ""
                Image {
                    anchors.fill: parent
                    source: Chilpa.openAlbumArt
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Label {
                    Layout.fillWidth: true
                    text: Chilpa.openAlbum
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                    color: "#411d1f"
                    elide: Text.ElideRight
                }
                Label {
                    Layout.fillWidth: true
                    visible: Chilpa.openAlbumArtist !== ""
                    text: Chilpa.openAlbumArtist
                    font.pixelSize: 13
                    color: "#411d1f"
                    elide: Text.ElideRight
                }
                Label {
                    Layout.fillWidth: true
                    text: Chilpa.openAlbumLine
                    font.pixelSize: 11
                    color: "#7a411d1f"
                    elide: Text.ElideRight
                }
            }
            Button {
                text: "Reproducir"
                onClicked: Chilpa.playAlbum(Chilpa.openAlbumId)
            }
            Button {
                text: "Encolar"
                onClicked: Chilpa.enqueueAlbum(Chilpa.openAlbumId)
            }
            Button {
                text: "Cerrar"
                onClicked: Chilpa.closeAlbum()
            }
        }

        // Lo que un source no pudo contestar se dice, no se esconde: una
        // lista corta sin explicación parece una biblioteca pobre.
        Label {
            Layout.fillWidth: true
            visible: Chilpa.searchProblems !== ""
            text: "No contestaron: " + Chilpa.searchProblems
            font.pixelSize: 11
            color: "#a05a2c"
            elide: Text.ElideRight
        }

    ListView {
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: Chilpa.results

        delegate: Rectangle {
            width: ListView.view.width
            height: 30
            radius: 6
            color: row.containsMouse ? "#f7f1ec" : "transparent"

            readonly property var cols: modelData.split("\t")
            // Columnas 6 a 8 solo vienen de la búsqueda; al abrir un álbum
            // la fila es más corta y estas quedan vacías.
            readonly property string sourceName: cols.length > 6 ? cols[6] : ""
            readonly property string alsoIn: cols.length > 7 ? cols[7] : ""
            // Lo que todavía no se puede tocar se ve apagado en lugar de
            // fingir que un clic va a sonar.
            readonly property bool playable: cols.length > 8 ? cols[8] === "1" : true

            opacity: playable ? 1.0 : 0.55

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 10

                Label {
                    Layout.preferredWidth: parent.width * 0.30
                    text: cols[1]
                    elide: Text.ElideRight
                    color: "#411d1f"
                }
                Label {
                    Layout.preferredWidth: parent.width * 0.20
                    text: cols[2]
                    elide: Text.ElideRight
                    color: "#7a411d1f"
                }
                Label {
                    Layout.fillWidth: true
                    text: cols[3]
                    elide: Text.ElideRight
                    color: "#7a411d1f"
                }
                // Lo que el análisis sacó. Solo sale cuando ya se analizó:
                // una columna vacía no le dice nada a nadie.
                Label {
                    Layout.preferredWidth: parent.width * 0.12
                    horizontalAlignment: Text.AlignRight
                    text: cols.length > 5 ? cols[5] : ""
                    elide: Text.ElideRight
                    font.pixelSize: 11
                    color: "#7a411d1f"
                }
                // De dónde salió, y en qué otros sources está la misma
                // canción. Solo se dibuja si hay más de uno en juego.
                Label {
                    visible: alsoIn !== "" || (sourceName !== "" && sourceName !== "Mi biblioteca")
                    text: alsoIn !== "" ? sourceName + " + " + alsoIn : sourceName
                    font.pixelSize: 11
                    elide: Text.ElideRight
                    Layout.preferredWidth: parent.width * 0.18
                    color: "#7a411d1f"
                }
                Label {
                    text: cols[4]
                    color: "#7a411d1f"
                }
            }

            MouseArea {
                id: row
                anchors.fill: parent
                hoverEnabled: true
                // Sobre lo que no se puede tocar, el cursor no promete un
                // clic que no va a hacer nada.
                cursorShape: playable ? Qt.PointingHandCursor : Qt.ArrowCursor
                enabled: playable
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                onClicked: (event) => {
                    if (event.button === Qt.MiddleButton)
                        Chilpa.enqueueResults(index)
                    else
                        Chilpa.playResults(index)
                }
            }
        }
    }
    }
}
