// Los resultados de la búsqueda.
//
// Una lista simple: título, artista y duración. Un clic reproduce desde ahí,
// encolando lo que sigue, que es como se espera de una búsqueda.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: raiz
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"

    property string deviceId: ""
    property bool bitPerfect: true

    ListView {
        anchors.fill: parent
        anchors.margins: 10
        clip: true
        model: Chilpa.results

        delegate: Rectangle {
            width: ListView.view.width
            height: 30
            radius: 6
            color: fila.containsMouse ? "#f7f1ec" : "transparent"

            readonly property var datos: modelData.split("\t")

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 10

                Label {
                    Layout.preferredWidth: parent.width * 0.45
                    text: datos[1]
                    elide: Text.ElideRight
                    color: "#411d1f"
                }
                Label {
                    Layout.preferredWidth: parent.width * 0.25
                    text: datos[2]
                    elide: Text.ElideRight
                    color: "#7a411d1f"
                }
                Label {
                    Layout.fillWidth: true
                    text: datos[3]
                    elide: Text.ElideRight
                    color: "#7a411d1f"
                }
                Label {
                    text: datos[4]
                    color: "#7a411d1f"
                }
            }

            MouseArea {
                id: fila
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: Chilpa.playResults(index, raiz.deviceId, raiz.bitPerfect)
            }
        }
    }
}
