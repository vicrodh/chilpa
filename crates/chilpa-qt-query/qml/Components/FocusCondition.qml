import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: condition
    required property var row
    property bool any: false
    signal edited(var change)
    width: parent ? parent.width : 0
    spacing: Metrics.gap6
    function change(kind, values) { edited(Object.assign({ kind: kind, field: row.field }, values)) }
    Text { text: condition.row.label; color: Theme.color.textSecondary; font.pointSize: Type.label; font.weight: Type.medium }
    Flow {
        width: parent.width
        spacing: Metrics.gap6
        Repeater {
            model: condition.row.include.concat(condition.row.exclude)
            Row {
                required property int index
                required property string modelData
                readonly property bool excluded: index >= condition.row.include.length
                spacing: Metrics.gap6
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: index > 0 || parent.excluded
                    text: parent.excluded ? (index === condition.row.include.length ? I18n.tr("none of", I18n.rev) : I18n.tr("or", I18n.rev)) : I18n.tr("or", I18n.rev)
                    color: Theme.color.textSecondary
                    font.pointSize: Type.meta
                }
                FocusValue {
                    text: modelData
                    excluded: parent.excluded
                    onToggled: condition.change("value", { value: modelData, state: excluded ? "include" : "exclude" })
                    onRemoved: condition.change("value", { value: modelData, state: "none" })
                }
            }
        }
        FocusValue {
            visible: condition.row.missing
            text: condition.row.field === "genre" ? I18n.tr("No genre", I18n.rev) : condition.row.field === "composer" ? I18n.tr("No composer", I18n.rev) : condition.row.field === "year" ? I18n.tr("No year", I18n.rev) : I18n.tr("No label", I18n.rev)
            onToggled: condition.change("missing", { on: false })
            onRemoved: condition.change("missing", { on: false })
        }
    }
    Row {
        width: parent.width
        visible: condition.row.range !== null
        height: visible ? years.height : 0
        Loader {
            id: years
            width: parent.width - removeRange.width
            active: condition.row.range !== null
            sourceComponent: FocusYears {
                range: condition.row.range
                onEdited: (from, to) => condition.change("range", { from: from, to: to })
            }
        }
        IconButton { id: removeRange; glyph: Icons.x; tip: I18n.tr("Remove", I18n.rev); onClicked: condition.change("range", { from: null, to: null }) }
    }
    Row {
        visible: condition.row.within !== null
        height: visible ? implicitHeight : 0
        spacing: Metrics.gap6
        SelectField {
            objectName: "focusAdded"
            value: String(condition.row.within)
            readonly property var presets: ["7", "30", "90", "365"]
            values: presets.includes(value) ? presets : [value].concat(presets)
            options: values.map(n => I18n.trArgs("Last {} days", [n], I18n.rev))
            onPicked: (value) => condition.change("within", { days: Number(value) })
        }
        IconButton { glyph: Icons.x; tip: I18n.tr("Remove", I18n.rev); onClicked: condition.change("within", { days: 0 }) }
    }
    Row {
        visible: condition.row.minimum !== null
        height: visible ? implicitHeight : 0
        spacing: Metrics.gap6
        SelectField {
            objectName: "focusRating"
            value: String(condition.row.minimum)
            values: ["0", "1", "2", "3", "4", "5"]
            options: values.map(n => I18n.trCount("At least {} star", "At least {} stars", Number(n), I18n.rev))
            onPicked: (value) => condition.change("minimum", { stars: Number(value) })
        }
        IconButton { glyph: Icons.x; tip: I18n.tr("Remove", I18n.rev); onClicked: condition.change("minimum", { stars: null }) }
    }
}
