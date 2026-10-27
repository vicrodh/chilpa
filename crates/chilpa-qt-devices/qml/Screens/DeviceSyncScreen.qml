import QtQuick
import app.chilpa.devices
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"

Item {
    id: screen
    property var backend: DeviceSync
    readonly property var status: JSON.parse(backend.snapshot || "{}")
    readonly property var document: JSON.parse(backend.catalog || "{}")
    readonly property var music: JSON.parse(backend.browser || "{}")
    property bool creating: false
    property bool reviewing: false
    property bool recorded: false
    property string lastPhase: ""
    readonly property bool setup: creating || (!document.has_profiles && !status.busy)
    readonly property bool compactSections: head.width < Metrics.deviceSelectWidth * 2 // derived: the space of two storage selectors

    function settle() {
        if (status.phase === lastPhase) return
        const previous = lastPhase
        lastPhase = status.phase || ""
        if (previous === "browsing" || previous === "reconnecting") return
        if (["review", "transferring", "done"].includes(lastPhase)) { reviewing = true; recorded = false }
        else if (lastPhase === "idle") { reviewing = false; if (!music.source) recorded = false }
    }
    Component.onCompleted: { backend.enter(); settle() }
    Component.onDestruction: backend.leave()
    onStatusChanged: settle()
    Connections { target: screen.backend; function onProfileCreated() { screen.creating = false } }
    StorageEvents { objectName: "deviceStorageEvents"; active: screen.visible && (!screen.Window.window || (screen.Window.window.visible && screen.Window.window.visibility !== Window.Minimized)); onChanged: screen.backend.connectionsChanged() }
    Timer { objectName: "deviceProgressPoll"; interval: Motion.poll; repeat: true; running: screen.visible && screen.backend.busy && (!screen.Window.window || (screen.Window.window.visible && screen.Window.window.visibility !== Window.Minimized)); onTriggered: screen.backend.poll() }

    Column {
        id: head
        x: Metrics.contentX; y: Metrics.contentTop
        width: parent.width - Metrics.contentHorizontalMargins
        spacing: Metrics.gap12
        DeviceHeader {
            width: parent.width
            document: screen.document; status: screen.status
            onProfileAsked: id => { screen.recorded = false; screen.backend.selectProfile(id) }
            onDestinationAsked: id => { screen.recorded = false; screen.backend.selectDestination(id) }
            onRefreshAsked: { screen.recorded = false; screen.backend.refresh() }
            onCreateAsked: screen.creating = true
        }
        DeviceNotice { width: parent.width; visible: screen.status.error !== null && screen.status.error !== undefined; error: screen.status.error || null; busy: screen.backend.busy }
        Flow {
            visible: !screen.setup && screen.document.has_profiles === true
            width: parent.width
            spacing: Metrics.gap12
            SegmentedControl {
                objectName: "deviceSections"
                options: [{ text: screen.compactSections ? "" : I18n.tr("Music", I18n.rev), glyph: Icons.music, tip: I18n.tr("Music", I18n.rev) },
                    { text: screen.compactSections ? "" : I18n.tr("Synced music", I18n.rev), glyph: Icons.deviceMobile, tip: I18n.tr("Synced music", I18n.rev) },
                    { text: screen.compactSections ? "" : screen.status.phase === "done" ? I18n.tr("Last sync", I18n.rev) : I18n.tr("Changes", I18n.rev), glyph: Icons.list, tip: I18n.tr("Changes", I18n.rev) }]
                current: screen.reviewing ? 2 : screen.recorded ? 1 : 0
                enabled: !screen.backend.busy
                onActivated: index => { screen.reviewing = index === 2; screen.recorded = index === 1; if (screen.recorded) screen.backend.browse("recorded"); else screen.backend.closeBrowser() }
            }
            Text { height: Metrics.button28; verticalAlignment: Text.AlignVCenter; text: screen.document.selected || ""; color: Theme.color.textSecondary; font.pointSize: Type.small }
        }
    }
    Loader {
        id: body
        x: Metrics.contentX
        width: parent.width - Metrics.contentHorizontalMargins
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: foot.top
        anchors.bottomMargin: Metrics.gap12
        visible: screen.setup || screen.reviewing || screen.recorded || !screen.music.source
        sourceComponent: screen.setup ? pairing : screen.reviewing ? changes : screen.recorded ? browser : selections
    }
    Loader { anchors.fill: body; active: !screen.setup && !screen.reviewing && !screen.recorded && !!screen.music.source; visible: active; sourceComponent: browser }
    Column {
        id: foot
        x: Metrics.contentX
        width: parent.width - Metrics.contentHorizontalMargins
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.playerSpace + Metrics.gap20
        spacing: Metrics.gap8
        CircleCheck {
            id: pruning
            objectName: "deviceRemoveUnselected"
            visible: !screen.setup && !screen.reviewing && !screen.recorded && !screen.music.source && screen.document.has_profiles === true
            width: parent.width
            text: I18n.tr("Also remove music files no longer selected", I18n.rev)
            Accessible.description: I18n.tr("Unselected playlists are removed after approval. Enable this option to remove unselected music too.", I18n.rev)
            checked: screen.status.remove_unselected === true
            enabled: !screen.backend.busy
            onClicked: screen.backend.removals(!screen.status.remove_unselected)
            HoverHandler { enabled: pruning.visible; onHoveredChanged: hovered ? TipState.show(pruning.Accessible.description, pruning) : TipState.hide(pruning) }
            Component.onDestruction: TipState.hide(pruning)
        }
        DeviceFooter {
            width: parent.width
            status: screen.status; document: screen.document
            onPreviewAsked: screen.backend.preview(false)
            onRecoverAsked: screen.backend.preview(true)
            onSyncAsked: (token, removals) => screen.backend.apply(token, removals)
            onCancelAsked: screen.backend.cancel()
        }
    }
    Component { id: pairing; DevicePairing { mounted: screen.status.destination === "mounted"; busy: screen.backend.busy; onCreateAsked: (name, folder) => screen.backend.createProfile(name, folder); onDismissed: screen.creating = false } }
    Component { id: selections; DeviceSelection { rows: screen.document.options || []; query: screen.document.filter || ""; kind: screen.document.selection_kind || "all"; page: screen.document.selection_page || 0; pages: screen.document.selection_pages || 1; busy: screen.backend.busy; onToggled: id => screen.backend.toggle(id); onOpened: id => screen.backend.browse(id); onKindAsked: kind => screen.backend.selectionKind(kind); onPageAsked: page => screen.backend.selectionPage(page); onFilterEdited: text => screen.backend.filter(text) } }
    Component { id: browser; DeviceBrowser { document: screen.music; busy: screen.backend.busy; onAlbumAsked: id => screen.backend.openBrowserAlbum(id); onBackAsked: screen.backend.browserBack(); onFilterEdited: text => screen.backend.filterBrowser(text); onViewAsked: view => screen.backend.browserView(view); onPageAsked: at => screen.backend.browserPage(at); onReloadAsked: screen.backend.reloadBrowser() } }
    Component { id: changes; DeviceChanges { emptyText: screen.status.phase === "review" ? I18n.tr("No files need to change.", I18n.rev) : I18n.tr("Review changes to check what this storage needs.", I18n.rev); rows: JSON.parse(screen.backend.pageRows || "[]"); page: screen.backend.page; pages: screen.document.page_count || 1; busy: screen.backend.busy; onPageAsked: at => screen.backend.changePage(at) } }
}
