import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.query
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Primitives"
import "../Theme"

// Columns | Refine beside the crumb line: the area under it shows one or
// the other, and choosing the one shown hides it. While Refine is not
// showing, its segment carries the count of conditions, so a query out of
// sight is never invisible (F1). Narrow, only the glyphs (F7).
Segments {
    id: control
    objectName: "refineSwitch"
    property bool refineMode: SettingsModel.kept("ui.library.focus") === "true"
    // How many conditions the query holds, groups counted by their own.
    readonly property var reading: {
        I18n.rev
        return JSON.parse(QueryChips.refine(Library.queryText) || "{\"rows\":[]}")
    }
    readonly property int conditions: reading.complex ? (reading.complex.count || 1)
        : (reading.rows || []).reduce((n, row) => n + (row.group ? row.rows.length : 1), 0)
    property bool narrow: false
    readonly property bool refineShown: SettingsModel.panesOpen && refineMode
    current: SettingsModel.panesOpen ? (refineMode ? 1 : 0) : -1
    tall: Refine.modeTall
    pad: Refine.modePad
    round: Refine.modeRound
    textSize: Type.row
    bare: narrow
    options: [
        { text: I18n.tr("Columns", I18n.rev), glyph: Icons.columns3 },
        { text: I18n.tr("Refine", I18n.rev), glyph: Icons.adjustmentsHorizontal, badge: refineShown ? 0 : conditions }
    ]
    // Undo walks back whatever changed the query: the panel, the columns or
    // the console. The switch lives as long as the Library screen does.
    Connections {
        target: Library
        function onQueryTextChanged() { QueryChips.observe(Library.queryText) }
    }
    Component.onCompleted: QueryChips.observe(Library.queryText)
    function show(refine) {
        refineMode = refine
        SettingsModel.keep("ui.library.focus", refine ? "true" : "false")
        SettingsModel.panesOpen = true
    }
    onActivated: (index) => {
        const same = refineMode === (index === 1)
        refineMode = index === 1
        SettingsModel.keep("ui.library.focus", refineMode ? "true" : "false")
        SettingsModel.panesOpen = same ? !SettingsModel.panesOpen : true
    }
}
