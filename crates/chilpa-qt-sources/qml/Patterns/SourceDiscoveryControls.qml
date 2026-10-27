import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/SectionChoices.js" as SectionChoices
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Item {
    id: controls
    property var about: ({})
    readonly property string preference: "ui.source." + (about.plugin || "") + ".discover." + (about.tab || "home")
    property var preferences: []
    property var selectedGenres: []
    property bool filtering: false
    readonly property var sections: SectionChoices.order((about.shelves || []).map(s => Object.assign({}, s, {size:0})), preferences)
    // A section the source sent empty has nothing to draw, so the page gives
    // it no slot: no skeleton shows and then folds away. Customize still
    // lists it.
    readonly property var shown: sections.filter(s => s.enabled !== false
        && ((s.items || []).length > 0 || (s.extra_tracks || []).length > 0 || !!s.error))
    implicitWidth: buttons.implicitWidth
    implicitHeight: buttons.implicitHeight
    function restore() {
        try { preferences = JSON.parse(SettingsModel.kept(preference) || "[]") } catch (_) { preferences = [] }
        selectedGenres = (about.genre_filter || "").split(",").filter(Boolean)
    }
    onPreferenceChanged: restore()
    onAboutChanged: restore()
    function change(index, field, value) {
        const rows = sections.map(s => ({id:s.id, enabled:s.enabled !== false, size:s.size || 0}))
        rows[index][field] = value
        preferences = rows; SettingsModel.keep(preference, JSON.stringify(rows))
    }
    function move(index, delta) {
        const rows = sections.map(s => ({id:s.id, enabled:s.enabled !== false, size:s.size || 0}))
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
            glyph: Icons.settings
            tip: I18n.tr("Customize", I18n.rev)
            onClicked: { customization.active = true; customization.item.open() }
        }
    }
    Popup {
        id: genres
        PopupInput { surface: genres }
        objectName: "discoveryGenrePopup"
        x: controls.width - width
        y: controls.height + Metrics.gap8
        width: Math.min(Metrics.sourcesPopoverWidth, Overlay.overlay ? Overlay.overlay.width - Metrics.modalInsets : Metrics.sourcesPopoverWidth)
        height: Math.min(implicitHeight, Overlay.overlay ? Overlay.overlay.height - Metrics.modalInsets : implicitHeight)
        padding: Metrics.gap16
        modal: true; dim: false; focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnReleaseOutside
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
                    width: parent.width - Metrics.gap16; spacing: Metrics.gap8
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
    Loader {
        id: customization
        active: false
        sourceComponent: SectionsCustomizer {
            sections: controls.sections
            onToggled: (index, on) => controls.change(index, "enabled", on)
            onSized: (index, size) => controls.change(index, "size", size)
            onMoved: (index, delta) => controls.move(index, delta)
            onRestored: { controls.preferences = []; SettingsModel.keep(controls.preference, "[]") }
        }
    }
}
