// La cola, con la pista que suena marcada.
//
// Una pista que todavía no suena se puede quitar; la que suena no, que para
// eso está el botón de siguiente. Quitarla no la saca del renglón: se apaga
// y se tacha, así la lista no brinca debajo de la vista.
import QtQuick
import QtQuick.Controls
import com.blitzfc.chilpa

Rectangle {
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"

    ListView {
        id: lista
        anchors.fill: parent
        anchors.margins: 10
        clip: true
        model: Chilpa.queue
        currentIndex: Chilpa.queueIndex
        // Al avanzar la cola, la pista que suena se queda a la vista.
        onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)

        delegate: Rectangle {
            width: lista.width
            height: 26
            radius: 6
            color: index === Chilpa.queueIndex
                ? "#f3e7e7"
                : (raton.containsMouse ? "#f8f4f0" : "transparent")

            readonly property bool quitada: Chilpa.queueRemoved[index] === "1"
            readonly property bool sonando: index === Chilpa.queueIndex

            MouseArea {
                id: raton
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.NoButton
            }

            Label {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.leftMargin: 8
                anchors.right: quitar.left
                anchors.rightMargin: 6
                elide: Text.ElideRight
                text: (index + 1) + ".  " + modelData
                font.strikeout: parent.quitada
                opacity: parent.quitada ? 0.45 : 1
                color: parent.sonando ? "#d7272b" : "#411d1f"
            }

            Label {
                id: quitar
                anchors.verticalCenter: parent.verticalCenter
                anchors.right: parent.right
                anchors.rightMargin: 8
                text: "quitar"
                font.pixelSize: 11
                color: "#7a411d1f"
                visible: raton.containsMouse && !parent.quitada
                    && index > Chilpa.queueIndex
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Chilpa.removeFromQueue(index)
                }
            }
        }
    }
}
