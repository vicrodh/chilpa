import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// Under the Library's columns: play, shuffle or queue what the filter
// leaves, how much it is, and tracks or records.
Item {
    id: bar

    property TrackListModel tracks
    // The columns the list of records shows, and the ones chosen at the gear.
    property var columns: []
    signal columnsChosen(var columns)

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
            id: queueButton
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Queue options", I18n.rev)
            glyph: Icons.playlistAdd
            round: true
            size: Metrics.pillHeight
            onClicked: queueMenu.show(queueButton.x, queueButton.y + queueButton.height + Metrics.gap4)
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            leftPadding: Metrics.gap8
            text: bar.tracks.summary
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }

    QueueMenu {
        id: queueMenu
        model: bar.tracks
        laterLabel: I18n.tr("Library selection", I18n.rev)
    }

    AlbumColumnsGear {
        visible: SettingsModel.libraryView === "tracks"
        anchors.right: views.left
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        shown: bar.columns
        onChosen: (columns) => bar.columnsChosen(columns)
    }
    SegmentedControl {
        id: views
        objectName: "libraryViewSwitcher"
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        options: [
            { glyph: Icons.list, tip: I18n.tr("List", I18n.rev) },
            { glyph: Icons.layoutGrid, tip: I18n.tr("Grid", I18n.rev) },
            { glyph: Lucide.glasses, iconFamily: Lucide.font, tip: I18n.tr("Nerdy", I18n.rev) }
        ]
        current: SettingsModel.libraryView === "nerdy" ? 2 : SettingsModel.libraryView === "grid" ? 1 : 0
        onActivated: (index) => SettingsModel.libraryView = index === 2 ? "nerdy" : index === 1 ? "grid" : "tracks"
    }
}
