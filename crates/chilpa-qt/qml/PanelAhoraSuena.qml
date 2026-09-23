// La tarjeta de lo que suena: portada, título, formato, posición y el
// veredicto de bit-perfect.
//
// El veredicto se dice como es. Si la salida se abrió con otro formato, se
// marca "convertido" en rojo: un reproductor que dice bit-perfect sin serlo
// no sirve para nada.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: raiz
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"
    implicitHeight: compacto ? 112 : 152

    // En una barra de abajo la tarjeta no cabe entera: la portada se
    // encoge y el reloj deja de gritar.
    property bool compacto: false

    // mm:ss, que es como se lee una duración.
    function reloj(segundos) {
        const m = Math.floor(segundos / 60)
        const s = Math.floor(segundos % 60)
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    RowLayout {
        anchors.fill: parent
        anchors.margins: raiz.compacto ? 10 : 16
        spacing: 14

        Rectangle {
            Layout.preferredWidth: raiz.compacto ? 64 : 120
            Layout.preferredHeight: raiz.compacto ? 64 : 120
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
                visible: Chilpa.artwork === "" && !raiz.compacto
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: raiz.compacto ? 2 : 6

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
                // El formato de verdad, y lo que el análisis sacó de la
                // pista cuando ya se analizó.
                text: [Chilpa.format, Chilpa.analysis].filter(t => t !== "").join("  ·  ")
                color: "#7a411d1f"
                visible: !raiz.compacto
            }
            Label {
                text: Chilpa.totalSeconds > 0
                    ? reloj(Chilpa.seconds) + " / " + reloj(Chilpa.totalSeconds)
                    : reloj(Chilpa.seconds)
                font.pixelSize: raiz.compacto ? 13 : 26
                color: "#411d1f"
            }

            // La barra también sirve para saltar: se arrastra o se hace clic
            // en cualquier punto.
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
