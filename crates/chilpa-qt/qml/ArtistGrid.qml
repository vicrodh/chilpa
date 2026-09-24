// The library's artists.
//
// A click filters the album grid to that artist. The cover is one of their
// albums': an initial on a grey square tells nobody anything. It is square,
// like the albums: rounding it asks for a mask and one more effect to gain
// nothing.
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

    // Each line comes as "name, albums, tracks, cover".
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
                    text: Chilpa.translate("no cover", Chilpa.trRev)
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
                text: Chilpa.translateCount("{} album", "{} albums", parseInt(cols[1]), Chilpa.trRev) +
                      " · " +
                      Chilpa.translateCount("{} track", "{} tracks", parseInt(cols[2]), Chilpa.trRev)
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
