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
    property AlbumGridModel albums
    property string summary
    property bool albumless: false
    signal albumlessToggled()
    // The columns the list of records shows, and the ones chosen at the gear.
    property var columns: []
    signal columnsChosen(var columns)

    readonly property bool wrapped: actions.unwrappedWidth + loose.implicitWidth + tally.implicitWidth + (viewTools.visible ? viewTools.implicitWidth : 0) + Metrics.gap8 * 3 > width
    readonly property bool separateCount: wrapped && loose.implicitWidth + tally.implicitWidth + Metrics.gap8 > width
    readonly property bool separateViews: wrapped && viewTools.visible
        && (separateCount || loose.implicitWidth + tally.implicitWidth + viewTools.implicitWidth + Metrics.gap8 * 2 > width)
    implicitHeight: Math.max(actions.height, loose.y + loose.height, tally.y + tally.height, viewTools.visible ? viewTools.y + viewTools.height : 0)

    Flow {
        id: actions
        objectName: "libraryActions"
        readonly property real unwrappedWidth: play.implicitWidth + shuffle.implicitWidth + queueButton.width + Metrics.gap8 * 2 // derived: two gaps for the base controls
            + (!bar.albumless ? randomAlbum.width + spacing : 0) // derived: one gap per adjoining control
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
            id: randomAlbum
            objectName: "libraryRandomAlbum"
            visible: !bar.albumless
            enabled: bar.albums && bar.albums.ready && bar.albums.albums > 0 && !QueueActivity.busy
            tip: I18n.tr("Random album", I18n.rev)
            glyph: Lucide.dices; iconFamily: Lucide.font; round: true; size: Metrics.pillHeight
            onClicked: bar.albums.queueCollection("random-album", false, 0, I18n.tr("Library", I18n.rev))
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
        id: tally
        objectName: "libraryTrackSummary"
        x: bar.separateCount ? 0 : loose.x + loose.width + Metrics.gap8
        y: bar.separateCount ? loose.y + loose.height + Metrics.gap8 : loose.y + (loose.height - height) / 2
        width: Math.max(0, (!viewTools.visible || bar.separateViews ? bar.width : viewTools.x) - x - Metrics.gap8)
        verticalAlignment: Text.AlignVCenter
        text: bar.summary
        elide: Text.ElideRight
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }

    QueueMenu {
        id: queueMenu
        model: bar.tracks
        laterLabel: I18n.tr("Library selection", I18n.rev)
    }

    AlbumViewTools {
        id: viewTools
        objectName: "libraryViewTools"
        visible: !bar.albumless
        y: bar.separateViews ? Math.max(loose.y + loose.height, tally.y + tally.height) + Metrics.gap8 : loose.y
        anchors.right: parent.right
        view: SettingsModel.libraryView
        columns: bar.columns
        onColumnsChosen: (columns) => bar.columnsChosen(columns)
        onViewChosen: (view) => SettingsModel.libraryView = view
    }
}
