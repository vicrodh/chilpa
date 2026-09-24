// The compact layout: the library keeps the window and what is playing lives
// in a bar at the bottom, like in a player of old.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    anchors.fill: parent
    anchors.margins: 16
    spacing: 10

    OutputPanel { Layout.fillWidth: true }

    ActionsPanel { Layout.alignment: Qt.AlignLeft }

    // The bulk of the window: what is being looked at and, to the right,
    // the lyrics of what is playing.
    RowLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        spacing: 10

        Results {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Chilpa.searchText !== "" || Chilpa.openAlbum !== ""
        }

        Library {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: Chilpa.searchText === "" && Chilpa.openAlbum === ""
        }

        LyricsPanel {
            Layout.preferredWidth: 300
            Layout.fillHeight: true
            visible: Chilpa.lyrics.length > 0
        }
    }

    QueuePanel {
        Layout.fillWidth: true
        Layout.preferredHeight: 110
        visible: Chilpa.queue.length > 1
    }

    // The bottom bar: transport and what is playing, on one line.
    RowLayout {
        Layout.fillWidth: true
        spacing: 10
        TransportPanel { Layout.alignment: Qt.AlignVCenter }
        NowPlayingPanel {
            Layout.fillWidth: true
            Layout.preferredHeight: 112
            Layout.minimumHeight: 112
            compact: true
        }
        VisualizerPanel {
            Layout.preferredWidth: 260
            Layout.preferredHeight: 112
            visible: Chilpa.playing
        }
    }
}
