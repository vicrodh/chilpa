import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: choices
    property var selected: null
    signal toggled(string column)
    signal reordered(var columns)
    visible: selected !== null
    spacing: Metrics.gap4
    Text {
        text: I18n.tr("Columns", I18n.rev)
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textTertiary
    }
    readonly property var available: [
            {column:"year", label:I18n.tr("Year", I18n.rev)},
            {column:"genre", label:I18n.tr("Genre", I18n.rev)},
            {column:"label", label:I18n.tr("Label", I18n.rev)},
            {column:"composer", label:I18n.tr("Composer", I18n.rev)},
            {column:"rating", label:I18n.tr("Rating", I18n.rev)},
            {column:"bits", label:I18n.tr("Bit depth", I18n.rev)},
            {column:"rate", label:I18n.tr("Sample rate", I18n.rev)},
            {column:"artist", label:I18n.tr("Artist", I18n.rev)},
            {column:"album", label:I18n.tr("Album", I18n.rev)}
        ].filter(option => option.column !== "rating" || Prefs.ratings)
    readonly property var enabledColumns: selected === null ? [] : selected.filter((key, index) => selected.indexOf(key) === index && available.some(option => option.column === key))
    readonly property var ordered: enabledColumns.map(key => available.find(option => option.column === key))
        .concat(available.filter(option => enabledColumns.indexOf(option.column) < 0))
    function move(column, offset) {
        const from = enabledColumns.indexOf(column), to = from + offset
        if (from < 0 || to < 0 || to >= enabledColumns.length) return
        const columns = selected.slice(), at = columns.indexOf(column), destination = columns.indexOf(enabledColumns[to])
        columns.splice(at, 1)
        columns.splice(destination, 0, column)
        reordered(columns)
    }
    Repeater {
        model: choices.ordered
        Item {
            id: option
            required property var modelData
            readonly property int position: choices.enabledColumns.indexOf(modelData.column)
            width: choices.width
            height: Math.max(Metrics.button28, label.implicitHeight + Metrics.gap4)
            Text {
                id: label
                anchors.left: parent.left
                anchors.right: moves.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                text: modelData.label
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
            Row {
                id: moves
                anchors.right: switcher.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                width: visible ? implicitWidth : 0
                visible: option.position >= 0
                IconButton {
                    objectName: "paneMoveUp-" + option.modelData.column
                    glyph: Icons.chevronUp
                    tip: I18n.tr("Move up", I18n.rev)
                    size: Metrics.button24
                    enabled: option.position > 0
                    onClicked: choices.move(option.modelData.column, -1)
                }
                IconButton {
                    objectName: "paneMoveDown-" + option.modelData.column
                    glyph: Icons.chevronDown
                    tip: I18n.tr("Move down", I18n.rev)
                    size: Metrics.button24
                    enabled: option.position >= 0 && option.position < choices.enabledColumns.length - 1
                    onClicked: choices.move(option.modelData.column, 1)
                }
            }
            Toggle {
                id: switcher
                objectName: "paneToggle-" + modelData.column
                compact: true
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                checked: choices.selected !== null && choices.selected.indexOf(modelData.column) >= 0
                onToggled: choices.toggled(modelData.column)
            }
        }
    }
}
