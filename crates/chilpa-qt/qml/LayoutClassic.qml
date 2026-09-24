// The classic layout: everything in one column, top to bottom.
//
// A layout has no logic: it chooses which panels are seen and where.
// Everything they need they read from the bridge, so adding another layout
// is adding a `Layout<name>.qml` next to this one and choosing it in the
// preferences.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.blitzfc.chilpa

ColumnLayout {
    anchors.fill: parent
    anchors.margins: 20
    spacing: 14

    OutputPanel { Layout.fillWidth: true }

    ActionsPanel { Layout.alignment: Qt.AlignLeft }

    TransportPanel { Layout.alignment: Qt.AlignLeft }

    // What is playing and, beside it, its lyrics when there are any.
    //
    // With a minimum, not just a preferred: when the window runs short, a
    // ColumnLayout shrinks what it can, and this card shrunk eats the
    // clock.
    RowLayout {
        Layout.fillWidth: true
        Layout.preferredHeight: 152
        Layout.minimumHeight: 152
        Layout.alignment: Qt.AlignTop
        spacing: 14

        NowPlayingPanel { Layout.fillWidth: true; Layout.fillHeight: true }

        LyricsPanel {
            Layout.preferredWidth: 300
            Layout.fillHeight: true
            visible: Chilpa.lyrics.length > 0
        }
    }

    VisualizerPanel {
        Layout.fillWidth: true
        Layout.preferredHeight: 110
        Layout.minimumHeight: 110
        visible: Chilpa.playing
    }

    // The results, when there is something typed in the search or an album
    // open.
    Results {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.minimumHeight: 200
        visible: Chilpa.searchText !== "" || Chilpa.openAlbum !== ""
    }

    Library {
        Layout.fillWidth: true
        Layout.fillHeight: true
        // A row of covers with their text lines and, above, the tabs. Below
        // this the last line of each card is clipped, which is the first thing
        // that can be lost without getting in the way.
        Layout.minimumHeight: 260
        // It only hides when there are results or an open album taking its
        // place. It used to hide with an empty library too, and that left its
        // other tabs with no way in: on a fresh install there was no way to
        // reach approving a plugin (ADR-008) without scanning music first.
        visible: Chilpa.searchText === "" && Chilpa.openAlbum === ""
    }

    // The queue only shows itself when there is more than one track.
    QueuePanel {
        Layout.fillWidth: true
        Layout.preferredHeight: 150
        // The queue is what gives first when the window runs short: it can
        // always be scrolled inside.
        Layout.minimumHeight: 60
        visible: Chilpa.queue.length > 1
    }

    // With no queue and no library, this filler takes the leftover space
    // and everything else stays at the top. An invisible item takes no room
    // in a Layout, so it vanishes as soon as there is something to show.
    Item {
        Layout.fillHeight: true
        visible: Chilpa.queue.length <= 1 && Chilpa.albums.length === 0
    }
}
