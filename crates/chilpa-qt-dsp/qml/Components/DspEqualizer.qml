import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    property var draft: ({})
    property var curve: []
    readonly property string filtersData: JSON.stringify(draft.filters || [])
    property var filterRows: []
    property real restoreY: 0
    function restoreScroll() { if (filters) filters.contentY = Math.max(filters.originY, Math.min(restoreY, filters.contentHeight - filters.height)) }
    function syncFilters() {
        if (filters) restoreY = filters.contentY
        filterRows = JSON.parse(filtersData)
        Qt.callLater(restoreScroll)
    }
    onFiltersDataChanged: syncFilters()
    Component.onCompleted: syncFilters()
    property real maximumCurveHeight: Metrics.dspCurveHeight
    property real viewportHeight: Metrics.dspPanelHeight
    property int selectedIndex: 0
    readonly property var currentFilter: draft.filters?.[selectedIndex] || ({})
    readonly property real valueWidth: Math.max(0, (width - Metrics.dspBandIndexWidth - Metrics.toggleCompactWidth - Metrics.dspBandShapeWidth - Metrics.dspChannelWidth - Metrics.button28 - Metrics.gap8 * 7) / 3) // derived: eight columns reserve three equally sized numeric fields
    signal edited(var value)
    signal command(string action, var data)
    signal previewCommand(string action, var data)
    spacing: Metrics.gap12
    onDraftChanged: selectedIndex = Math.max(0, Math.min(selectedIndex, (draft.filters || []).length - 1))
    Row {
        id: mode
        width: parent.width
        spacing: Metrics.gap12
        Toggle { checked: editor.draft.eq_enabled !== false; onToggled: on => { const next = JSON.parse(JSON.stringify(editor.draft)); next.eq_enabled = on; editor.edited(next) } }
        Text { text: I18n.tr("EQ enabled", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.body }
        Text { text: (editor.draft.filters || []).length + " / 64"; color: Theme.color.textTertiary; font.pointSize: Type.body }
    }
    DspCurve {
        id: chart
        width: parent.width
        maximumHeight: editor.maximumCurveHeight
        values: editor.curve
        filters: editor.draft.filters || []
        interactive: editor.draft.eq_enabled !== false
        selected: editor.selectedIndex
        onPicked: index => editor.selectedIndex = index
        onPreviewed: (index, frequency, gain) => editor.previewCommand("filter_preview", {index: index, frequency: frequency, gain_db: gain})
        onCommitted: (index, frequency, gain) => editor.command("filter_commit", {index: index, frequency: frequency, gain_db: gain})
        onQCommitted: (index, q) => editor.command("filter_edit", {index: index, field: "q", value: q})
    }
    Row {
        id: headings
        width: parent.width
        spacing: Metrics.gap8
        Text { width: Metrics.dspBandIndexWidth; text: "#"; color: Theme.color.textTertiary; font.pointSize: Type.small }
        Text { width: Metrics.toggleCompactWidth; elide: Text.ElideRight; text: I18n.tr("On", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.small }
        Text { width: Metrics.dspBandShapeWidth; text: I18n.tr("Filter", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.small }
        Text { width: Metrics.dspChannelWidth; text: I18n.tr("Channel", I18n.rev); color: Theme.color.textTertiary; font.pointSize: Type.small }
        Repeater {
            model: [I18n.tr("Frequency", I18n.rev) + " · Hz", I18n.tr("Gain", I18n.rev) + " · dB", "Q"]
            Text { required property string modelData; width: editor.valueWidth; text: modelData; color: Theme.color.textTertiary; font.pointSize: Type.small }
        }
    }
    ListView {
        id: filters
        objectName: "dspFilterList"
        width: parent.width
        height: Math.max(Metrics.dspFilterRowHeight, editor.viewportHeight - mode.height - chart.height - headings.height - actions.height - editor.spacing * 4) // derived: remaining space belongs to virtualized filters
        model: editor.filterRows
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        Controls.ScrollBar.vertical: ViewportScrollBar {}
        delegate: DspFilterRow {
            required property int index
            required property var modelData
            width: filters.width
            bandIndex: index
            current: editor.selectedIndex === index
            filter: modelData
            onSelected: editor.selectedIndex = index
            onCommand: (action, data) => editor.command(action, data)
        }
    }
    Flow {
        id: actions
        width: parent.width
        spacing: Metrics.gap8
        PillButton { objectName: "dspAddFilter"; text: I18n.tr("Add filter", I18n.rev); enabled: (editor.draft.filters || []).length < 64; onClicked: editor.command("filter_add", {}) }
        PillButton { text: editor.currentFilter.locked ? I18n.tr("Unlock filter", I18n.rev) : I18n.tr("Lock filter", I18n.rev); enabled: !!editor.currentFilter.shape; onClicked: editor.command("filter_edit", {index: editor.selectedIndex, field: "locked", value: !editor.currentFilter.locked}) }
        PillButton { text: I18n.tr("Move up", I18n.rev); enabled: editor.selectedIndex > 0; onClicked: { editor.command("filter_move", {index: editor.selectedIndex, target: editor.selectedIndex - 1}); --editor.selectedIndex } }
        PillButton { text: I18n.tr("Move down", I18n.rev); enabled: editor.selectedIndex + 1 < filters.count; onClicked: { editor.command("filter_move", {index: editor.selectedIndex, target: editor.selectedIndex + 1}); ++editor.selectedIndex } }
        PillButton { text: I18n.tr("Color group", I18n.rev) + " " + (Number(editor.currentFilter.color || 0) % 4 + 1); enabled: !!editor.currentFilter.shape && !editor.currentFilter.locked; onClicked: editor.command("filter_edit", {index: editor.selectedIndex, field: "color", value: (Number(editor.currentFilter.color || 0) + 1) % 4}) }
    }
}
