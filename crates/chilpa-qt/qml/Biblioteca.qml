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

    // Qué se está mirando: los discos, los artistas o las estadísticas.
    // Entrar a un artista devuelve a los discos, ya filtrados.
    property string pestana: "albumes"
    readonly property bool porArtista: pestana === "artistas"

    function verPestana(cual) {
        pestana = cual
        if (cual === "artistas") {
            // Entrar a los artistas quita el filtro: si no, se vería la
            // lista completa y la reja de uno solo.
            Chilpa.openArtistAlbums("")
            Chilpa.refreshArtists()
        } else if (cual === "estadisticas") {
            Chilpa.refreshStats()
        } else if (cual === "listas") {
            Chilpa.refreshPlaylists()
        } else if (cual === "asistente") {
            Chilpa.runWizard()
        }
    }

    Component.onCompleted: {
        // Para probar la vista sin ratón: CHILPA_PESTANA=artistas.
        if (Chilpa.initialTab !== "") verPestana(Chilpa.initialTab)
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
                text: {
                    if (raiz.pestana === "artistas")
                        return Chilpa.artists.length +
                            (Chilpa.artists.length === 1 ? " artista" : " artistas")
                    if (raiz.pestana === "estadisticas") return "Estadísticas"
                    if (raiz.pestana === "asistente") return "Asistente de salida"
                    if (raiz.pestana === "listas")
                        return Chilpa.playlists.length +
                            (Chilpa.playlists.length === 1 ? " lista" : " listas")
                    return Chilpa.albums.length === 0
                        ? "La biblioteca está vacía"
                        : Chilpa.albums.length +
                            (Chilpa.albums.length === 1 ? " álbum" : " álbumes")
                }
                font.pixelSize: 14
                font.weight: Font.DemiBold
                color: "#411d1f"
            }

            Repeater {
                model: [
                    { clave: "albumes", texto: "Álbumes" },
                    { clave: "artistas", texto: "Artistas" },
                    { clave: "listas", texto: "Listas" },
                    { clave: "estadisticas", texto: "Estadísticas" },
                    { clave: "asistente", texto: "Salida" }
                ]
                delegate: Button {
                    text: modelData.texto
                    checkable: true
                    checked: raiz.pestana === modelData.clave
                    onClicked: raiz.verPestana(modelData.clave)
                }
            }

            // El artista al que está filtrada la reja, con su salida.
            Button {
                visible: raiz.pestana === "albumes" && Chilpa.openArtist !== ""
                text: "← " + Chilpa.openArtist
                onClicked: raiz.verPestana("artistas")
            }

            // Las vistas rápidas: lo mismo de siempre, mirado de otra forma.
            ComboBox {
                Layout.preferredWidth: 180
                visible: raiz.pestana === "albumes"
                model: Chilpa.albumViews
                currentIndex: Chilpa.albumView
                onActivated: Chilpa.albumView = index
            }

            Item { Layout.fillWidth: true }
            // Una barra propia, de dos rectángulos: la del estilo del
            // sistema se queja cuando aparece dentro de un RowLayout y llena
            // la consola de avisos que no son nuestros.
            Rectangle {
                visible: Chilpa.scanning
                Layout.preferredWidth: 160
                Layout.preferredHeight: 6
                radius: 3
                color: "#e6ddd5"
                Rectangle {
                    height: parent.height
                    width: parent.width * Math.max(0, Math.min(1, Chilpa.scanProgress))
                    radius: parent.radius
                    color: "#d7272b"
                }
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
            visible: raiz.pestana === "artistas"
        }

        PanelEstadisticas {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: raiz.pestana === "estadisticas"
        }

        PanelListas {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: raiz.pestana === "listas"
        }

        PanelAsistente {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: raiz.pestana === "asistente"
        }

        GridView {
            id: cuadricula
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: raiz.pestana === "albumes"
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
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                    // Un clic abre el álbum, el doble clic lo echa a andar y
                    // el de en medio lo encola sin cortar lo que suena.
                    onClicked: (evento) => {
                        if (evento.button === Qt.MiddleButton)
                            Chilpa.enqueueAlbum(parseInt(datos[0]))
                        else
                            Chilpa.openAlbumTracks(parseInt(datos[0]), datos[1])
                    }
                    onDoubleClicked: Chilpa.playAlbum(parseInt(datos[0]))
                }
            }
        }
    }
}
