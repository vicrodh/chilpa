// What has been listened to.
//
// It all comes out of the play log, which is append-only: the numbers are
// added up again every time they are looked at, not kept in a counter that
// can drift out of sync without anyone noticing.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    id: root
    spacing: 10

    function fields(line) {
        return line.split("\t")
    }

    // The summary, in label and value lines.
    Flow {
        Layout.fillWidth: true
        spacing: 18

        Repeater {
            model: Chilpa.stats
            delegate: ColumnLayout {
                spacing: 0
                readonly property var cols: root.fields(modelData)
                Label {
                    text: cols[1]
                    font.pixelSize: 20
                    font.weight: Font.DemiBold
                    color: "#411d1f"
                }
                Label {
                    text: cols[0]
                    font.pixelSize: 11
                    color: "#7a411d1f"
                }
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 16

        TrackList {
            Layout.fillWidth: true
            Layout.fillHeight: true
            title: "Más escuchadas"
            tracks: Chilpa.topTracks
        }

        TrackList {
            Layout.fillWidth: true
            Layout.fillHeight: true
            title: "Lo último que sonó"
            tracks: Chilpa.recentTracks
            withPlayCounts: false
        }
    }
}
