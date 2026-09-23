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
    id: raiz
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

    ListView {
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: Chilpa.results

        delegate: Rectangle {
            width: ListView.view.width
            height: 30
            radius: 6
            color: fila.containsMouse ? "#f7f1ec" : "transparent"

            readonly property var datos: modelData.split("\t")

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 10

                Label {
                    Layout.preferredWidth: parent.width * 0.45
                    text: datos[1]
                    elide: Text.ElideRight
                    color: "#411d1f"
                }
                Label {
                    Layout.preferredWidth: parent.width * 0.25
                    text: datos[2]
                    elide: Text.ElideRight
                    color: "#7a411d1f"
                }
                Label {
                    Layout.fillWidth: true
                    text: datos[3]
                    elide: Text.ElideRight
                    color: "#7a411d1f"
                }
                Label {
                    text: datos[4]
                    color: "#7a411d1f"
                }
            }

            MouseArea {
                id: fila
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                onClicked: (evento) => {
                    if (evento.button === Qt.MiddleButton)
                        Chilpa.enqueueResults(index)
                    else
                        Chilpa.playResults(index)
                }
            }
        }
    }
    }
}
