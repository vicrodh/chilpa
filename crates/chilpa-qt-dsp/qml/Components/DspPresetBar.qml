import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Row {
    id: bar
    property var presets: []
    property var favorites: []
    property string selectedKey: ""
    property bool edited: false
    property string selectedName: ""
    property string selectedCategory: ""
    readonly property var entries: {
        if (!selectedKey || !selectedName || presets.some(e => e.plugin + "/" + e.preset.id === selectedKey)) return presets
        const boundary = selectedKey.lastIndexOf("/")
        return [{plugin: selectedKey.slice(0, boundary), preset: {id: selectedKey.slice(boundary + 1), name: selectedName, category: selectedCategory}}].concat(presets)
    }
    signal chosen(var entry)
    signal favoriteRequested()
    spacing: Metrics.gap10
    SelectField {
        objectName: "dspPresetPicker"
        width: Math.max(0, bar.width - favorite.width - bar.spacing)
        menuWidthLimit: Math.min(bar.width, Metrics.dspPanelWidth - Metrics.modalInsets)
        options: bar.entries.map(e => e.preset.name)
        details: bar.entries.map(e => e.preset.category)
        values: bar.entries.map(e => e.plugin + "/" + e.preset.id)
        value: bar.selectedKey
        placeholder: I18n.tr("Choose a preset", I18n.rev)
        onPicked: value => {
            const entry = bar.entries.find(e => e.plugin + "/" + e.preset.id === value)
            if (entry) bar.chosen(entry)
        }
    }
    IconButton {
        id: favorite
        objectName: "dspFavorite"
        glyph: Icons.star
        tip: I18n.tr("Favorite preset", I18n.rev)
        checked: bar.favorites.includes(bar.selectedKey)
        enabled: !!bar.selectedKey && !bar.edited
        activeFocusOnTab: enabled
        Accessible.name: tip
        Accessible.role: Accessible.Button
        Keys.onReturnPressed: clicked()
        Keys.onSpacePressed: clicked()
        onClicked: bar.favoriteRequested()
    }
}
