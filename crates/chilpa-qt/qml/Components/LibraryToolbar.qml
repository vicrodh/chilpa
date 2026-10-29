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
    property bool albumless: false
    signal albumlessToggled()
    // The columns the list of records shows, and the ones chosen at the gear.
    property var columns: []
    signal columnsChosen(var columns)

    readonly property bool wrapped: actions.unwrappedWidth + loose.implicitWidth + (viewTools.visible ? viewTools.implicitWidth : 0) + Metrics.gap8 * 3 > width
    readonly property bool separateViews: wrapped && viewTools.visible && loose.implicitWidth + viewTools.implicitWidth + Metrics.gap8 > width
    implicitHeight: Math.max(actions.height, loose.y + loose.height, viewTools.visible ? viewTools.y + viewTools.height : 0)

    Flow {
        id: actions
        objectName: "libraryActions"
        readonly property real unwrappedWidth: play.implicitWidth + shuffle.implicitWidth + queueButton.width + Metrics.gap8 * 2 // derived: three controls and two gaps
        width: Math.min(unwrappedWidth, bar.width)
        spacing: Metrics.gap8

        PillButton {
            id: play
            kind: "primary"
            glyph: Icons.playerPlayFilled
            filled: true
            text: I18n.tr("Play", I18n.rev)
            onClicked: bar.tracks.playAll(false)
        }
        PillButton {
            id: shuffle
            glyph: Icons.arrowsShuffle
            text: I18n.tr("Shuffle", I18n.rev)
            onClicked: bar.tracks.playAll(true)
        }
        IconButton {
            id: queueButton
            tip: I18n.tr("Queue options", I18n.rev)
            glyph: Icons.playlistAdd
            round: true
            size: Metrics.pillHeight
            onClicked: queueMenu.show(queueButton.x, queueButton.y + queueButton.height + Metrics.gap4)
        }
    }
    PillButton {
        id: loose
        objectName: "albumlessToggle"
        x: bar.wrapped ? 0 : actions.width + Metrics.gap8
        y: bar.wrapped ? actions.height + Metrics.gap8 : 0
        text: I18n.tr("No album tracks", I18n.rev)
        kind: bar.albumless ? "primary" : "secondary"
        onClicked: bar.albumlessToggled()
    }
    Text {
        objectName: "libraryTrackSummary"
        x: loose.x + loose.width + Metrics.gap8
        y: loose.y
        height: Metrics.pillHeight
        width: Math.max(0, (!viewTools.visible || bar.separateViews ? bar.width : viewTools.x) - x - Metrics.gap8)
        verticalAlignment: Text.AlignVCenter
        text: bar.tracks.summary
        elide: Text.ElideRight
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }

    QueueMenu {
        id: queueMenu
        model: bar.tracks
        laterLabel: I18n.tr("Library selection", I18n.rev)
    }

    Row {
        id: viewTools
        objectName: "libraryViewTools"
        visible: !bar.albumless
        y: bar.separateViews ? loose.y + loose.height + Metrics.gap8 : loose.y
        anchors.right: parent.right
        spacing: Metrics.gap8
        AlbumColumnsGear {
            visible: SettingsModel.libraryView === "tracks"
            anchors.verticalCenter: parent.verticalCenter
            shown: bar.columns
            onChosen: (columns) => bar.columnsChosen(columns)
        }
        SegmentedControl {
            objectName: "libraryViewSwitcher"
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
}
