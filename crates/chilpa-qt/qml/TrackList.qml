// A track list with its count. The tops and the history use it.
//
// A double click starts it playing: it is no use seeing what is played most
// if it then has to be hunted down somewhere else.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    id: root
    spacing: 6

    property string title: ""
    /// Lines of "title, artist, album, count, path".
    property var tracks: []
    /// Whether the play count is shown.
    property bool withPlayCounts: true

    Label {
        text: root.title
        font.pixelSize: 13
        font.weight: Font.DemiBold
        color: "#411d1f"
    }

    ListView {
        id: list
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: root.tracks

        delegate: Item {
            width: list.width
            height: 34

            readonly property var cols: modelData.split("\t")

            RowLayout {
                anchors.fill: parent
                anchors.rightMargin: 6
                spacing: 8

                Label {
                    Layout.preferredWidth: 22
                    text: (index + 1) + "."
                    font.pixelSize: 11
                    color: "#7a411d1f"
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Label {
                        Layout.fillWidth: true
                        text: cols[0]
                        elide: Text.ElideRight
                        font.pixelSize: 12
                        color: "#411d1f"
                    }
                    Label {
                        Layout.fillWidth: true
                        text: [cols[1], cols[2]].filter(t => t !== "").join(" · ")
                        elide: Text.ElideRight
                        font.pixelSize: 10
                        color: "#7a411d1f"
                    }
                }
                Label {
                    visible: root.withPlayCounts
                    text: cols[3] + (cols[3] === "1" ? " escucha" : " escuchas")
                    font.pixelSize: 11
                    color: "#7a411d1f"
                }
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onDoubleClicked: Chilpa.play(cols[4])
            }
        }
    }
}
