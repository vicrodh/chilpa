import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.pages
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/playlists/qml/Components"
import "../Components"
import "../Patterns"

GridView {
    id: page
    required property string source
    property bool leaving: false
    readonly property bool activePage: visible && !leaving
    property string document: "{}"
    function refreshDocument() {
        if (visible && !leaving) document = Sources.page(source, Library.remoteRev + Sources.revision) || "{}"
    }
    onSourceChanged: refreshDocument()
    onVisibleChanged: refreshDocument()
    onLeavingChanged: refreshDocument()
    Connections { target: Library; function onRemoteRevChanged() { page.refreshDocument() } }
    Connections { target: Sources; function onRevisionChanged() { page.refreshDocument() } }
    readonly property var about: JSON.parse(document)
    Component.onCompleted: refreshDocument()
    readonly property int columns: Math.max(1, CardLayout.columns(width - Metrics.contentX * 2))
    leftMargin: Metrics.contentX; rightMargin: Metrics.contentX
    topMargin: Metrics.contentTop; bottomMargin: Metrics.contentBottom
    cellWidth: Math.max(1,(width - Metrics.contentX * 2) / columns) // derived: padding on both sides
    cellHeight: cellWidth + Metrics.cardTextGap + Metrics.cardTextHeight + Metrics.gap16
    cacheBuffer: height * 0.5
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}
    KeyedModel { id: rows; items: page.about.items || []; keyField: "ref" }
    model: rows
    TrackListModel { id: tracks; objectName: "collectionTracks"; kind: page.activePage ? "source" : ""; key: page.source; filterRev: Library.remoteRev; limit: 100; playingPath: Player.path }
    AlbumMenu { id: albumsMenu; canExpand:false }
    TrackMenu { id: tracksMenu; model:tracks }
    SourcePlaylistEditor { id: playlistEditor }
    Timer {
        interval: Math.max(250,Math.min(60000,page.about.refresh_after_ms || page.about.retry_after_ms || 1000))
        running: page.activePage && page.about.updating === true && page.about.request_pending !== true
        repeat: true
        onTriggered: NativePages.revalidate(page.source,false)
    }
    function refreshCollection() {
        const base=about.base_key || source
        const key=NativePages.requestKey(base,JSON.stringify({filters:about.filters || {},order:about.order || "added"}))
        if (!key) return
        if (key !== source) Navigation.goKey("source",key)
        NativePages.revalidate(key,true)
    }
    function openItem(item) {
        if(item.kind === "track") {
            const row = item._host?.track_row ?? -1
            if(tracks.ready && row >= 0 && row < tracks.count && item.available !== false) tracks.play(row)
        } else if(["album","playlist"].includes(item.kind) && item._host?.card_id) Navigation.go("album",item._host.card_id)
        else if(item.ref) Navigation.goKey("source",(page.about.plugin || "") + " native:" + item.ref)
    }
    function showMenu(item,x,y) {
        if(item.kind === "track") {
            const row=item._host?.track_row ?? -1
            if(tracks.ready && row >= 0 && row < tracks.count) tracksMenu.show(row,x,y)
        } else if(["album","playlist"].includes(item.kind) && item._host?.card_id) albumsMenu.show(item._host.card_id,item.title || "",item.artist || "",0,x,y)
    }
    header: Column {
        x: Metrics.contentX
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        spacing: Metrics.gap16
        Text {
            text: page.about.title || I18n.tr("Library",I18n.rev)
            textFormat: Text.PlainText
            font.pointSize: Type.title; font.weight:Type.bold
            color:Theme.color.textPrimary
        }
        CollectionControls { width:parent.width; source:page.source; about:page.about }
        Flow {
            width: parent.width; spacing:Metrics.gap8
            PillButton { text:I18n.tr("Refresh",I18n.rev); glyph:Icons.refresh; onClicked:page.refreshCollection() }
            PillButton { visible:page.about.playlist_edit === true; text:I18n.tr("New playlist",I18n.rev); onClicked:playlistEditor.begin({plugin:page.about.plugin},"create") }
            BusyIndicator { width:Metrics.button28;height:width;visible:page.about.updating===true;running:visible&&page.activePage }
            Text { visible:page.about.sync_loaded!==undefined&&page.about.sync_loaded!==null;text: I18n.trCount("{} item","{} items",page.about.sync_loaded||0,I18n.rev);color:Theme.color.textSecondary;font.pointSize:Type.small }
        }
        Text { width:parent.width; visible:!!page.about.error; text:page.about.error || "";textFormat:Text.PlainText;wrapMode:Text.Wrap;color:Theme.color.textSecondary }
        PillButton { visible:!!page.about.stale_cursor&&!!page.about.restart_key; text:I18n.tr("Reload",I18n.rev);onClicked:Navigation.goKey("source",page.about.restart_key) }
    }
    delegate: Item {
        id: cell
        required property var entry
        width:page.cellWidth;height:page.cellHeight
        readonly property bool inViewport: y+height>page.contentY&&y<page.contentY+page.height
        CollectionTile {
            width:parent.width-Metrics.gridGap
            entry:cell.entry
            plugin:page.about.plugin || ""
            active:page.activePage&&cell.inViewport
            favorites:page.about.actions?.favorite===true
            playable:entry.kind==="track"?tracks.ready&&entry._host?.track_row>=0:["album","playlist"].includes(entry.kind)
            onOpened:page.openItem(entry)
            onPlayRequested:entry.kind==="track"?page.openItem(entry):AlbumActions.play(entry._host.card_id,false)
            onMenuRequested:(x,y)=>{const at=mapToItem(page,x,y);page.showMenu(entry,at.x,at.y)}
        }
    }
    footer: Column {
        width:page.width-Metrics.contentX*2 // derived: padding on both sides
        spacing:Metrics.gap16
        Text { visible:!page.about.loading&&!page.about.updating&&rows.count===0&&!page.about.error;text:I18n.tr("No results",I18n.rev);color:Theme.color.textSecondary }
        BusyIndicator { visible:page.about.loading===true;width:Metrics.button32;height:width;running:visible&&page.activePage }
        PillButton { objectName:"collectionNextPage";visible:!!page.about.next_key;text:I18n.tr("Next page",I18n.rev);onClicked:Navigation.goKey("source",page.about.next_key) }
    }
}
