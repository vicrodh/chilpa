// The album grid.
//
// It is the first piece of the library view: cover, title, artist and how
// many tracks it carries. A click starts the whole thing playing.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: root
    radius: 14

    // What is being looked at: the albums, the artists or the stats.
    // Entering an artist returns to the albums, already filtered.
    property string tab: "albumes"
    readonly property bool byArtist: tab === "artistas"

    function showTab(which) {
        tab = which
        if (which === "artistas") {
            // Entering the artists clears the filter: otherwise the full list
            // would show alongside a grid of just one.
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
        // For exercising the view without a mouse: CHILPA_TAB=artists.
        if (Chilpa.initialTab !== "") showTab(Chilpa.initialTab)
    }
    color: "#ffffff"
    border.color: "#1a411d1f"

    // Each line comes as "id, title, artist, year, tracks, cover".
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
                    { key: "albumes", text: "Álbumes" },
                    { key: "artistas", text: "Artistas" },
                    { key: "listas", text: "Listas" },
                    { key: "estadisticas", text: "Estadísticas" },
                    { key: "asistente", text: "Salida" },
                    { key: "plugins", text: "Plugins" }
                ]
                delegate: Button {
                    text: modelData.text
                    checkable: true
                    checked: root.tab === modelData.key
                    onClicked: root.showTab(modelData.key)
                }
            }

            // The artist the grid is filtered to, with its way out.
            Button {
                visible: root.tab === "albumes" && Chilpa.openArtist !== ""
                text: "← " + Chilpa.openArtist
                onClicked: root.showTab("artistas")
            }

            // The quick views: the same as ever, looked at another way.
            ComboBox {
                Layout.preferredWidth: 180
                visible: root.tab === "albumes"
                model: Chilpa.albumViews
                currentIndex: Chilpa.albumView
                onActivated: Chilpa.albumView = index
            }

            Item { Layout.fillWidth: true }
            // A bar of our own, two rectangles: the system style's one
            // complains when it appears inside a RowLayout and fills the
            // console with warnings that are not ours.
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
                    // The year and the tracks on their own line: stuck to the
                    // artist the elide ate them.
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
                    // A click opens the album, a double click sets it going and
                    // the middle one queues it without cutting off what plays.
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
