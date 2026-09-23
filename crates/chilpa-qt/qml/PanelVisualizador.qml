// Lo que de verdad está saliendo por el dispositivo.
//
// No es la decodificación ni una animación bonita: son las muestras que se
// entregaron, copiadas después de empaquetarlas. Si esto se mueve, hay audio
// saliendo; si se queda plano, no lo hay, y eso ya es un diagnóstico.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: raiz
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"
    implicitHeight: 110

    // Se redibuja cuando llegan datos nuevos, no con un temporizador propio:
    // dos relojes desacompasados se ven peor que uno solo.
    property var scope: Chilpa.scope
    property var spectrum: Chilpa.spectrum
    onScopeChanged: onda.requestPaint()
    onSpectrumChanged: barras.requestPaint()

    /// Qué se dibuja. Lo decide el puente, que lo recuerda entre sesiones.
    readonly property bool porEspectro: Chilpa.visualizer === "espectro"

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        Canvas {
            id: barras
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: raiz.porEspectro

            onPaint: {
                const ctx = getContext("2d")
                ctx.reset()
                const bandas = raiz.spectrum
                if (!bandas || bandas.length === 0) return
                const ancho = width / bandas.length
                for (let i = 0; i < bandas.length; i++) {
                    const v = Math.min(1, parseFloat(bandas[i]) || 0)
                    const alto = Math.max(1, v * height)
                    // Los graves en rojo, los agudos en vino: el degradado
                    // dice en qué parte del espectro se está mirando.
                    const t = i / bandas.length
                    ctx.fillStyle = Qt.rgba(0.84 - 0.6 * t, 0.15 + 0.05 * t, 0.17 + 0.05 * t, 1)
                    ctx.fillRect(i * ancho, height - alto, Math.max(1, ancho - 2), alto)
                }
            }
        }

        Canvas {
            id: onda
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !raiz.porEspectro

            onPaint: {
                const ctx = getContext("2d")
                ctx.reset()
                const w = width
                const h = height
                const medio = h / 2

                ctx.strokeStyle = "#1a411d1f"
                ctx.lineWidth = 1
                ctx.beginPath()
                ctx.moveTo(0, medio)
                ctx.lineTo(w, medio)
                ctx.stroke()

                const puntos = raiz.scope
                if (!puntos || puntos.length === 0) return

                const ancho = w / puntos.length
                ctx.fillStyle = "#d7272b"
                for (let i = 0; i < puntos.length; i++) {
                    // Cada punto es el pico de su tajada: se dibuja
                    // simétrico, como una onda.
                    const v = Math.min(1, parseFloat(puntos[i]) || 0)
                    const alto = Math.max(1, v * medio)
                    ctx.fillRect(i * ancho, medio - alto, Math.max(1, ancho - 1), alto * 2)
                }
            }
        }

        // Los niveles por canal, con su número: un medidor sin número no
        // sirve para comparar nada.
        RowLayout {
            Layout.fillWidth: true
            spacing: 10

            Repeater {
                model: Chilpa.levels
                delegate: RowLayout {
                    spacing: 6
                    readonly property var datos: modelData.split("\t")
                    readonly property real pico: parseFloat(datos[0]) || 0
                    readonly property real rms: parseFloat(datos[1]) || 0

                    Label {
                        text: index === 0 ? "I" : (index === 1 ? "D" : (index + 1))
                        font.pixelSize: 10
                        color: "#7a411d1f"
                    }
                    Rectangle {
                        Layout.preferredWidth: 90
                        Layout.preferredHeight: 6
                        radius: 3
                        color: "#f0eae4"
                        Rectangle {
                            width: parent.width * Math.min(1, rms)
                            height: parent.height
                            radius: parent.radius
                            color: "#2e7d32"
                        }
                        Rectangle {
                            x: Math.min(parent.width - 2, parent.width * Math.min(1, pico))
                            width: 2
                            height: parent.height
                            color: pico > 0.999 ? "#d7272b" : "#411d1f"
                        }
                    }
                    Label {
                        text: pico > 0 ? (20 * Math.log(pico) / Math.LN10).toFixed(1) + " dB" : "—"
                        font.pixelSize: 10
                        color: pico > 0.999 ? "#d7272b" : "#7a411d1f"
                    }
                }
            }
            Item { Layout.fillWidth: true }

            // Cambiar de dibujo sin menús: un clic en el nombre.
            Label {
                text: raiz.porEspectro ? "espectro" : "onda"
                font.pixelSize: 10
                color: "#7a411d1f"
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -6
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Chilpa.chooseVisualizer(
                        raiz.porEspectro ? "onda" : "espectro")
                }
            }
        }
    }
}
