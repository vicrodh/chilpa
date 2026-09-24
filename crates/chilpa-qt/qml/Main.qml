// The window. It arranges nothing: it loads the chosen layout and lets it work.
//
// Chilpa will have several layouts, editable in the future, so the window
// only takes care of what no layout should repeat: the tick that collects
// the player's events, starting with a file from the command line, and
// loading whichever layout applies.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ApplicationWindow {
    id: window
    width: 980
    // Tall so the classic layout fits whole with the grid, the queue and
    // the visualizer; if the screen is smaller, Qt trims the window and the
    // last thing to give is the queue.
    height: 860
    visible: true
    // The title says what is playing: in a taskbar it is all that is seen
    // of the window.
    title: Chilpa.playing && Chilpa.title !== ""
        ? (Chilpa.artist !== "" ? Chilpa.artist + " — " + Chilpa.title : Chilpa.title)
        : "Chilpa"
    color: "#faf6f2"

    // The layout is a `Layout<name>.qml` file next to this one. If the
    // saved name does not exist, it falls back to the classic one instead
    // of leaving the window empty.
    Loader {
        id: layout
        anchors.fill: parent
        source: "Layout" + Chilpa.layout + ".qml"
        onStatusChanged: if (status === Loader.Error && Chilpa.layout !== "Classic") {
            console.warn("no se pudo cargar el layout", Chilpa.layout)
            Chilpa.layout = "Classic"
        }
    }

    // It waits for the search to bring results and plays the one that was asked for.
    Timer {
        id: playWhenReady
        interval: 250
        repeat: true
        property int tries: 0
        onTriggered: {
            tries += 1
            if (Chilpa.results.length >= Chilpa.initialPlay) {
                stop()
                Chilpa.playResults(Chilpa.initialPlay - 1)
            } else if (tries > 40) {
                stop()
                console.warn("no llegaron resultados que tocar")
            }
        }
    }

    Component.onCompleted: {
        // The plugins are loaded here and not on opening their tab: the
        // search asks the sources, and it cannot depend on the user having
        // been through there first. A broken one does not load and is
        // noted; startup never stops because of it (ADR-003).
        Chilpa.loadPlugins()

        // For exercising without keyboard or mouse: CHILPA_SEARCH leaves the
        // search typed in and CHILPA_ALBUM opens an album by id.
        if (Chilpa.initialSearch !== "") Chilpa.search(Chilpa.initialSearch)
        // A source's results arrive after the search, so it waits for them
        // before playing the first one.
        if (Chilpa.initialPlay > 0) playWhenReady.start()
        if (Chilpa.initialConnect !== "") Chilpa.connectAccount(Chilpa.initialConnect)
        if (Chilpa.initialAlbum > 0) Chilpa.openAlbumTracks(Chilpa.initialAlbum, "Álbum")
        Chilpa.refreshAlbums()
        if (Chilpa.initialRescan) Chilpa.rescan()
        if (Chilpa.initialAnalysis) Chilpa.analyzeLibrary()
        // Started with a file: it plays by itself on whatever output is asked for.
        if (Chilpa.autoplayPath !== "") {
            // An output asked for on the command line plays, but it does not
            // stay as the preferred one: it is for testing, not for choosing.
            if (Chilpa.autoplayDevice !== "") Chilpa.device = Chilpa.autoplayDevice
            if (Chilpa.autoplayPath.endsWith("/"))
                Chilpa.playFolder(Chilpa.autoplayPath)
            else
                Chilpa.play(Chilpa.autoplayPath)
        }
    }

    // The list of shortcuts, which otherwise has to be guessed.
    Dialog {
        id: help
        title: "Atajos"
        anchors.centerIn: Overlay.overlay
        width: 380
        Component.onCompleted: if (Chilpa.initialTab === "ayuda") open()

        ColumnLayout {
            anchors.fill: parent
            spacing: 4
            Repeater {
                model: [
                    ["Espacio", "pausa y reanuda"],
                    ["Ctrl+O", "abrir un archivo"],
                    ["Ctrl+F", "buscar"],
                    ["Escape", "cerrar lo que esté abierto"],
                    ["Ctrl + flechas", "pista siguiente y anterior"],
                    ["Flechas", "diez segundos adelante y atrás"],
                    ["F1", "esta lista"]
                ]
                delegate: RowLayout {
                    Layout.fillWidth: true
                    spacing: 12
                    Label {
                        Layout.preferredWidth: 160
                        text: modelData[0]
                        font.family: "monospace"
                        font.pixelSize: 12
                    }
                    Label {
                        Layout.fillWidth: true
                        text: modelData[1]
                        font.pixelSize: 12
                    }
                }
            }

            // A button of our own: Qt's standard one comes out in English.
            Button {
                Layout.alignment: Qt.AlignRight
                Layout.topMargin: 6
                text: "Cerrar"
                onClicked: help.close()
            }
        }
    }

    Shortcut {
        sequence: "F1"
        onActivated: help.open()
    }

    Timer {
        interval: 100
        running: true
        repeat: true
        onTriggered: Chilpa.poll()
    }
}
