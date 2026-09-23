// La cola, con la pista que suena marcada.
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
            color: index === Chilpa.queueIndex ? "#f3e7e7" : "transparent"
            Label {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.leftMargin: 8
                anchors.right: parent.right
                elide: Text.ElideRight
                text: (index + 1) + ".  " + modelData
                color: index === Chilpa.queueIndex ? "#d7272b" : "#411d1f"
            }
        }
    }
}
