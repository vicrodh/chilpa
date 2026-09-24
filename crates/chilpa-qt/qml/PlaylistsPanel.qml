// The saved lists.
//
// Each one says where it comes from. Today they are all from here; when a
// plugin brings its own, they will show in the same list and can be
// separated by that column, which is the whole point.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    id: root
    spacing: 8

    // Each line comes as "id, name, provider, tracks".
    function fields(line) {
        return line.split("\t")
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        TextField {
            id: name
            Layout.preferredWidth: 220
            placeholderText: "Nombre de la lista"
            onAccepted: root.save()
        }
        Button {
            text: "Guardar la cola"
            enabled: Chilpa.queue.length > 0 && name.text.trim() !== ""
            onClicked: root.save()
        }
        Item { Layout.fillWidth: true }
    }

    function save() {
        if (name.text.trim() === "") return
        Chilpa.saveQueueAs(name.text)
        name.text = ""
    }

    // The analysis: what fills the tempo lists.
    RowLayout {
        Layout.fillWidth: true
        spacing: 8

        Button {
            text: Chilpa.analyzing ? "Detener el análisis" : "Analizar la biblioteca"
            onClicked: Chilpa.analyzing ? Chilpa.cancelAnalysis() : Chilpa.analyzeLibrary()
        }
        Rectangle {
            visible: Chilpa.analyzing
            Layout.preferredWidth: 140
            Layout.preferredHeight: 6
            radius: 3
            color: "#e6ddd5"
            Rectangle {
                height: parent.height
                width: parent.width * Math.max(0, Math.min(1, Chilpa.analysisProgress))
                radius: parent.radius
                color: "#d7272b"
            }
        }
        Label {
            Layout.fillWidth: true
            text: Chilpa.analysisStatus
            elide: Text.ElideRight
            font.pixelSize: 11
            color: "#7a411d1f"
        }
    }

    // The smart ones first: they are not saved, they are resolved again
    // every time they are looked at, so they always carry what qualifies
    // today.
    Label {
        text: "Inteligentes"
        font.pixelSize: 12
        font.weight: Font.DemiBold
        color: "#7a411d1f"
    }

    Flow {
        Layout.fillWidth: true
        spacing: 8

        Repeater {
            model: Chilpa.smartLists
            delegate: Button {
                readonly property var cols: modelData.split("\t")
                text: cols[0] + "  (" + cols[1] + ")"
                enabled: cols[1] !== "0"
                onClicked: Chilpa.playSmartList(cols[0])
            }
        }
    }

    Label {
        Layout.topMargin: 6
        visible: Chilpa.genres.length > 0
        text: "Géneros"
        font.pixelSize: 12
        font.weight: Font.DemiBold
        color: "#7a411d1f"
    }

    Flow {
        Layout.fillWidth: true
        spacing: 8
        visible: Chilpa.genres.length > 0

        Repeater {
            model: Chilpa.genres
            delegate: Button {
                readonly property var cols: modelData.split("\t")
                text: cols[0] + "  (" + cols[1] + ")"
                onClicked: Chilpa.playGenre(cols[0])
            }
        }
    }

    Label {
        Layout.topMargin: 6
        text: "Guardadas"
        font.pixelSize: 12
        font.weight: Font.DemiBold
        color: "#7a411d1f"
    }

    Label {
        visible: Chilpa.playlists.length === 0
        text: "Todavía no hay listas guardadas"
        color: "#7a411d1f"
    }

    ListView {
        id: list
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: Chilpa.playlists

        delegate: Rectangle {
            width: list.width
            height: 40
            radius: 8
            color: row.containsMouse ? "#f6f1ec" : "transparent"

            readonly property var cols: root.fields(modelData)

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
                        text: cols[1]
                        elide: Text.ElideRight
                        font.pixelSize: 13
                        color: "#411d1f"
                    }
                    Label {
                        text: cols[3] + (cols[3] === "1" ? " pista · " : " pistas · ") + cols[2]
                        font.pixelSize: 10
                        color: "#7a411d1f"
                    }
                }
                Button {
                    text: "Reproducir"
                    onClicked: Chilpa.playSavedPlaylist(parseInt(cols[0]))
                }
                Button {
                    text: "Borrar"
                    onClicked: Chilpa.deletePlaylist(parseInt(cols[0]))
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
