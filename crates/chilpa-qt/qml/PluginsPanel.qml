// Los plugins instalados, qué piden y cómo se están portando.
//
// Un plugin que no cargó sale aquí con su razón, no en un log que nadie
// mira. De la cuarentena solo lo saca el usuario, con este botón.
//
// Y nadie corre sin permiso (ADR-008): uno recién puesto aparece con lo que
// pide escrito en palabras y no hace nada hasta que se le diga que sí. No
// hay un botón de "aprobar todo": si aprobar fuera un trámite, no sería una
// decisión.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    id: root
    spacing: 8

    function fields(line) {
        return line.split("\t")
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
        id: list
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        spacing: 4
        model: Chilpa.plugins

        delegate: Rectangle {
            width: list.width
            // Uno pendiente crece para que quepa lo que pide: esconderlo
            // tras un "ver detalles" es invitar a aprobar sin leer.
            height: pending ? 64 + asks.length * 17 : 46
            radius: 8
            color: pending ? "#fdf6ec" : (row.containsMouse ? "#f6f1ec" : "transparent")
            border.color: pending ? "#e0a96d" : "transparent"

            readonly property var cols: root.fields(modelData)
            readonly property bool quarantined: cols[2] === "en cuarentena"
            readonly property bool pending: cols[2] === "pendiente de aprobación"
            // Lo que falta aprobar, un renglón por cosa.
            readonly property var asks: cols.length > 4 && cols[4] !== "" ? cols[4].split(" · ") : []
            // Uno que ya corría y ahora pide más no es uno nuevo.
            readonly property bool wantsMore: cols.length > 5 && cols[5] === "mas"

            ColumnLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 6
                anchors.topMargin: 4
                anchors.bottomMargin: 4
                spacing: 2

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        Label {
                            Layout.fillWidth: true
                            text: cols[1]
                            elide: Text.ElideRight
                            font.pixelSize: 13
                            color: "#411d1f"
                        }
                        Label {
                            Layout.fillWidth: true
                            text: cols[3] !== "" ? cols[3] : cols[0]
                            elide: Text.ElideRight
                            font.pixelSize: 10
                            color: quarantined ? "#d7272b" : "#7a411d1f"
                        }
                    }
                    Label {
                        text: cols[2]
                        font.pixelSize: 11
                        color: quarantined ? "#d7272b"
                             : pending ? "#a05a2c" : "#2e7d32"
                    }
                    Button {
                        visible: pending
                        text: wantsMore ? "Aprobar lo nuevo" : "Aprobar"
                        onClicked: Chilpa.approvePlugin(cols[0])
                    }
                    Button {
                        visible: !pending && !quarantined
                        text: "Quitar permiso"
                        onClicked: Chilpa.revokePlugin(cols[0])
                    }
                    Button {
                        visible: quarantined
                        text: "Reactivar"
                        onClicked: Chilpa.reactivatePlugin(cols[0])
                    }
                }

                // Lo que pide, que es lo que se está aprobando.
                Label {
                    Layout.fillWidth: true
                    visible: pending
                    text: asks.length === 0
                        ? "No pide ningún permiso, pero igual decides tú si corre."
                        : (wantsMore ? "Ahora pide además:" : "Este plugin quiere:")
                    font.pixelSize: 10
                    color: "#7a411d1f"
                }
                Repeater {
                    model: asks
                    Label {
                        Layout.fillWidth: true
                        visible: pending
                        text: "· " + modelData
                        font.pixelSize: 11
                        elide: Text.ElideRight
                        color: "#411d1f"
                    }
                }
            }

            MouseArea {
                id: row
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
            }
        }
    }
}
