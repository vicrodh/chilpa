// The search results.
//
// A plain list: title, artist and duration. A click plays from there,
// queueing what follows, which is what is expected of a search. A middle
// click queues without cutting off what is playing.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

Rectangle {
    id: root
    radius: 14
    color: "#ffffff"
    border.color: "#1a411d1f"


    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 10
        spacing: 8

        // The header only appears when an album has been opened.
        RowLayout {
            Layout.fillWidth: true
            visible: Chilpa.openAlbum !== ""
            spacing: 12

            Rectangle {
                Layout.preferredWidth: 64
                Layout.preferredHeight: 64
                radius: 8
                color: "#f0eae4"
                visible: Chilpa.openAlbumArt !== ""
                Image {
                    anchors.fill: parent
                    source: Chilpa.openAlbumArt
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Label {
                    Layout.fillWidth: true
                    text: Chilpa.openAlbum
                    font.pixelSize: 16
                    font.weight: Font.DemiBold
                    color: "#411d1f"
                    elide: Text.ElideRight
                }
                Label {
                    Layout.fillWidth: true
                    visible: Chilpa.openAlbumArtist !== ""
                    text: Chilpa.openAlbumArtist
                    font.pixelSize: 13
                    color: "#411d1f"
                    elide: Text.ElideRight
                }
                Label {
                    Layout.fillWidth: true
                    text: Chilpa.openAlbumLine
                    font.pixelSize: 11
                    color: "#7a411d1f"
                    elide: Text.ElideRight
                }
            }
            Button {
                text: Chilpa.translate("Play", Chilpa.trRev)
                onClicked: Chilpa.playAlbum(Chilpa.openAlbumId)
            }
            Button {
                text: Chilpa.translate("Queue", Chilpa.trRev)
                onClicked: Chilpa.enqueueAlbum(Chilpa.openAlbumId)
            }
            Button {
                text: Chilpa.translate("Close", Chilpa.trRev)
                onClicked: Chilpa.closeAlbum()
            }
        }

        // What a source could not answer is said, not hidden: a short list
        // with no explanation looks like a poor library.
        Label {
            Layout.fillWidth: true
            visible: Chilpa.searchProblems !== ""
            text: Chilpa.translate("No answer from", Chilpa.trRev) + ": " + Chilpa.searchProblems
            font.pixelSize: 11
            color: "#a05a2c"
            elide: Text.ElideRight
        }

    ListView {
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: Chilpa.results

        delegate: Rectangle {
            width: ListView.view.width
            height: 30
            radius: 6
            color: row.containsMouse ? "#f7f1ec" : "transparent"

            readonly property var cols: modelData.split("\t")
            // Columns 6 to 8 only come from the search; on opening an album
            // the row is shorter and these are left empty.
            readonly property string sourceName: cols.length > 6 ? cols[6] : ""
            readonly property string alsoIn: cols.length > 7 ? cols[7] : ""
            // What cannot be played yet is shown greyed instead of
            // pretending a click will make a sound.
            readonly property bool playable: cols.length > 8 ? cols[8] === "1" : true

            opacity: playable ? 1.0 : 0.55

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 10

                Label {
                    Layout.preferredWidth: parent.width * 0.30
                    text: cols[1]
                    elide: Text.ElideRight
                    color: "#411d1f"
                }
                Label {
                    Layout.preferredWidth: parent.width * 0.20
                    text: cols[2]
                    elide: Text.ElideRight
                    color: "#7a411d1f"
                }
                Label {
                    Layout.fillWidth: true
                    text: cols[3]
                    elide: Text.ElideRight
                    color: "#7a411d1f"
                }
                // What the analysis found. It only shows once analysed: an
                // empty column tells nobody anything.
                Label {
                    Layout.preferredWidth: parent.width * 0.12
                    horizontalAlignment: Text.AlignRight
                    text: cols.length > 5 ? cols[5] : ""
                    elide: Text.ElideRight
                    font.pixelSize: 11
                    color: "#7a411d1f"
                }
                // Where it came from, and which other sources hold the same
                // song. It is only drawn if more than one is in play.
                Label {
                    visible: alsoIn !== "" || (sourceName !== "" && sourceName !== Chilpa.translate("My library", Chilpa.trRev))
                    text: alsoIn !== "" ? sourceName + " + " + alsoIn : sourceName
                    font.pixelSize: 11
                    elide: Text.ElideRight
                    Layout.preferredWidth: parent.width * 0.18
                    color: "#7a411d1f"
                }
                Label {
                    text: cols[4]
                    color: "#7a411d1f"
                }
            }

            MouseArea {
                id: row
                anchors.fill: parent
                hoverEnabled: true
                // Over what cannot be played, the cursor does not promise a
                // click that will do nothing.
                cursorShape: playable ? Qt.PointingHandCursor : Qt.ArrowCursor
                enabled: playable
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                onClicked: (event) => {
                    if (event.button === Qt.MiddleButton)
                        Chilpa.enqueueResults(index)
                    else
                        Chilpa.playResults(index)
                }
            }
        }
    }
    }
}
