import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Components"

Column {
    id: controls
    property var about: ({})
    readonly property string preference: "ui.source." + (about.plugin || "") + ".discover." + (about.tab || "home")
    property var preferences: []
    property var selectedGenres: []
    property bool filtering: false
    readonly property var sections: {
        const available = about.shelves || []
        const kept = preferences.map(p => Object.assign({}, available.find(s => s.id === p.id) || {}, p)).filter(p => p.id && available.some(s => s.id === p.id))
        return kept.concat(available.filter(s => !kept.some(p => p.id === s.id)))
    }
    readonly property var shown: sections.filter(s => s.enabled !== false)
    spacing: Metrics.gap8
    function restore() {
        try { preferences = JSON.parse(SettingsModel.kept(preference) || "[]") } catch (_) { preferences = [] }
        selectedGenres = (about.genre_filter || "").split(",").filter(Boolean)
    }
    onPreferenceChanged: restore()
    onAboutChanged: restore()
    function change(index, field, value) {
        const rows = sections.map(s => ({id:s.id, enabled:s.enabled !== false, size:s.size || 18}))
        rows[index][field] = value
        preferences = rows; SettingsModel.keep(preference, JSON.stringify(rows))
    }
    function move(index, delta) {
        const rows = sections.map(s => ({id:s.id, enabled:s.enabled !== false, size:s.size || 18}))
        const other = index + delta
        if (other < 0 || other >= rows.length) return
        const row = rows[index]; rows[index] = rows[other]; rows[other] = row
        preferences = rows; SettingsModel.keep(preference, JSON.stringify(rows))
    }
    Row {
        spacing: Metrics.gap8
        PillButton { text: I18n.tr("Genres", I18n.rev); onClicked: controls.filtering = !controls.filtering }
        PillButton { text: I18n.tr("Customize", I18n.rev); onClicked: customization.open() }
    }
    Flow {
        width: parent.width; visible: controls.filtering; spacing: Metrics.gap8
        Repeater {
            model: controls.about.genres || []
            PillButton {
                required property var modelData
                text: (controls.selectedGenres.includes(String(modelData.id)) ? "✓ " : "") + modelData.name
                onClicked: {
                    const id = String(modelData.id)
                    controls.selectedGenres = controls.selectedGenres.includes(id) ? controls.selectedGenres.filter(g => g !== id) : controls.selectedGenres.concat([id])
                }
            }
        }
        PillButton { text: I18n.tr("Apply", I18n.rev); onClicked: Navigation.goKey("source", controls.about.plugin + " native:discover:" + controls.about.tab + ":" + controls.selectedGenres.join(",")) }
        PillButton { text: I18n.tr("Clear", I18n.rev); onClicked: controls.selectedGenres = [] }
    }
    Modal {
        id: customization
        width: Math.min(Metrics.settingsMaxWidth, parent.width - Metrics.modalInsets)
        height: Math.min(parent.height - Metrics.modalInsets, Metrics.heroArt * 3) // derived: bounded dialog with a scrolling list
        Column {
            anchors.fill: parent; spacing: Metrics.gap16
            Text { id: title; text: I18n.tr("Customize", I18n.rev); font.pointSize: Type.titleSmall; color: Theme.color.textPrimary }
            ListView {
                id: choices
                width: parent.width; height: parent.height - title.height - actions.height - parent.spacing * 2 // derived: fixed header and footer
                clip: true; spacing: Metrics.gap8; model: controls.sections
                ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}
                delegate: Row {
                    required property var modelData
                    required property int index
                    width: choices.width - Metrics.gap16; spacing: Metrics.gap8
                    Toggle { id: enabledToggle; checked: modelData.enabled !== false; onToggled: on => controls.change(index, "enabled", on) }
                    Text { width: parent.width - enabledToggle.width - up.width - down.width - count.width - parent.spacing * 4; text: modelData.title || ""; elide: Text.ElideRight; color: Theme.color.textPrimary; font.pointSize: Type.body; height: count.height; verticalAlignment: Text.AlignVCenter } // derived: flexible label between fixed controls
                    IconButton { id: up; glyph: Icons.chevronUp; tip: I18n.tr("Move up", I18n.rev); enabled: index > 0; onClicked: controls.move(index, -1) }
                    IconButton { id: down; glyph: Icons.chevronDown; tip: I18n.tr("Move down", I18n.rev); enabled: index < controls.sections.length - 1; onClicked: controls.move(index, 1) }
                    SortButton { id: count; value: String(modelData.size || 18); options: [{value:"6",text:"6"},{value:"12",text:"12"},{value:"18",text:"18"},{value:"24",text:"24"}]; onPicked: value => controls.change(index, "size", Number(value)) }
                }
            }
            Row {
                id: actions; spacing: Metrics.gap8
                PillButton { text: I18n.tr("Restore defaults", I18n.rev); onClicked: { controls.preferences = []; SettingsModel.keep(controls.preference, "[]") } }
                PillButton { text: I18n.tr("Close", I18n.rev); onClicked: customization.close() }
            }
        }
    }
}
