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
    property string tab: "albums"
    readonly property bool byArtist: tab === "artists"

    function showTab(which) {
        tab = which
        if (which === "artists") {
            // Entering the artists clears the filter: otherwise the full list
            // would show alongside a grid of just one.
            Chilpa.openArtistAlbums("")
            Chilpa.refreshArtists()
        } else if (which === "stats") {
            Chilpa.refreshStats()
        } else if (which === "playlists") {
            Chilpa.refreshPlaylists()
        } else if (which === "wizard") {
            Chilpa.runWizard()
        } else if (which === "plugins") {
            Chilpa.loadPlugins()
        } else if (which === "settings") {
            Chilpa.refreshLibraryFolders()
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
                    if (root.tab === "artists")
                        return Chilpa.translateCount(
                            "{} artist", "{} artists", Chilpa.artists.length, Chilpa.trRev)
                    if (root.tab === "stats") return Chilpa.translate("Statistics", Chilpa.trRev)
                    if (root.tab === "wizard")
                        return Chilpa.translate("Output wizard", Chilpa.trRev)
                    if (root.tab === "plugins") return "Plugins"
                    if (root.tab === "settings")
                        return Chilpa.translate("Settings", Chilpa.trRev)
                    if (root.tab === "playlists")
                        return Chilpa.translateCount(
                            "{} playlist", "{} playlists", Chilpa.playlists.length, Chilpa.trRev)
                    return Chilpa.albums.length === 0
                        ? Chilpa.translate("The library is empty", Chilpa.trRev)
                        : Chilpa.translateCount(
                            "{} album", "{} albums", Chilpa.albums.length, Chilpa.trRev)
                }
                font.pixelSize: 14
                font.weight: Font.DemiBold
                color: "#411d1f"
            }

            Repeater {
                model: [
                    { key: "albums", text: Chilpa.translate("Albums", Chilpa.trRev) },
                    { key: "artists", text: Chilpa.translate("Artists", Chilpa.trRev) },
                    { key: "playlists", text: Chilpa.translate("Playlists", Chilpa.trRev) },
                    { key: "stats", text: Chilpa.translate("Statistics", Chilpa.trRev) },
                    { key: "wizard", text: Chilpa.translate("Output", Chilpa.trRev) },
                    { key: "plugins", text: "Plugins" },
                    { key: "settings", text: Chilpa.translate("Settings", Chilpa.trRev) }
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
                visible: root.tab === "albums" && Chilpa.openArtist !== ""
                text: "← " + Chilpa.openArtist
                onClicked: root.showTab("artists")
            }

            // The quick views: the same as ever, looked at another way.
            ComboBox {
                Layout.preferredWidth: 180
                visible: root.tab === "albums"
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
            visible: root.tab === "artists"
        }

        StatsPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "stats"
        }

        PlaylistsPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "playlists"
        }

        WizardPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "wizard"
        }

        PluginsPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "plugins"
        }

        SettingsPanel {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "settings"
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.tab === "albums"
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
                            text: Chilpa.translate("no cover", Chilpa.trRev)
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
                        text: (cols[3] ? cols[3] + " · " : "") +
                              Chilpa.translateCount("{} track", "{} tracks", parseInt(cols[4]), Chilpa.trRev) +
                              (parseInt(cols[6]) > 0
                                  ? " · " + Chilpa.translateCount("{} play", "{} plays", parseInt(cols[6]), Chilpa.trRev)
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
