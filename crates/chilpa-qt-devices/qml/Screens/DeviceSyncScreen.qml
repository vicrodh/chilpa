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
    property bool creating: false
    property bool reviewing: false
    property string lastPhase: ""
    readonly property bool setup: creating || (!document.has_profiles && !status.busy)

    function settle() {
        if (status.phase === lastPhase) return
        lastPhase = status.phase || ""
        if (["review", "transferring", "done"].includes(lastPhase)) reviewing = true
        else if (lastPhase === "idle") reviewing = false
    }
    Component.onCompleted: { backend.enter(); settle() }
    Component.onDestruction: backend.leave()
    onStatusChanged: settle()
    Connections { target: screen.backend; function onProfileCreated() { screen.creating = false } }
    Timer { objectName: "deviceProgressPoll"; interval: Motion.poll; repeat: true; running: screen.visible && screen.backend.busy && (!screen.Window.window || (screen.Window.window.visible && screen.Window.window.visibility !== Window.Minimized)); onTriggered: screen.backend.poll() }

    Column {
        id: head
        x: Metrics.contentX; y: Metrics.contentTop
        width: parent.width - Metrics.contentHorizontalMargins
        spacing: Metrics.gap12
        DeviceHeader {
            width: parent.width
            document: screen.document; status: screen.status
            onProfileAsked: id => screen.backend.selectProfile(id)
            onDestinationAsked: id => screen.backend.selectDestination(id)
            onRefreshAsked: screen.backend.refresh()
            onCreateAsked: screen.creating = true
        }
        DeviceNotice { width: parent.width; visible: screen.status.error !== null && screen.status.error !== undefined; error: screen.status.error || null; busy: screen.backend.busy }
        Flow {
            visible: !screen.setup && screen.document.has_profiles === true
            width: parent.width
            spacing: Metrics.gap12
            SegmentedControl {
                options: [{ text: I18n.tr("Music", I18n.rev) }, { text: screen.status.phase === "done" ? I18n.tr("Last sync", I18n.rev) : I18n.tr("Changes", I18n.rev) }]
                current: screen.reviewing ? 1 : 0
                onActivated: index => screen.reviewing = index === 1
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
        sourceComponent: screen.setup ? pairing : screen.reviewing ? changes : selections
    }
    Column {
        id: foot
        x: Metrics.contentX
        width: parent.width - Metrics.contentHorizontalMargins
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.playerSpace + Metrics.gap20
        spacing: Metrics.gap8
        CircleCheck {
            objectName: "deviceRemoveUnselected"
            visible: !screen.setup && !screen.reviewing && screen.document.has_profiles === true
            width: parent.width
            text: I18n.tr("Remove managed files no longer selected", I18n.rev)
            checked: screen.status.remove_unselected === true
            enabled: !screen.backend.busy
            onClicked: screen.backend.removals(!screen.status.remove_unselected)
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
    Component { id: selections; DeviceSelection { rows: screen.document.options || []; busy: screen.backend.busy; onToggled: id => screen.backend.toggle(id); onFilterEdited: text => screen.backend.filter(text) } }
    Component { id: changes; DeviceChanges { emptyText: screen.status.phase === "review" ? I18n.tr("No files need to change.", I18n.rev) : I18n.tr("Review changes to check what this storage needs.", I18n.rev); rows: JSON.parse(screen.backend.pageRows || "[]"); page: screen.backend.page; pages: screen.document.page_count || 1; busy: screen.backend.busy; onPageAsked: at => screen.backend.changePage(at) } }
}
