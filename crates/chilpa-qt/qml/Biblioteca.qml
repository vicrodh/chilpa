// La cuadrícula de álbumes.
//
// Es el primer pedazo de la vista de biblioteca: portada, título, artista y
// cuántas pistas trae. Un clic la pone a sonar completa.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: raiz
    radius: 14

    // Qué se está mirando: la reja de discos o la de artistas. Entrar a un
    // artista devuelve a los discos, ya filtrados.
    property bool porArtista: false

    Component.onCompleted: {
        // Para probar la vista sin ratón: CHILPA_PESTANA=artistas.
        if (Chilpa.initialTab === "artistas") {
            porArtista = true
            Chilpa.refreshArtists()
        }
    }
    color: "#ffffff"
    border.color: "#1a411d1f"

    // Cada renglón viene como "id, título, artista, año, pistas, portada".
    function campos(renglon) {
        return renglon.split("\t")
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            Label {
                text: raiz.porArtista
                    ? Chilpa.artists.length + (Chilpa.artists.length === 1 ? " artista" : " artistas")
                    : (Chilpa.albums.length === 0
                        ? "La biblioteca está vacía"
                        : Chilpa.albums.length + (Chilpa.albums.length === 1 ? " álbum" : " álbumes"))
                font.pixelSize: 14
                font.weight: Font.DemiBold
                color: "#411d1f"
            }

            Button {
                text: raiz.porArtista ? "Álbumes" : "Artistas"
                onClicked: {
                    raiz.porArtista = !raiz.porArtista
                    if (raiz.porArtista) {
                        // Entrar a los artistas quita el filtro: si no, se
                        // vería la lista completa y la reja de uno solo.
                        Chilpa.openArtistAlbums("")
                        Chilpa.refreshArtists()
                    }
                }
            }

            // El artista al que está filtrada la reja, con su salida.
            Button {
                visible: !raiz.porArtista && Chilpa.openArtist !== ""
                text: "← " + Chilpa.openArtist
                onClicked: {
                    Chilpa.openArtistAlbums("")
                    raiz.porArtista = true
                    Chilpa.refreshArtists()
                }
            }

            // Las vistas rápidas: lo mismo de siempre, mirado de otra forma.
            ComboBox {
                Layout.preferredWidth: 180
                visible: !raiz.porArtista
                model: Chilpa.albumViews
                currentIndex: Chilpa.albumView
                onActivated: Chilpa.albumView = index
            }

            Item { Layout.fillWidth: true }
            ProgressBar {
                visible: Chilpa.scanning
                Layout.preferredWidth: 160
                from: 0
                to: 1
                value: Chilpa.scanProgress
            }
            Label {
                text: Chilpa.libraryStatus
                color: "#7a411d1f"
                font.pixelSize: 12
                elide: Text.ElideLeft
                Layout.maximumWidth: 320
            }
        }

        RejaDeArtistas {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: raiz.porArtista
        }

        GridView {
            id: cuadricula
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !raiz.porArtista
            clip: true
            cellWidth: 160
            cellHeight: 216
            model: Chilpa.albums

            delegate: Item {
                width: cuadricula.cellWidth - 8
                height: cuadricula.cellHeight - 8

                readonly property var datos: raiz.campos(modelData)

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 6

                    Rectangle {
                        Layout.preferredWidth: parent.width
                        Layout.preferredHeight: parent.width
                        radius: 8
                        color: "#f0eae4"
                        Image {
                            anchors.fill: parent
                            source: datos[5] ? "file://" + datos[5] : ""
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            visible: datos[5] !== ""
                        }
                        Label {
                            anchors.centerIn: parent
                            text: "sin portada"
                            font.pixelSize: 11
                            color: "#7a411d1f"
                            visible: datos[5] === ""
                        }
                    }

                    Label {
                        Layout.fillWidth: true
                        text: datos[1]
                        elide: Text.ElideRight
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                        color: "#411d1f"
                    }
                    Label {
                        Layout.fillWidth: true
                        text: datos[2]
                        elide: Text.ElideRight
                        font.pixelSize: 12
                        color: "#7a411d1f"
                    }
                    // El año y las pistas en su propio renglón: pegados al
                    // artista se los comía el elide.
                    Label {
                        Layout.fillWidth: true
                        text: (datos[3] ? datos[3] + " · " : "") + datos[4] +
                              (datos[4] === "1" ? " pista" : " pistas") +
                              (parseInt(datos[6]) > 0
                                  ? " · " + datos[6] + (datos[6] === "1" ? " escucha" : " escuchas")
                                  : "")
                        font.pixelSize: 11
                        color: "#7a411d1f"
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    // Un clic abre el álbum; el doble clic lo echa a andar.
                    onClicked: Chilpa.openAlbumTracks(parseInt(datos[0]), datos[1])
                    onDoubleClicked: Chilpa.playAlbum(parseInt(datos[0]))
                }
            }
        }
    }
}
