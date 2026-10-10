import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// A block of tracks on an artist's page: Popular Tracks, or Appears On
// when the source lists it as tracks. Five rows first (or as many as the
// section asks: ten for what the listener keeps) and "Load more" opens the rest, in the window, with nothing fetched; "View less" folds
// them back. Popular Tracks has Play and a menu of placements for all of
// them; Appears On has neither (QBZ).
Column {
    id: block
    property var section: ({})
    property bool actions: true
    property bool expanded: false
    readonly property string modelKind: section.model || "source"
    readonly property int first: section.preview || Metrics.previewRows
    readonly property int preview: expanded ? 0 : first
    readonly property bool shown: tracks.count > 0
    spacing: Metrics.gap12
    visible: shown
    TrackListModel { id: tracks; playingPath: Player.path; kind: block.modelKind; key: block.section.model_key || block.section.key || ""; limit: block.preview; filterRev: Library.remoteRev + Library.remoteArtRev }
    TrackListModel { id: all; kind: block.modelKind; key: block.section.model_key || block.section.key || ""; filterRev: Library.remoteRev + Library.remoteArtRev }
    Item {
        width: parent.width; height: Metrics.shelfHeader
        Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: block.section.title || ""; textFormat: Text.PlainText; font.pointSize: Type.section; font.weight: Type.heading; font.letterSpacing: Type.sectionTracking; color: Theme.color.textPrimary }
        Row {
            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap8; visible: block.actions
            IconButton { objectName: "playTracks"; round: true; size: Metrics.button32; glyph: Icons.playerPlayFilled; filled: true; tip: I18n.tr("Play", I18n.rev); onClicked: all.playAll(false) }
            IconButton {
                id: menuButton; objectName: "tracksMenu"; round: true; size: Metrics.button32; glyph: Icons.dots; tip: I18n.tr("Queue actions", I18n.rev)
                onClicked: menu.open()
                Menu {
                    id: menu; objectName: "tracksMenuPopup"; y: parent.height + Metrics.gap4; x: parent.width - width
                    entries: [
                        { text: I18n.tr("Play all next", I18n.rev), act: "next", glyph: Lucide.listStart, family: Lucide.font },
                        { text: I18n.tr("Play all later", I18n.rev), act: "later", glyph: Lucide.listPlus, family: Lucide.font },
                        { text: I18n.tr("Add all to queue", I18n.rev), act: "queue", glyph: Lucide.listEnd, family: Lucide.font },
                        { text: I18n.tr("Shuffle all", I18n.rev), act: "shuffle", glyph: Icons.arrowsShuffle },
                        { text: I18n.tr("Add all to playlist…", I18n.rev), act: "pick", glyph: Icons.playlistAdd }
                    ]
                    onChosen: (index) => {
                        switch (entries[index].act) {
                        case "next": all.playNextAll(); break
                        case "later": all.playLaterAll(block.section.title || ""); break
                        case "queue": all.queueAll(); break
                        case "shuffle": all.playAll(true); break
                        case "pick": all.selectAll(); picker.trackIdsJson = all.selectedTrackIdsJson(); all.clearSelection(); picker.show(menuButton.x, menuButton.y + menuButton.height); break
                        }
                    }
                }
            }
        }
    }
    TrackList { width: parent.width; height: implicitHeight; fitContent: true; model: tracks; header: false; showArtist: true; showAlbum: block.section.role !== "appears" }
    // All of them, or five again: the toggle is the page's, nothing is asked for.
    PillButton {
        objectName: "tracksMore"; visible: block.expanded || all.count > block.first; kind: "ghost"
        text: block.expanded ? I18n.tr("View less", I18n.rev) : I18n.tr("Load more", I18n.rev)
        onClicked: block.expanded = !block.expanded
    }
    PlaylistPicker { id: picker; parent: block }
}
