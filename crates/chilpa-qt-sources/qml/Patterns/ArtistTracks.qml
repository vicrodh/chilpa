import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// A block of tracks on an artist's page: Popular Tracks, or Appears On
// when the source lists it as tracks. Five rows first (or as many as the
// section asks: ten for what the listener keeps). Load more reveals thirty
// cached rows at a time, then requests a page only on an explicit click.
// Popular Tracks has Play and a menu of placements for all of
// them; Appears On has neither (QBZ).
Column {
    id: block
    property var section: ({})
    property Flickable viewport: null
    property bool actions: true
    property int shownRows: first
    readonly property bool expanded: shownRows > first
    property bool initialized: false
    readonly property string memoryKey: (section.model || "source") + ":" + (section.model_key || section.key || "")
    function restore() {
        const saved = Visit.recall("artist.trackRows", {})
        shownRows = Math.max(first, Math.min(10000, saved[memoryKey] || first))
    }
    function remember() {
        // At most 128 section counts per window, released with Visit.
        const saved = Visit.recall("artist.trackRows", {})
        const keys = Object.keys(saved).filter(key => key !== memoryKey).slice(-127)
        const next = {}
        for (const key of keys) next[key] = saved[key]
        next[memoryKey] = shownRows
        Visit.keep("artist.trackRows", next)
    }
    onMemoryKeyChanged: if (initialized) restore()
    onShownRowsChanged: if (initialized) remember()
    Component.onCompleted: { restore(); initialized = true }
    function loadMore() {
        if (shownRows < all.count) shownRows = Math.min(all.count, shownRows + 30)
        else if (section.more) {
            shownRows = Math.min(10000, Math.max(shownRows, all.count) + 30)
            Sources.command("section_more", section.key || "", "")
        } else shownRows = first
    }
    readonly property string modelKind: section.model || "source"
    readonly property int first: section.preview || Metrics.previewRows
    readonly property int preview: shownRows
    readonly property bool shown: tracks.count > 0
    readonly property bool ready: tracks.ready && all.ready
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
                    id: menu; objectName: "tracksMenuPopup"; y: menuButton.height + Metrics.gap4; x: menuButton.width - width
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
    TrackList { width: parent.width; height: implicitHeight; fitContent: true; viewport: block.viewport; model: tracks; header: false; showArtist: true; showAlbum: block.section.role !== "appears" }
    Text { width: parent.width; visible: !!block.section.error; text: block.section.error || ""; textFormat: Text.PlainText; wrapMode: Text.WordWrap; font.pointSize: Type.body; color: Theme.color.statusNegative }
    PillButton {
        objectName: "tracksMore"; visible: block.expanded || all.count > block.first || !!block.section.more; kind: "ghost"
        enabled: block.section.loading !== true
        text: block.section.loading === true ? I18n.tr("Loading…", I18n.rev)
            : block.shownRows < all.count || block.section.more ? I18n.tr("Load more", I18n.rev) : I18n.tr("View less", I18n.rev)
        onClicked: block.loadMore()
    }
    PlaylistPicker { id: picker; parent: block }
}
