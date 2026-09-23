// Los plugins instalados y cómo se están portando.
//
// Un plugin que no cargó sale aquí con su razón, no en un log que nadie
// mira. Y de la cuarentena solo lo saca el usuario, con este botón.
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
            text: "Buscar plugins"
            onClicked: Chilpa.loadPlugins()
        }
        Label {
            Layout.fillWidth: true
            text: Chilpa.plugins.length === 0
                ? "No hay plugins instalados"
                : Chilpa.plugins.length + (Chilpa.plugins.length === 1 ? " plugin" : " plugins")
            font.pixelSize: 12
            color: "#7a411d1f"
        }
    }

    ListView {
        id: lista
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: Chilpa.plugins

        delegate: Rectangle {
            width: lista.width
            height: 46
            radius: 8
            color: fila.containsMouse ? "#f6f1ec" : "transparent"

            readonly property var datos: raiz.campos(modelData)
            readonly property bool castigado: datos[2] === "en cuarentena"

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 6
                spacing: 10

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Label {
                        Layout.fillWidth: true
                        text: datos[1]
                        elide: Text.ElideRight
                        font.pixelSize: 13
                        color: "#411d1f"
                    }
                    Label {
                        Layout.fillWidth: true
                        text: datos[3] !== "" ? datos[3] : datos[0]
                        elide: Text.ElideRight
                        font.pixelSize: 10
                        color: castigado ? "#d7272b" : "#7a411d1f"
                    }
                }
                Label {
                    text: datos[2]
                    font.pixelSize: 11
                    color: castigado ? "#d7272b" : "#2e7d32"
                }
                Button {
                    visible: castigado
                    text: "Reactivar"
                    onClicked: Chilpa.reactivatePlugin(datos[0])
                }
            }

            MouseArea {
                id: fila
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
            }
        }
    }
}
