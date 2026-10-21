import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import QtQuick.Controls.Basic as Controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "../Theme"

// Refine (F2, F4, F5): the query as rows, one per condition and a box per
// group, in a panel that grows with them up to the height its grip gives,
// the rows scrolling on their own while the head and the add field stay.
// Every edit goes to the facade, which answers with the new query text; the
// panel keeps no query of its own.
Rectangle {
    id: panel
    objectName: "refinePanel"
    property string query: Library.queryText
    // The tracks the query leaves, as the list says it.
    property string summary: ""
    property string loading: ""
    property bool narrow: false
    // German labels need the wider column (124 instead of 104).
    property bool longLabels: I18n.language === "de"
    property bool consoleShown: false
    // The field whose values are open, or nothing.
    property string picking: ""
    // The group new conditions go into: its id, "new" while it is still empty.
    property string target: ""
    signal detachAsked()

    readonly property var reading: {
        I18n.rev
        return JSON.parse(QueryChips.refine(panel.query) || "{\"mode\":\"all\",\"rows\":[]}")
    }
    readonly property var rows: reading.rows || []
    readonly property int conditions: rows.reduce((n, row) => n + (row.group ? row.rows.length : 1), 0)
    // A row's identity across edits: the facade's ids are places, and a
    // place changes when a row before it goes.
    function keyOf(row) {
        return row.group ? "group:" + row.rows.map(keyOf).join("|") : [row.field, row.kind, row.negated ? "-" : "+"].join(":")
    }
    readonly property var items: rows.map(row => Object.assign({ key: keyOf(row) }, row))
        .concat(target === "new" ? [{ key: "new", id: "new", group: true, mode: "any", rows: [] }] : [])

    function edit(change) {
        const adds = ["value", "range", "rating", "added", "none", "words"]
        if (target !== "" && change.target === undefined && adds.indexOf(change.op) >= 0)
            change.target = target
        if (change.target === "new")
            target = ""
        Library.applyQuery(QueryChips.refineEdit(panel.query, JSON.stringify(change)))
    }

    implicitHeight: head.implicitHeight + list.contentHeight + Refine.rowsBottom + Refine.footTop + foot.implicitHeight + Refine.footBottom
    color: Theme.ambient ? Theme.color.ambientRaised : Theme.color.surfacePane
    radius: Refine.panelRound
    border.width: Metrics.hairline
    border.color: Theme.color.strokeHairline
    clip: true

    RefineHead {
        id: head
        width: parent.width
        conditions: panel.conditions
        mode: panel.reading.mode || "all"
        summary: panel.summary
        loading: panel.loading
        consoleShown: panel.consoleShown
        canUndo: QueryChips.canUndo
        canDetach: !panel.narrow
        onModeChosen: (mode) => panel.edit({ op: "mode", mode: mode })
        onUndoAsked: Library.applyQuery(QueryChips.undo())
        onClearAsked: { panel.target = ""; Library.applyQuery("") }
        onConsoleToggled: panel.consoleShown = !panel.consoleShown
        onDetachAsked: panel.detachAsked()
    }
    KeyedModel {
        id: keyed
        keyField: "key"
        items: panel.items
    }
    MotionListView {
        id: list
        objectName: "refineRows"
        anchors.top: head.bottom
        x: Refine.rowsSide
        width: parent.width - Refine.rowsSide * 2 // derived: the rows' inset on both sides
        height: Math.max(0, Math.min(contentHeight, panel.height - head.height - Refine.rowsBottom - Refine.footTop - foot.height - Refine.footBottom))
        clip: true
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds
        spacing: Refine.rowsApart
        model: keyed
        Controls.ScrollBar.vertical: ViewportScrollBar {}
        delegate: Loader {
            id: slot
            required property var entry
            width: list.width
            sourceComponent: entry.group ? groupBox : conditionRow
            Component {
                id: conditionRow
                ConditionRow {
                    row: slot.entry
                    narrow: panel.narrow
                    longLabels: panel.longLabels
                    picking: panel.picking === slot.entry.field
                    onRemoved: panel.edit({ op: "remove", id: slot.entry.id })
                    onOperatorChosen: (key) => panel.edit({ op: "operator", id: slot.entry.id, to: key })
                    onValueRemoved: (value) => panel.edit({ op: "removeValue", id: slot.entry.id, value: value })
                    onPickAsked: panel.picking = slot.entry.field
                }
            }
            Component {
                id: groupBox
                ConditionGroup {
                    group: slot.entry
                    target: panel.target === slot.entry.id
                    narrow: panel.narrow
                    longLabels: panel.longLabels
                    picking: panel.picking
                    onModeChosen: (mode) => slot.entry.id === "new" ? null : panel.edit({ op: "mode", mode: mode, target: slot.entry.id })
                    onTargetToggled: panel.target = panel.target === slot.entry.id ? "" : slot.entry.id
                    onUngrouped: slot.entry.id === "new" ? panel.target = "" : panel.edit({ op: "ungroup", id: slot.entry.id })
                    onRemoved: slot.entry.id === "new" ? panel.target = "" : panel.edit({ op: "remove", id: slot.entry.id })
                    onRowRemoved: (id) => panel.edit({ op: "remove", id: id })
                    onRowOperator: (id, key) => panel.edit({ op: "operator", id: id, to: key })
                    onRowValueRemoved: (id, value) => panel.edit({ op: "removeValue", id: id, value: value })
                    onRowPickAsked: (field) => panel.picking = field
                }
            }
        }
    }
    RefineFoot {
        id: foot
        anchors.top: list.bottom
        anchors.topMargin: Refine.rowsBottom + Refine.footTop
        x: Refine.footSide
        width: parent.width - Refine.footSide * 2 // derived: the foot's inset on both sides
        picking: panel.picking
        narrow: panel.narrow
        canGroup: panel.rows.length > 0 && !panel.reading.complex
        onFieldChosen: (field) => panel.picking = panel.picking === field ? "" : field
        onGroupAsked: panel.target = "new"
    }
}
