// El smoke test visual: elegir salida, abrir un archivo y verlo sonar.
//
// Es deliberadamente feo y chico. Sirve para comprobar de un vistazo que el
// motor suena, que la posición avanza y que el bit-perfect se declara con la
// verdad. El layout de verdad llega con la decisión de Widgets contra Quick.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import com.blitzfc.chilpa

ApplicationWindow {
    id: ventana
    width: 860
    height: 760
    visible: true
    title: "Chilpa"
    color: "#faf6f2"

    // Los atajos de siempre. Espacio pausa, Escape limpia la búsqueda, y las
    // flechas mueven la posición diez segundos.
    Shortcut {
        sequence: "Space"
        enabled: !busqueda.activeFocus
        onActivated: if (Chilpa.playing) Chilpa.pause(!Chilpa.paused)
    }
    Shortcut {
        sequence: "Escape"
        onActivated: {
            busqueda.text = ""
            Chilpa.closeAlbum()
        }
    }
    Shortcut {
        sequence: "Ctrl+F"
        onActivated: busqueda.forceActiveFocus()
    }
    Shortcut {
        sequence: "Ctrl+O"
        onActivated: dialogo.open()
    }
    Shortcut {
        sequence: "Ctrl+Right"
        enabled: Chilpa.playing
        onActivated: Chilpa.nextTrack()
    }
    Shortcut {
        sequence: "Ctrl+Left"
        enabled: Chilpa.playing
        onActivated: Chilpa.previousTrack(salidas.deviceId, bitPerfect.checked)
    }
    Shortcut {
        sequence: StandardKey.MoveToNextChar
        enabled: !busqueda.activeFocus && Chilpa.playing
        onActivated: Chilpa.seek(Math.min(Chilpa.seconds + 10, Chilpa.totalSeconds))
    }
    Shortcut {
        sequence: StandardKey.MoveToPreviousChar
        enabled: !busqueda.activeFocus && Chilpa.playing
        onActivated: Chilpa.seek(Math.max(Chilpa.seconds - 10, 0))
    }

    // mm:ss, que es como se lee una duración.
    function reloj(segundos) {
        const m = Math.floor(segundos / 60)
        const s = Math.floor(segundos % 60)
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    // La salida y la política se recuerdan entre sesiones.
    function recordar() {
        Chilpa.remember(salidas.deviceId, bitPerfect.checked)
    }

    Component.onCompleted: {
        // Para probar sin teclado: CHILPA_BUSCAR deja la búsqueda escrita.
        if (Chilpa.initialSearch !== "") busqueda.text = Chilpa.initialSearch
        if (Chilpa.initialAlbum > 0) Chilpa.openAlbumTracks(Chilpa.initialAlbum, "Álbum")
        Chilpa.refreshOutputs()
        Chilpa.refreshAlbums()
        const guardada = Chilpa.outputIds.indexOf(Chilpa.rememberedOutput)
        if (guardada >= 0) salidas.currentIndex = guardada
        bitPerfect.checked = Chilpa.rememberedBitPerfect
        // Arrancada con un archivo: se elige la salida pedida y suena sola.
        if (Chilpa.autoplayPath !== "") {
            if (Chilpa.autoplayDevice !== "") {
                const i = Chilpa.outputIds.indexOf(Chilpa.autoplayDevice)
                if (i >= 0) salidas.currentIndex = i
            }
            // Si apunta a una carpeta, se encola entera.
            if (Chilpa.autoplayPath.endsWith("/"))
                Chilpa.playFolder(Chilpa.autoplayPath, salidas.deviceId, bitPerfect.checked)
            else
                Chilpa.play(Chilpa.autoplayPath, salidas.deviceId, bitPerfect.checked)
        }
    }

    Timer {
        interval: 100
        running: true
        repeat: true
        onTriggered: Chilpa.poll()
    }

    FolderDialog {
        id: dialogoEscaneo
        title: "Escanear una carpeta a la biblioteca"
        onAccepted: Chilpa.scanFolder(selectedFolder)
    }

    FolderDialog {
        id: dialogoCarpeta
        title: "Abrir una carpeta"
        onAccepted: Chilpa.playFolder(selectedFolder, salidas.deviceId, bitPerfect.checked)
    }

    FileDialog {
        id: dialogoLista
        title: "Abrir una lista"
        nameFilters: ["Listas (*.m3u *.m3u8)"]
        onAccepted: Chilpa.playPlaylist(selectedFile, salidas.deviceId, bitPerfect.checked)
    }

    FileDialog {
        id: dialogoGuardar
        title: "Guardar la cola"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "m3u"
        nameFilters: ["Listas (*.m3u)"]
        onAccepted: Chilpa.saveQueue(selectedFile)
    }

    FileDialog {
        id: dialogo
        title: "Abrir un archivo de audio"
        nameFilters: ["Audio (*.flac *.wav *.m4a *.mp3 *.ogg *.aiff *.dsf)"]
        onAccepted: Chilpa.play(selectedFile, salidas.deviceId, bitPerfect.checked)
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 20
        spacing: 14

        RowLayout {
            spacing: 10
            Label { text: "Salida"; color: "#411d1f" }
            ComboBox {
                id: salidas
                Layout.fillWidth: true
                model: Chilpa.outputs
                // El id de la salida elegida. No se llama currentValue:
                // ComboBox ya declara esa propiedad como final.
                onActivated: ventana.recordar()
                readonly property string deviceId:
                    currentIndex >= 0 && currentIndex < Chilpa.outputIds.length
                        ? Chilpa.outputIds[currentIndex]
                        : ""
            }
            TextField {
                id: busqueda
                Layout.preferredWidth: 200
                placeholderText: "Buscar"
                onTextChanged: Chilpa.search(text)
            }
            CheckBox {
                id: bitPerfect
                text: "Bit-perfect"
                checked: true
                onToggled: ventana.recordar()
            }
        }

        RowLayout {
            spacing: 10
            Button {
                text: "Abrir y reproducir"
                onClicked: dialogo.open()
            }
            Button {
                text: "Abrir carpeta"
                onClicked: dialogoCarpeta.open()
            }
            Button {
                text: "Abrir lista"
                onClicked: dialogoLista.open()
            }
            Button {
                text: "Guardar cola"
                enabled: Chilpa.queue.length > 0
                onClicked: dialogoGuardar.open()
            }
            Button {
                text: "Escanear a la biblioteca"
                onClicked: dialogoEscaneo.open()
            }
            Button {
                text: "Anterior"
                enabled: Chilpa.playing
                onClicked: Chilpa.previousTrack(salidas.deviceId, bitPerfect.checked)
            }
            Button {
                text: Chilpa.paused ? "Reanudar" : "Pausa"
                enabled: Chilpa.playing
                onClicked: Chilpa.pause(!Chilpa.paused)
            }
            Button {
                text: "Siguiente"
                enabled: Chilpa.playing && Chilpa.queue.length > 1
                onClicked: Chilpa.nextTrack()
            }
            Button {
                text: "Parar"
                enabled: Chilpa.playing
                onClicked: Chilpa.stop()
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 152
            Layout.alignment: Qt.AlignTop
            radius: 14
            color: "#ffffff"
            border.color: "#1a411d1f"

            RowLayout {
                anchors.fill: parent
                anchors.margins: 16
                spacing: 14

                Rectangle {
                    Layout.preferredWidth: 120
                    Layout.preferredHeight: 120
                    radius: 8
                    color: "#f0eae4"
                    visible: Chilpa.playing || Chilpa.artwork !== ""
                    Image {
                        anchors.fill: parent
                        source: Chilpa.artwork
                        fillMode: Image.PreserveAspectCrop
                        visible: Chilpa.artwork !== ""
                        asynchronous: true
                    }
                    Label {
                        anchors.centerIn: parent
                        text: "sin portada"
                        color: "#7a411d1f"
                        font.pixelSize: 11
                        visible: Chilpa.artwork === ""
                    }
                }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 6

                Label {
                    Layout.fillWidth: true
                    text: Chilpa.title === "" ? "Nada sonando" : Chilpa.title
                    elide: Text.ElideRight
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                    color: "#411d1f"
                }
                Label {
                    Layout.fillWidth: true
                    visible: Chilpa.artist !== "" || Chilpa.album !== ""
                    text: [Chilpa.artist, Chilpa.album].filter(t => t !== "").join(" · ")
                    elide: Text.ElideRight
                    font.pixelSize: 14
                    color: "#411d1f"
                }
                Label {
                    text: Chilpa.format
                    color: "#7a411d1f"
                }
                Label {
                    text: Chilpa.totalSeconds > 0
                        ? reloj(Chilpa.seconds) + " / " + reloj(Chilpa.totalSeconds)
                        : reloj(Chilpa.seconds)
                    font.pixelSize: 26
                    color: "#411d1f"
                }

                // La barra también sirve para saltar: se arrastra o se
                // hace clic en cualquier punto.
                Slider {
                    Layout.fillWidth: true
                    visible: Chilpa.totalSeconds > 0
                    from: 0
                    to: Chilpa.totalSeconds
                    value: Chilpa.seconds
                    onMoved: Chilpa.seek(value)
                }
                Label {
                    text: Chilpa.bitPerfect ? "bit-perfect" : (Chilpa.playing ? "convertido" : "")
                    color: Chilpa.bitPerfect ? "#2e7d32" : "#d7272b"
                }
                Label {
                    Layout.fillWidth: true
                    text: Chilpa.error
                    wrapMode: Text.Wrap
                    color: "#d7272b"
                    visible: Chilpa.error !== ""
                }

                Item { Layout.fillHeight: true }
            }
            }
        }

        // Los resultados, cuando hay algo escrito en la búsqueda.
        Resultados {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 200
            deviceId: salidas.deviceId
            bitPerfect: bitPerfect.checked
            visible: busqueda.text !== "" || Chilpa.openAlbum !== ""
        }

        // La biblioteca, con sus álbumes.
        Biblioteca {
            Layout.fillWidth: true
            Layout.fillHeight: true
            // Una fila de portadas cabe completa, con sus tres renglones de
            // texto; menos que esto los recorta.
            Layout.minimumHeight: 260
            deviceId: salidas.deviceId
            bitPerfect: bitPerfect.checked
            visible: Chilpa.albums.length > 0 && busqueda.text === "" && Chilpa.openAlbum === ""
        }

        // La cola. Se asoma solo cuando hay más de una pista, y solo
        // entonces se queda con el espacio que sobra.
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 150
            radius: 14
            color: "#ffffff"
            border.color: "#1a411d1f"
            visible: Chilpa.queue.length > 1

            ListView {
                id: listaCola
                anchors.fill: parent
                anchors.margins: 10
                clip: true
                model: Chilpa.queue
                currentIndex: Chilpa.queueIndex
                onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)

                delegate: Rectangle {
                    width: listaCola.width
                    height: 26
                    radius: 6
                    color: index === Chilpa.queueIndex ? "#f3e7e7" : "transparent"
                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.left: parent.left
                        anchors.leftMargin: 8
                        anchors.right: parent.right
                        elide: Text.ElideRight
                        text: (index + 1) + ".  " + modelData
                        color: index === Chilpa.queueIndex ? "#d7272b" : "#411d1f"
                    }
                }
            }
        }

        // Sin cola, este relleno se queda con el espacio que sobra y todo lo
        // demás permanece arriba. Un elemento invisible no ocupa lugar en un
        // Layout, así que desaparece en cuanto hay cola.
        Item {
            Layout.fillHeight: true
            visible: Chilpa.queue.length <= 1 && Chilpa.albums.length === 0
        }
    }
}
