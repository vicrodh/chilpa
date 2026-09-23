// Los artistas de la biblioteca.
//
// Un clic filtra la reja de álbumes a ese artista. La portada es la de uno
// de sus discos: una inicial en un cuadro gris no le dice nada a nadie. Va
// cuadrada, como los discos: redondearla pide una máscara y un efecto más
// para no ganar nada.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

GridView {
    id: grid
    clip: true
    cellWidth: 160
    cellHeight: 216
    model: Chilpa.artists

    // Cada renglón viene como "nombre, discos, pistas, portada".
    function fields(line) {
        return line.split("\t")
    }

    delegate: Item {
        width: grid.cellWidth - 8
        height: grid.cellHeight - 8

        readonly property var cols: grid.fields(modelData)

        ColumnLayout {
            anchors.fill: parent
            spacing: 6

            Rectangle {
                Layout.preferredWidth: parent.width
                Layout.preferredHeight: parent.width
                radius: 8
                color: "#f0eae4"
                Image {
                    anchors.fill: parent
                    source: cols[3] ? "file://" + cols[3] : ""
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    visible: cols[3] !== ""
                }
                Label {
                    anchors.centerIn: parent
                    text: "sin portada"
                    font.pixelSize: 11
                    color: "#7a411d1f"
                    visible: cols[3] === ""
                }
            }

            Label {
                Layout.fillWidth: true
                text: cols[0]
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
                font.pixelSize: 13
                font.weight: Font.DemiBold
                color: "#411d1f"
            }
            Label {
                Layout.fillWidth: true
                text: cols[1] + (cols[1] === "1" ? " disco · " : " discos · ") +
                      cols[2] + (cols[2] === "1" ? " pista" : " pistas")
                horizontalAlignment: Text.AlignHCenter
                font.pixelSize: 11
                color: "#7a411d1f"
            }
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: Chilpa.openArtistAlbums(cols[0])
        }
    }
}
