import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Item {
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
    implicitWidth: buttons.implicitWidth
    implicitHeight: buttons.implicitHeight
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
        id: buttons
        spacing: Metrics.gap6
        PillButton {
            id: genresButton
            objectName: "discoveryGenres"
            visible: (controls.about.genres || []).length > 0
            text: I18n.tr("Genres", I18n.rev) + (controls.selectedGenres.length ? " (" + controls.selectedGenres.length + ")" : "")
            glyph: SourceGlyphs.listFilter; iconFamily: SourceGlyphs.font
            kind: controls.selectedGenres.length ? "primary" : "secondary"
            onClicked: genres.opened ? genres.close() : genres.open()
        }
        IconButton {
            objectName: "discoveryCustomize"
            size: Metrics.button32; round: true
            glyph: SourceGlyphs.homeGear; iconFamily: SourceGlyphs.font
            tip: I18n.tr("Customize", I18n.rev)
            onClicked: customization.open()
        }
    }
    Popup {
        id: genres
        objectName: "discoveryGenrePopup"
        x: controls.width - width
        y: controls.height + Metrics.gap8
        width: Math.min(Metrics.sourcesPopoverWidth, Overlay.overlay ? Overlay.overlay.width - Metrics.modalInsets : Metrics.sourcesPopoverWidth)
        height: Math.min(implicitHeight, Overlay.overlay ? Overlay.overlay.height - Metrics.modalInsets : implicitHeight)
        padding: Metrics.gap16
        modal: true; dim: false; focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { radius: Metrics.radiusPanel; color: Theme.color.surfaceRaised; Edge {} }
        onOpened: genreSearch.text = ""
        contentItem: Column {
            spacing: Metrics.gap12
            Row {
                width: parent.width
                Text { width: parent.width - closeGenres.width; text: I18n.tr("Genres", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.section }
                IconButton { id: closeGenres; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: genres.close() }
            }
            SearchField { id: genreSearch; width: parent.width; placeholder: I18n.tr("Search", I18n.rev); onEscaped: text = "" }
            Flickable {
                width: parent.width
                height: Math.min(genreChoices.implicitHeight, Metrics.heroArt)
                contentHeight: genreChoices.implicitHeight; clip: true
                ScrollBar.vertical: ViewportScrollBar {}
                Flow {
                    id: genreChoices
                    width: parent.width; spacing: Metrics.gap8
                    Repeater {
                        model: (controls.about.genres || []).filter(g => g.name.toLowerCase().includes(genreSearch.text.toLowerCase()))
                        PillButton {
                            required property var modelData
                            text: modelData.name
                            kind: controls.selectedGenres.includes(String(modelData.id)) ? "primary" : "secondary"
                            onClicked: {
                                const id = String(modelData.id)
                                controls.selectedGenres = controls.selectedGenres.includes(id) ? controls.selectedGenres.filter(g => g !== id) : controls.selectedGenres.concat([id])
                            }
                        }
                    }
                }
            }
            Row {
                spacing: Metrics.gap8
                PillButton { text: I18n.tr("Apply", I18n.rev); kind: "primary"; onClicked: { genres.close(); Navigation.goKey("source", controls.about.plugin + " native:discover:" + controls.about.tab + ":" + controls.selectedGenres.join(",")) } }
                PillButton { text: I18n.tr("Clear", I18n.rev); onClicked: controls.selectedGenres = [] }
            }
        }
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
