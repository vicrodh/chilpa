import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Patterns"

// Settings > Music servers > Offline music, after QBZ's offline cache
// (OfflineManagerView, Qt, 2.1.2): what the servers' music keeps on this
// device, its limit and its folders. A menu's download happens where the
// menu was, never here. The facade reads the listing on a thread; this page
// asks for it while it changes and stops asking once it is still. Back is
// the window's.
Item {
    id: page
    property var status: ({})
    property var doc: ({})
    property var filters: ({sort: "az", failed: false, server: 0, artist: ""})
    property var checked: ({})
    property var removal: ({})
    property int revision: 0
    property bool loading: true
    property string said: ""
    property int section: 0
    readonly property bool busy: status.running === true
    readonly property real contentWidth: Math.min(Metrics.settingsMaxWidth + Metrics.settingsNav, width - Metrics.contentX * 2) // derived: padding on both sides
    readonly property bool shown: visible && (!Window.window || (Window.window.visible && Window.window.visibility !== Window.Minimized))

    function act(action, kind, selected) { return JSON.parse(Offline.call(action, kind, selected) || "{}") }
    function read() {
        const answer = act("manager", String(revision), JSON.stringify(filters))
        loading = answer.loading === true
        said = answer.said || ""
        if (answer.doc) {
            revision = answer.revision
            doc = answer.doc
            const present = new Set((doc.rows || []).map(row => row.id))
            const kept = {}
            for (const id of Object.keys(checked)) if (present.has(Number(id))) kept[id] = true
            checked = kept
        }
    }
    function refresh() { act("refresh", "", ""); read() }
    function poll() {
        const before = (status.done || 0) + (status.failed || 0), working = busy
        status = act("status", "", "[]")
        if (working !== busy || before !== (status.done || 0) + (status.failed || 0)) refresh()
        else read()
    }
    function begin(kind, selected) { status = act("start", kind, selected); refresh() }
    function ask(kind, selected) {
        removal = act("removal_info", kind, selected)
        if (removal.ticket) confirmation.open()
        else status = removal
    }
    Component.onCompleted: { status = act("status", "", "[]"); read() }
    // Leaving lets the facade drop the listing; the next visit reads it again.
    Component.onDestruction: { Offline.call("dismiss_removal", "", ""); Offline.call("leave", "", "") }
    Connections {
        target: Offline
        function onRequested() { page.section = 0 }
    }
    Timer {
        objectName: "offlinePoll"
        interval: Motion.poll
        repeat: true
        running: page.shown && (page.busy || page.loading)
        onTriggered: page.poll()
    }

    Column {
        id: head
        x: Metrics.contentX; y: Metrics.contentTop
        width: page.contentWidth
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: I18n.tr("Offline music", I18n.rev)
            textFormat: Text.PlainText; elide: Text.ElideRight
            font.pointSize: Type.title; font.weight: Type.bold; font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        OfflineSummary {
            objectName: "offlineSummary"
            width: parent.width
            doc: page.doc
            busy: page.busy
            onLimitPicked: value => { page.act("limit", value, ""); page.refresh() }
            onFolderOpened: Qt.openUrlExternally(page.doc.root.path)
            onEverythingRemoved: page.ask("downloads", "[]")
        }
        Text {
            width: parent.width
            text: I18n.tr("Downloads stop when offline music reaches this size, and nothing is deleted to make room. Each file can be up to 512 MiB. Unlimited lets offline music use all the free space on the disk, with no size limit per file.", I18n.rev)
            textFormat: Text.PlainText; wrapMode: Text.WordWrap
            font.pointSize: Type.small; color: Theme.color.textTertiary
        }
        Item {
            objectName: "offlineProgress"
            visible: page.busy || (page.status.total || 0) > 0 || (page.status.error || "") !== ""
            width: parent.width
            height: visible ? Math.max(progress.height, cancel.height) : 0
            StatusArea {
                id: progress
                objectName: "offlineDownloadStatus"
                width: parent.width - cancel.width - Metrics.gap12
                lines: Metrics.statusMessageLines / 2 // derived: half the usual room
                progress: page.status.total > 0 ? ((page.status.done || 0) + (page.status.failed || 0)) / page.status.total : -1
                text: [page.status.title || "",
                    page.busy && page.status.phase === "verifying" ? I18n.tr("Verifying downloaded audio…", I18n.rev) : page.busy && page.status.phase === "checking" ? I18n.tr("Checking downloaded audio…", I18n.rev) : "",
                    page.status.total ? (page.status.removing ? I18n.trArgs("{} of {} files removed. {} failed.", [page.status.done || 0, page.status.total, page.status.failed || 0], I18n.rev)
                        : I18n.trArgs("{} of {} downloaded. {} failed.", [page.status.done || 0, page.status.total, page.status.failed || 0], I18n.rev)) : "",
                    page.status.error || (page.status.cancelled ? I18n.tr("Download cancelled.", I18n.rev) : "")].filter(line => line !== "").join("\n")
            }
            PillButton {
                id: cancel
                objectName: "cancelOfflineDownload"
                anchors.right: parent.right
                enabled: page.busy
                text: I18n.tr("Cancel", I18n.rev)
                onClicked: page.status = page.act("cancel", "", "[]")
            }
        }
        SegmentedControl {
            objectName: "offlineSections"
            options: [{text: I18n.tr("Tracks", I18n.rev)}, {text: I18n.tr("Folders", I18n.rev)}]
            current: page.section
            onActivated: index => page.section = index
        }
    }
    OfflineTracks {
        objectName: "offlineTracks"
        visible: page.section === 0
        x: head.x; width: head.width
        anchors.top: head.bottom; anchors.topMargin: Metrics.gap12; anchors.bottom: parent.bottom
        doc: page.doc; filters: page.filters; checked: page.checked; busy: page.busy; loading: page.loading
        onFiltersEdited: next => { page.filters = next; page.checked = ({}); page.read() }
        onToggled: id => { const next = Object.assign({}, page.checked); if (next[id]) delete next[id]; else next[id] = true; page.checked = next }
        onEveryTrackChecked: { const all = {}; for (const row of page.doc.rows || []) if (row.kind === "track" && row.id > 0) all[row.id] = true; page.checked = all }
        onChecksCleared: page.checked = ({})
        onRetried: ids => page.begin("tracks", JSON.stringify(ids))
        onRemoved: ids => page.ask("tracks", JSON.stringify(ids))
    }
    OfflineFolders {
        objectName: "offlineFolders"
        visible: page.section === 1
        x: head.x; width: head.width
        anchors.top: head.bottom; anchors.topMargin: Metrics.gap12; anchors.bottom: parent.bottom
        doc: page.doc; said: page.said
        onRootSaved: page.refresh()
        onServerFolderChosen: (id, folder) => { const answer = page.act("server_folder", String(id), folder); page.said = answer.error || ""; page.loading = true; page.refresh() }
    }
    ConfirmationDialog {
        id: confirmation
        objectName: "offlineRemovalConfirmation"
        title: I18n.tr("Remove downloaded audio?", I18n.rev)
        message: I18n.trArgs("This permanently deletes {} downloaded audio files from this device.", [page.removal.count || 0], I18n.rev)
        destructive: true
        doubleConfirmation: true
        confirmText: I18n.tr("Remove", I18n.rev)
        onConfirmed: { page.status = page.act("remove", "", page.removal.ticket); page.checked = ({}) }
        onDismissed: Offline.call("dismiss_removal", "", "")
    }
}
