// La ventana. No acomoda nada: carga el layout elegido y lo deja hacer.
//
// Chilpa va a tener varios acomodos, y en el futuro modificables, así que la
// ventana solo se encarga de lo que ningún layout debería repetir: el pulso
// que recoge los eventos del reproductor, el arranque con un archivo por la
// línea de comandos, y cargar el layout que toque.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ApplicationWindow {
    id: window
    width: 980
    // Alto para que el acomodo clásico quepa entero con la reja, la cola y
    // el visualizador; si la pantalla es más chica, Qt recorta la ventana y
    // lo último en ceder es la cola.
    height: 860
    visible: true
    // El título dice qué suena: en una barra de tareas es lo único que se
    // ve de la ventana.
    title: Chilpa.playing && Chilpa.title !== ""
        ? (Chilpa.artist !== "" ? Chilpa.artist + " — " + Chilpa.title : Chilpa.title)
        : "Chilpa"
    color: "#faf6f2"

    // El layout es un archivo `Layout<nombre>.qml` al lado de este. Si el
    // nombre guardado no existe, se cae al clásico en vez de dejar la
    // ventana vacía.
    Loader {
        id: layout
        anchors.fill: parent
        source: "Layout" + Chilpa.layout + ".qml"
        onStatusChanged: if (status === Loader.Error && Chilpa.layout !== "Classic") {
            console.warn("no se pudo cargar el layout", Chilpa.layout)
            Chilpa.layout = "Classic"
        }
    }

    // Espera a que la búsqueda traiga resultados y toca el que se pidió.
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
        // Los plugins se cargan aquí y no al abrir su pestaña: la búsqueda
        // le pregunta a los sources, y no puede depender de que el usuario
        // haya pasado por ahí primero. Uno roto no carga y se anota; el
        // arranque nunca se detiene por eso (ADR-003).
        Chilpa.loadPlugins()

        // Para probar sin teclado ni ratón: CHILPA_SEARCH deja la búsqueda
        // escrita y CHILPA_ALBUM abre un álbum por id.
        if (Chilpa.initialSearch !== "") Chilpa.search(Chilpa.initialSearch)
        // Los resultados de un source llegan después de la búsqueda, así que
        // se espera a que estén antes de tocar el primero.
        if (Chilpa.initialPlay > 0) playWhenReady.start()
        if (Chilpa.initialAlbum > 0) Chilpa.openAlbumTracks(Chilpa.initialAlbum, "Álbum")
        Chilpa.refreshAlbums()
        if (Chilpa.initialRescan) Chilpa.rescan()
        if (Chilpa.initialAnalysis) Chilpa.analyzeLibrary()
        // Arrancada con un archivo: suena sola en la salida que se le pida.
        if (Chilpa.autoplayPath !== "") {
            // Una salida pedida por la línea de comandos suena, pero no se
            // queda de preferida: es para probar, no para elegir.
            if (Chilpa.autoplayDevice !== "") Chilpa.device = Chilpa.autoplayDevice
            if (Chilpa.autoplayPath.endsWith("/"))
                Chilpa.playFolder(Chilpa.autoplayPath)
            else
                Chilpa.play(Chilpa.autoplayPath)
        }
    }

    // La lista de atajos, que si no hay que adivinarla.
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

            // Botón propio: el estándar de Qt sale en inglés.
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
