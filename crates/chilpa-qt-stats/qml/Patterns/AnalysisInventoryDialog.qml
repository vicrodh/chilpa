import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"
import "../Components"
import "../Theme"

Modal {
    id: dialog
    objectName: "analysisInventoryDialog"
    property var inventorySource: null
    property string filter: "all"
    property int runId: 0
    property bool history: false
    property var summary: ({ total: 0, standard: 0, mood: 0, matched: 0, runs: [] })
    property alias tracks: trackRows
    property string error: ""
    width: Math.min(Metrics.analysisInventoryWidth, parent.width - Metrics.modalInset)
    height: Math.min(Metrics.analysisInventoryHeight, parent.height - Metrics.modalInset)
    ListModel { id: trackRows; dynamicRoles: true }
    ListModel { id: runRows; dynamicRoles: true }

    function date(value) { return value ? Qt.formatDateTime(new Date(value * 1000), Qt.DefaultLocaleShortDate) : "" }
    function state(value) {
        return value === "completed" ? I18n.tr("Completed", I18n.rev)
            : value === "cancelled" ? I18n.tr("Cancelled", I18n.rev)
            : value === "interrupted" ? I18n.tr("Interrupted", I18n.rev)
            : value === "failed" ? I18n.tr("Failed", I18n.rev) : I18n.tr("Analyzing", I18n.rev)
    }
    function show() {
        filter = "all"; runId = 0; history = false; search.text = ""
        open(); fetch(true)
    }
    function sync(model, rows, start, limit) {
        for (let i = 0; i < rows.length; i++) {
            const row = rows[i], at = start + i
            if (at >= model.count) model.append(row)
            else if (model.get(at).id !== row.id) {
                let found = -1
                for (let j = at + 1; j < model.count; j++) if (model.get(j).id === row.id) { found = j; break }
                if (found >= 0) model.move(found, at, 1)
                else model.insert(at, row)
            }
            for (const key of Object.keys(row)) {
                if (model.get(at)[key] !== row[key]) { model.set(at, row); break }
            }
        }
        if (model.count > limit) model.remove(limit, model.count - limit)
    }
    function fetch(reset, live) {
        try {
            const start = live ? Math.floor(Math.max(0, list.indexAt(0, list.contentY)) / 100) * 100 : reset ? 0 : trackRows.count
            const raw = inventorySource ? inventorySource.inventory(filter, search.text, start, runId)
                : Stats.inventory(filter, search.text, start, runId)
            const answer = JSON.parse(raw)
            if (answer.error) { error = answer.error; return }
            error = ""; summary = answer
            sync(runRows, (answer.runs || []).map(row => Object.assign({}, row, { folder: row.folder || "", finished_at: row.finished_at || 0 })), 0, (answer.runs || []).length)
            const rows = answer.tracks.map(row => Object.assign({}, row, { standard_at: row.standard_at || 0, mood_at: row.mood_at || 0, processed_at: row.processed_at || 0, result: row.result || "" }))
            if (reset) trackRows.clear()
            sync(trackRows, rows, start, Math.min(answer.matched, Math.max(trackRows.count, start + rows.length)))
            if (reset) list.positionViewAtBeginning()
        } catch (e) { error = I18n.tr("Could not read the analysis inventory.", I18n.rev) }
    }
    function choose(value) { if (filter !== value) { filter = value; fetch(true) } }
    function chooseRun(id) { runId = id; filter = "all"; history = false; fetch(true) }
    Timer { id: searchDelay; interval: Charts.inventoryPoll / 4; onTriggered: dialog.fetch(true) }
    Timer { interval: Charts.inventoryPoll; repeat: true; running: dialog.opened; onTriggered: dialog.fetch(false, true) }

    Item {
        anchors.fill: parent
        Row {
            id: heading
            width: parent.width
            Text { width: parent.width - closeButton.width; text: I18n.tr("Analysis inventory", I18n.rev); font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary }
            IconButton { id: closeButton; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
        }
        Text {
            id: counts
            anchors.top: heading.bottom; anchors.topMargin: Metrics.gap12
            width: parent.width
            text: I18n.tr("Local tracks", I18n.rev) + ": " + summary.total + "  ·  " + I18n.tr("Standard analyzed", I18n.rev) + ": " + summary.standard + "  ·  " + I18n.tr("Mood stored", I18n.rev) + ": " + summary.mood
            font.pointSize: Type.body; color: Theme.color.textSecondary
        }
        Column {
            id: activity
            anchors.top: counts.bottom; anchors.topMargin: Metrics.gap8
            width: parent.width; spacing: Metrics.gap4
            readonly property var latest: dialog.summary.runs && dialog.summary.runs.length ? dialog.summary.runs[0] : null
            readonly property bool preparing: dialog.summary.running === true && (!latest || (dialog.summary.active || []).indexOf(latest.id) < 0)
            Text {
                width: parent.width; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary
                text: activity.preparing ? I18n.tr("Preparing analysis…", I18n.rev) : activity.latest ? (activity.latest.kind === "mood" ? I18n.tr("Mood", I18n.rev) : I18n.tr("Standard analysis", I18n.rev)) + " · " + dialog.state(activity.latest.state) + " · " + activity.latest.processed + " / " + activity.latest.total + " · " + I18n.tr("Previously saved", I18n.rev) + ": " + activity.latest.previous
                    : dialog.summary.running ? I18n.tr("Preparing analysis…", I18n.rev) : I18n.tr("Saved results remain available between sessions.", I18n.rev)
            }
            Text { width: parent.width; elide: Text.ElideRight; visible: activity.latest !== null && !activity.preparing; text: activity.latest ? activity.latest.error || activity.latest.last || dialog.date(activity.latest.started_at) : ""; font.pointSize: Type.meta; color: Theme.color.textTertiary }
            Rectangle {
                width: parent.width; height: Metrics.progress; radius: height; visible: dialog.summary.running === true
                color: Theme.color.strokeHairline
                Rectangle { width: parent.width * (activity.latest && activity.latest.total ? Math.min(1, activity.latest.processed / activity.latest.total) : 0); height: parent.height; radius: parent.radius; color: Theme.color.accent }
            }
        }
        SegmentedControl {
            id: tabs
            objectName: "inventoryTabs"
            anchors.top: activity.bottom; anchors.topMargin: Metrics.gap12
            options: [{ text: I18n.tr("Tracks", I18n.rev) }, { text: I18n.tr("Analysis history", I18n.rev) }]
            current: dialog.history ? 1 : 0
            onActivated: index => dialog.history = index === 1
        }
        Item {
            id: trackBody
            anchors.top: tabs.bottom; anchors.topMargin: Metrics.gap12; anchors.bottom: footer.top; anchors.bottomMargin: Metrics.gap12
            width: parent.width; visible: !dialog.history
            Kit.SearchField { id: search; objectName: "inventorySearch"; width: parent.width; placeholder: I18n.tr("Search title, artist or album", I18n.rev); onTextChanged: searchDelay.restart(); onAccepted: { searchDelay.stop(); dialog.fetch(true) } }
            Row {
                id: filters
                anchors.top: search.bottom; anchors.topMargin: Metrics.gap8; spacing: Metrics.gap6
                PillButton { text: I18n.tr("All", I18n.rev); kind: dialog.filter === "all" ? "primary" : "secondary"; onClicked: dialog.choose("all") }
                PillButton { text: I18n.tr("Standard done", I18n.rev); kind: dialog.filter === "standard" ? "primary" : "secondary"; onClicked: dialog.choose("standard") }
                PillButton { text: I18n.tr("Mood done", I18n.rev); kind: dialog.filter === "mood" ? "primary" : "secondary"; onClicked: dialog.choose("mood") }
                PillButton { text: I18n.tr("Standard pending", I18n.rev); kind: dialog.filter === "standard-pending" ? "primary" : "secondary"; onClicked: dialog.choose("standard-pending") }
                PillButton { text: I18n.tr("Mood pending", I18n.rev); kind: dialog.filter === "mood-pending" ? "primary" : "secondary"; onClicked: dialog.choose("mood-pending") }
            }
            Row {
                id: notice
                anchors.top: filters.bottom; anchors.topMargin: Metrics.gap8; spacing: Metrics.gap8
                Text { anchors.verticalCenter: parent.verticalCenter; text: dialog.error || I18n.tr("Showing", I18n.rev) + " " + trackRows.count + " / " + dialog.summary.matched; font.pointSize: Type.meta; color: dialog.error ? Theme.color.statusWarning : Theme.color.textTertiary }
                PillButton { objectName: "inventoryAllRuns"; visible: dialog.runId > 0; text: I18n.tr("All saved results", I18n.rev); onClicked: dialog.chooseRun(0) }
                PillButton { objectName: "inventoryCurrentRun"; visible: dialog.runId === 0 && activity.latest !== null; text: I18n.tr("Latest run", I18n.rev); onClicked: dialog.chooseRun(activity.latest.id) }
            }
            ListView {
                id: list
                objectName: "inventoryTrackList"
                anchors.top: notice.bottom; anchors.topMargin: Metrics.gap8; anchors.bottom: parent.bottom; width: parent.width
                model: trackRows; clip: true; spacing: Metrics.gap2; boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ViewportScrollBar {}
                delegate: AnalysisInventoryRow { required property int index; entry: trackRows.get(index); alternate: index % 2 === 0; width: ListView.view.width - Metrics.gap12 }
            }
            Text { anchors.centerIn: list; visible: trackRows.count === 0 && !dialog.error; text: dialog.runId ? I18n.tr("No tracks have been processed in this run yet.", I18n.rev) : I18n.tr("No tracks match this filter.", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.body }
        }
        ListView {
            objectName: "inventoryRunList"
            anchors.top: tabs.bottom; anchors.topMargin: Metrics.gap12; anchors.bottom: footer.top; anchors.bottomMargin: Metrics.gap12
            width: parent.width; visible: dialog.history; model: runRows; clip: true; spacing: Metrics.gap8; boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar {}
            delegate: AnalysisRunRow { required property int index; entry: runRows.get(index); stamp: dialog.date(entry.started_at); status: dialog.state(entry.state); width: ListView.view.width - Metrics.gap12; onAsked: dialog.chooseRun(entry.id) }
            Text { anchors.centerIn: parent; visible: runRows.count === 0; text: I18n.tr("New analysis runs will appear here. Your saved track results are available in Tracks.", I18n.rev); width: parent.width - Metrics.gap24; wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter; color: Theme.color.textTertiary; font.pointSize: Type.body }
        }
        Column {
            id: footer
            anchors.bottom: parent.bottom; width: parent.width; spacing: Metrics.gap8
            Text { width: parent.width; text: I18n.tr("Results are saved after every track. Run Analyze again to continue the pending tracks.", I18n.rev); wrapMode: Text.WordWrap; font.pointSize: Type.meta; color: Theme.color.textTertiary }
            Row {
                anchors.right: parent.right; spacing: Metrics.gap8
                PillButton { text: I18n.tr("Refresh", I18n.rev); onClicked: dialog.fetch(true) }
                PillButton { objectName: "inventoryLoadMore"; visible: !dialog.history; text: I18n.tr("Load more", I18n.rev); enabled: trackRows.count < dialog.summary.matched && dialog.error === ""; onClicked: dialog.fetch(false) }
            }
        }
    }
}
