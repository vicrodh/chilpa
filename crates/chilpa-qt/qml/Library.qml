// La cuadrícula de álbumes.
//
// Es el primer pedazo de la vista de biblioteca: portada, título, artista y
// cuántas pistas trae. Un clic la pone a sonar completa.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: root
    radius: 14

    // Qué se está mirando: los discos, los artistas o las estadísticas.
    // Entrar a un artista devuelve a los discos, ya filtrados.
    property string tab: "albumes"
    readonly property bool byArtist: tab === "artistas"

    function showTab(which) {
        tab = which
        if (which === "artistas") {
            // Entrar a los artistas quita el filtro: si no, se vería la
            // lista completa y la reja de uno solo.
            Chilpa.openArtistAlbums("")
            Chilpa.refreshArtists()
        } else if (which === "estadisticas") {
            Chilpa.refreshStats()
        } else if (which === "listas") {
            Chilpa.refreshPlaylists()
        } else if (which === "asistente") {
            Chilpa.runWizard()
        } else if (which === "plugins") {
            Chilpa.loadPlugins()
        }
    }

    Component.onCompleted: {
        // Para probar la vista sin ratón: CHILPA_PESTANA=artistas.
        if (Chilpa.initialTab !== "") showTab(Chilpa.initialTab)
    }
    color: "#ffffff"
    border.color: "#1a411d1f"

    // Cada renglón viene como "id, título, artista, año, pistas, portada".
    function fields(line) {
        return line.split("\t")
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        RowLayout {
            Layout.fillWidth: true
            Label {
                text: {
                    if (root.tab === "artistas")
                        return Chilpa.artists.length +
                            (Chilpa.artists.length === 1 ? " artista" : " artistas")
                    if (root.tab === "estadisticas") return "Estadísticas"
                    if (root.tab === "asistente") return "Asistente de salida"
                    if (root.tab === "plugins") return "Plugins"
                    if (root.tab === "listas")
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
                    { clave: "albumes", text: "Álbumes" },
                    { clave: "artistas", text: "Artistas" },
                    { clave: "listas", text: "Listas" },
                    { clave: "estadisticas", text: "Estadísticas" },
                    { clave: "asistente", text: "Salida" },
                    { clave: "plugins", text: "Plugins" }
                ]
                delegate: Button {
                    text: modelData.text
                    checkable: true
                    checked: root.tab === modelData.clave
                    onClicked: root.showTab(modelData.clave)
                }
            }

            // El artista al que está filtrada la reja, con su salida.
            Button {
                visible: root.tab === "albumes" && Chilpa.openArtist !== ""
                text: "← " + Chilpa.openArtist
                onClicked: root.showTab("artistas")
            }

            // Las vistas rápidas: lo mismo de siempre, mirado de otra forma.
            ComboBox {
                Layout.preferredWidth: 180
                visible: root.tab === "albumes"
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

        ArtistGrid {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "artistas"
        }

        StatsPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "estadisticas"
        }

        PlaylistsPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "listas"
        }

        WizardPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "asistente"
        }

        PluginsPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "plugins"
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "albumes"
            clip: true
            cellWidth: 160
            cellHeight: 216
            model: Chilpa.albums

            delegate: Item {
                width: grid.cellWidth - 8
                height: grid.cellHeight - 8

                readonly property var cols: root.fields(modelData)

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
                            source: cols[5] ? "file://" + cols[5] : ""
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            visible: cols[5] !== ""
                        }
                        Label {
                            anchors.centerIn: parent
                            text: "sin portada"
                            font.pixelSize: 11
                            color: "#7a411d1f"
                            visible: cols[5] === ""
                        }
                    }

                    Label {
                        Layout.fillWidth: true
                        text: cols[1]
                        elide: Text.ElideRight
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                        color: "#411d1f"
                    }
                    Label {
                        Layout.fillWidth: true
                        text: cols[2]
                        elide: Text.ElideRight
                        font.pixelSize: 12
                        color: "#7a411d1f"
                    }
                    // El año y las pistas en su propio renglón: pegados al
                    // artista se los comía el elide.
                    Label {
                        Layout.fillWidth: true
                        text: (cols[3] ? cols[3] + " · " : "") + cols[4] +
                              (cols[4] === "1" ? " pista" : " pistas") +
                              (parseInt(cols[6]) > 0
                                  ? " · " + cols[6] + (cols[6] === "1" ? " escucha" : " escuchas")
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
                    onClicked: (event) => {
                        if (event.button === Qt.MiddleButton)
                            Chilpa.enqueueAlbum(parseInt(cols[0]))
                        else
                            Chilpa.openAlbumTracks(parseInt(cols[0]), cols[1])
                    }
                    onDoubleClicked: Chilpa.playAlbum(parseInt(cols[0]))
                }
            }
        }
    }
}
