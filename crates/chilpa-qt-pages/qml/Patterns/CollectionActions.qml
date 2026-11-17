import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.pages
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/playlists/qml/Components"

// Explicit row actions own at most one transient page and one open preview.
Item {
    id: actions
    required property var window
    required property var about
    property bool activePage: false
    property var pendingAction: null
    property string actionError: ""
    property bool refreshPending: false
    property bool silentWork: false
    property bool manualRefreshPending: false
    property var favoriteAction: null
    property int quickIndex: -1
    property var quickAction: ({})
    readonly property var prepared: decode(window.action)
    function decode(text) { try { return JSON.parse(text || "{}") } catch (_) { return {} } }
    function reset() {
        pendingAction=null; actionError=""; refreshPending=false; favoriteAction=null
        silentWork=false; manualRefreshPending=false; quickIndex=-1; quickAction={}
        albumsMenu.close(); tracksMenu.close()
    }
    onActivePageChanged: if (!activePage) reset()
    onQuickIndexChanged: if (quickIndex < 0) {
        quickAction={}
        window.releasePreview()
        Qt.callLater(refreshIfNeeded)
    }
    function createPlaylist() { playlistEditor.begin({plugin:about.plugin},"create") }
    Connections { target:Sources; function onRevisionChanged() { actions.catalogChanged() } }
    Connections { target:Library; function onRemoteRevChanged() { actions.catalogChanged() } }
    Connections {
        target:actions.window
        function onActionChanged() { Qt.callLater(actions.performAction) }
        function onBusyChanged() { Qt.callLater(actions.finishWork) }
    }
    function catalogChanged() {
        if (!activePage) return
        refreshPending=true
        if (favoriteAction && !Sources.favoritePending(about.plugin || "",favoriteAction.ref,Sources.revision)) favoriteAction=null
        Qt.callLater(refreshIfNeeded)
    }
    function refreshIfNeeded() {
        if (!activePage || window.busy || pendingAction || manualRefreshPending || quickIndex >= 0 || albumsMenu.visible || tracksMenu.visible || !refreshPending) return
        refreshPending=false
        refreshQuietly()
    }
    function refreshQuietly() {
        if (!activePage || window.busy || manualRefreshPending || quickIndex >= 0) return
        silentWork=true
        window.refresh(false)
        if (!window.busy) silentWork=false
    }
    function finishWork() {
        if (!activePage || window.busy) return
        silentWork=false
        if (manualRefreshPending) { manualRefreshPending=false;window.refresh(true) }
        else refreshIfNeeded()
    }
    TrackListModel {
        id: tracks
        objectName: "collectionTracks"
        kind: key ? "source" : ""
        key: actions.activePage && actions.prepared.kind === "track" ? actions.prepared.key || "" : ""
        limit: 100
        playingPath: Player.path
        onReadyChanged: Qt.callLater(actions.performAction)
    }
    AlbumMenu { id:albumsMenu; objectName:"collectionAlbumMenu"; canExpand:true; onQuick:at=>actions.pinQuick(actions.prepared,at); onClosed:Qt.callLater(actions.releaseAction) }
    TrackMenu { id:tracksMenu; objectName:"collectionTrackMenu"; model:tracks; onClosed:Qt.callLater(actions.releaseAction) }
    SourcePlaylistEditor { id:playlistEditor }
    function refreshCollection() {
        silentWork=false
        if (window.busy) { manualRefreshPending=true;return }
        window.refresh(true)
    }
    function requestAction(item,purpose,x,y,on,at) {
        if (!activePage || !item.rowToken || pendingAction || albumsMenu.visible || tracksMenu.visible) return
        if (item.available === false && purpose === "play") return
        silentWork=false
        pendingAction={purpose:purpose,token:item.rowToken,x:x || 0,y:y || 0,on:on === true,at:at ?? -1}
        actionError=""
        if (!window.prepareAction(item.rowToken)) pendingAction=null
    }
    function openItem(item,at) {
        if (["album","playlist"].includes(item.kind) && SettingsModel.quickViewClick) requestQuick(item,at)
        else requestAction(item,item.kind === "track" ? "play" : "open",0,0,false,at)
    }
    function requestQuick(item,at) {
        if (quickIndex === at) { quickIndex=-1;return }
        requestAction(item,"quick",0,0,false,at)
    }
    function pinQuick(action,at) {
        if (!activePage || !action.cardId || !["album","playlist"].includes(action.kind) || !window.pinAction()) return
        quickAction=action
        quickIndex=at
    }
    function showMenu(item,x,y,at) { requestAction(item,"menu",x,y,false,at) }
    function releaseAction() {
        if (!albumsMenu.visible && !tracksMenu.visible && !pendingAction) { window.releaseAction(); refreshIfNeeded() }
    }
    function performAction() {
        if (!activePage || !pendingAction) return
        const action=prepared
        if (action.failed || action.error) { actionError=action.error || "";pendingAction=null;releaseAction();return }
        if (action.kind === "article" && action.navigationKey) {
            const purpose=pendingAction.purpose
            pendingAction=null
            if (purpose === "open") Navigation.goKey("source",action.navigationKey)
            Qt.callLater(releaseAction)
            return
        }
        if (!action.lease) return
        const request=pendingAction
        if (action.kind === "track" && request.purpose !== "favorite" && (!tracks.ready || tracks.key !== action.key)) return
        pendingAction=null
        if (request.purpose === "favorite") {
            favoriteAction={token:request.token,ref:action.ref}
            Sources.command("native",about.plugin || "",JSON.stringify({action:"favorite",ref:action.ref,on:request.on}))
        }
        else if (request.purpose === "quick") pinQuick(action,request.at)
        else if (request.purpose === "menu") {
            if (action.kind === "track" && action.trackRow >= 0 && action.trackRow < tracks.count) tracksMenu.show(action.trackRow,request.x,request.y)
            else if (action.cardId && ["album","playlist"].includes(action.kind)) albumsMenu.show(action.cardId,action.title || "",action.artist || "",request.at,request.x,request.y)
        } else if (request.purpose === "play") {
            if (action.available !== false) {
                if (action.kind === "track" && action.trackRow >= 0 && action.trackRow < tracks.count) tracks.play(action.trackRow)
                else if (action.cardId) AlbumActions.play(action.cardId,false)
            }
        } else if (action.cardId && ["album","playlist"].includes(action.kind)) Navigation.go("album",action.cardId)
        else if (action.navigationKey) Navigation.goKey("source",action.navigationKey)
        Qt.callLater(releaseAction)
    }
}
