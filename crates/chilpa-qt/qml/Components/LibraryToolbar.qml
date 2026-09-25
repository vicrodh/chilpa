import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// Under the Library's columns: play, shuffle or queue what the filter
// leaves, how much it is, and tracks or records.
Item {
    id: bar

    property TrackListModel tracks

    implicitHeight: Metrics.pillHeight

    Row {
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8

        PillButton {
            kind: "primary"
            glyph: Icons.playerPlayFilled
            filled: true
            text: I18n.tr("Play", I18n.rev)
            onClicked: bar.tracks.playAll(false)
        }
        PillButton {
            glyph: Icons.arrowsShuffle
            text: I18n.tr("Shuffle", I18n.rev)
            onClicked: bar.tracks.playAll(true)
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Add to the queue", I18n.rev)
            glyph: Icons.playlistAdd
            round: true
            size: Metrics.pillHeight
            onClicked: bar.tracks.queueAll()
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: Metrics.gap8
            text: bar.tracks.summary
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }

    SegmentedControl {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        options: [
            { glyph: Icons.list, tip: I18n.tr("Tracks", I18n.rev) },
            { glyph: Icons.layoutGrid, tip: I18n.tr("Records", I18n.rev) }
        ]
        current: SettingsModel.libraryView === "grid" ? 1 : 0
        onActivated: (index) => SettingsModel.libraryView = index === 1 ? "grid" : "tracks"
    }
}
