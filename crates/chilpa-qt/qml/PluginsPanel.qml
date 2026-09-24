// The installed plugins, what they ask for and how they are behaving.
//
// A plugin that did not load shows up here with its reason, not in a log
// nobody looks at. Only the user takes it out of quarantine, with this
// button.
//
// And nobody runs without permission (ADR-008): a freshly dropped one shows
// up with what it asks for written in words and does nothing until it is
// told yes. There is no "approve all" button: if approving were paperwork,
// it would not be a decision.
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
            // A pending one grows so what it asks for fits: hiding it
            // behind a "see details" is inviting approval without reading.
            height: pending ? 64 + asks.length * 17 : 46
            radius: 8
            color: pending ? "#fdf6ec" : (row.containsMouse ? "#f6f1ec" : "transparent")
            border.color: pending ? "#e0a96d" : "transparent"

            readonly property var cols: root.fields(modelData)
            readonly property bool quarantined: cols[2] === "en cuarentena"
            readonly property bool pending: cols[2] === "pendiente de aprobación"
            // What is left to approve, one line per thing.
            readonly property var asks: cols.length > 4 && cols[4] !== "" ? cols[4].split(" · ") : []
            // One that was already running and now asks for more is not a new one.
            readonly property bool wantsMore: cols.length > 5 && cols[5] === "mas"
            // How its account stands: "", "missing", "connected" or "expired".
            readonly property string account: cols.length > 6 ? cols[6] : ""
            readonly property string accountHost: cols.length > 7 ? cols[7] : ""

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
                    // The account: what it lacks or what it has. It only
                    // appears on one that already runs; before approval there
                    // is nothing to connect.
                    Label {
                        visible: !pending && account === "conectada"
                        text: "cuenta conectada"
                        font.pixelSize: 11
                        color: "#2e7d32"
                    }
                    Label {
                        visible: !pending && account === "vencida"
                        text: "la cuenta caducó"
                        font.pixelSize: 11
                        color: "#a05a2c"
                    }
                    Button {
                        visible: !pending && !quarantined && (account === "falta" || account === "vencida")
                        text: account === "vencida" ? "Reconectar" : "Conectar cuenta"
                        onClicked: Chilpa.connectAccount(cols[0])
                    }
                    Button {
                        visible: !pending && account === "conectada"
                        text: "Desconectar"
                        onClicked: Chilpa.disconnectAccount(cols[0])
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

                // What it asks for, which is what is being approved.
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
