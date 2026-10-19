import QtQuick
import app.chilpa.kit
import app.chilpa.themes
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The theme's control in Settings (QBZ's Theme row): a button that cycles
// the list through every theme, the dark ones and the light ones, and the
// list itself, with its search. Auto and Custom are listed only with every
// theme, as QBZ lists them.
Row {
    id: picker

    // Every theme, as the row of Settings carries them.
    property var options: []
    property var values: []
    property string value

    signal picked(string value)

    readonly property var doc: {
        try { return JSON.parse(ThemeEditor.state) } catch (_) { return ({}) }
    }
    readonly property int filter: picker.doc.filter || 0
    readonly property var light: picker.doc.light || []
    // The places in the full list the filter keeps.
    readonly property var kept: {
        const out = []
        for (let i = 0; i < picker.values.length; i++) {
            const slug = picker.values[i]
            const synthetic = slug === "auto" || slug === "custom"
            const isLight = picker.light.indexOf(slug) >= 0
            if (picker.filter === 1 && (isLight || synthetic)) continue
            if (picker.filter === 2 && (!isLight || synthetic)) continue // derived: the light filter
            out.push(i)
        }
        return out
    }

    spacing: Metrics.gap8

    Component.onCompleted: ThemeEditor.reload()
    Connections { target: ThemeModel; function onModeChanged() { ThemeEditor.reload() } }
    Connections { target: I18n; function onRevChanged() { ThemeEditor.reload() } }

    IconButton {
        objectName: "themeFilter"
        anchors.verticalCenter: parent.verticalCenter
        glyph: picker.filter === 1 ? Icons.moon : picker.filter === 2 ? Icons.sun : Icons.sunMoon // derived: the light filter
        tip: picker.filter === 1 ? I18n.tr("Dark themes", I18n.rev)
            : picker.filter === 2 ? I18n.tr("Light themes", I18n.rev) // derived: the light filter
            : I18n.tr("All themes", I18n.rev)
        onClicked: ThemeEditor.setFilter((picker.filter + 1) % 3) // derived: three filters
    }
    SelectField {
        objectName: "themeList"
        anchors.verticalCenter: parent.verticalCenter
        options: picker.kept.map((i) => picker.options[i])
        values: picker.kept.map((i) => picker.values[i])
        value: picker.value
        onPicked: (value) => picker.picked(value)
    }
}
