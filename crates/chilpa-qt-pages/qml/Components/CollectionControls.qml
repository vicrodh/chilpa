import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.pages
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Flow {
    id: controls
    required property var about
    required property string source
    spacing: Metrics.gap8
    function applyField(key, value, excludes) {
        const fields = Object.assign({}, about.filters || {})
        fields[key] = value
        for (const excluded of excludes || []) fields[excluded] = false
        navigate({filters: fields, order: about.order || "added"})
    }
    function navigate(query) {
        const base = about.base_key || source
        const route = NativePages.requestKey(base, JSON.stringify(query))
        if (!route) return
        SettingsModel.keep("ui.source.collection." + base, route)
        Navigation.goKey("source", route)
    }
    KeyedModel { id: fields; items: controls.about.filter_controls || []; keyField: "key" }
    Repeater {
        model: fields
        Loader {
            id: field
            required property var entry
            readonly property var value: (controls.about.filters || {})[entry.key]
            sourceComponent: entry.type === "search" ? search : entry.type === "toggle" ? toggle : choice
            Component {
                id: search
                SearchField {
                    width: Metrics.screenSearch
                    maximumLength: 256
                    placeholder: field.entry.label
                    text: String(field.value || "")
                    onAccepted: controls.applyField(field.entry.key, text, [])
                    onCleared: controls.applyField(field.entry.key, "", [])
                    onEscaped: controls.applyField(field.entry.key, "", [])
                }
            }
            Component {
                id: toggle
                PillButton {
                    text: field.entry.label
                    kind: field.value === true ? "primary" : "secondary"
                    glyph: field.value === true ? Icons.check : ""
                    onClicked: controls.applyField(field.entry.key, field.value !== true, field.entry.excludes)
                }
            }
            Component {
                id: choice
                SortButton {
                    directed: false
                    value: String(field.value ?? "")
                    options: (field.entry.options || []).map(option => ({value: String(option.value), text: option.label}))
                    onPicked: value => {
                        const option = (field.entry.options || []).find(option => String(option.value) === value)
                        controls.applyField(field.entry.key, value, option?.excludes || field.entry.excludes)
                    }
                }
            }
        }
    }
    SortButton {
        objectName: "collectionOrder"
        visible: options.length > 0
        directed: false
        value: controls.about.order || "added"
        options: (controls.about.order_options || []).map(option => ({value: option.value, text: option.label}))
        onPicked: value => controls.navigate({filters:controls.about.filters || {},order:value})
    }
}
