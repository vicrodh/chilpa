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
import "qrc:/qt/qml/app/chilpa/sources/qml/Components"
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "../Components"
import "../Patterns"

Item {
    id: page
    required property string source
    property bool leaving: false
    property var scrollMemory: null
    property bool completed: false
    readonly property alias scrollView: grid.scrollView
    readonly property bool activePage: visible && !leaving
    readonly property var state: decode(collection.document)
    readonly property var about: state.header || ({})
    property bool restorePending: true
    property double seenGeneration: 0
    function decode(text) { try { return JSON.parse(text || "{}") } catch (_) { return {} } }
    function activate() {
        if (activePage) openCollection()
        else {
            actions.reset()
            visible ? collection.retire() : collection.close()
        }
    }
    function openCollection() {
        restorePending=true; seenGeneration=0; actions.reset()
        collection.open(source)
    }
    onSourceChanged: if (completed && activePage) openCollection()
    onActivePageChanged: if (completed) { activePage ? Qt.callLater(activate) : activate() }
    onVisibleChanged: if (completed && !visible) collection.close()
    Component.onCompleted: { completed=true; activate() }
    Component.onDestruction: collection.close()
    NativeCollectionModel { id:collection; objectName:"nativeCollectionModel"; active:page.activePage }
    CollectionActions { id:actions; window:collection; about:page.about; activePage:page.activePage }
    PulseClock { slow:true; minimumInterval:100; running:page.activePage && collection.busy; onTriggered:collection.poll() }
    Timer {
        interval:Math.max(250,Math.min(60000,page.about.refresh_after_ms || page.about.retry_after_ms || 1000))
        running:page.activePage && actions.quickIndex < 0 && !collection.busy && page.about.updating === true && page.about.request_pending !== true
        repeat:true
        onTriggered:actions.refreshQuietly()
    }
    // Only the row count is virtualized; no full-library ListModel is built.
    QtObject {
        id: rows
        readonly property int maxRows: 0
        readonly property bool mixed: false
        readonly property int rev: collection.revision
        property alias expanded: actions.quickIndex
        function albumId(at,rev) { return at === actions.quickIndex ? actions.quickAction.cardId || 0 : 0 }
    }
    AlbumGrid {
        id: grid
        anchors.fill:parent
        anchors.leftMargin:Metrics.contentX; anchors.rightMargin:Metrics.contentX
        model:rows
        windowCount:collection.count
        topMargin:Metrics.contentTop; bottomMargin:Metrics.contentBottom
        cardDelegate: Component {
            CollectionTile {
                required property int index
                readonly property int at: (parent?.first || 0) + index
                x: Metrics.cardRing + index * (grid.cardWidth + Metrics.gridGap)
                y: parent?.lead || 0
                visible: !!parent && index < parent.span
                readonly property var entryData: parent && collection ? page.decode(collection.entry(at,collection.revision)) : ({})
                width:grid.cardWidth
                entry:entryData
                plugin:page.about.plugin || ""
                active:page.activePage && parent?.onScreen === true
                enabled:!!entryData.rowToken
                favorites:page.about.actions?.favorite === true
                favoriteBusy:!!entryData.rowToken && actions.favoriteAction?.token === entryData.rowToken
                playable:["track","album","playlist"].includes(entryData.kind)
                expanded:actions.quickIndex === at
                cardId:expanded ? actions.quickAction.cardId || 0 : actions.prepared.ref === entryData.ref ? actions.prepared.cardId || 0 : 0
                onOpened:actions.openItem(entryData,at)
                onQuickRequested:actions.requestQuick(entryData,at)
                onPlayRequested:actions.requestAction(entryData,"play",0,0,false,at)
                onMenuRequested:(x,y)=>{ const point=mapToItem(page,x,y); actions.showMenu(entryData,point.x,point.y,at) }
                onFavoriteRequested:on=>actions.requestAction(entryData,"favorite",0,0,on,at)
            }
        }
        header:Column {
            width:grid.width
            spacing:Metrics.gap16
            SourceEntityHeader {
                width:parent.width
                currentPage:page.activePage
                info:page.about.entity ? Object.assign({}, page.about.entity, {chosen_art:NativePages.referenceCoverOverride(page.about.plugin || "",page.about.entity.ref || "",Library.remoteArtRev)}) : ({})
            }
            Text { visible:!page.about.entity; text:page.about.title || I18n.tr("Library",I18n.rev); textFormat:Text.PlainText; font.pointSize:Type.title; font.weight:Type.bold; color:Theme.color.textPrimary }
            Flickable {
                width:parent.width
                visible:tabs.model.length > 0
                height:visible ? tabs.implicitHeight : 0
                contentWidth:tabs.implicitWidth
                contentHeight:height
                clip:true
                interactive:contentWidth > width
                flickableDirection:Flickable.HorizontalFlick
                boundsBehavior:Flickable.StopAtBounds
                SourceTabs {
                    id:tabs
                    objectName:"collectionTabs"
                    model:page.about.show_tabs === true ? page.about.tabs || [] : []
                    navigate:key=>{
                        const saved=SettingsModel.kept("ui.source.collection."+key)
                        Navigation.goKey("source",saved && NativePages.sameNavigation(saved,key,Sources.revision) ? saved : key)
                    }
                }
            }
            CollectionControls {
                width:parent.width; source:page.source; about:page.about
                busy:collection.busy; silentWork:actions.silentWork; active:page.activePage
                onRefreshRequested:actions.refreshCollection()
                onCreatePlaylistRequested:actions.createPlaylist()
            }
            Text { width:parent.width; visible:!!page.state.error || !!actions.actionError; text:actions.actionError || page.state.error || ""; textFormat:Text.PlainText; wrapMode:Text.Wrap; color:Theme.color.textSecondary }
            PillButton { visible:page.state.stale === true; text:I18n.tr("Reload",I18n.rev); enabled:!collection.busy; onClicked:actions.refreshCollection() }
        }
        footer:Column {
            width:grid.width
            spacing:Metrics.gap16
            Text { visible:!collection.busy && !page.about.updating && collection.count === 0 && !page.state.error; text:I18n.tr("No results",I18n.rev); color:Theme.color.textSecondary }
            BusyIndicator { objectName:"collectionPagingFeedback";visible:collection.busy && !actions.silentWork && collection.count > 0; width:Metrics.button32; height:width; running:visible && page.activePage }
        }
        onColumnsChanged:Qt.callLater(page.seekViewport)
    }
    function seekViewport() {
        if (!activePage || restorePending) return
        let first=collection.count
        for (const row of scrollView.contentItem.children)
            if (row.first !== undefined && row.y+row.height > scrollView.contentY && row.y < scrollView.contentY+scrollView.height)
                first=Math.min(first,row.first)
        if (first === collection.count) first=0
        collection.seek(first,first+(Math.ceil(height/grid.rowHeight)+2)*grid.columns-1)
    }
    Connections { target:page.scrollView; function onContentYChanged() { Qt.callLater(page.seekViewport) } }
    onHeightChanged:Qt.callLater(seekViewport)
    Connections {
        target:collection
        function onCountChanged() { Qt.callLater(page.seekViewport) }
        function onDocumentChanged() { Qt.callLater(page.restorePosition) }
    }
    function restorePosition() {
        if (!activePage || !collection.count) return
        const generation=state.generation || 0
        if (seenGeneration && generation !== seenGeneration) { restorePending=true; actions.quickIndex=-1 }
        seenGeneration=generation
        if (!restorePending) return
        restorePending=false
        const anchor=Math.max(0,Math.min(state.anchorIndex || 0,collection.count-1))
        if (scrollMemory) scrollMemory.restore()
        else if (anchor > 0) scrollView.positionViewAtIndex(Math.floor(anchor/grid.columns),ListView.Beginning)
        Qt.callLater(seekViewport)
    }
}
