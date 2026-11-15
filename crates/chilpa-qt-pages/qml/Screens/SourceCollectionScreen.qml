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
    property bool completed: false
    readonly property bool activePage: visible && !leaving
    readonly property var state: decode(collection.document)
    readonly property var about: state.header || ({})
    property var pendingAction: null
    property string actionError: ""
    property bool refreshPending: false
    property bool silentWork: false
    property bool manualRefreshPending: false
    property var favoriteAction: null
    property bool restorePending: true
    property double seenGeneration: 0
    readonly property var prepared: decode(collection.action)
    function decode(text) { try { return JSON.parse(text || "{}") } catch (_) { return {} } }
    function activate() {
        if (activePage) openCollection()
        else {
            pendingAction=null; refreshPending=false; favoriteAction=null; silentWork=false; manualRefreshPending=false
            albumsMenu.close(); tracksMenu.close()
            visible ? collection.retire() : collection.close()
        }
    }
    function openCollection() {
        restorePending=true;seenGeneration=0;pendingAction=null;actionError="";refreshPending=false;favoriteAction=null;silentWork=false;manualRefreshPending=false
        collection.open(source)
    }
    onSourceChanged: if (completed && activePage) openCollection()
    onActivePageChanged: if (completed) { activePage ? Qt.callLater(activate) : activate() }
    onVisibleChanged: if (completed && !visible) collection.close()
    Component.onCompleted: { completed=true; activate() }
    Component.onDestruction: collection.close()
    NativeCollectionModel { id:collection; objectName:"nativeCollectionModel"; active:page.activePage }
    PulseClock { slow:true; minimumInterval:100; running:page.activePage && collection.busy; onTriggered:collection.poll() }
    Timer {
        interval:Math.max(250,Math.min(60000,page.about.refresh_after_ms || page.about.retry_after_ms || 1000))
        running:page.activePage && !collection.busy && page.about.updating === true && page.about.request_pending !== true
        repeat:true
        onTriggered:page.refreshQuietly()
    }
    readonly property int columns: Math.max(1, CardLayout.columns(width - Metrics.contentX * 2))
    leftMargin:Metrics.contentX; rightMargin:Metrics.contentX
    topMargin:Metrics.contentTop; bottomMargin:Metrics.contentBottom
    cellWidth:Math.max(1,(width-Metrics.contentX*2)/columns) // derived: padding on both sides
    cellHeight:cellWidth+Metrics.cardTextGap+Metrics.cardTextHeight+Metrics.gap16
    cacheBuffer:height*0.5
    clip:true
    boundsBehavior:Flickable.StopAtBounds
    ScrollBar.vertical:ViewportScrollBar {}
    model:collection.count
    function seekViewport() {
        if (!activePage || restorePending) return
        silentWork=false
        const first=Math.max(0,Math.floor((contentY-(headerItem?.height || 0))/cellHeight))*columns
        collection.seek(first,first+(Math.ceil(height/cellHeight)+2)*columns-1)
    }
    onContentYChanged:Qt.callLater(seekViewport)
    onHeightChanged:Qt.callLater(seekViewport)
    onColumnsChanged:Qt.callLater(seekViewport)
    Connections {
        target:collection
        function onCountChanged() { Qt.callLater(page.seekViewport) }
        function onActionChanged() { Qt.callLater(page.performAction) }
        function onBusyChanged() { Qt.callLater(page.finishWork) }
        function onDocumentChanged() { Qt.callLater(page.restorePosition) }
    }
    function restorePosition() {
        if (!activePage || !collection.count) return
        const generation=state.generation || 0
        if (seenGeneration && generation !== seenGeneration) restorePending=true
        seenGeneration=generation
        if (!restorePending) return
        restorePending=false
        positionViewAtIndex(Math.max(0,Math.min(state.anchorIndex || 0,collection.count-1)),GridView.Beginning)
        Qt.callLater(seekViewport)
    }
    Connections { target:Sources; function onRevisionChanged() { page.catalogChanged() } }
    Connections { target:Library; function onRemoteRevChanged() { page.catalogChanged() } }
    function catalogChanged() {
        if (!activePage) return
        refreshPending=true
        if (favoriteAction && !Sources.favoritePending(about.plugin || "",favoriteAction.ref,Sources.revision)) favoriteAction=null
        Qt.callLater(refreshIfNeeded)
    }
    function refreshIfNeeded() {
        if (!activePage || collection.busy || pendingAction || manualRefreshPending || albumsMenu.visible || tracksMenu.visible || !refreshPending) return
        refreshPending=false
        refreshQuietly()
    }
    function refreshQuietly() {
        if (!activePage || collection.busy || manualRefreshPending) return
        silentWork=true
        collection.refresh(false)
        if (!collection.busy) silentWork=false
    }
    function finishWork() {
        if (!activePage || collection.busy) return
        silentWork=false
        if (manualRefreshPending) { manualRefreshPending=false;collection.refresh(true) }
        else refreshIfNeeded()
    }
    TrackListModel { id:tracks; objectName:"collectionTracks"; kind:page.activePage && !!page.prepared.key ? "source" : ""; key:page.prepared.key || ""; limit:100; playingPath:Player.path; onReadyChanged:Qt.callLater(page.performAction) }
    AlbumMenu { id:albumsMenu; objectName:"collectionAlbumMenu"; canExpand:false; onClosed:Qt.callLater(page.releaseAction) }
    TrackMenu { id:tracksMenu; objectName:"collectionTrackMenu"; model:tracks; onClosed:Qt.callLater(page.releaseAction) }
    SourcePlaylistEditor { id:playlistEditor }
    function refreshCollection() {
        silentWork=false
        if (collection.busy) { manualRefreshPending=true;return }
        collection.refresh(true)
    }
    function requestAction(item,purpose,x,y,on) {
        if (!activePage || !item.rowToken || pendingAction || albumsMenu.visible || tracksMenu.visible) return
        if (item.available === false && purpose === "play") return
        silentWork=false
        pendingAction={purpose:purpose,token:item.rowToken,x:x || 0,y:y || 0,on:on === true}
        actionError=""
        if (!collection.prepareAction(item.rowToken)) pendingAction=null
    }
    function openItem(item) { requestAction(item,item.kind === "track" ? "play" : "open") }
    function showMenu(item,x,y) { requestAction(item,"menu",x,y) }
    function releaseAction() {
        if (!albumsMenu.visible && !tracksMenu.visible && !pendingAction) { collection.releaseAction(); refreshIfNeeded() }
    }
    function performAction() {
        if (!activePage || !pendingAction) return
        const action=prepared
        if (action.failed || action.error) { actionError=action.error || "";pendingAction=null;releaseAction();return }
        if (!action.lease) return
        const request=pendingAction
        if (action.kind === "track" && request.purpose !== "favorite" && (!tracks.ready || tracks.key !== action.key)) return
        pendingAction=null
        if (request.purpose === "favorite") {
            favoriteAction={token:request.token,ref:action.ref}
            Sources.command("native",about.plugin || "",JSON.stringify({action:"favorite",ref:action.ref,on:request.on}))
        }
        else if (request.purpose === "menu") {
            if (action.kind === "track" && action.trackRow >= 0 && action.trackRow < tracks.count) tracksMenu.show(action.trackRow,request.x,request.y)
            else if (action.cardId && ["album","playlist"].includes(action.kind)) albumsMenu.show(action.cardId,action.title || "",action.artist || "",0,request.x,request.y)
        } else if (request.purpose === "play") {
            if (action.available !== false) {
                if (action.kind === "track" && action.trackRow >= 0 && action.trackRow < tracks.count) tracks.play(action.trackRow)
                else if (action.cardId) AlbumActions.play(action.cardId,false)
            }
        } else if (action.cardId && ["album","playlist"].includes(action.kind)) Navigation.go("album",action.cardId)
        else if (action.navigationKey) Navigation.goKey("source",action.navigationKey)
        Qt.callLater(releaseAction)
    }
    header:Column {
        x:Metrics.contentX
        width:page.width-Metrics.contentX*2 // derived: padding on both sides
        spacing:Metrics.gap16
        Text { text:page.about.title || I18n.tr("Library",I18n.rev); textFormat:Text.PlainText; font.pointSize:Type.title; font.weight:Type.bold; color:Theme.color.textPrimary }
        CollectionControls { width:parent.width; source:page.source; about:page.about }
        Flow {
            width:parent.width; spacing:Metrics.gap8
            PillButton { objectName:"collectionRefresh";text:I18n.tr("Refresh",I18n.rev); glyph:Icons.refresh; enabled:!collection.busy || page.silentWork; onClicked:page.refreshCollection() }
            PillButton { visible:page.about.playlist_edit === true; text:I18n.tr("New playlist",I18n.rev); onClicked:playlistEditor.begin({plugin:page.about.plugin},"create") }
            BusyIndicator { objectName:"collectionLoadingFeedback";width:Metrics.button28; height:width; visible:collection.busy && !page.silentWork || page.about.updating === true; running:visible && page.activePage }
            Text { visible:page.about.sync_loaded !== undefined && page.about.sync_loaded !== null; text:I18n.trCount("{} item","{} items",page.about.sync_loaded || 0,I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.small }
        }
        Text { width:parent.width; visible:!!page.state.error || !!page.actionError; text:page.actionError || page.state.error || ""; textFormat:Text.PlainText; wrapMode:Text.Wrap; color:Theme.color.textSecondary }
        PillButton { visible:page.state.stale === true; text:I18n.tr("Reload",I18n.rev); enabled:!collection.busy; onClicked:page.refreshCollection() }
    }
    delegate:Item {
        id:cell
        required property int index
        readonly property var entry:page.decode(collection.entry(index,collection.revision))
        width:page.cellWidth; height:page.cellHeight
        readonly property bool inViewport:y+height>page.contentY && y<page.contentY+page.height
        CollectionTile {
            width:parent.width-Metrics.gridGap
            entry:cell.entry
            plugin:page.about.plugin || ""
            active:page.activePage && cell.inViewport
            enabled:!!cell.entry.rowToken
            favorites:page.about.actions?.favorite === true
            favoriteBusy:page.favoriteAction?.token === cell.entry.rowToken
            playable:["track","album","playlist"].includes(entry.kind)
            onOpened:page.openItem(entry)
            onPlayRequested:page.requestAction(entry,"play")
            onMenuRequested:(x,y)=>{ const at=mapToItem(page,x,y); page.showMenu(entry,at.x,at.y) }
            onFavoriteRequested:on=>page.requestAction(entry,"favorite",0,0,on)
        }
    }
    footer:Column {
        width:page.width-Metrics.contentX*2 // derived: padding on both sides
        spacing:Metrics.gap16
        Text { visible:!collection.busy && !page.about.updating && collection.count === 0 && !page.state.error; text:I18n.tr("No results",I18n.rev); color:Theme.color.textSecondary }
        BusyIndicator { objectName:"collectionPagingFeedback";visible:collection.busy && !page.silentWork && collection.count > 0; width:Metrics.button32; height:width; running:visible && page.activePage }
    }
}
