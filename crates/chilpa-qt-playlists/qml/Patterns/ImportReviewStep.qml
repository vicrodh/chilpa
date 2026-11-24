import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "../Theme"

// The matches, one line per song and destination: progress while they are
// found, filters (with their counts when the importer gives them), and
// pages of the importer's size.
Item {
    id: step
    property var status: ({})
    property var places: ({})
    property string filter: "all"
    property int offset: 0
    readonly property int pageSize: ImportFrame.page
    readonly property var counts: status.counts || null
    readonly property var targets: status.targets || []
    readonly property int total: status.row_count || 0
    readonly property var filters: [{id: "all", text: I18n.tr("All", I18n.rev)}]
        .concat(counts ? [{id: "found", text: I18n.tr("Found", I18n.rev)}, {id: "options", text: I18n.tr("With options", I18n.rev)}] : [])
        .concat([{id: "unmatched", text: I18n.tr("Not found", I18n.rev)}, {id: "error", text: I18n.tr("Errors", I18n.rev)}])
    signal asked(string filter, int offset)
    signal chose(int index, string target, string source, string reference)
    // Sources that failed on every song: said once above the list.
    readonly property var down: !counts || !counts.all ? [] : Object.keys(status.source_errors || {}).filter(id => status.source_errors[id] >= counts.all)
    property var asking: ({})
    function count(id) { return !counts ? -1 : id === "unmatched" ? counts.not_found ?? -1 : counts[id] ?? -1 }
    function quality(value) {
        const q = value || {}, fields = []
        if (q.codec) fields.push(q.codec.toUpperCase())
        if (q.tier === "hi_res") fields.push("Hi-Res")
        if (q.bits) fields.push(q.bits + " bit")
        if (q.rate) fields.push((q.rate / 1000) + " kHz")
        return fields.join(" · ")
    }
    function trouble(errors) { return Object.keys(errors || {}).filter(id => down.indexOf(id) < 0).map(id => I18n.trArgs("{} did not answer", [(places[id] || {}).name || id], I18n.rev)).join(" · ") }
    // One menu for every row: the song's choice first, then its options.
    function ask(entry, target, anchor) {
        const current = (entry.matches || {})[target], options = (entry.alternatives || {})[target] || []
        const line = c => [(places[c.source] || {}).name || c.source, c.version || ""].filter(Boolean).join(" · ")
        const meta = c => [quality(c.quality), Math.round((c.confidence || 0) * 100) + "%"].filter(Boolean).join(" · ")
        asking = {index: entry.index, target: target, options: options, offset: current ? 1 : 0}
        picker.entries = (current ? [{text: line(current), detail: meta(current), checked: true}] : []).concat(options.map(c => ({text: line(c), detail: meta(c)})))
        const at = anchor.mapToItem(step, 0, anchor.height)
        picker.x = at.x; picker.y = at.y
        picker.open()
    }
    Menu {
        id: picker
        objectName: "importOptionsMenu"
        onChosen: index => {
            const option = step.asking.options[index - step.asking.offset]
            if (option) step.chose(step.asking.index, step.asking.target, option.source, option.ref)
        }
    }
    Column {
        id: head
        width: parent.width; spacing: Metrics.gap12
        StatusArea {
            objectName: "importProgress"
            width: parent.width; visible: step.status.phase === "matching"
            label: I18n.tr("Finding matches…", I18n.rev); text: step.status.current || ""
            progress: (step.status.total_work || 0) > 0 ? (step.status.completed || 0) / step.status.total_work : -1
        }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            Repeater {
                model: step.filters
                Chip {
                    required property var modelData
                    objectName: "importFilter_" + modelData.id
                    active: step.filter === modelData.id
                    text: step.count(modelData.id) >= 0 ? I18n.trArgs("{} · {}", [modelData.text, String(step.count(modelData.id))], I18n.rev) : modelData.text
                    onClicked: step.asked(modelData.id, 0)
                }
            }
        }
        Text {
            objectName: "importSourceDown"
            width: head.width; visible: step.down.length > 0; wrapMode: Text.Wrap; textFormat: Text.PlainText
            font.pointSize: Type.small; color: Theme.color.statusWarning
            text: step.down.map(id => I18n.trArgs("{} did not answer; the other sources were searched.", [(step.places[id] || {}).name || id], I18n.rev)).join(" ")
        }
        Repeater {
            model: step.status.warnings || []
            Text { required property string modelData; width: head.width; wrapMode: Text.Wrap; textFormat: Text.PlainText; text: modelData; font.pointSize: Type.small; color: Theme.color.textTertiary }
        }
    }
    ListView {
        id: list
        objectName: "importRows"
        anchors.top: head.bottom; anchors.topMargin: Metrics.gap12
        anchors.bottom: pager.visible ? pager.top : parent.bottom; anchors.bottomMargin: pager.visible ? Metrics.gap8 : 0
        width: parent.width; clip: true
        model: step.status.rows || []
        Controls.ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.pageGutter }
        delegate: Column {
            id: entry
            required property var modelData
            required property int index
            width: ListView.view.width
            Repeater {
                model: step.targets.length ? step.targets : [""]
                ImportReviewRow {
                    required property string modelData
                    required property int index
                    readonly property var found: (entry.modelData.matches || {})[modelData] || null
                    width: entry.width; rowIndex: entry.index
                    number: index === 0 ? entry.modelData.index : 0
                    original: index === 0 ? entry.modelData.original : null
                    destination: step.targets.length > 1 ? (step.places[modelData] || {}).name || modelData : ""
                    match: found
                    sourceInfo: found ? (step.places[found.source] || {}).info || ({}) : ({})
                    quality: found ? step.quality(found.quality) : ""
                    problem: index === 0 ? step.trouble(entry.modelData.errors) : ""
                    problemDetail: index === 0 ? Object.values(entry.modelData.errors || {}).join("\n") : ""
                    optionCount: ((entry.modelData.alternatives || {})[modelData] || []).length
                    onOptionsAsked: anchor => step.ask(entry.modelData, modelData, anchor)
                }
            }
        }
    }
    Row {
        id: pager
        objectName: "importPager"
        visible: step.total > step.pageSize
        anchors.bottom: parent.bottom; anchors.right: parent.right
        spacing: Metrics.gap8
        Text {
            anchors.verticalCenter: parent.verticalCenter
            font.pointSize: Type.small; color: Theme.color.textSecondary
            text: I18n.trArgs("Showing {}–{} of {}", [String(step.offset + 1), String(Math.min(step.offset + step.pageSize, step.total)), String(step.total)], I18n.rev)
        }
        IconButton { glyph: Icons.chevronLeft; tip: I18n.tr("Previous page", I18n.rev); enabled: step.offset > 0; onClicked: step.asked(step.filter, Math.max(0, step.offset - step.pageSize)) }
        IconButton { glyph: Icons.chevronRight; tip: I18n.tr("Next page", I18n.rev); enabled: step.offset + step.pageSize < step.total; onClicked: step.asked(step.filter, step.offset + step.pageSize) }
    }
}
