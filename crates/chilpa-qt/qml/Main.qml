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
    width: 640
    height: 320
    visible: true
    title: "Chilpa"
    color: "#faf6f2"

    // mm:ss, que es como se lee una duración.
    function reloj(segundos) {
        const m = Math.floor(segundos / 60)
        const s = Math.floor(segundos % 60)
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    Component.onCompleted: {
        Chilpa.refreshOutputs()
        // Arrancada con un archivo: se elige la salida pedida y suena sola.
        if (Chilpa.autoplayPath !== "") {
            if (Chilpa.autoplayDevice !== "") {
                const i = Chilpa.outputIds.indexOf(Chilpa.autoplayDevice)
                if (i >= 0) salidas.currentIndex = i
            }
            Chilpa.play(Chilpa.autoplayPath, salidas.deviceId, bitPerfect.checked)
        }
    }

    Timer {
        interval: 100
        running: true
        repeat: true
        onTriggered: Chilpa.poll()
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
                readonly property string deviceId:
                    currentIndex >= 0 && currentIndex < Chilpa.outputIds.length
                        ? Chilpa.outputIds[currentIndex]
                        : ""
            }
            CheckBox {
                id: bitPerfect
                text: "Bit-perfect"
                checked: true
            }
        }

        RowLayout {
            spacing: 10
            Button {
                text: "Abrir y reproducir"
                onClicked: dialogo.open()
            }
            Button {
                text: "Parar"
                enabled: Chilpa.playing
                onClicked: Chilpa.stop()
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: 14
            color: "#ffffff"
            border.color: "#1a411d1f"

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 16
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

                ProgressBar {
                    Layout.fillWidth: true
                    visible: Chilpa.totalSeconds > 0
                    from: 0
                    to: Chilpa.totalSeconds
                    value: Chilpa.seconds
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
            }
        }
    }
}
