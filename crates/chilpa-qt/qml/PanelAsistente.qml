// El asistente de alta fidelidad.
//
// Es un diagnóstico, no una configuración: mide la salida elegida y dice,
// formato por formato, si lo que saldría es lo mismo que entró. No apaga
// nada ni escribe ajustes. El de QBZ escribía una lista de frecuencias
// supuestas, y eso es peor que no saber.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    id: raiz
    spacing: 8

    function campos(renglon) {
        return renglon.split("\t")
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 10
        Button {
            text: "Medir la salida"
            onClicked: Chilpa.runWizard()
        }
        Label {
            Layout.fillWidth: true
            text: Chilpa.wizardSummary
            elide: Text.ElideRight
            font.pixelSize: 12
            color: "#411d1f"
        }
    }

    Label {
        visible: Chilpa.wizardRows.length === 0
        text: "Dale a medir para ver qué puede tu salida"
        color: "#7a411d1f"
    }

    ListView {
        id: lista
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: Chilpa.wizardRows

        delegate: Item {
            width: lista.width
            height: 26

            readonly property var datos: raiz.campos(modelData)
            readonly property bool perfecto: datos[2] === "bit-perfect"
            readonly property bool imposible: datos[2] === "no se puede"

            RowLayout {
                anchors.fill: parent
                anchors.rightMargin: 6
                spacing: 10
                Label {
                    Layout.preferredWidth: 140
                    text: datos[0]
                    elide: Text.ElideRight
                    font.pixelSize: 12
                    color: "#411d1f"
                }
                Label {
                    Layout.fillWidth: true
                    text: datos[1]
                    elide: Text.ElideRight
                    font.pixelSize: 11
                    color: "#7a411d1f"
                }
                Label {
                    text: datos[2]
                    font.pixelSize: 11
                    font.weight: perfecto ? Font.DemiBold : Font.Normal
                    color: perfecto ? "#2e7d32" : (imposible ? "#d7272b" : "#7a411d1f")
                }
            }
        }
    }
}
